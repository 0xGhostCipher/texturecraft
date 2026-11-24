//! The `effect.set` keys of a generator's controls, as its panel sends them

use serde_json::{Value, json};
use texturecraft_engine::core::{Generator, GeneratorKind, LightParameters, MicroDetails, SectionParameters, TextureLayer, WearParameters};
use texturecraft_engine::mask_cmds::{section_key_applies, wear_key_applies};
use texturecraft_engine::micro_params::micro_key_applies;

/// The keys of `extra` added to the object `params`
pub(crate) fn merged(mut params: Value, extra: Value) -> Value {
    if let (Some(params), Value::Object(extra)) = (params.as_object_mut(), extra) {
        params.extend(extra);
    }
    params
}

/// Every wear control a generator of `kind` has, set to `wear`
pub(crate) fn wear_params(kind: GeneratorKind, wear: &WearParameters) -> Value {
    let mut params = json!({
        "wearLevel": wear.level,
        "wearContrast": wear.contrast,
        "grungeAmount": wear.grunge_amount,
        "grungeScale": wear.grunge_scale,
        "triplanar": wear.triplanar,
        "triplanarContrast": wear.triplanar_contrast,
        "edgesSmoothness": wear.edges_smoothness,
        "aoMasking": wear.ao_masking,
        "curvatureWeight": wear.curvature_weight,
        "edgesMasking": wear.edges_masking,
        "useCustomGrunge": wear.custom_grunge,
    });
    if let Some(params) = params.as_object_mut() {
        params.retain(|key, _| wear_key_applies(kind, key));
    }
    params
}

/// Every section control a generator of `kind` has, set to `section`
pub(crate) fn section_params(kind: GeneratorKind, section: &SectionParameters) -> Value {
    let mut params = json!({
        "sectionInvert": section.invert,
        "sectionBlur": section.blur,
        "sectionBalance": section.balance,
        "sectionContrast": section.contrast,
        "sectionBrightness": section.brightness,
    });
    if let Some(params) = params.as_object_mut() {
        for (name, axis) in ["rightLeft", "topBottom", "frontBack"].into_iter().zip(&section.axes) {
            params.insert(format!("{name}Invert"), json!(axis.invert));
            params.insert(format!("{name}Blend"), json!(axis.blend.id()));
            params.insert(format!("{name}Weight"), json!(axis.weight));
        }
        params.retain(|key, _| section_key_applies(kind, key));
    }
    params
}

/// The texture's controls (its image is the `texture` input)
pub(crate) fn texture_params(texture: &TextureLayer) -> Value {
    json!({
        "useTexture": texture.enabled,
        "textureOpacity": texture.opacity,
        "textureInvert": texture.invert,
        "textureGrayscale": texture.grayscale.id(),
        "textureBlend": texture.blend.id(),
        "textureScale": texture.scale,
        "textureContrast": texture.contrast,
        "textureBrightness": texture.brightness,
        "textureTriplanar": texture.triplanar,
        "textureTriplanarContrast": texture.triplanar_contrast,
    })
}

/// Every light control, set to `light`
pub(crate) fn light_params(light: &LightParameters) -> Value {
    json!({
        "lightHorizontal": light.horizontal,
        "lightVertical": light.vertical,
        "highlightGlossiness": light.glossiness,
        "highlightLevel": light.highlight_level,
        "lightAttenuation": light.attenuation,
    })
}

/// Every micro details control a generator of `kind` has, set to `micro` (its images are the `microHeight` and `microNormal` inputs)
pub(crate) fn micro_params(kind: GeneratorKind, micro: &MicroDetails) -> Value {
    let mut params = json!({
        "useMicroDetails": micro.enabled,
        "microHeight": micro.height,
        "microNormal": micro.normal,
        "microCurvatureType": micro.curvature_type.label(),
        "microCurvatureIntensity": micro.curvature_intensity,
        "microHeightIntensity": micro.height_intensity,
        "microAoRadius": micro.ao_radius,
        "microAoDepth": micro.ao_depth,
    });
    if let Some(params) = params.as_object_mut() {
        params.retain(|key, _| micro_key_applies(kind, key));
    }
    params
}

/// The `effect.set` params that put generator effect `id` back to the defaults of its kind; its images stay in their slots
pub(crate) fn defaults_of(id: u64, generator: &Generator) -> Value {
    let kind = generator.kind;
    let defaults = Generator::new(kind);
    let params = json!({
        "effect": id,
        "seed": defaults.seed,
        "invert": defaults.invert,
        "resolution": format!("{:?}", defaults.resolution.mode),
        "customResolution": defaults.resolution.custom,
        "resolutionFactor": defaults.resolution.factor,
    });

    // A generator on the reference's controls restores those only: the earlier model's keys would move it back onto that model
    let global = json!({"blur": defaults.global.blur, "balance": defaults.global.balance, "contrast": defaults.global.contrast});
    let own = match (&defaults.wear, &defaults.section, &defaults.light) {
        (Some(wear), _, _) => wear_params(kind, wear),
        (None, _, Some(light)) => light_params(light),
        // The texture's controls only once it has them: its image stays in its slot
        (None, Some(section), None) if generator.texture.is_some() => {
            merged(merged(section_params(kind, section), global), texture_params(&TextureLayer::default()))
        }
        (None, Some(section), None) => merged(section_params(kind, section), global),
        (None, None, None) => json!({
            "level": defaults.level,
            "softness": defaults.softness,
            "noise": defaults.noise_amount,
            "noiseScale": defaults.noise_scale,
            "offset": defaults.offset,
            "direction": defaults.direction,
            "blur": defaults.global.blur,
            "balance": defaults.global.balance,
            "contrast": defaults.global.contrast,
        }),
    };
    let mut params = merged(params, own);
    if let (None, Some(detail)) = (&defaults.wear, defaults.detail) {
        params["mode"] = json!(format!("{:?}", detail.mode));
        params["sizes"] = json!(detail.weights);
    }

    // Micro details only once it has them, as the texture's
    if generator.micro.is_some()
        && let Some(micro) = MicroDetails::defaults(kind)
    {
        params = merged(params, micro_params(kind, &micro));
    }
    params
}

/// Whether Restore defaults would change generator `generator`: its controls differ from its kind's, images and resolution aside
pub(crate) fn differs_from_defaults(generator: &Generator) -> bool {
    let mut restored = Generator::new(generator.kind);
    restored.resolution = generator.resolution;
    restored.inputs = generator.inputs.clone();
    restored.grunge = generator.grunge.clone();
    if let Some(texture) = &generator.texture {
        restored.texture = Some(Box::new(TextureLayer { source: texture.source.clone(), ..TextureLayer::default() }));
    }
    if let Some(micro) = &generator.micro
        && let Some(defaults) = MicroDetails::defaults(generator.kind)
    {
        restored.micro = Some(Box::new(MicroDetails { height_source: micro.height_source.clone(), normal_source: micro.normal_source.clone(), ..defaults }));
    }
    restored.controls = generator.controls.clone();

    // Saved before the wear, section or light controls: the earlier model's own values count
    if generator.wear.is_none() {
        restored.wear = None;
    }
    if generator.section.is_none() {
        restored.section = None;
    }
    if generator.light.is_none() {
        restored.light = None;
    }
    if generator.detail.is_none() {
        restored.detail = None;
    }
    restored != *generator
}
