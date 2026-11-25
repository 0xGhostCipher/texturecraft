use std::sync::Weak;

use texturecraft_core::BlockScales;

/// What a field was built from: the working layout (the g-buffer's address, size, coverage, UV tile block and tile scales), the mesh's positions and UVs, the mesh maps it reads and its settings' bits
#[derive(Clone)]
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
    /// The mesh maps it samples, each with the size it was baked at: one allocation read as another layout reads other texels
    pub(crate) maps: Vec<(Weak<Vec<f32>>, (u32, u32))>,
    pub(crate) settings: Vec<u32>,
}

impl PartialEq for FieldCacheKey {
    fn eq(&self, other: &FieldCacheKey) -> bool {
        let same_maps = self.maps.len() == other.maps.len()
            && self.maps.iter().zip(&other.maps).all(|((a, a_size), (b, b_size))| a.strong_count() > 0 && a.ptr_eq(b) && a_size == b_size);
        same_maps
            && self.layout == other.layout
            && self.size == other.size
            && self.coverage == other.coverage
            && self.block == other.block
            && self.scales == other.scales
            && self.positions == other.positions
            && self.uvs == other.uvs
            && self.settings == other.settings
    }
}
