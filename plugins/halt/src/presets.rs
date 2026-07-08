//! HALT factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded flat-JSON
//! blob parsed by `suite_core::presets`. The same list drives the GUI selector (grouped by the
//! `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Presets set the *character* knobs ONLY — the four momentary mode buttons (tape-stop / stutter
//! / reverse / half-speed) are LIVE PERFORMANCE STATE and are never stored (Batch-A HALT
//! precedent). Categories name the *gesture* a preset is dialled-in for (Tape-Stop Feels /
//! Stutter Feels / Reverse Sweeps / Half-Time Grooves) even though hitting the button is up to
//! the performer; the stored knobs shape how that gesture (and any mode chained after it) reads.
//!
//! Value encodings (plain numeric): `stutdiv` 0..4 (1/4,1/8,1/16,1/32,1/64); `decay` 0..1;
//! `pitchstep` −12..12 st; `tapesync` 0..4 (Free,1beat,1/2bar,1bar,2bar); `tapefree` s (0.05..4);
//! `tapecurve` 0..1 (0 exp · 0.5 linear · 1 log); `taperel` 0/1 (Ramp/Instant);
//! `quant` 0..3 (Off,1/16,1/8,1/4); `mix` 0..1; `out` dB (kept ≤ 0 — the wet replays the capture
//! at unity, so a positive trim could clip).

use crate::dsp::{QuantDiv, Settings, StutterDiv, TapeRelease, TapeSync};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category (first-appearance order):
/// Tape-Stop Feels / Stutter Feels / Reverse Sweeps / Half-Time Grooves.
pub const PRESET_JSON: &[&str] = &[
    // ---- Tape-Stop Feels --------------------------------------------------
    r#"{ "name": "Warehouse Power-Down", "category": "Tape-Stop Feels",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 4, "tapefree": 1.0, "tapecurve": 0.85, "taperel": 0,
         "quant": 0, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Turbine Halt", "category": "Tape-Stop Feels",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 0, "tapefree": 0.3, "tapecurve": 0.1, "taperel": 1,
         "quant": 0, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Vinyl Brake Slow", "category": "Tape-Stop Feels",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 0, "tapefree": 2.0, "tapecurve": 0.55, "taperel": 0,
         "quant": 0, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Dying Cassette", "category": "Tape-Stop Feels",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 0, "tapefree": 3.5, "tapecurve": 0.9, "taperel": 0,
         "quant": 0, "mix": 0.9, "out": -2.0 }"#,
    r#"{ "name": "Snap Stop", "category": "Tape-Stop Feels",
         "stutdiv": 2, "decay": 0.0, "pitchstep": 0,
         "tapesync": 1, "tapefree": 1.0, "tapecurve": 0.05, "taperel": 1,
         "quant": 0, "mix": 1.0, "out": -0.5 }"#,
    // ---- Stutter Feels ----------------------------------------------------
    r#"{ "name": "Gutter Roll 16th", "category": "Stutter Feels",
         "stutdiv": 2, "decay": 0.3, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 1, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Amen Skip 32nd", "category": "Stutter Feels",
         "stutdiv": 3, "decay": 0.15, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 2, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Machine Gun 64th", "category": "Stutter Feels",
         "stutdiv": 4, "decay": 0.5, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 1, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Rising Glitch Ladder", "category": "Stutter Feels",
         "stutdiv": 2, "decay": 0.4, "pitchstep": 5,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 1, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Subsink Stutter", "category": "Stutter Feels",
         "stutdiv": 1, "decay": 0.25, "pitchstep": -7,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 2, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Quarter-Note Chop", "category": "Stutter Feels",
         "stutdiv": 0, "decay": 0.2, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 3, "mix": 1.0, "out": -0.5 }"#,
    // ---- Reverse Sweeps ---------------------------------------------------
    r#"{ "name": "Backwash Riser", "category": "Reverse Sweeps",
         "stutdiv": 2, "decay": 0.2, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.7, "taperel": 0,
         "quant": 0, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Inverted Cathedral", "category": "Reverse Sweeps",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 4, "tapefree": 1.0, "tapecurve": 0.75, "taperel": 0,
         "quant": 0, "mix": 0.85, "out": -1.5 }"#,
    r#"{ "name": "Reverse Undertow", "category": "Reverse Sweeps",
         "stutdiv": 3, "decay": 0.35, "pitchstep": -3,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 1, "mix": 1.0, "out": -2.0 }"#,
    // ---- Half-Time Grooves ------------------------------------------------
    r#"{ "name": "Half-Time Haze", "category": "Half-Time Grooves",
         "stutdiv": 2, "decay": 0.15, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.65, "taperel": 0,
         "quant": 0, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Molasses Drop", "category": "Half-Time Grooves",
         "stutdiv": 1, "decay": 0.1, "pitchstep": 0,
         "tapesync": 4, "tapefree": 1.0, "tapecurve": 0.8, "taperel": 0,
         "quant": 0, "mix": 0.9, "out": -1.0 }"#,
    r#"{ "name": "Concrete Slowdive", "category": "Half-Time Grooves",
         "stutdiv": 1, "decay": 0.3, "pitchstep": -5,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.6, "taperel": 1,
         "quant": 2, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Sewer Lullaby", "category": "Half-Time Grooves",
         "stutdiv": 4, "decay": 0.45, "pitchstep": -2,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 1, "mix": 0.75, "out": -2.0 }"#,
];

