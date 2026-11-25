use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::util::id_type_map::RawKey;
use serde_json::Value;

use super::map_preview_pictures::MapPreviewPictures;
use crate::TextureCraftApp;

/// Pixels across an output map's hover preview: enough to read the packing, cheap to sample
const PREVIEW_SIDE_PIXELS: u32 = 150;

/// The output maps' hover pictures for one set of export params and one document revision, by output map index (None: no file for that map)
#[derive(Clone, Default)]
pub(super) struct MapPreviewCache {
    key: u64,
    pictures: HashMap<usize, Option<Arc<MapPreviewPictures>>>,
}

impl MapPreviewCache {
    fn id() -> egui::Id {
        egui::Id::unique("export_map_preview_cache")
    }

    fn key(app: &TextureCraftApp, params: &Value) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        params.hash(&mut hasher);
        (app.session.project_id, app.session.revision).hash(&mut hasher);
        hasher.finish()
    }

    /// Output map `index`'s pictures once made; None until then
    pub(super) fn get(ctx: &egui::Context, index: usize) -> Option<Option<Arc<MapPreviewPictures>>> {
        ctx.data(|data| {
            let cache = data.get_temp_raw(RawKey::new::<Self>(Self::id()))?.downcast_ref::<Self>()?;
            cache.pictures.get(&index).cloned()
        })
    }

    /// Release all cached pictures when their tab is dismissed.
    pub(super) fn clear(ctx: &egui::Context) {
        ctx.data_mut(|data| data.remove::<Self>(Self::id()));
    }

    /// Forget pictures made for other params or an older document, so `get` never shows them
    pub(super) fn keep_current(app: &TextureCraftApp, ctx: &egui::Context, params: &Value) {
        let key = Self::key(app, params);
        ctx.data_mut(|data| {
            let cache = data.get_temp_mut_or_default::<MapPreviewCache>(Self::id());
            if cache.key != key {
                *cache = MapPreviewCache { key, pictures: HashMap::new() };
            }
        });
    }

    /// Make output map `index`'s pictures for `params` unless they are made, and draw again to show them
    pub(super) fn fill(app: &mut TextureCraftApp, ctx: &egui::Context, params: &Value, index: usize) {
        if Self::get(ctx, index).is_some() {
            return;
        }

        let preview = app.session.export_preview(params, index, PREVIEW_SIDE_PIXELS).ok().flatten();
        let pictures = preview.map(|preview| Arc::new(MapPreviewPictures::new(ctx, &preview, index)));
        ctx.data_mut(|data| data.get_temp_mut_or_default::<MapPreviewCache>(Self::id()).pictures.insert(index, pictures));
        ctx.request_repaint();
    }
}
