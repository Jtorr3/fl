//! EMBER factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//! Values are plain (un-normalized): ms for the factor-band time constants, dB for
//! gate/tail gain, 0..1 for fitting/mix/freeze-mix, 0/1 for freeze.
//!
//! Per-band curves may be authored two ways: a scalar `"attack"` / `"decay"` fills all 8
//! bands uniformly; per-band overrides `"atk0".."atk7"` / `"dec0".."dec7"` (low→high
//! frequency) take precedence for a frequency-dependent curve.
//!
//! Categories (preset-bar sections): Pads / Fades / Freezes / Rhythmic / Textures. Names
//! are purpose-driven and genre-aware (dark techno / atmospheric dnb / dungeon-ambient) —
//! never settings descriptions.

use crate::dsp::{Settings, N_BANDS};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Pads: slow bloom, medium-long tails, blended --------------------------------
    // Slow bloom + medium tail, gentle envelope glue, blended.
    r#"{ "name": "Bloom Pad", "category": "Pads", "attack": 160.0, "decay": 2500.0,
         "fitting": 0.3, "freeze": 0, "gate": -60.0, "tailgain": 1.5, "mix": 0.8 }"#,
    r#"{ "name": "Last Train Home", "category": "Pads", "attack": 220.0, "decay": 3500.0,
         "fitting": 0.4, "freeze": 0, "gate": -62.0, "tailgain": 2.0, "mix": 0.7 }"#,
    r#"{ "name": "Velvet Dusk", "category": "Pads", "attack": 300.0, "decay": 4200.0,
         "fitting": 0.5, "freeze": 0, "gate": -58.0, "tailgain": 1.0, "mix": 0.65 }"#,
    r#"{ "name": "Grief Wash", "category": "Pads", "attack": 400.0, "decay": 6000.0,
         "fitting": 0.6, "freeze": 0, "gate": -64.0, "tailgain": 2.5, "mix": 0.6 }"#,
    r#"{ "name": "Slow Bloom Choir", "category": "Pads", "attack": 520.0, "decay": 5000.0,
         "fitting": 0.55, "freeze": 0, "gate": -60.0, "tailgain": 1.8, "mix": 0.55 }"#,
    // ---- Fades: fast attack, per-band decay curves, mostly wet ------------------------
    // Fast attack; lows fade quickly, highs shimmer on — a spectral gate-fade.
    r#"{ "name": "Spectral Gate-Fade", "category": "Fades", "attack": 3.0,
         "dec0": 120.0, "dec1": 160.0, "dec2": 220.0, "dec3": 350.0,
         "dec4": 700.0, "dec5": 1500.0, "dec6": 3500.0, "dec7": 6000.0,
         "fitting": 0.15, "freeze": 0, "gate": -42.0, "tailgain": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Highs Shimmer On", "category": "Fades", "attack": 4.0,
         "dec0": 100.0, "dec1": 150.0, "dec2": 300.0, "dec3": 600.0,
         "dec4": 1200.0, "dec5": 3000.0, "dec6": 8000.0, "dec7": 15000.0,
         "fitting": 0.2, "freeze": 0, "gate": -46.0, "tailgain": 3.0, "mix": 1.0 }"#,
    r#"{ "name": "Lowlight Fade", "category": "Fades", "attack": 6.0,
         "dec0": 8000.0, "dec1": 5000.0, "dec2": 2500.0, "dec3": 1200.0,
         "dec4": 600.0, "dec5": 300.0, "dec6": 150.0, "dec7": 80.0,
         "fitting": 0.1, "freeze": 0, "gate": -50.0, "tailgain": -3.0, "mix": 0.9 }"#,
    r#"{ "name": "Ash Falls Slow", "category": "Fades", "attack": 8.0,
         "dec0": 400.0, "dec1": 600.0, "dec2": 900.0, "dec3": 1400.0,
         "dec4": 2200.0, "dec5": 3500.0, "dec6": 5500.0, "dec7": 9000.0,
         "fitting": 0.25, "freeze": 0, "gate": -54.0, "tailgain": 1.0, "mix": 0.85 }"#,
    // ---- Freezes: hold the captured spectrum as a drone ------------------------------
    // Freeze the captured spectrum into a held drone.
    r#"{ "name": "Freeze Drone", "category": "Freezes", "attack": 8.0, "decay": 8000.0,
         "fitting": 0.2, "freeze": 1, "gate": -60.0, "tailgain": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Afterlife Wide", "category": "Freezes", "attack": 5.0, "decay": 12000.0,
         "fitting": 0.4, "freeze": 1, "freezemix": 0.9, "gate": -58.0, "tailgain": 2.0, "mix": 1.0 }"#,
    r#"{ "name": "Drowned Ghost Sit", "category": "Freezes", "attack": 12.0, "decay": 20000.0,
         "fitting": 0.3, "freeze": 1, "freezemix": 0.7, "gate": -62.0, "tailgain": 0.0, "mix": 0.9 }"#,
    r#"{ "name": "Held Breath", "category": "Freezes", "attack": 6.0, "decay": 10000.0,
         "fitting": 0.15, "freeze": 1, "freezemix": 0.5, "gate": -56.0, "tailgain": -2.0, "mix": 1.0 }"#,
    r#"{ "name": "Cathedral Freeze", "category": "Freezes", "attack": 24.0, "decay": 30000.0,
         "fitting": 0.5, "freeze": 1, "gate": -60.0, "tailgain": 2.5, "mix": 1.0 }"#,
    // ---- Rhythmic: shorter curves, gated, spectral smear on drums / breaks ------------
    // Heavy envelope fitting for a smeared "glue" wash.
    r#"{ "name": "Fitting Glue", "category": "Rhythmic", "attack": 25.0, "decay": 600.0,
         "fitting": 0.85, "freeze": 0, "gate": -58.0, "tailgain": 0.0, "mix": 0.55 }"#,
    r#"{ "name": "Pulse Smear", "category": "Rhythmic", "attack": 10.0, "decay": 300.0,
         "fitting": 0.5, "freeze": 0, "gate": -50.0, "tailgain": 0.0, "mix": 0.5 }"#,
    r#"{ "name": "Break Ghosting", "category": "Rhythmic", "attack": 5.0, "decay": 450.0,
         "fitting": 0.4, "freeze": 0, "gate": -48.0, "tailgain": 1.0, "mix": 0.45 }"#,
    r#"{ "name": "Rolling Tail DnB", "category": "Rhythmic", "attack": 8.0, "decay": 900.0,
         "fitting": 0.35, "freeze": 0, "gate": -52.0, "tailgain": 2.0, "mix": 0.5 }"#,
    r#"{ "name": "Stutter Bloom", "category": "Rhythmic", "attack": 15.0, "decay": 220.0,
         "fitting": 0.6, "freeze": 0, "gate": -46.0, "tailgain": -1.0, "mix": 0.6 }"#,
    // ---- Textures: extreme washes, heavy fitting, endless decay -----------------------
    // Very long tails that wash out over many seconds, boosted slightly, mostly wet.
    r#"{ "name": "Infinite Wash", "category": "Textures", "attack": 40.0, "decay": 45000.0,
         "fitting": 0.5, "freeze": 0, "gate": -66.0, "tailgain": 3.0, "mix": 0.75 }"#,
    r#"{ "name": "Endless Corridor", "category": "Textures", "attack": 30.0, "decay": 60000.0,
         "fitting": 0.7, "freeze": 0, "gate": -70.0, "tailgain": 3.5, "mix": 0.7 }"#,
    r#"{ "name": "Rust Cathedral", "category": "Textures", "attack": 60.0, "decay": 50000.0,
         "fitting": 0.8, "freeze": 0, "gate": -64.0, "tailgain": 2.0, "mix": 0.65 }"#,
    r#"{ "name": "Sewer Bloom", "category": "Textures", "attack": 80.0, "decay": 40000.0,
         "fitting": 0.9, "freeze": 0, "gate": -60.0, "tailgain": 1.0, "mix": 0.6 }"#,
    r#"{ "name": "Total Spectral Collapse", "category": "Textures", "attack": 1.0, "decay": 55000.0,
         "fitting": 1.0, "freeze": 0, "gate": -74.0, "tailgain": 4.0, "mix": 0.85 }"#,
];

