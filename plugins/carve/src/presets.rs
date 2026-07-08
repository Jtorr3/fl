//! CARVE factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI
//! selector (grouped by the `"category"` tag into preset-bar sections) and the
//! offline render tests.
//!
//! Value encodings (plain, un-normalized): `amount`/`sens`/`mix` 0..1; `maxdepth` dB
//! (0..24); `threshold` dB (−90..0); `tilt` −1..1 (− cuts lows more / + cuts highs
//! more); `attack`/`release` ms; `out` dB (kept ≤ 0 for headroom); `listen` is an
//! integer index (0=Off / 1=Sidechain / 2=Delta — see `dsp::ListenMode`).
//!
//! Categories (preset-bar sections, first-appearance order): Vocal-Space /
//! Kick-vs-Bass / Bus-Glue / Aggressive / Utility. Names are purpose-driven and
//! genre-aware (dark techno / atmospheric dnb) — never settings descriptions.

use crate::dsp::{ListenMode, Settings};
use nih_plug::util::db_to_gain;
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Vocal-Space ------------------------------------------------------
    // Carve a vocal-shaped pocket into a music bed: mid-focused, musical times.
    r#"{ "name": "Vocal Space", "category": "Vocal-Space", "amount": 0.8, "maxdepth": 9.0,
         "threshold": -46.0, "tilt": 0.1, "attack": 12.0, "release": 180.0, "sens": 0.55,
         "mix": 1.0 }"#,
    r#"{ "name": "Sewer Choir Pocket", "category": "Vocal-Space", "amount": 0.65, "maxdepth": 11.0,
         "threshold": -48.0, "tilt": 0.25, "attack": 18.0, "release": 260.0, "sens": 0.45,
         "mix": 0.9, "out": -1.0 }"#,
    r#"{ "name": "Podcast De-Mask", "category": "Vocal-Space", "amount": 0.55, "maxdepth": 7.0,
         "threshold": -42.0, "tilt": 0.35, "attack": 10.0, "release": 150.0, "sens": 0.4,
         "mix": 1.0 }"#,
    r#"{ "name": "Whisper Room", "category": "Vocal-Space", "amount": 0.4, "maxdepth": 5.0,
         "threshold": -50.0, "tilt": 0.15, "attack": 22.0, "release": 300.0, "sens": 0.3,
         "mix": 0.85 }"#,
    // ---- Kick-vs-Bass -----------------------------------------------------
    // Low-biased (tilt down) so only the sub/low-mid pocket ducks under the kick.
    r#"{ "name": "Kick Vs Bass", "category": "Kick-vs-Bass", "amount": 0.9, "maxdepth": 14.0,
         "threshold": -50.0, "tilt": -0.6, "attack": 5.0, "release": 90.0, "sens": 0.7,
         "mix": 1.0 }"#,
    r#"{ "name": "Sub Trench", "category": "Kick-vs-Bass", "amount": 0.85, "maxdepth": 16.0,
         "threshold": -52.0, "tilt": -0.75, "attack": 4.0, "release": 70.0, "sens": 0.8,
         "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "808 Room Service", "category": "Kick-vs-Bass", "amount": 0.75, "maxdepth": 12.0,
         "threshold": -48.0, "tilt": -0.5, "attack": 6.0, "release": 110.0, "sens": 0.6,
         "mix": 1.0 }"#,
    r#"{ "name": "Rolling Bassline Duck", "category": "Kick-vs-Bass", "amount": 0.8, "maxdepth": 13.0,
         "threshold": -47.0, "tilt": -0.4, "attack": 3.0, "release": 80.0, "sens": 0.65,
         "mix": 1.0 }"#,
    // ---- Bus-Glue ---------------------------------------------------------
    // Shallow, wide-knee, slow — glue and de-masking without audible pumping.
    r#"{ "name": "Master Bus Tuck", "category": "Bus-Glue", "amount": 0.5, "maxdepth": 5.0,
         "threshold": -40.0, "tilt": 0.0, "attack": 20.0, "release": 300.0, "sens": 0.3,
         "mix": 0.8 }"#,
    r#"{ "name": "Gentle Glue Duck", "category": "Bus-Glue", "amount": 0.45, "maxdepth": 6.0,
         "threshold": -42.0, "tilt": 0.4, "attack": 25.0, "release": 250.0, "sens": 0.35,
         "mix": 0.9 }"#,
    r#"{ "name": "Drum Bus Breathe", "category": "Bus-Glue", "amount": 0.55, "maxdepth": 8.0,
         "threshold": -44.0, "tilt": -0.15, "attack": 16.0, "release": 200.0, "sens": 0.4,
         "mix": 0.85 }"#,
    // ---- Aggressive -------------------------------------------------------
    // Near-max depth, tight knee, fast — obvious spectral ducking / pumping.
    r#"{ "name": "Aggressive Carve", "category": "Aggressive", "amount": 1.0, "maxdepth": 22.0,
         "threshold": -52.0, "tilt": 0.0, "attack": 4.0, "release": 120.0, "sens": 0.9,
         "mix": 1.0 }"#,
    r#"{ "name": "Total Spectral Void", "category": "Aggressive", "amount": 1.0, "maxdepth": 24.0,
         "threshold": -58.0, "tilt": 0.0, "attack": 2.0, "release": 60.0, "sens": 1.0,
         "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Pumping Wall", "category": "Aggressive", "amount": 0.95, "maxdepth": 20.0,
         "threshold": -50.0, "tilt": -0.2, "attack": 8.0, "release": 45.0, "sens": 0.85,
         "mix": 1.0, "out": -1.0 }"#,
    // ---- Utility ----------------------------------------------------------
    // Diagnostic: Δ-listen so you hear exactly what's being carved out.
    r#"{ "name": "Delta Inspector", "category": "Utility", "amount": 1.0, "maxdepth": 20.0,
         "threshold": -50.0, "tilt": 0.0, "attack": 6.0, "release": 140.0, "sens": 0.8,
         "listen": 2.0, "mix": 1.0 }"#,
];

