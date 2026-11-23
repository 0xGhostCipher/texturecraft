//! `file.exportTextures` with `materialX: true`: a `.mtlx` OpenPBR material next to the textures.

use serde_json::{Value, json};
use texturecraft_engine::geom::primitives;
use texturecraft_engine::{Session, execute};

const OPENPBR: &str = "OpenPBR (separate maps)";
const DEFAULT: &str = "Document channels + Normal + AO (No Alpha)";

/// A scratch folder that is removed when it goes out of scope, so a failing test leaves nothing behind.
struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> Scratch {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tc_mtlx_{tag}_{}_{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Scratch(dir)
}

fn export(s: &mut Session, dir: &std::path::Path, preset: &str, mtlx: bool) -> Value {
    execute(s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "preset": preset, "materialX": mtlx})).unwrap()
}

fn files(r: &Value) -> Vec<String> {
    r["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect()
}

#[test]
fn writes_a_material_that_names_the_exported_textures() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let dir = temp("ball");
    let r = export(&mut s, &dir, OPENPBR, true);
    let all = files(&r);
    let mtlx: Vec<&String> = all.iter().filter(|f| f.ends_with(".mtlx")).collect();
    assert_eq!(mtlx.len(), s.set_count(), "one material per texture set: {all:?}");
    for path in mtlx {
        let set = path.rsplit(['/', '\\']).next().unwrap().trim_end_matches(".mtlx").to_string();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("<open_pbr_surface") && text.contains("<surfacematerial"), "{text}");
        // Every texture the document points at was written beside it, and belongs to its set.
        let mut refs = 0;
        for part in text.split("name=\"file\" type=\"filename\" value=\"").skip(1) {
            let file = part.split('"').next().unwrap();
            assert!(dir.join(file).is_file(), "{file} is not in {all:?}");
            assert!(file.starts_with(&set), "{file} is not a texture of {set}");
            refs += 1;
        }
        assert!(refs >= 3, "base colour, roughness and normal at least: {text}");
        assert!(text.contains("name=\"base_color\"") && text.contains("name=\"geometry_normal\""), "{text}");
    }
}

#[test]
fn off_by_default() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let dir = temp("off");
    assert!(!files(&export(&mut s, &dir, OPENPBR, false)).iter().any(|f| f.ends_with(".mtlx")));
    let dir = temp("off-other");
    assert!(!files(&export(&mut s, &dir, DEFAULT, false)).iter().any(|f| f.ends_with(".mtlx") || f.ends_with("_base_color.png")));
}

#[test]
fn warns_when_a_template_with_openpbr_names_has_no_channels_to_write() {
    use texturecraft_engine::formats::{Dest, ExportPreset, FileFormat, MapSpec, SourceRef};
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();

    // The default set has no coat
    let preset = ExportPreset {
        name: "coat".into(),
        maps: vec![MapSpec {
            file: "$mesh_$textureSet_coat_weight".into(),
            format: FileFormat::Png,
            bit_depth: 8,
            channels: vec![(Dest::L, SourceRef::Channel { channel: texturecraft_engine::core::Channel::CoatWeight, component: 0 })],
            inverted: Vec::new(),
        }],
    };
    let dir = temp("nocoat");
    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "preset": preset, "materialX": true})).unwrap();
    assert!(!files(&r).iter().any(|f| f.ends_with(".mtlx")));
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("OpenPBR")), "{r}");
}

/// The `file` values a `.mtlx` refers to
fn referenced(text: &str) -> Vec<String> {
    text.split("name=\"file\" type=\"filename\" value=\"").skip(1).map(|part| part.split('"').next().unwrap().to_string()).collect()
}

fn names(r: &Value) -> Vec<String> {
    files(r).iter().map(|f| f.rsplit(['/', '\\']).next().unwrap().to_string()).collect()
}