/// Fill an 8-band array from a preset: per-band `prefixN` overrides, else a uniform
/// `scalar_key`, else the provided default array.
pub fn band_from_preset(
    p: &Preset,
    prefix: &str,
    scalar_key: &str,
    default: &[f32; N_BANDS],
) -> [f32; N_BANDS] {
    let uniform = p.get(scalar_key);
    let mut out = *default;
    for (j, slot) in out.iter_mut().enumerate() {
        if let Some(v) = p.get(&format!("{prefix}{j}")) {
            *slot = v;
        } else if let Some(v) = uniform {
            *slot = v;
        }
    }
    out
}

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        attack_ms: band_from_preset(p, "atk", "attack", &d.attack_ms),
        decay_ms: band_from_preset(p, "dec", "decay", &d.decay_ms),
        fitting: g("fitting", d.fitting),
        freeze: g("freeze", 0.0) >= 0.5,
        freeze_mix: g("freezemix", d.freeze_mix),
        gate_db: g("gate", d.gate_db),
        tail_gain_db: g("tailgain", d.tail_gain_db),
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
        assert!(presets.len() >= 5, "need >= 5 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.attack_ms[0] - d.attack_ms[0]).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.decay_ms[0] - d.decay_ms[0]).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.fitting - d.fitting).abs() > 1e-3 {
                diffs += 1;
            }
            if s.freeze != d.freeze {
                diffs += 1;
            }
            if (s.gate_db - d.gate_db).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.tail_gain_db - d.tail_gain_db).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }

    /// Count how many `Settings` "controls" differ between two presets. Each 8-band curve
    /// (attack / decay) counts as ONE control (differs if any band differs); bool `freeze`
    /// by equality; the remaining scalars by a loose float epsilon. Drives both the
    /// differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.freeze != b.freeze {
            n += 1;
        }
        let arr_diff = |x: &[f32; N_BANDS], y: &[f32; N_BANDS]| {
            x.iter().zip(y.iter()).any(|(p, q)| (p - q).abs() > 1e-3)
        };
        if arr_diff(&a.attack_ms, &b.attack_ms) {
            n += 1;
        }
        if arr_diff(&a.decay_ms, &b.decay_ms) {
            n += 1;
        }
        let fs = [
            (a.fitting, b.fitting),
            (a.freeze_mix, b.freeze_mix),
            (a.gate_db, b.gate_db),
            (a.tail_gain_db, b.tail_gain_db),
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
        assert!(presets.len() >= 15, "EMBER bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the
        // default in >= 4 controls, and every preset is categorised.
        for (p, s) in presets.iter().zip(&settings) {
            assert!(p.category.is_some(), "preset '{}' has no category", p.name);
            let diffs = count_diffs(s, &d);
            assert!(diffs >= 4, "preset '{}' differs from default in only {diffs} controls", p.name);
        }

        // Rule 3 (no near-duplicates): every preset differs from EVERY other in >= 2.
        for i in 0..settings.len() {
            for j in (i + 1)..settings.len() {
                let diffs = count_diffs(&settings[i], &settings[j]);
                assert!(
                    diffs >= 2,
                    "presets '{}' and '{}' differ in only {diffs} controls (near-duplicate)",
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
