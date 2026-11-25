use texturecraft_core::BlockScales;

use crate::sampled_map::SampledMap;

/// What a field was built from: the working layout (the g-buffer's address, size, coverage, UV tile block and tile scales), the mesh's positions and UVs, the mesh maps it reads and its settings' bits
#[derive(Clone, PartialEq)]
pub(crate) struct FieldCacheKey {
    pub(crate) layout: usize,
    pub(crate) size: (u32, u32),
    /// Every texel's triangle, hashed: [`crate::field_cache::coverage_identity`]
    pub(crate) coverage: u64,
    pub(crate) block: (u32, u32),
    pub(crate) scales: BlockScales,
    /// The mesh's positions and UVs, each by address and length: a UV edit changes island fields
    pub(crate) positions: (usize, usize),
    pub(crate) uvs: (usize, usize),
    pub(crate) maps: Vec<SampledMap>,
    pub(crate) settings: Vec<u32>,
}