#[test]
fn any_template_gets_the_openpbr_maps_beside_its_own_files() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let plain = temp("plain");
    let own = names(&export(&mut s, &plain, DEFAULT, false));
    let dir = temp("default-template");
    let r = export(&mut s, &dir, DEFAULT, true);
    let all = names(&r);
    for file in &own {
        assert!(all.contains(file), "the template's own file {file} is still written: {all:?}");
    }

    let mtlx: Vec<String> = files(&r).into_iter().filter(|f| f.ends_with(".mtlx")).collect();
    assert_eq!(mtlx.len(), s.set_count(), "{all:?}");
    for path in &mtlx {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("<open_pbr_surface") && text.contains("name=\"base_color\"") && text.contains("name=\"geometry_normal\""), "{text}");
        let refs = referenced(&text);
        assert!(refs.len() >= 3, "{text}");
        for file in refs {
            assert!(dir.join(&file).is_file(), "{file} is not in {all:?}");
            assert!(!own.contains(&file), "{file} is a file of the chosen template");
        }
    }

    // Every file is written once
    let mut sorted = all.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), all.len(), "{all:?}");
}

#[test]
fn the_openpbr_template_writes_no_second_set_of_maps() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let without = temp("openpbr-without");
    let plain = names(&export(&mut s, &without, OPENPBR, false));
    let dir = temp("openpbr-with");
    let mut all = names(&export(&mut s, &dir, OPENPBR, true));
    all.retain(|f| !f.ends_with(".mtlx"));
    let mut plain = plain;
    plain.sort();
    all.sort();
    assert_eq!(all, plain);
}

#[test]
fn per_map_overrides_of_the_template_do_not_reach_the_openpbr_maps() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let dir = temp("override");

    // Map 0 is the base colour of the default template and also of the OpenPBR template; map 19 is the default template's normal map
    let r = execute(
        &mut s,
        "file.exportTextures",
        &json!({"dir": dir.display().to_string(), "preset": DEFAULT, "materialX": true, "maps": [{"map": 0, "enabled": false}, {"map": 19, "format": "tiff", "bitDepth": 16}]}),
    )
    .unwrap();
    let all = names(&r);
    assert!(!all.iter().any(|f| f.ends_with("_baseColor.png")), "the template's base colour was left out: {all:?}");
    assert!(all.iter().any(|f| f.ends_with("_base_color.png")), "the material's base colour is still written: {all:?}");
    assert!(all.iter().any(|f| f.ends_with("_geometry_normal.png")), "{all:?}");
    assert!(all.iter().any(|f| f.ends_with("_normal.tif")), "the override applies to the template's own normal map: {all:?}");
    assert!(!all.iter().any(|f| f.ends_with(".tif") && f.contains("_geometry_")), "{all:?}");
}

#[test]
fn a_file_type_a_material_cannot_use_falls_back_to_png() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    for (format, bit_depth, ext) in [("psd", 16, "psd"), ("pgm", 8, "pgm"), ("jng", 8, "jng"), ("tiff", 16, "tif")] {
        let dir = temp(format);
        let r = execute(
            &mut s,
            "file.exportTextures",
            &json!({"dir": dir.display().to_string(), "preset": DEFAULT, "format": format, "bitDepth": bit_depth, "materialX": true}),
        )
        .unwrap();
        let all = names(&r);
        assert!(all.iter().any(|f| f.ends_with(&format!("_baseColor.{ext}"))), "{format}: the template's files keep the file type: {all:?}");
        let mtlx = files(&r).into_iter().find(|f| f.ends_with(".mtlx")).unwrap_or_else(|| panic!("{format}: no .mtlx in {all:?}"));
        let text = std::fs::read_to_string(mtlx).unwrap();
        let wanted = if format == "tiff" { "tif" } else { "png" };
        assert!(text.contains(&format!("_base_color.{wanted}\"")), "{format}: {text}");
        for file in referenced(&text) {
            assert!(dir.join(&file).is_file(), "{format}: {file} is not in {all:?}");
        }
    }
}

