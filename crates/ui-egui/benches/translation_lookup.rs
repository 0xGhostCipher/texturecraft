//! Time warm lookups with the Japanese catalog embedded by the regular build.
//! With `CRAFT_I18N_DIR` pointing at the pinned catalog checkout, run
//! `cargo bench -p texturecraft-ui-egui --bench translation_lookup --profile dev`.

use std::{hint::black_box, time::Instant};

use texturecraft_ui_egui::i18n::{set_language, tr};

include!(concat!(env!("OUT_DIR"), "/craft_i18n.rs"));

fn main() -> Result<(), String> {
    // Cargo can run this binary under test --all-targets too. Only bench passes --bench.
    if !std::env::args().any(|arg| arg == "--bench") {
        return Ok(());
    }

    let &(_, shared, app) = CRAFT_I18N.iter().find(|(code, _, _)| *code == "ja").ok_or("set CRAFT_I18N_DIR to the pinned craft-i18n checkout")?;

    // The plain rows with literal English keys are representative of the labels tr() looks up.
    let mut keys: Vec<_> = shared
        .lines()
        .chain(app.lines())
        .filter_map(|row| row.strip_prefix('\t')?.split_once('\t').map(|(key, _)| key))
        .filter(|key| !key.is_empty() && !key.contains('\\'))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.is_empty() {
        return Err("the Japanese catalog has no plain translation keys".into());
    }

    set_language("ja");
    black_box(tr(keys[0]));
    const ROUNDS: usize = 100;
    let started = Instant::now();
    for _ in 0..ROUNDS {
        for key in &keys {
            black_box(tr(black_box(key)));
        }
    }
    let ns_per_lookup = started.elapsed().as_secs_f64() * 1e9 / (ROUNDS * keys.len()) as f64;
    println!("{ns_per_lookup:.0} ns per lookup across {} Japanese keys", keys.len());
    assert!(ns_per_lookup < 1000.0, "{ns_per_lookup:.0} ns per lookup exceeded the 1000 ns budget");
    Ok(())
}
