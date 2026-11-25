//! A texture set's PSD: each output map a layer, a `/` in its name a layer group

use craft_psd::{Compression, GroupSpec, LayerSpec, PixelData, PsdBuilder};
use texturecraft_texel::linear_to_srgb;

use super::encoding::Encoding;
use super::map_renderer::MapRenderer;
use super::{ExportedFile, FileFormat, PSD_MAX_FILE_BYTES, PSD_RGBA_PLANES, PlannedFile, quantize_integer};
use crate::{FormatError, Result};

/// Where alpha sits in an RGBA texel
const ALPHA_INDEX: usize = 3;

/// Render the layers of `planned` one at a time and write them as one PSD
pub(super) fn render_psd(renderer: &MapRenderer, planned: PlannedFile) -> Result<ExportedFile> {
    let PlannedFile { name, tile, bit_depth, mut warnings, layers, .. } = planned;
    let size = renderer.size(tile)?;

    // Each layer as straight RGBA in 0-1, sRGB-encoded where the map's file would be, dropped once quantized
    let mut composite = vec![0.0f32; size.0 as usize * size.1 as usize * PSD_RGBA_PLANES];
    let mut pixels = Vec::with_capacity(layers.len());
    let mut embedded = None;
    for layer in layers.iter().rev() {
        let map = renderer.render(layer.map, tile, FileFormat::Psd, bit_depth)?;
        embedded = map.embed.or(embedded);
        let rgba = to_rgba(&map.data, map.component_count, &map.srgb);
        drop(map);
        over(&mut composite, &rgba);
        let encoding = Encoding { format: FileFormat::Psd, depth: bit_depth, dither: layer.dither };
        let bytes = quantize_integer(encoding, size.0, PSD_RGBA_PLANES, rgba.into_iter())?;
        pixels.push((layer.name.as_str(), pixel_data(bytes, bit_depth)));
    }
    if let Some(space) = embedded {
        warnings.push(format!("PSD export does not embed a colour space; the colours are in {}", space.name()));
    }

    let encoding = Encoding { format: FileFormat::Psd, depth: bit_depth, dither: false };
    let composite = pixel_data(quantize_integer(encoding, size.0, PSD_RGBA_PLANES, composite.into_iter())?, bit_depth);

    let bytes = write_layers(size, bit_depth, pixels, composite).map_err(|error| FormatError::Invalid(format!("psd encoding failed: {error}")))?;
    if bytes.len() as u64 >= PSD_MAX_FILE_BYTES {
        return Err(FormatError::Invalid(format!("\"{name}\" would reach the 2 GiB PSD file limit; use a smaller export size or fewer PSD maps")));
    }

    Ok(ExportedFile { name, bytes, width: size.0, height: size.1, warnings })
}

/// L, LA, RGB or RGBA samples as RGBA (gray repeated, opaque without alpha), sRGB transfer applied to the components `srgb` marks, values clamped to 0-1
fn to_rgba(data: &[f32], component_count: usize, srgb: &[bool]) -> Vec<f32> {
    let encode = |component: usize, value: f32| if srgb.get(component).copied().unwrap_or(false) { linear_to_srgb(value) } else { value.clamp(0.0, 1.0) };
    data.chunks_exact(component_count.max(1))
        .flat_map(|texel| {
            let encoded = |component| encode(component, texel.get(component).copied().unwrap_or(0.0));
            match component_count {
                1 => [encoded(0), encoded(0), encoded(0), 1.0],
                2 => [encoded(0), encoded(0), encoded(0), encoded(1)],
                3 => [encoded(0), encoded(1), encoded(2), 1.0],
                _ => [encoded(0), encoded(1), encoded(2), encoded(3)],
            }
        })
        .collect()
}

/// Lay straight-alpha RGBA `layer` over `below`, in place
fn over(below: &mut [f32], layer: &[f32]) {
    for (under, top) in below.as_chunks_mut::<PSD_RGBA_PLANES>().0.iter_mut().zip(layer.as_chunks::<PSD_RGBA_PLANES>().0) {
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

/// Native-endian quantized RGBA samples as the PSD builder takes them
fn pixel_data(bytes: Vec<u8>, depth: u8) -> PixelData {
    if depth == 16 { PixelData::Rgba16(bytes.as_chunks::<2>().0.iter().map(|sample| u16::from_ne_bytes(*sample)).collect()) } else { PixelData::Rgba8(bytes) }
}

/// An RGB PSD of `layers` (bottom first, each named by its `/`-separated group path) over the merged `composite`, uncompressed so its size is known up front
pub(super) fn write_layers((width, height): (u32, u32), depth: u8, layers: Vec<(&str, PixelData)>, composite: PixelData) -> craft_psd::Result<Vec<u8>> {
    let mut builder = PsdBuilder::new(width, height).depth(u16::from(depth)).compression(Compression::Raw);

    // The builder stacks bottom up, so the groups open and close in that order too
    let mut open: Vec<&str> = Vec::new();
    for (path, pixels) in layers {
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

        builder.push_layer(LayerSpec::new(name, 0, 0, width, height, pixels));
    }
    for _ in open {
        builder.end_group()?;
    }

    builder.composite(composite);
    builder.to_bytes()
}