#[test]
fn the_openpbr_maps_follow_the_size_and_colour_space_of_the_export() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let dir = temp("size");
    let r = execute(
        &mut s,
        "file.exportTextures",
        &json!({"dir": dir.display().to_string(), "preset": DEFAULT, "size": 256, "colorSpace": "ACEScg", "materialX": true}),
    )
    .unwrap();
    let png = files(&r).into_iter().find(|f| f.ends_with("_base_color.png")).unwrap();
    assert_eq!(image::image_dimensions(png).unwrap(), (256, 256));
    let text = std::fs::read_to_string(files(&r).into_iter().find(|f| f.ends_with(".mtlx")).unwrap()).unwrap();
    assert!(text.contains("_base_color.png\" colorspace=\"acescg\""), "{text}");

    // Only colour inputs carry a colour space, and the data maps are still referenced
    let roughness = referenced(&text).into_iter().find(|f| f.ends_with("_specular_roughness.png")).unwrap_or_else(|| panic!("{text}"));
    assert!(dir.join(&roughness).is_file());
    for part in text.split("name=\"file\" type=\"filename\" value=\"").skip(1) {
        let (file, rest) = part.split_once('"').unwrap();
        if rest.starts_with(" colorspace=\"acescg\"") {
            assert!(["_base_color.", "_fuzz_color.", "_emission_color."].iter().any(|input| file.contains(input)), "{file}");
        }
    }
}

#[test]
fn a_file_of_the_template_is_never_replaced_by_an_openpbr_map() {
    use texturecraft_engine::formats::{Dest, ExportPreset, FileFormat, MapSpec, SourceRef};
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    execute(&mut s, "textureSet.rename", &json!({"ts": 0, "name": "a"})).unwrap();

    // Set 1's file from the template has the name set 0's OpenPBR base colour would get
    execute(&mut s, "textureSet.rename", &json!({"ts": 1, "name": "a_base_color"})).unwrap();
    let source = |component| SourceRef::Channel { channel: texturecraft_engine::core::Channel::BaseColor, component };
    let preset = ExportPreset {
        name: "base colour only".into(),
        maps: vec![MapSpec {
            file: "$mesh_$textureSet".into(),
            format: FileFormat::Png,
            bit_depth: 8,
            channels: vec![(Dest::R, source(0)), (Dest::G, source(1)), (Dest::B, source(2))],
            inverted: Vec::new(),
        }],
    };
    let params = |dir: &std::path::Path, mtlx: bool| json!({"dir": dir.display().to_string(), "preset": preset, "ts": [0, 1], "materialX": mtlx});
    let plain = temp("clash-plain");
    execute(&mut s, "file.exportTextures", &params(&plain, false)).unwrap();
    let dir = temp("clash");
    let r = execute(&mut s, "file.exportTextures", &params(&dir, true)).unwrap();
    assert_eq!(std::fs::read(dir.join("material_ball_a_base_color.png")).unwrap(), std::fs::read(plain.join("material_ball_a_base_color.png")).unwrap());
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("file name of a map of the chosen template")), "{r}");
    let first = std::fs::read_to_string(dir.join("material_ball_a.mtlx")).unwrap();
    assert!(!referenced(&first).contains(&"material_ball_a_base_color.png".to_string()), "{first}");
}

#[test]
fn float_colour_maps_are_tagged_linear() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let dir = temp("exr");
    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "preset": OPENPBR, "format": "exr", "materialX": true})).unwrap();
    let mtlx = files(&r).into_iter().find(|f| f.ends_with(".mtlx")).unwrap();
    let text = std::fs::read_to_string(mtlx).unwrap();
    assert!(text.contains("_base_color.exr\" colorspace=\"lin_rec709\"") && !text.contains("srgb_texture"), "{text}");
}

