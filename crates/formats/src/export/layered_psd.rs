//! A texture set's PSD: each output map a layer, a `/` in its name a layer group

use craft_psd::{Compression, GroupSpec, LayerSpec, PixelData, PsdBuilder};
use texturecraft_core::TextureSet;
use texturecraft_texel::linear_to_srgb;

use super::encoding::Encoding;
use super::map_renderer::MapRenderer;
use super::{
    ExportOptions, ExportedFile, FileFormat, PSD_MAX_FILE_BYTES, PSD_RGBA_PLANES, Padding, PlannedFile, PlannedLayer, check_psd_size, psd_size,
    quantize_integer, quantize_u16,
};
use crate::{FormatError, Result};

/// Where alpha sits in an RGBA texel
const ALPHA_INDEX: usize = 3;

/// Budget for the pixel buffers of one PSD export, including the renderer's padding and encoder
const MAX_PSD_WORKING_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Check file limits and pixel-buffer working memory before constructing the renderer.
pub(super) fn preflight<'a>(
    set: &TextureSet,
    options: &ExportOptions,
    tile: Option<u32>,
    depth: u8,
    layers: impl Iterator<Item = &'a PlannedLayer> + Clone,
) -> Result<(u32, u32)> {
    let texels = |(width, height)| u64::from(width) * u64::from(height);
    let folder_count = layers.clone().map(|layer| layer.name.matches('/').count()).sum();
    let sizes = layers.map(|layer| psd_size(set, tile, options.map_size(layer.map)));
    let (canvas, file_bytes) = check_psd_size(sizes.clone(), folder_count, depth)?;
    let float_bytes_per_texel = (PSD_RGBA_PLANES * size_of::<f32>()) as u64;
    let source_bytes = texels(psd_size(set, tile, None)).saturating_mul(float_bytes_per_texel);
    let largest_layer_bytes = sizes.map(texels).max().unwrap_or(0) * float_bytes_per_texel;
    let composite_bytes = texels(canvas) * float_bytes_per_texel;

    // Resizing overlaps source and destination, and RGBA conversion can overlap two output buffers
    let mut scratch_bytes = source_bytes.saturating_add(largest_layer_bytes).max(2 * largest_layer_bytes);
    if matches!(options.padding, Padding::DilationDiffusion(_)) {
        // A copied block, its weights, and the smaller pyramid levels fit within this source allowance
        scratch_bytes = scratch_bytes.max(source_bytes.saturating_mul(3));
    }

    let mut padding_bytes = 0;
    let mut padding_peak = 0;
    if options.padding != Padding::None {
        let atlas_texels = texels(set.size());
        padding_bytes = atlas_texels.saturating_mul(size_of::<u32>() as u64);

        // The atlas keeps its dilation indices after the frontiers are freed
        padding_peak = padding_bytes.saturating_add(atlas_texels.saturating_mul((4 * size_of::<usize>()) as u64));
    }

    // The encoder can hold staged pixels, planes and the output at once
    let rendering = scratch_bytes.saturating_add(composite_bytes).saturating_add(file_bytes);
    let encoding = 3 * file_bytes;
    let working_bytes = padding_bytes.saturating_add(rendering.max(encoding)).max(padding_peak);
    if working_bytes >= MAX_PSD_WORKING_BYTES {
        let (width, height) = canvas;
        return Err(FormatError::Invalid(format!(
            "a PSD at {width}×{height} would exceed the working memory budget; use a smaller export size, fewer PSD maps or another format"
        )));
    }

    Ok(canvas)
}

/// Render each layer at its requested size, aligned to the upper-left corner of the PSD canvas.
pub(super) fn render_psd(renderer: &MapRenderer, planned: PlannedFile, size: (u32, u32)) -> Result<ExportedFile> {
    let PlannedFile { name, tile, bit_depth, mut warnings, layers, .. } = planned;

    // Keep only the current layer in floating point alongside the merged image
    let mut composite = vec![0.0f32; size.0 as usize * size.1 as usize * PSD_RGBA_PLANES];
    let mut pixels = Vec::with_capacity(layers.len());
    let mut embedded = None;
    for layer in layers.iter().rev() {
        let map = renderer.render(layer.map, tile, FileFormat::Psd, bit_depth)?;
        embedded = map.embed.or(embedded);
        let rgba = to_rgba(map.data, map.component_count, &map.srgb);
        over(&mut composite, size.0, &rgba, map.size.0);

        let encoding = Encoding { format: FileFormat::Psd, depth: bit_depth, dither: layer.dither };
        let layer_pixels = pixel_data(rgba, map.size.0, encoding)?;
        pixels.push((layer.name.as_str(), map.size, layer_pixels));
    }
    if let Some(space) = embedded {
        warnings.push(format!("PSD export does not embed a colour space; the colours are in {}", space.name()));
    }

    let encoding = Encoding { format: FileFormat::Psd, depth: bit_depth, dither: false };
    let composite = pixel_data(composite, size.0, encoding)?;

    let bytes = write_layers(size, bit_depth, pixels, composite).map_err(|error| FormatError::Invalid(format!("psd encoding failed: {error}")))?;
    if bytes.len() as u64 >= PSD_MAX_FILE_BYTES {
        return Err(FormatError::Invalid(format!("\"{name}\" would reach the 2 GiB PSD file limit; use a smaller export size or fewer PSD maps")));
    }

    Ok(ExportedFile { name, bytes, width: size.0, height: size.1, warnings })
}

