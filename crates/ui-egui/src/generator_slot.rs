//! The generator slot atop a generator's properties: its orb and name, opening a picker that swaps its kind

use egui::{Sense, Ui, vec2};
use egui_phosphor::regular as ic;
use serde_json::json;
use texturecraft_engine::core::{Generator, GeneratorKind};

use crate::TextureCraftApp;
use crate::i18n::tr;
use crate::picker_group::PickerGroup;
use crate::picker_tab::PickerTab;
use crate::resource_picker;
use crate::resource_picker_item::ResourcePickerItem;
use crate::resource_slot::{ResourceSlot, SLOT_HEIGHT};
use crate::tile_picture::TilePicture;

/// The slot of generator effect `effect`: a pick runs `effect.set {generator}`
pub(crate) fn show(app: &mut TextureCraftApp, ui: &mut Ui, effect: u64, generator: &Generator) {
    let ctx = ui.ctx().clone();
    let current = generator.kind;
    let thumbnail = app.thumbs.generator(&ctx, current);
    let name = tr(current.label());

    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), SLOT_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name));
    let popup = ui.make_persistent_id(("generator_picker", effect));
    if response.clicked() {
        resource_picker::toggle(&ctx, popup);
    }

    let highlighted = response.hovered() || egui::Popup::is_id_open(&ctx, popup);
    let slot = ResourceSlot { highlighted, focused: response.has_focus(), thumbnail: thumbnail.as_ref(), caption: tr("Generator"), name };
    slot.paint(ui, rect);

    let tabs = || {
        let mut items: Vec<ResourcePickerItem<GeneratorKind>> = GeneratorKind::ALL
            .into_iter()
            .filter(|kind| kind.in_picker())
            .map(|kind| ResourcePickerItem { id: kind, label: tr(kind.label()).to_owned(), caption: None, picture: TilePicture::Generator(kind) })
            .collect();

        // One kind of resource: one grid in name order, no group captions
        items.sort_by_cached_key(|item| item.label.to_lowercase());
        vec![PickerTab { icon: ic::SPARKLE, name: tr("Generators"), groups: vec![PickerGroup { header: None, items }] }]
    };
    let picked = resource_picker::show(&response, popup, &mut app.thumbs, Some(&current), true, tabs);
    if let Some(kind) = picked.filter(|kind| *kind != current) {
        app.run("effect.set", json!({"effect": effect, "generator": kind.id()}));
    }
}
