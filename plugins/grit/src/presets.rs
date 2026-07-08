//! GRIT factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI
//! selector (grouped by the `"category"` tag into preset-bar sections) and the
//! offline render tests. Values are plain (un-normalized): dB for gains, Hz for
//! cutoffs, 0..1 for depth/mix, octaves for width, and 0/1 for the two toggles.
//! `mode` and `shape` are integer indices (see `dsp::Mode` / `dsp::ShapeKind`:
//! mode 0=EnvDrive 1=WaveshapeSc; shape 0=Tube 1=Tape 2=Fold 3=Hard).
//!
//! Categories (preset-bar sections): Kick-Driven / Vocal / Bus / Texture / Extreme.
//! Names are purpose-driven and genre-aware (dark techno / atmospheric dnb) — never
//! settings descriptions.

use crate::dsp::{Mode, Settings, ShapeKind};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Kick-Driven ------------------------------------------------------
    r#"{ "name": "Warehouse Thump", "category": "Kick-Driven", "mode": 0, "shape": 0,
         "trim": 0.0, "drive": 8.0, "depth": 0.9, "curve": 1.1, "attack": 1.0,
         "release": 50.0, "sc_focus": 60.0, "sc_width": 1.0, "sc_listen": 0,
         "pre_hp": 25.0, "pre_lp": 7000.0, "post_hp": 35.0, "post_lp": 10000.0,
         "auto_gain": 1, "mix": 1.0, "out": -3.2 }"#,
    r#"{ "name": "Kick Bass Grit", "category": "Kick-Driven", "mode": 0, "shape": 0,
         "trim": 0.0, "drive": 6.0, "depth": 0.85, "curve": 1.2, "attack": 2.0,
         "release": 60.0, "sc_focus": 80.0, "sc_width": 1.2, "sc_listen": 0,
         "pre_hp": 30.0, "pre_lp": 6000.0, "post_hp": 40.0, "post_lp": 12000.0,
         "auto_gain": 1, "mix": 1.0, "out": -2.8 }"#,
    r#"{ "name": "Concrete Slam", "category": "Kick-Driven", "mode": 1, "shape": 3,
         "trim": 0.0, "drive": 5.0, "depth": 0.7, "curve": 1.4, "attack": 0.5,
         "release": 35.0, "sc_focus": 90.0, "sc_width": 0.8, "sc_listen": 0,
         "pre_hp": 30.0, "pre_lp": 9000.0, "post_hp": 45.0, "post_lp": 13000.0,
         "auto_gain": 1, "mix": 0.9, "out": -4.0 }"#,
    r#"{ "name": "Sub Punch Driver", "category": "Kick-Driven", "mode": 0, "shape": 1,
         "trim": 0.0, "drive": 4.0, "depth": 1.0, "curve": 0.9, "attack": 3.0,
         "release": 80.0, "sc_focus": 45.0, "sc_width": 1.3, "sc_listen": 0,
         "pre_hp": 20.0, "pre_lp": 5000.0, "post_hp": 30.0, "post_lp": 8000.0,
         "auto_gain": 1, "mix": 1.0, "out": -2.4 }"#,
    // ---- Vocal ------------------------------------------------------------
    r#"{ "name": "Vocal Crush", "category": "Vocal", "mode": 1, "shape": 1,
         "trim": 2.0, "drive": 10.0, "depth": 0.7, "curve": 1.0, "attack": 5.0,
         "release": 90.0, "sc_focus": 2500.0, "sc_width": 2.0, "sc_listen": 0,
         "pre_hp": 120.0, "pre_lp": 12000.0, "post_hp": 100.0, "post_lp": 14000.0,
         "auto_gain": 1, "mix": 0.6, "out": -1.0 }"#,
    r#"{ "name": "Drowned Ghost Vox", "category": "Vocal", "mode": 1, "shape": 2,
         "trim": 0.0, "drive": 9.0, "depth": 0.6, "curve": 1.0, "attack": 8.0,
         "release": 140.0, "sc_focus": 1800.0, "sc_width": 2.5, "sc_listen": 0,
         "pre_hp": 150.0, "pre_lp": 8000.0, "post_hp": 120.0, "post_lp": 9000.0,
         "auto_gain": 1, "mix": 0.45, "out": 0.0 }"#,
    r#"{ "name": "Radio Ghost", "category": "Vocal", "mode": 1, "shape": 3,
         "trim": 2.0, "drive": 12.0, "depth": 0.5, "curve": 1.0, "attack": 4.0,
         "release": 70.0, "sc_focus": 3000.0, "sc_width": 1.5, "sc_listen": 0,
         "pre_hp": 300.0, "pre_lp": 5000.0, "post_hp": 250.0, "post_lp": 6000.0,
         "auto_gain": 1, "mix": 0.7, "out": -1.0 }"#,
    r#"{ "name": "Tape Choir Bed", "category": "Vocal", "mode": 0, "shape": 1,
         "trim": 0.0, "drive": 5.0, "depth": 0.4, "curve": 1.0, "attack": 15.0,
         "release": 250.0, "sc_focus": 900.0, "sc_width": 3.0, "sc_listen": 0,
         "pre_hp": 100.0, "pre_lp": 10000.0, "post_hp": 80.0, "post_lp": 11000.0,
         "auto_gain": 1, "mix": 0.35, "out": 0.0 }"#,
    // ---- Bus --------------------------------------------------------------
    r#"{ "name": "Drum Bus Pump-Drive", "category": "Bus", "mode": 0, "shape": 1,
         "trim": 0.0, "drive": 4.0, "depth": 0.8, "curve": 1.5, "attack": 1.0,
         "release": 40.0, "sc_focus": 120.0, "sc_width": 1.4, "sc_listen": 0,
         "pre_hp": 25.0, "pre_lp": 16000.0, "post_hp": 30.0, "post_lp": 18000.0,
         "auto_gain": 1, "mix": 0.8, "out": -1.0 }"#,
    r#"{ "name": "Glue Bus Heat", "category": "Bus", "mode": 0, "shape": 0,
         "trim": 0.0, "drive": 3.0, "depth": 0.4, "curve": 1.0, "attack": 8.0,
         "release": 150.0, "sc_focus": 2000.0, "sc_width": 3.5, "sc_listen": 0,
         "pre_hp": 30.0, "pre_lp": 18000.0, "post_hp": 25.0, "post_lp": 19000.0,
         "auto_gain": 1, "mix": 0.5, "out": -0.5 }"#,
    r#"{ "name": "Broken Tape Bus", "category": "Bus", "mode": 0, "shape": 1,
         "trim": 1.0, "drive": 6.0, "depth": 0.6, "curve": 1.0, "attack": 6.0,
         "release": 180.0, "sc_focus": 400.0, "sc_width": 2.0, "sc_listen": 0,
         "pre_hp": 40.0, "pre_lp": 12000.0, "post_hp": 45.0, "post_lp": 13000.0,
         "auto_gain": 1, "mix": 0.6, "out": -1.0 }"#,
    // ---- Texture ----------------------------------------------------------
    r#"{ "name": "Pad Ring-Fold", "category": "Texture", "mode": 1, "shape": 2,
         "trim": 0.0, "drive": 8.0, "depth": 0.9, "curve": 1.0, "attack": 10.0,
         "release": 200.0, "sc_focus": 700.0, "sc_width": 2.5, "sc_listen": 0,
         "pre_hp": 60.0, "pre_lp": 9000.0, "post_hp": 50.0, "post_lp": 11000.0,
         "auto_gain": 1, "mix": 0.5, "out": -1.0 }"#,
    r#"{ "name": "Grief Wash Drive", "category": "Texture", "mode": 1, "shape": 2,
         "trim": 0.0, "drive": 10.0, "depth": 0.85, "curve": 1.0, "attack": 20.0,
         "release": 300.0, "sc_focus": 500.0, "sc_width": 2.8, "sc_listen": 0,
         "pre_hp": 50.0, "pre_lp": 7000.0, "post_hp": 45.0, "post_lp": 9000.0,
         "auto_gain": 1, "mix": 0.4, "out": -2.0 }"#,
    r#"{ "name": "Rust Bloom", "category": "Texture", "mode": 0, "shape": 3,
         "trim": 1.0, "drive": 7.0, "depth": 0.55, "curve": 1.6, "attack": 12.0,
         "release": 220.0, "sc_focus": 1200.0, "sc_width": 1.8, "sc_listen": 0,
         "pre_hp": 70.0, "pre_lp": 6500.0, "post_hp": 60.0, "post_lp": 8500.0,
         "auto_gain": 1, "mix": 0.55, "out": -1.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    r#"{ "name": "Techno Rumble Driver", "category": "Extreme", "mode": 0, "shape": 3,
         "trim": 3.0, "drive": 9.0, "depth": 1.0, "curve": 0.8, "attack": 3.0,
         "release": 120.0, "sc_focus": 55.0, "sc_width": 1.0, "sc_listen": 0,
         "pre_hp": 20.0, "pre_lp": 5000.0, "post_hp": 35.0, "post_lp": 9000.0,
         "auto_gain": 1, "mix": 1.0, "out": -6.4 }"#,
    r#"{ "name": "Total Annihilation", "category": "Extreme", "mode": 1, "shape": 3,
         "trim": 6.0, "drive": 18.0, "depth": 1.0, "curve": 0.7, "attack": 0.5,
         "release": 30.0, "sc_focus": 70.0, "sc_width": 0.6, "sc_listen": 0,
         "pre_hp": 20.0, "pre_lp": 4000.0, "post_hp": 30.0, "post_lp": 7000.0,
         "auto_gain": 1, "mix": 1.0, "out": -9.2 }"#,
    // Distorted-808 destroyer (Akiaura / agonyOST reference): keeps the sub, stays
    // glossy-dark. Waveshape mode so the kick pumps the bias; Tape shape for a
    // compressed, non-fizzy edge; dark pre/post filtering tames the high harmonics.
    r#"{ "name": "Sub Detonator", "category": "Extreme", "mode": 1, "shape": 1,
         "trim": 2.0, "drive": 15.0, "depth": 0.9, "curve": 0.9, "attack": 1.0,
         "release": 60.0, "sc_focus": 55.0, "sc_width": 0.8, "sc_listen": 0,
         "pre_hp": 25.0, "pre_lp": 5500.0, "post_hp": 30.0, "post_lp": 7000.0,
         "auto_gain": 1, "mix": 1.0, "out": -5.5 }"#,
    r#"{ "name": "Fold Abyss", "category": "Extreme", "mode": 1, "shape": 2,
         "trim": 3.0, "drive": 14.0, "depth": 1.0, "curve": 1.3, "attack": 2.0,
         "release": 90.0, "sc_focus": 55.0, "sc_width": 0.7, "sc_listen": 0,
         "pre_hp": 20.0, "pre_lp": 3500.0, "post_hp": 25.0, "post_lp": 6000.0,
         "auto_gain": 1, "mix": 1.0, "out": -6.7 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for any
