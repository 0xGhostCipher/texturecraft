//! `project.rename`: the name `$project` stands for in export file names.

use serde_json::json;
use texturecraft_engine::{Session, execute};

use texturecraft_engine::geom::primitives;

#[test]
fn a_renamed_project_exports_under_its_new_name_until_undone() {
    let mut session = Session::new_project("hero", primitives::cube(2.0), "builtin:cube", None, 128).unwrap();
    let mut preset = execute(&mut session, "export.preset.get", &json!({"name": "Document channels + Normal + AO (No Alpha)"})).unwrap();
    preset["maps"][0]["file"] = json!("$project_$textureSet");

    execute(&mut session, "project.rename", &json!({"name": "  Knight final "})).unwrap();

    let plan = execute(&mut session, "export.plan", &json!({"preset": preset})).unwrap().to_string();
    assert!(plan.contains("Knight_final_") && !plan.contains("hero_"), "{plan}");

    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert_eq!(session.doc.project.name, "hero");
}

#[test]
fn an_unusable_or_unchanged_project_name_edits_nothing() {
    let mut session = Session::new_project("hero", primitives::cube(2.0), "builtin:cube", None, 128).unwrap();
    for name in [json!(null), json!(7), json!("   "), json!("a\nb"), json!("x".repeat(129))] {
        assert!(execute(&mut session, "project.rename", &json!({"name": name.clone()})).is_err(), "{name}");
    }

    execute(&mut session, "project.rename", &json!({"name": "hero"})).unwrap();

    assert_eq!(session.doc.project.name, "hero");
    assert!(!session.modified, "nothing changed");
}