/// L, LA, RGB or RGBA samples as RGBA (gray repeated, opaque without alpha), sRGB transfer applied to the components `srgb` marks, values clamped to 0-1
fn to_rgba(mut data: Vec<f32>, component_count: usize, srgb: &[bool]) -> Vec<f32> {
    let encode = |component: usize, value: f32| if srgb.get(component).copied().unwrap_or(false) { linear_to_srgb(value) } else { value.clamp(0.0, 1.0) };

    if component_count == PSD_RGBA_PLANES {
        for texel in data.as_chunks_mut::<PSD_RGBA_PLANES>().0 {
            for (component, value) in texel.iter_mut().enumerate() {
                *value = encode(component, *value);
            }
        }

        return data;
    }

    data.chunks_exact(component_count.max(1))
        .flat_map(|texel| {
            let encoded = |component| encode(component, texel.get(component).copied().unwrap_or(0.0));
            match component_count {
                1 | 2 => {
                    let gray = encoded(0);
                    let alpha = if component_count == 2 { encoded(1) } else { 1.0 };
                    [gray, gray, gray, alpha]
                }
                3 => [encoded(0), encoded(1), encoded(2), 1.0],
                _ => [encoded(0), encoded(1), encoded(2), encoded(3)],
            }
        })
        .collect()
}

/// Composite a straight-alpha RGBA layer at the top left of the canvas.
fn over(canvas: &mut [f32], canvas_width: u32, layer: &[f32], layer_width: u32) {
    let canvas_rows = canvas.chunks_exact_mut(canvas_width as usize * PSD_RGBA_PLANES);
    let layer_rows = layer.chunks_exact(layer_width as usize * PSD_RGBA_PLANES);
    for (canvas_row, layer_row) in canvas_rows.zip(layer_rows) {
        let canvas_texels = canvas_row.as_chunks_mut::<PSD_RGBA_PLANES>().0;
        let layer_texels = layer_row.as_chunks::<PSD_RGBA_PLANES>().0;
        for (under, top) in canvas_texels.iter_mut().zip(layer_texels) {
            let (top_alpha, under_alpha) = (top[ALPHA_INDEX], under[ALPHA_INDEX]);
            let alpha = top_alpha + under_alpha * (1.0 - top_alpha);
            if alpha <= 0.0 {
                continue;
            }

            for (value, top_value) in under.iter_mut().zip(top).take(ALPHA_INDEX) {
                *value = (top_value * top_alpha + *value * under_alpha * (1.0 - top_alpha)) / alpha;
            }
            under[ALPHA_INDEX] = alpha;
        }
    }
}

/// Quantize directly into the sample type the PSD builder accepts.
fn pixel_data(samples: Vec<f32>, width: u32, encoding: Encoding) -> Result<PixelData> {
    if encoding.depth == 16 {
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(samples.len()).map_err(|error| FormatError::Invalid(format!("PSD sample buffer allocation failed: {error}")))?;
        pixels.extend(samples.into_iter().map(quantize_u16));
        return Ok(PixelData::Rgba16(pixels));
    }

    quantize_integer(encoding, width, PSD_RGBA_PLANES, samples.into_iter()).map(PixelData::Rgba8)
}

/// An RGB PSD of `layers` (bottom first, each named by its `/`-separated group path) over the merged `composite`, uncompressed so its size is known up front
pub(super) fn write_layers(
    (width, height): (u32, u32),
    depth: u8,
    layers: Vec<(&str, (u32, u32), PixelData)>,
    composite: PixelData,
) -> craft_psd::Result<Vec<u8>> {
    let mut builder = PsdBuilder::new(width, height).depth(u16::from(depth)).compression(Compression::Raw);

    // The builder stacks bottom up, so the groups open and close in that order too
    let mut open: Vec<&str> = Vec::new();
    for (path, (layer_width, layer_height), pixels) in layers {
        let mut parts: Vec<&str> = path.split('/').collect();
        let name = parts.pop().unwrap_or_default();
        let shared = open.iter().zip(&parts).take_while(|(open, part)| open == part).count();
        for _ in shared..open.len() {
            builder.end_group()?;
        }
        open.truncate(shared);
        for group in parts.iter().skip(shared) {
            builder.begin_group(GroupSpec::new(*group));
            open.push(group);
        }

        builder.push_layer(LayerSpec::new(name, 0, 0, layer_width, layer_height, pixels));
    }
    for _ in open {
        builder.end_group()?;
    }

    builder.composite(composite);
    builder.to_bytes()
}
