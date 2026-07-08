//! SMUDGE factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain, un-normalized): amounts `scramble`/`delay`/`blur`/`stretch`, `dfb`
//! feedback, `cdepth`, `mix` are 0..1; `srange` 0..1; `dtilt`/`btilt` are −1..1; `srate`/`crate`
//! are frames (int ≥1); `btau` ms (5..2000); `sfactor` 0.5..2. Any omitted key falls back to
//! `Settings::default()`. SMUDGE has no output-gain param — level is bounded by design (each
//! op is energy-normalised, the delay tail is held ≤ recent input energy, and a final wet-path
//! soft-clip guarantees |wet| < 1), so `mix` is the only leveller here.
//!
//! Categories (preset-bar sections, first-appearance menu order):
//!   Veils / Scatter / Echo Fields / Frozen / Chaos / Ruin.
//! Names are purpose-driven and genre-aware (dark techno / atmospheric dnb / Sewerslvt-adjacent
//! spectral decay) — never settings descriptions.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Veils (subtle drifting textures) ---------------------------------
    r#"{ "name": "Gentle Haze", "category": "Veils", "scramble": 0.15, "srange": 0.3,
         "srate": 8, "blur": 0.4, "btau": 300.0, "mix": 0.4 }"#,
    r#"{ "name": "Dust Settling", "category": "Veils", "blur": 0.3, "btau": 500.0,
         "btilt": 0.2, "stretch": 0.1, "sfactor": 1.05, "mix": 0.3 }"#,
    r#"{ "name": "Grey Curtain", "category": "Veils", "scramble": 0.2, "srange": 0.25,
         "srate": 12, "delay": 0.2, "dtilt": -0.3, "dfb": 0.25, "mix": 0.35 }"#,
    // ---- Scatter (bin-shuffle glitch) -------------------------------------
    r#"{ "name": "Frequency Rain", "category": "Scatter", "scramble": 0.7, "srange": 0.6,
         "srate": 2, "delay": 0.4, "dtilt": 0.8, "dfb": 0.5, "mix": 0.6 }"#,
    r#"{ "name": "Bin Shuffle Ritual", "category": "Scatter", "scramble": 0.85, "srange": 0.8,
         "srate": 1, "blur": 0.2, "btau": 150.0, "mix": 0.7 }"#,
    r#"{ "name": "Shattered Glass Choir", "category": "Scatter", "scramble": 0.75, "srange": 0.9,
         "srate": 2, "delay": 0.35, "dtilt": 1.0, "dfb": 0.55, "mix": 0.6 }"#,
    // ---- Echo Fields (spectral delay forward) -----------------------------
    r#"{ "name": "Spectral Echoes", "category": "Echo Fields", "delay": 0.8, "dtilt": 1.0,
         "dfb": 0.7, "scramble": 0.1, "srange": 0.4, "srate": 6, "mix": 0.6 }"#,
    r#"{ "name": "Cathedral of Rust", "category": "Echo Fields", "delay": 0.7, "dtilt": -0.8,
         "dfb": 0.75, "blur": 0.3, "btau": 600.0, "mix": 0.55 }"#,
    r#"{ "name": "Sunken Reverb Cell", "category": "Echo Fields", "delay": 0.6, "dtilt": 0.3,
         "dfb": 0.6, "blur": 0.5, "btau": 400.0, "btilt": -0.4, "mix": 0.6 }"#,
    // ---- Frozen (blur / freeze clouds) ------------------------------------
    r#"{ "name": "Time Smear", "category": "Frozen", "blur": 0.9, "btau": 800.0, "btilt": 0.3,
         "stretch": 0.4, "sfactor": 1.3, "mix": 0.7 }"#,
    r#"{ "name": "Frozen Blur", "category": "Frozen", "blur": 1.0, "btau": 2000.0,
         "delay": 0.5, "dfb": 0.85, "mix": 0.8 }"#,
    r#"{ "name": "Glacier Drift", "category": "Frozen", "blur": 0.95, "btau": 1500.0,
         "btilt": -0.5, "stretch": 0.2, "sfactor": 0.85, "mix": 0.6 }"#,
    // ---- Chaos (sample-and-hold macro moving) -----------------------------
    r#"{ "name": "Chaos Engine", "category": "Chaos", "scramble": 0.6, "srange": 0.5,
         "srate": 4, "delay": 0.6, "dtilt": 0.2, "dfb": 0.6, "blur": 0.5, "btau": 250.0,
         "stretch": 0.5, "sfactor": 1.5, "crate": 8, "cdepth": 0.8, "mix": 0.8 }"#,
    r#"{ "name": "Drifting Apparition", "category": "Chaos", "scramble": 0.3, "srate": 8,
         "delay": 0.4, "dfb": 0.5, "blur": 0.6, "btau": 700.0, "stretch": 0.3, "sfactor": 1.2,
         "crate": 32, "cdepth": 0.5, "mix": 0.6 }"#,
    r#"{ "name": "Possession Macro", "category": "Chaos", "scramble": 0.7, "srange": 0.7,
         "srate": 2, "delay": 0.7, "dtilt": -0.5, "dfb": 0.7, "blur": 0.6, "stretch": 0.6,
         "sfactor": 1.6, "crate": 4, "cdepth": 1.0, "mix": 0.85 }"#,
    // ---- Ruin (extreme / destructive) -------------------------------------
    r#"{ "name": "Total Dissolution", "category": "Ruin", "scramble": 1.0, "srange": 1.0,
         "srate": 1, "delay": 1.0, "dtilt": 1.0, "dfb": 0.95, "blur": 1.0, "btau": 2000.0,
         "btilt": 1.0, "stretch": 1.0, "sfactor": 2.0, "crate": 1, "cdepth": 1.0, "mix": 1.0 }"#,
    r#"{ "name": "Nyquist Wraith", "category": "Ruin", "scramble": 0.9, "srange": 0.85,
         "srate": 1, "delay": 0.5, "dfb": 0.6, "stretch": 0.9, "sfactor": 2.0, "mix": 0.9 }"#,
    r#"{ "name": "Downward Spiral", "category": "Ruin", "scramble": 0.5, "delay": 0.8,
         "dtilt": -1.0, "dfb": 0.9, "blur": 0.8, "btau": 1200.0, "stretch": 0.85, "sfactor": 0.5,
         "crate": 16, "cdepth": 0.7, "mix": 0.9 }"#,
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

    /// Count how many `Settings` fields differ between two presets (the two integer rates by
    /// equality, all floats by a loose epsilon). Drives both the differ-from-default and
    /// pairwise-distinctness quality gates. SMUDGE has no enum/bool/fixed-constant fields, so
    /// every one of the 14 params `settings_from_preset` sets is compared.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.scramble_rate != b.scramble_rate {
            n += 1;
        }
        if a.chaos_rate != b.chaos_rate {
            n += 1;
        }
        let fs = [
            (a.scramble_amt, b.scramble_amt),
            (a.scramble_range, b.scramble_range),
            (a.delay_amt, b.delay_amt),
            (a.delay_tilt, b.delay_tilt),
            (a.delay_feedback, b.delay_feedback),
            (a.blur_amt, b.blur_amt),
            (a.blur_tau_ms, b.blur_tau_ms),
            (a.blur_tilt, b.blur_tilt),
            (a.stretch_amt, b.stretch_amt),
            (a.stretch_factor, b.stretch_factor),
            (a.chaos_depth, b.chaos_depth),
            (a.mix, b.mix),
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
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 15, "SMUDGE bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the default
        // in >= 4 params, and every preset is categorised.
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
