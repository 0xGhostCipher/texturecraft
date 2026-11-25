use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use texturecraft_core::BakedMap;
use texturecraft_geom::UvGBuffer;

use crate::eval::Inputs;
use crate::field_cache_key::FieldCacheKey;
use crate::sampled_map::SampledMap;
use crate::working_field::working_block;

/// Most fields cached, and the bytes they may hold (the newest is kept whatever its size)
pub(crate) const CACHE_ENTRIES: usize = 8;
const CACHE_BYTES: usize = 256 << 20;

/// Texels hashed together before their hashes are combined, so the coverage hashes in parallel
const COVERAGE_CHUNK_TEXELS: usize = 1 << 16;

/// The 64-bit FNV prime, spreading each step of the coverage hash over every bit
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The identity of `gbuf`'s coverage: its size and the triangle under every texel, hashed (read once per evaluation, as it reads every texel)
pub(crate) fn coverage_identity(gbuf: &UvGBuffer) -> u64 {
    let chunk_hash =
        |chunk: &[u32]| chunk.iter().fold(chunk.len() as u64, |hash, triangle| (hash.rotate_left(5) ^ u64::from(*triangle)).wrapping_mul(FNV_PRIME));
    let chunks: Vec<u64> = gbuf.tri.par_chunks(COVERAGE_CHUNK_TEXELS).map(chunk_hash).collect();
    chunks.iter().fold((u64::from(gbuf.width) << 32) | u64::from(gbuf.height), |hash, chunk| (hash.rotate_left(7) ^ chunk).wrapping_mul(FNV_PRIME))
}

/// Built fields by what they were built from, most recently used last
pub(crate) struct FieldCache {
    entries: Mutex<Vec<(FieldCacheKey, Arc<Vec<f32>>)>>,
}

impl FieldCache {
    pub(crate) const fn new() -> FieldCache {
        FieldCache { entries: Mutex::new(Vec::new()) }
    }

    /// The field of `inp`'s working layout (its coverage: [`coverage_identity`]) reading the baked `maps` with `settings`, built by `build` (given the UV tile block) when missing, outside the lock: the build runs parallel work
    pub(crate) fn get_or_build(
        &self,
        inp: &Inputs,
        coverage: u64,
        maps: &[BakedMap],
        settings: Vec<u32>,
        build: impl FnOnce((u32, u32)) -> Vec<f32>,
    ) -> Arc<Vec<f32>> {
        let gbuf = inp.gbuf;
        let block = working_block(inp);
        let read = maps.iter().filter_map(|map| {
            inp.baked.maps.get(map).map(|data| SampledMap { data: Arc::downgrade(data), baked_size: (inp.baked.resolution, inp.baked.height(*map)) })
        });
        let key = FieldCacheKey {
            layout: gbuf.tri.as_ptr() as usize,
            size: (gbuf.width, gbuf.height),
            coverage,
            block,
            scales: inp.block_scales.clone(),
            positions: (inp.mesh.positions.as_ptr() as usize, inp.mesh.positions.len()),
            uvs: (inp.mesh.uvs.as_ptr() as usize, inp.mesh.uvs.len()),
            maps: read.collect(),
            settings,
        };

        // A hit moves to the most recently used end
        let cached = {
            let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.retain(|(key, _)| key.maps.iter().all(|map| map.data.strong_count() > 0));
            let hit = entries.iter().position(|(cached, _)| *cached == key).map(|index| entries.remove(index));
            hit.map(|(cached, field)| {
                entries.push((cached, field.clone()));
                field
            })
        };
        if let Some(field) = cached {
            return field;
        }

        let field = Arc::new(build(block));
        let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|(cached, _)| *cached != key);
        entries.push((key, field.clone()));
        let mut bytes = 0usize;
        let keep = entries
            .iter()
            .rev()
            .take_while(|(_, built)| {
                bytes = bytes.saturating_add(built.len().saturating_mul(size_of::<f32>()));
                bytes <= CACHE_BYTES
            })
            .count()
            .clamp(1, CACHE_ENTRIES);
        let excess = entries.len().saturating_sub(keep);
        entries.drain(..excess);
        field
    }
}