#[test]
fn colour_maps_are_tagged_with_the_export_colour_space() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    for (space, tag, warns) in [("ACEScg", "acescg", false), ("ACES2065-1", "lin_ap0_scene", true), ("sRGB", "srgb_texture", false)] {
        let dir = temp(tag);
        let r = execute(
            &mut s,
            "file.exportTextures",
            &json!({"dir": dir.display().to_string(), "preset": OPENPBR, "format": "exr", "colorSpace": space, "materialX": true}),
        )
        .unwrap();
        let mtlx = files(&r).into_iter().find(|f| f.ends_with(".mtlx")).unwrap();
        let text = std::fs::read_to_string(mtlx).unwrap();
        assert!(text.contains(&format!("_base_color.exr\" colorspace=\"{tag}\"")), "{space}: {text}");
        let warned = r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("ACES2065-1"));
        assert_eq!(warned, warns, "{space}: {r}");
    }
}

#[test]
fn pgm_colour_maps_are_tagged_with_the_written_luminance_space() {
    let mut session = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    for (space, tag) in [(None, "srgb_texture"), (Some("ACEScg"), "lin_rec709"), (Some("ACES2065-1"), "lin_rec709")] {
        let directory = temp(tag);
        let result = execute(
            &mut session,
            "file.exportTextures",
            &json!({"dir": directory.display().to_string(), "preset": OPENPBR, "format": "pgm", "colorSpace": space, "materialX": true}),
        )
        .unwrap();

        let material = files(&result).into_iter().find(|path| path.ends_with(".mtlx")).unwrap();
        let text = std::fs::read_to_string(material).unwrap();
        assert!(text.contains(&format!("_base_color.pgm\" colorspace=\"{tag}\"")), "{space:?}: {text}");
        assert!(!result["warnings"].to_string().contains("lin_ap0_scene"), "{result}");
    }
}

#[test]
fn explicit_srgb_float_tiff_tags_match_gray_and_rgb_samples() {
    use texturecraft_engine::formats::{Dest, ExportPreset, FileFormat, MapSpec, SourceRef};
    use texturecraft_engine::{core::Channel, texel::linear_to_srgb};

    let mut session = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    execute(&mut session, "layer.add.fill", &json!({})).unwrap();
    execute(&mut session, "layer.fill.set", &json!({"channel": "baseColor", "color": [0.25, 0.5, 0.75]})).unwrap();
    let source = |component| SourceRef::Channel { channel: Channel::BaseColor, component };
    let mut linear_sample = None;
    for (channels, tag) in
        [(vec![(Dest::L, source(0))], "lin_rec709"), (vec![(Dest::R, source(0)), (Dest::G, source(1)), (Dest::B, source(2))], "srgb_texture")]
    {
        let directory = temp(tag);
        let preset = ExportPreset {
            name: "TIFF base colour".into(),
            maps: vec![MapSpec { file: "$mesh_$textureSet_base_color".into(), format: FileFormat::Tiff, bit_depth: 32, channels, inverted: Vec::new() }],
        };
        let result = execute(
            &mut session,
            "file.exportTextures",
            &json!({"dir": directory.display().to_string(), "preset": preset, "ts": 0, "colorSpace": "sRGB", "materialX": true}),
        )
        .unwrap();

        let paths = files(&result);
        let texture = paths.iter().find(|file| file.ends_with(".tif")).unwrap();
        let material = std::fs::read_to_string(paths.iter().find(|file| file.ends_with(".mtlx")).unwrap()).unwrap();
        let warned = result["warnings"].to_string().contains("written in linear Rec.709");
        let samples = image::open(texture).unwrap().to_rgb32f();
        assert!(material.contains(&format!("_base_color.tif\" colorspace=\"{tag}\"")), "{material}");
        assert_eq!(warned, tag == "lin_rec709", "{result}");
        if let Some(expected) = linear_sample {
            let (x, y, red) = expected;
            assert!((samples.get_pixel(x, y)[0] - linear_to_srgb(red)).abs() < 1e-5);
        } else {
            let (x, y, pixel) = samples.enumerate_pixels().find(|(_, _, pixel)| (0.01..0.99).contains(&pixel[0])).unwrap();
            assert_eq!(pixel.0, [pixel[0]; 3]);
            linear_sample = Some((x, y, pixel[0]));
        }
    }
}

