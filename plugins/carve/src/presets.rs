//! CARVE factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `amount`/`sens`/`mix` 0..1; `maxdepth` dB (0..24); `threshold` dB;
//! `tilt` −1..1; `attack`/`release` ms; `out` dB; `listen` 0=Off / 1=Sidechain / 2=Delta.

use crate::dsp::{ListenMode, Settings};
use nih_plug::util::db_to_gain;
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Carve a vocal-shaped pocket into a music bed: mid-focused, musical times, moderate depth.
    r#"{ "name": "Vocal Space", "amount": 0.8, "maxdepth": 9.0, "threshold": -46.0,
         "tilt": 0.1, "attack": 12.0, "release": 180.0, "sens": 0.55, "mix": 1.0 }"#,
    // Kick sidechains bass: fast, deep, low-biased so only the sub/low-mid pocket ducks.
    r#"{ "name": "Kick Vs Bass", "amount": 0.9, "maxdepth": 14.0, "threshold": -50.0,
         "tilt": -0.6, "attack": 5.0, "release": 90.0, "sens": 0.7, "mix": 1.0 }"#,
    // Gentle master-bus tuck: shallow, wide knee, slow release — glue without pumping.
    r#"{ "name": "Master Bus Tuck", "amount": 0.5, "maxdepth": 5.0, "threshold": -40.0,
         "tilt": 0.0, "attack": 20.0, "release": 300.0, "sens": 0.3, "mix": 0.8 }"#,
    // Aggressive carve: near-max depth, tight knee, fast — obvious spectral ducking.
    r#"{ "name": "Aggressive Carve", "amount": 1.0, "maxdepth": 22.0, "threshold": -52.0,
         "tilt": 0.0, "attack": 4.0, "release": 120.0, "sens": 0.9, "mix": 1.0 }"#,
    // Gentle glue duck: soft, high-biased (spare the lows), slow — an airy top-end tuck.
    r#"{ "name": "Gentle Glue Duck", "amount": 0.45, "maxdepth": 6.0, "threshold": -42.0,
         "tilt": 0.4, "attack": 25.0, "release": 250.0, "sens": 0.35, "mix": 0.9 }"#,
    // Delta inspector: same as Aggressive but Δ-listen so you hear exactly what's carved out.
    r#"{ "name": "Delta Inspector", "amount": 1.0, "maxdepth": 20.0, "threshold": -50.0,
         "tilt": 0.0, "attack": 6.0, "release": 140.0, "sens": 0.8, "listen": 2.0, "mix": 1.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.amount - d.amount).abs() > 1e-3 { diffs += 1; }
            if (s.max_depth_db - d.max_depth_db).abs() > 1e-3 { diffs += 1; }
            if (s.threshold_db - d.threshold_db).abs() > 1e-3 { diffs += 1; }
            if (s.tilt - d.tilt).abs() > 1e-3 { diffs += 1; }
            if (s.attack_ms - d.attack_ms).abs() > 1e-3 { diffs += 1; }
            if (s.release_ms - d.release_ms).abs() > 1e-3 { diffs += 1; }
            if (s.sens - d.sens).abs() > 1e-3 { diffs += 1; }
            if s.listen != d.listen { diffs += 1; }
            if (s.mix - d.mix).abs() > 1e-3 { diffs += 1; }
            if (s.out_gain - d.out_gain).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
