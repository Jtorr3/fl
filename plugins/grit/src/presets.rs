//! GRIT factory presets (SPECS list). Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`. The same list drives the GUI selector and the offline
//! render tests. Values are plain (un-normalized): dB for gains, Hz for cutoffs,
//! 0..1 for depth/mix, octaves for width, and 0/1 for the two toggles. `mode` and
//! `shape` are integer indices (see `dsp::Mode` / `dsp::ShapeKind`).

use crate::dsp::{Mode, Settings, ShapeKind};
use suite_core::presets::Preset;

/// The five factory presets, in menu order.
pub const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Kick Bass Grit", "mode": 0, "shape": 0, "trim": 0.0, "drive": 6.0,
         "depth": 0.85, "curve": 1.2, "attack": 2.0, "release": 60.0,
         "sc_focus": 80.0, "sc_width": 1.2, "sc_listen": 0,
         "pre_hp": 30.0, "pre_lp": 6000.0, "post_hp": 40.0, "post_lp": 12000.0,
         "auto_gain": 1, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Vocal Crush", "mode": 1, "shape": 1, "trim": 2.0, "drive": 10.0,
         "depth": 0.7, "curve": 1.0, "attack": 5.0, "release": 90.0,
         "sc_focus": 2500.0, "sc_width": 2.0, "sc_listen": 0,
         "pre_hp": 120.0, "pre_lp": 12000.0, "post_hp": 100.0, "post_lp": 14000.0,
         "auto_gain": 1, "mix": 0.6, "out": 0.0 }"#,
    r#"{ "name": "Pad Ring-Fold", "mode": 1, "shape": 2, "trim": 0.0, "drive": 8.0,
         "depth": 0.9, "curve": 1.0, "attack": 10.0, "release": 200.0,
         "sc_focus": 700.0, "sc_width": 2.5, "sc_listen": 0,
         "pre_hp": 60.0, "pre_lp": 9000.0, "post_hp": 50.0, "post_lp": 11000.0,
         "auto_gain": 1, "mix": 0.5, "out": -1.0 }"#,
    r#"{ "name": "Drum Bus Pump-Drive", "mode": 0, "shape": 1, "trim": 0.0, "drive": 4.0,
         "depth": 0.8, "curve": 1.5, "attack": 1.0, "release": 40.0,
         "sc_focus": 120.0, "sc_width": 1.4, "sc_listen": 0,
         "pre_hp": 25.0, "pre_lp": 16000.0, "post_hp": 30.0, "post_lp": 18000.0,
         "auto_gain": 1, "mix": 0.8, "out": 0.0 }"#,
    r#"{ "name": "Techno Rumble Driver", "mode": 0, "shape": 3, "trim": 3.0, "drive": 9.0,
         "depth": 1.0, "curve": 0.8, "attack": 3.0, "release": 120.0,
         "sc_focus": 55.0, "sc_width": 1.0, "sc_listen": 0,
         "pre_hp": 20.0, "pre_lp": 5000.0, "post_hp": 35.0, "post_lp": 9000.0,
         "auto_gain": 1, "mix": 1.0, "out": -1.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5, "need >= 5 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            // Count params that differ from the default (loose float compare).
            let mut diffs = 0;
            if s.mode != d.mode { diffs += 1; }
            if s.shape != d.shape { diffs += 1; }
            if (s.trim_db - d.trim_db).abs() > 1e-3 { diffs += 1; }
            if (s.drive_db - d.drive_db).abs() > 1e-3 { diffs += 1; }
            if (s.depth - d.depth).abs() > 1e-3 { diffs += 1; }
            if (s.curve - d.curve).abs() > 1e-3 { diffs += 1; }
            if (s.attack_ms - d.attack_ms).abs() > 1e-3 { diffs += 1; }
            if (s.release_ms - d.release_ms).abs() > 1e-3 { diffs += 1; }
            if (s.sc_focus_hz - d.sc_focus_hz).abs() > 1e-3 { diffs += 1; }
            if (s.sc_width_oct - d.sc_width_oct).abs() > 1e-3 { diffs += 1; }
            if (s.mix - d.mix).abs() > 1e-3 { diffs += 1; }
            if (s.post_lp_hz - d.post_lp_hz).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