#[test]
fn sets_with_the_same_file_name_warn_instead_of_overwriting() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    assert!(s.set_count() >= 2);
    execute(&mut s, "textureSet.rename", &json!({"ts": 0, "name": "a b"})).unwrap();
    execute(&mut s, "textureSet.rename", &json!({"ts": 1, "name": "A_B"})).unwrap();
    let dir = temp("clash");
    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "preset": OPENPBR, "ts": "all", "materialX": true})).unwrap();
    assert_eq!(files(&r).iter().filter(|f| f.ends_with(".mtlx")).count(), s.set_count() - 1, "{r}");
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("\"A_B\"")), "{r}");
}

#[test]
fn a_material_never_points_at_a_texture_kept_for_another_set() {
    let mut session = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();

    // The first set's name expands to the same base-colour filename as the second
    execute(&mut session, "textureSet.rename", &json!({"ts": 0, "name": "$channel"})).unwrap();
    execute(&mut session, "textureSet.rename", &json!({"ts": 1, "name": "baseColor"})).unwrap();
    let directory = temp("shared-map");

    let result = export(&mut session, &directory, OPENPBR, true);

    let materials: Vec<_> = files(&result).into_iter().filter(|path| path.ends_with(".mtlx")).collect();
    assert!(materials.len() >= 2, "{result}");
    assert!(std::fs::read_to_string(&materials[0]).unwrap().contains("material_ball_baseColor_base_color.png"));
    for material in materials.iter().skip(1) {
        assert!(!std::fs::read_to_string(material).unwrap().contains("material_ball_baseColor_base_color.png"), "{material}");
    }
}

#[test]
fn udim_sets_use_the_udim_token() {
    let mut s = Session::new_project("t", primitives::udim_plane(), "builtin:udim_plane", None, 128).unwrap();
    let dir = temp("udim");
    let r = export(&mut s, &dir, OPENPBR, true);
    let all = files(&r);
    let mtlx = all.iter().find(|f| f.ends_with(".mtlx")).unwrap_or_else(|| panic!("no .mtlx in {all:?}"));
    let text = std::fs::read_to_string(mtlx).unwrap();
    assert!(text.contains(".&lt;UDIM&gt;.png"), "{text}");
    assert!(!text.contains(".1001.") && !text.contains(".1002."), "{text}");
    assert!(all.iter().any(|f| f.ends_with(".1001.png")) && all.iter().any(|f| f.ends_with(".1002.png")));
}

#[test]
fn the_udim_placeholder_replaces_the_tile_number_not_a_look_alike_in_the_name() {
    let mut s = Session::new_project("t", primitives::udim_plane(), "builtin:udim_plane", None, 128).unwrap();
    execute(&mut s, "textureSet.rename", &json!({"ts": 0, "name": "cloth.1001.body"})).unwrap();
    let dir = temp("lookalike");
    let r = export(&mut s, &dir, OPENPBR, true);
    let all = files(&r);
    let mtlx = all.iter().find(|f| f.ends_with(".mtlx")).unwrap_or_else(|| panic!("no .mtlx in {all:?}"));
    let text = std::fs::read_to_string(mtlx).unwrap();
    assert!(text.contains("cloth.1001.body"), "the set's own name stays as it is: {text}");
    assert!(text.contains("&lt;UDIM&gt;"), "{text}");
    // Every texture the document names exists for the first tile.
    for part in text.split("name=\"file\" type=\"filename\" value=\"").skip(1) {
        let file = part.split('"').next().unwrap().replace("&lt;UDIM&gt;", "1001");
        assert!(dir.join(&file).is_file(), "{file} is not in {all:?}");
    }
}

