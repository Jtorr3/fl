//! SMUDGE factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): amounts `scramble`/`delay`/`blur`/`stretch`, `dfb` feedback,
//! `cdepth`, `mix` are 0..1; `srange`/`dtilt`/`btilt` are −1..1 (srange 0..1); `srate`/`crate`
//! are frames (int); `btau` ms; `sfactor` 0.5..2.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Light temporal blur + a touch of slow scramble — a soft, drifting veil.
    r#"{ "name": "Gentle Haze", "scramble": 0.15, "srange": 0.3, "srate": 8,
         "blur": 0.4, "btau": 300.0, "mix": 0.4 }"#,
    // Fast bin-shuffle + short bright spectral delays — a shimmering scatter of frequencies.
    r#"{ "name": "Frequency Rain", "scramble": 0.7, "srange": 0.6, "srate": 2,
         "delay": 0.4, "dtilt": 0.8, "dfb": 0.5, "mix": 0.6 }"#,
    // Heavy magnitude blur + gentle upward stretch — smears transients into a wash.
    r#"{ "name": "Time Smear", "blur": 0.9, "btau": 800.0, "btilt": 0.3,
         "stretch": 0.4, "sfactor": 1.3, "mix": 0.7 }"#,
    // All four ops moving under a deep, slow sample-and-hold chaos macro.
    r#"{ "name": "Chaos Engine", "scramble": 0.6, "srange": 0.5, "srate": 4,
         "delay": 0.6, "dtilt": 0.2, "dfb": 0.6, "blur": 0.5, "btau": 250.0,
         "stretch": 0.5, "sfactor": 1.5, "crate": 8, "cdepth": 0.8, "mix": 0.8 }"#,
    // Near-total blur with very long τ + high-feedback delay — a frozen spectral cloud.
    r#"{ "name": "Frozen Blur", "blur": 1.0, "btau": 2000.0,
         "delay": 0.5, "dfb": 0.85, "dtilt": 0.0, "mix": 0.8 }"#,
    // Long high-band spectral delays with strong feedback — cascading spectral echoes.
    r#"{ "name": "Spectral Echoes", "delay": 0.8, "dtilt": 1.0, "dfb": 0.7,
         "scramble": 0.1, "srange": 0.4, "srate": 6, "mix": 0.6 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        scramble_amt: g("scramble", d.scramble_amt),
        scramble_range: g("srange", d.scramble_range),
        scramble_rate: g("srate", d.scramble_rate as f32).round().max(1.0) as u32,
        delay_amt: g("delay", d.delay_amt),
        delay_tilt: g("dtilt", d.delay_tilt),
        delay_feedback: g("dfb", d.delay_feedback),
        blur_amt: g("blur", d.blur_amt),
        blur_tau_ms: g("btau", d.blur_tau_ms),
        blur_tilt: g("btilt", d.blur_tilt),
        stretch_amt: g("stretch", d.stretch_amt),
        stretch_factor: g("sfactor", d.stretch_factor),
        chaos_rate: g("crate", d.chaos_rate as f32).round().max(1.0) as u32,
        chaos_depth: g("cdepth", d.chaos_depth),
        mix: g("mix", d.mix),
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
            if (s.scramble_amt - d.scramble_amt).abs() > 1e-3 { diffs += 1; }
            if (s.scramble_range - d.scramble_range).abs() > 1e-3 { diffs += 1; }
            if s.scramble_rate != d.scramble_rate { diffs += 1; }
            if (s.delay_amt - d.delay_amt).abs() > 1e-3 { diffs += 1; }
            if (s.delay_tilt - d.delay_tilt).abs() > 1e-3 { diffs += 1; }
            if (s.delay_feedback - d.delay_feedback).abs() > 1e-3 { diffs += 1; }
            if (s.blur_amt - d.blur_amt).abs() > 1e-3 { diffs += 1; }
            if (s.blur_tau_ms - d.blur_tau_ms).abs() > 1e-3 { diffs += 1; }
            if (s.blur_tilt - d.blur_tilt).abs() > 1e-3 { diffs += 1; }
            if (s.stretch_amt - d.stretch_amt).abs() > 1e-3 { diffs += 1; }
            if (s.stretch_factor - d.stretch_factor).abs() > 1e-3 { diffs += 1; }
            if s.chaos_rate != d.chaos_rate { diffs += 1; }
            if (s.chaos_depth - d.chaos_depth).abs() > 1e-3 { diffs += 1; }
            if (s.mix - d.mix).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
