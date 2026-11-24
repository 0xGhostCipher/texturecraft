//! `uv_layout.bin`: the UVs of a mesh whose layout was rebuilt on import, so reopening does not
//! depend on the unwrapper of the running version. A header (magic `TCUV`, version, corner count;
//! u32 LE each) is followed by `u`, `v` (f32 LE) for every triangle corner in index order.

use std::io::Write;

use texturecraft_geom::MAX_TRIANGLES;
use texturecraft_geom::glam::Vec2;

use super::KIND;
use crate::{Result, malformed};

/// Zip entry name of the saved layout.
pub const UV_LAYOUT_ENTRY: &str = "uv_layout.bin";

const UV_LAYOUT_MAGIC: &[u8; 4] = b"TCUV";
const UV_LAYOUT_VERSION: u32 = 1;
const HEADER_BYTES: usize = 12;
const CORNER_BYTES: usize = 8;

/// Largest entry a valid mesh can need: every corner of the most triangles accepted.
pub const MAX_UV_LAYOUT_BYTES: u64 = (HEADER_BYTES + MAX_TRIANGLES * 3 * CORNER_BYTES) as u64;

/// Encoded size of a layout of `corners` UVs.
pub fn uv_layout_bytes(corners: usize) -> u64 {
    (HEADER_BYTES + corners * CORNER_BYTES) as u64
}

pub fn write_uv_layout(w: &mut dyn Write, layout: &[Vec2]) -> std::io::Result<()> {
    let corners = u32::try_from(layout.len()).map_err(|_| std::io::Error::other("the UV layout has too many corners"))?;
    w.write_all(UV_LAYOUT_MAGIC)?;
    w.write_all(&UV_LAYOUT_VERSION.to_le_bytes())?;
    w.write_all(&corners.to_le_bytes())?;

    let mut body = Vec::with_capacity(layout.len() * CORNER_BYTES);
    for uv in layout {
        body.extend_from_slice(&uv.x.to_le_bytes());
        body.extend_from_slice(&uv.y.to_le_bytes());
    }
    w.write_all(&body)
}

/// Decode an entry; a wrong header, size or a non-finite UV is an error.
pub fn read_uv_layout(bytes: &[u8]) -> Result<Vec<Vec2>> {
    let bad = |why: &str| malformed(KIND, format!("{UV_LAYOUT_ENTRY}: {why}"));
    let Some((header, body)) = bytes.split_first_chunk::<HEADER_BYTES>() else { return Err(bad("truncated header")) };
    let [magic, version, corners] = header.as_chunks::<4>().0 else { return Err(bad("truncated header")) };

    if magic != UV_LAYOUT_MAGIC {
        return Err(bad("bad magic"));
    }
    if u32::from_le_bytes(*version) != UV_LAYOUT_VERSION {
        return Err(bad("unsupported version"));
    }
    let corners = u32::from_le_bytes(*corners) as usize;
    if corners.checked_mul(CORNER_BYTES) != Some(body.len()) {
        return Err(bad("the corner count does not match the entry size"));
    }

    let layout: Vec<Vec2> = body
        .as_chunks::<CORNER_BYTES>()
        .0
        .iter()
        .map(|corner| {
            let (u, v) = corner.split_at(4);
            Vec2::new(f32::from_le_bytes(u.try_into().unwrap_or_default()), f32::from_le_bytes(v.try_into().unwrap_or_default()))
        })
        .collect();
    if layout.iter().any(|uv| !uv.is_finite()) {
        return Err(bad("non-finite UVs"));
    }
    Ok(layout)
}