/// Build the DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        stutter_div: StutterDiv::from_index(g("stutdiv", 1.0) as usize),
        stutter_decay: g("decay", d.stutter_decay),
        stutter_pitch: g("pitchstep", d.stutter_pitch as f32) as i32,
        tape_sync: TapeSync::from_index(g("tapesync", 3.0) as usize),
        tape_free_s: g("tapefree", d.tape_free_s),
        tape_curve: g("tapecurve", d.tape_curve),
        tape_release: TapeRelease::from_index(g("taperel", 1.0) as usize),
        quantize: QuantDiv::from_index(g("quant", 0.0) as usize),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums / the integer pitch
    /// step by equality, floats by a loose epsilon). Covers exactly the ten fields
    /// `settings_from_preset` writes. Drives both the differ-from-default and pairwise-
    /// distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.stutter_div != b.stutter_div { n += 1; }
        if a.tape_sync != b.tape_sync { n += 1; }
        if a.tape_release != b.tape_release { n += 1; }
        if a.quantize != b.quantize { n += 1; }
        if a.stutter_pitch != b.stutter_pitch { n += 1; }
        let fs = [
            (a.stutter_decay, b.stutter_decay),
            (a.tape_free_s, b.tape_free_s),
            (a.tape_curve, b.tape_curve),
            (a.mix, b.mix),
            (a.out_db, b.out_db),
        ];
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 { n += 1; }
        }
        n
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 18, "HALT bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised and
        // differs from the default in >= 4 params.
        for (p, s) in presets.iter().zip(&settings) {
            assert!(p.category.is_some(), "preset '{}' has no category", p.name);
            let diffs = count_diffs(s, &d);
            assert!(diffs >= 4, "preset '{}' differs from default in only {diffs} params", p.name);
        }

        // Rule 3 (no near-duplicates): every preset differs from EVERY other in >= 2.
        for i in 0..settings.len() {
            for j in (i + 1)..settings.len() {
                let diffs = count_diffs(&settings[i], &settings[j]);
                assert!(
                    diffs >= 2,
                    "presets '{}' and '{}' differ in only {diffs} params (near-duplicate)",
                    presets[i].name, presets[j].name
                );
            }
        }

        // Names must be unique too.
        for i in 0..presets.len() {
            for j in (i + 1)..presets.len() {
                assert_ne!(presets[i].name, presets[j].name, "duplicate preset name");
            }
        }
        // Rule 4 (render passes universal assertions) is enforced by the preset-render loop in
        // `tests.rs` (`assert_universal` on every preset's warm render).
    }
}