#[test]
fn the_directx_warning_comes_once_and_only_with_a_normal_map() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    let dir = temp("dx");
    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "preset": OPENPBR, "materialX": true, "normalFormat": "directX"}))
        .unwrap();
    let warnings: Vec<&str> = r["warnings"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).filter(|w| w.contains("DirectX")).collect();
    assert_eq!(warnings.len(), 1, "{r}");
    let dir = temp("gl");
    let r = export(&mut s, &dir, OPENPBR, true);
    assert!(!r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("DirectX")), "{r}");
}

/// A tangent-space normal image of one colour as PNG bytes
fn normal_png(rgb: [u8; 3]) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(4, 4, image::Rgb(rgb));
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    bytes
}

fn green_at_centre(path: &str) -> u8 {
    let image = image::open(path).unwrap().to_rgb8();
    image.get_pixel(image.width() / 2, image.height() / 2)[1]
}

#[test]
fn the_materials_normal_map_is_opengl_whatever_the_export_asks_for() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();

    // A baked normal that tilts: green 64 in the file, 71 / 184 once normalised in OpenGL / DirectX
    let image = s.import_resource_bytes("n.png", normal_png([128, 64, 255])).unwrap();
    execute(&mut s, "textureSet.meshMap.set", &json!({"map": "normal", "image": image})).unwrap();
    let dir = temp("normal-format");
    let r = execute(
        &mut s,
        "file.exportTextures",
        &json!({"dir": dir.display().to_string(), "preset": DEFAULT, "ts": 0, "normalFormat": "directX", "materialX": true}),
    )
    .unwrap();
    let all = files(&r);
    let own = all.iter().find(|f| f.ends_with("_normal.png")).unwrap();
    let material = all.iter().find(|f| f.ends_with("_geometry_normal.png")).unwrap();
    assert!(green_at_centre(own).abs_diff(184) <= 3, "the template's own normal map is DirectX: {}", green_at_centre(own));
    assert!(green_at_centre(material).abs_diff(71) <= 3, "the material's normal map is OpenGL: {}", green_at_centre(material));
    assert!(!r["warnings"].to_string().contains("DirectX"), "nothing to flip in the material: {r}");
}

#[test]
fn the_bit_depth_of_the_export_applies_to_the_openpbr_maps_unless_the_file_type_fell_back() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    for (extra, expected) in [(json!({}), image::ColorType::Rgb16), (json!({"format": "psd"}), image::ColorType::Rgb8)] {
        let dir = temp("depth");
        let mut params = json!({"dir": dir.display().to_string(), "preset": DEFAULT, "ts": 0, "bitDepth": 16, "materialX": true});
        params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let r = execute(&mut s, "file.exportTextures", &params).unwrap();
        let png = files(&r).into_iter().find(|f| f.ends_with("_base_color.png")).unwrap();
        assert_eq!(image::open(png).unwrap().color(), expected, "{params}");
    }
}

#[test]
fn a_set_whose_openpbr_maps_were_all_skipped_gets_no_material_and_a_warning() {
    let mut s = Session::new_project("t", primitives::material_ball(), "builtin:material_ball", None, 128).unwrap();
    assert!(s.set_count() >= 2);
    execute(&mut s, "textureSet.rename", &json!({"ts": 0, "name": "a b"})).unwrap();
    execute(&mut s, "textureSet.rename", &json!({"ts": 1, "name": "A_B"})).unwrap();
    let dir = temp("all-skipped");
    let r = execute(&mut s, "file.exportTextures", &json!({"dir": dir.display().to_string(), "preset": DEFAULT, "ts": "all", "materialX": true})).unwrap();
    assert_eq!(files(&r).iter().filter(|f| f.ends_with(".mtlx")).count(), s.set_count() - 1, "{r}");
    let warnings: Vec<&str> = r["warnings"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
    assert!(warnings.contains(&"MaterialX: texture set \"A_B\" has no OpenPBR maps to refer to"), "{warnings:?}");
}
