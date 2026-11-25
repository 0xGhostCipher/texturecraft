//! A small picture of one output map, sampled from the texture set as the export samples it

use rayon::prelude::*;
use texturecraft_texel::linear_to_srgb;

use super::{At, Dest, ExportInput, ExportOptions, Sampler, layout, list_outputs};
use crate::{FormatError, Result};

/// The largest preview side: a preview is a glance, never a second export
pub const MAX_PREVIEW_SIDE: u32 = 512;

/// One output map, small: its channels in file order and the 8-bit values an sRGB file of it stores (colour channels sRGB-encoded, data as is)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPreview {
    pub width: u32,
    pub height: u32,
    pub channels: Vec<Dest>,
    /// `channels.len()` values per pixel, row by row
    pub pixels: Vec<u8>,
}

impl MapPreview {
    /// Output map `map` of `options`' template for `input`'s set, at most `side` wide; None when it writes no file; texels off the UV islands show the default
    pub fn sample(input: &ExportInput, options: &ExportOptions, map: usize, side: u32) -> Result<Option<MapPreview>> {
        let set = input.project.texture_set(input.ts)?;
        let spec = options.preset.maps.get(map).ok_or_else(|| FormatError::Invalid(format!("the template has no output map {map}")))?;
        let planned = list_outputs(input.project, input.ts, input.mesh_name, options)?;
        if !planned.iter().any(|file| file.maps().any(|written| written == map)) {
            return Ok(None);
        }

        let (composite_width, height) = (input.composite.resolution, input.composite.height);
        if composite_width == 0 || height == 0 || input.coverage.len() != composite_width as usize * height as usize {
            return Err(FormatError::Invalid(format!("composite size {composite_width}×{height} does not match its coverage")));
        }

        let width = side.clamp(1, MAX_PREVIEW_SIDE).min(composite_width);
        let rows = ((u64::from(width) * u64::from(height) / u64::from(composite_width)) as u32).max(1);
        let tile_res = (composite_width / set.tile_grid().map_or(1, |grid| grid.cols.max(1))).max(1);
        let sampler = Sampler::new(input, options, set, tile_res);
        let channel_layout = layout(set, spec);
        let count = channel_layout.len();
        let color = spec.is_color();
        let max_sample = f32::from(u8::MAX);

        // The texel under the centre of preview pixel `i` of `out` pixels across `of` texels
        let texel = |i: u32, out: u32, of: u32| ((u64::from(i) * 2 + 1) * u64::from(of) / (2 * u64::from(out))).min(u64::from(of) - 1) as u32;

        let mut pixels = vec![0u8; width as usize * rows as usize * count];
        pixels.par_chunks_mut(width as usize * count).enumerate().for_each(|(row, values)| {
            let y = texel(row as u32, rows, height);
            for column in 0..width {
                let x = texel(column, width, composite_width);
                let at = if sampler.covered(x, y) { At::Texel(x, y) } else { At::Default };
                let mut cache = None;
                for (component, (dest, source)) in channel_layout.iter().enumerate() {
                    let sample = sampler.sample(source, at, &mut cache);
                    let value = if spec.inverted.contains(dest) { 1.0 - sample } else { sample };
                    let stored = if color && *dest != Dest::A { linear_to_srgb(value) } else { value.clamp(0.0, 1.0) };
                    if let Some(slot) = values.get_mut(column as usize * count + component) {
                        *slot = (stored * max_sample + 0.5) as u8;
                    }
                }
            }
        });

        Ok(Some(MapPreview { width, height: rows, channels: channel_layout.iter().map(|(dest, _)| *dest).collect(), pixels }))
    }
}