/// Map a numeric `listen` code to the enum (0=Off, 1=Sidechain, 2=Delta).
pub fn listen_from_code(code: f32) -> ListenMode {
    match code.round() as i32 {
        1 => ListenMode::Sidechain,
        2 => ListenMode::Delta,
        _ => ListenMode::Off,
    }
}

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        amount: g("amount", d.amount),
        max_depth_db: g("maxdepth", d.max_depth_db),
        threshold_db: g("threshold", d.threshold_db),
        tilt: g("tilt", d.tilt),
        attack_ms: g("attack", d.attack_ms),
        release_ms: g("release", d.release_ms),
        sens: g("sens", d.sens),
        listen: listen_from_code(g("listen", 0.0)),
        mix: g("mix", d.mix),
        out_gain: db_to_gain(g("out", 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums by equality,
    /// floats by a loose epsilon). Covers every field `settings_from_preset` sets;
    /// there are no fixed constants to skip. Drives both the differ-from-default and
    /// pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.listen != b.listen {
            n += 1;
        }
        let fs = [
            (a.amount, b.amount),
            (a.max_depth_db, b.max_depth_db),
            (a.threshold_db, b.threshold_db),
            (a.tilt, b.tilt),
            (a.attack_ms, b.attack_ms),
            (a.release_ms, b.release_ms),
            (a.sens, b.sens),
            (a.mix, b.mix),
            (a.out_gain, b.out_gain),
        ];
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 {
                n += 1;
            }
        }
        n
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Expanded bank: SPECS target for this simpler utility FX.
        assert!(presets.len() >= 12, "CARVE bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised
        // and differs from the default in >= 4 params.
        for (p, s) in presets.iter().zip(&settings) {
            assert!(p.category.is_some(), "preset '{}' has no category", p.name);
            let diffs = count_diffs(s, &d);
            assert!(
                diffs >= 4,
                "preset '{}' differs from default in only {diffs} params",
                p.name
            );
        }

        // Rule 3 (no near-duplicates): every preset differs from EVERY other in >= 2.
        for i in 0..settings.len() {
            for j in (i + 1)..settings.len() {
                let diffs = count_diffs(&settings[i], &settings[j]);
                assert!(
                    diffs >= 2,
                    "presets '{}' and '{}' differ in only {diffs} params (near-duplicate)",
                    presets[i].name,
                    presets[j].name
                );
            }
        }

        // Names must be unique too.
        for i in 0..presets.len() {
            for j in (i + 1)..presets.len() {
                assert_ne!(presets[i].name, presets[j].name, "duplicate preset name");
            }
        }
        // Rule 4 (render passes universal assertions) is enforced by the
        // `render_tests::every_preset_renders_and_passes_universal` test in lib.rs.
    }
}