/// key the blob omits.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        mode: Mode::from_index(g("mode", 0.0) as usize),
        shape: ShapeKind::from_index(g("shape", 0.0) as usize),
        trim_db: g("trim", d.trim_db),
        drive_db: g("drive", d.drive_db),
        depth: g("depth", d.depth),
        curve: g("curve", d.curve),
        attack_ms: g("attack", d.attack_ms),
        release_ms: g("release", d.release_ms),
        sc_focus_hz: g("sc_focus", d.sc_focus_hz),
        sc_width_oct: g("sc_width", d.sc_width_oct),
        sc_listen: g("sc_listen", 0.0) >= 0.5,
        pre_hp_hz: g("pre_hp", d.pre_hp_hz),
        pre_lp_hz: g("pre_lp", d.pre_lp_hz),
        post_hp_hz: g("post_hp", d.post_hp_hz),
        post_lp_hz: g("post_lp", d.post_lp_hz),
        auto_gain: g("auto_gain", 1.0) >= 0.5,
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/bools by
    /// equality, floats by a loose epsilon). Drives both the differ-from-default and
    /// pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.mode != b.mode { n += 1; }
        if a.shape != b.shape { n += 1; }
        if a.sc_listen != b.sc_listen { n += 1; }
        if a.auto_gain != b.auto_gain { n += 1; }
        let fs = [
            (a.trim_db, b.trim_db), (a.drive_db, b.drive_db), (a.depth, b.depth),
            (a.curve, b.curve), (a.attack_ms, b.attack_ms), (a.release_ms, b.release_ms),
            (a.sc_focus_hz, b.sc_focus_hz), (a.sc_width_oct, b.sc_width_oct),
            (a.pre_hp_hz, b.pre_hp_hz), (a.pre_lp_hz, b.pre_lp_hz),
            (a.post_hp_hz, b.post_hp_hz), (a.post_lp_hz, b.post_lp_hz),
            (a.mix, b.mix), (a.out_db, b.out_db),
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
        assert!(presets.len() >= 15, "GRIT bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from
        // the default in >= 4 params. Every preset is categorised.
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
        // Rule 4 (render passes universal assertions) is enforced by the
        // `render_tests::every_preset_renders_and_passes_universal` test in lib.rs.
    }
}
