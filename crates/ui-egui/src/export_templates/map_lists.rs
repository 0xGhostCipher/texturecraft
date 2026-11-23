//! The maps the Output templates tab lists for dragging onto output channels, with the reference's
//! names (naming pattern OpenPBR). An entry without a source names a map TextureCraft does not
//! have yet; it is listed greyed.

use std::sync::LazyLock;

use texturecraft_engine::core::{BakedMap, Channel};
use texturecraft_engine::formats::{SourceRef, VirtualMap};

use crate::i18n::n_;

/// One entry of a map list: its name and what it reads (None: not available yet).
pub type MapEntry = (&'static str, Option<SourceRef>);

/// A group of the Input maps pane: its name and entries.
pub type MapGroup = (&'static str, Vec<MapEntry>);

fn channel(channel: Channel) -> Option<SourceRef> {
    Some(SourceRef::Channel { channel, component: 0 })
}

fn converted(map: VirtualMap) -> Option<SourceRef> {
    Some(SourceRef::Virtual { map, component: 0 })
}

fn mesh(map: BakedMap) -> Option<SourceRef> {
    Some(SourceRef::Baked { map, component: 0 })
}

/// Input maps in groups, as the reference lists them up to Fuzz/Sheen; the groups after it are
/// cut off in the reference screenshots and use the OpenPBR names of the channels left over.
pub static INPUT_MAPS: LazyLock<Vec<MapGroup>> = LazyLock::new(|| {
    vec![
        (
            n_("Base"),
            vec![
                (n_("Base weight"), None),
                (n_("Base color"), channel(Channel::BaseColor)),
                (n_("Base metalness"), channel(Channel::Metallic)),
                (n_("Base diffuse roughness"), None),
            ],
        ),
        (
            n_("Specular"),
            vec![
                (n_("Specular weight"), None),
                (n_("Specular color"), None),
                (n_("Specular roughness"), channel(Channel::Roughness)),
                (n_("Specular level"), channel(Channel::SpecularLevel)),
                (n_("Specular anisotropy"), channel(Channel::AnisotropyLevel)),
                (n_("Anisotropy angle"), channel(Channel::AnisotropyAngle)),
            ],
        ),
        (n_("Transmission"), vec![(n_("Transmission weight"), channel(Channel::Transmission)), (n_("Transmission color"), None)]),
        (
            n_("Subsurface"),
            vec![(n_("Subsurface weight"), channel(Channel::ScatterWeight)), (n_("Subsurface color"), None), (n_("Subsurface radius scale"), None)],
        ),
        (
            n_("Coat"),
            vec![
                (n_("Coat weight"), channel(Channel::CoatWeight)),
                (n_("Coat color"), None),
                (n_("Coat roughness"), channel(Channel::CoatRoughness)),
                (n_("Coat anisotropy"), None),
                (n_("Coat specular level"), None),
                (n_("Coat normal"), channel(Channel::CoatNormal)),
                (n_("Coat tangent"), None),
            ],
        ),
        (
            n_("Fuzz/Sheen"),
            vec![
                (n_("Fuzz weight"), channel(Channel::SheenWeight)),
                (n_("Fuzz color"), channel(Channel::SheenColor)),
                (n_("Fuzz roughness"), channel(Channel::SheenRoughness)),
            ],
        ),
        (n_("Emission"), vec![(n_("Emission color"), channel(Channel::Emissive))]),
        (n_("Geometry"), vec![(n_("Geometry opacity"), channel(Channel::Opacity)), (n_("Geometry normal"), channel(Channel::Normal))]),
        (
            n_("Other"),
            vec![
                (n_("Height"), channel(Channel::Height)),
                (n_("Ambient occlusion"), channel(Channel::AmbientOcclusion)),
                (n_("Specular"), channel(Channel::Specular)),
                (n_("Glossiness"), channel(Channel::Glossiness)),
            ],
        ),
    ]
});

/// Mesh maps, in the reference order and case.
pub static MESH_MAPS: LazyLock<Vec<MapEntry>> = LazyLock::new(|| {
    vec![
        (n_("Normal"), mesh(BakedMap::Normal)),
        (n_("World space normal"), mesh(BakedMap::WorldNormal)),
        (n_("ID"), mesh(BakedMap::Id)),
        (n_("Ambient occlusion"), mesh(BakedMap::AmbientOcclusion)),
        (n_("Curvature"), mesh(BakedMap::Curvature)),
        (n_("Position"), mesh(BakedMap::Position)),
        (n_("Thickness"), mesh(BakedMap::Thickness)),
        (n_("Height"), mesh(BakedMap::Height)),
        (n_("Bent normals"), mesh(BakedMap::BentNormal)),
        (n_("Opacity"), mesh(BakedMap::Opacity)),
    ]
});

/// Converted maps in the reference order, then the ones only TextureCraft has (templates use them).
pub static CONVERTED_MAPS: LazyLock<Vec<MapEntry>> = LazyLock::new(|| {
    vec![
        ("1/ior", None),
        (n_("Mixed AO"), converted(VirtualMap::AoMixed)),
        (n_("Diffuse"), None),
        ("f0", converted(VirtualMap::F0)),
        (n_("Glossiness"), converted(VirtualMap::Glossiness)),
        (n_("Glossiness²"), None),
        (n_("Normal DirectX"), converted(VirtualMap::NormalDirectX)),
        (n_("Normal OpenGL"), converted(VirtualMap::NormalOpenGl)),
        (n_("Reflection"), None),
        (n_("Specular"), None),
        ("Unity4 Diffuse", None),
        ("Unity4 Gloss", None),
        (n_("2D View"), None),
        (VirtualMap::HeightNormal.label(), converted(VirtualMap::HeightNormal)),
        (VirtualMap::Roughness.label(), converted(VirtualMap::Roughness)),
        (VirtualMap::SheenTint.label(), converted(VirtualMap::SheenTint)),
    ]
});

/// The listed name of `source` (any component), or None for a constant.
pub fn name_of(source: &SourceRef) -> Option<&'static str> {
    let lists = INPUT_MAPS.iter().flat_map(|(_, entries)| entries.iter()).chain(MESH_MAPS.iter()).chain(CONVERTED_MAPS.iter());
    let mut entries = lists.filter_map(|(name, entry)| entry.as_ref().map(|entry| (*name, entry)));
    entries.find(|(_, entry)| super::same_source(entry, source)).map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_channel_and_map_can_be_dragged() {
        let entries = INPUT_MAPS.iter().flat_map(|(_, entries)| entries.iter()).chain(MESH_MAPS.iter()).chain(CONVERTED_MAPS.iter());
        let listed: Vec<SourceRef> = entries.filter_map(|(_, source)| source.clone()).collect();

        for channel in Channel::ALL {
            assert!(listed.contains(&SourceRef::Channel { channel, component: 0 }), "{channel:?}");
        }
        for map in BakedMap::ALL {
            assert!(listed.contains(&SourceRef::Baked { map, component: 0 }), "{map:?}");
        }
        for map in VirtualMap::ALL {
            assert!(listed.contains(&SourceRef::Virtual { map, component: 0 }), "{map:?}");
        }
    }
}
