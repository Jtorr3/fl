//! EMBER factory presets (SPECS list). Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`. The same list drives the GUI selector and the offline render
//! tests. Values are plain (un-normalized): ms for the factor-band time constants, dB for
//! gate/tail gain, 0..1 for fitting/mix, 0/1 for freeze.
//!
//! Per-band curves may be authored two ways: a scalar `"attack"` / `"decay"` fills all 8
//! bands uniformly; per-band overrides `"atk0".."atk7"` / `"dec0".."dec7"` (low→high
//! frequency) take precedence for a frequency-dependent curve.

use crate::dsp::{Settings, N_BANDS};
use suite_core::presets::Preset;

/// The five factory presets, in menu order (SPECS: bloom pad, infinite wash, freeze drone,
/// spectral gate-fade, fitting-glue).
pub const PRESET_JSON: &[&str] = &[
    // Slow bloom + medium tail, gentle envelope glue, blended.
    r#"{ "name": "Bloom Pad", "attack": 160.0, "decay": 2500.0,
         "fitting": 0.3, "freeze": 0, "gate": -60.0, "tailgain": 1.5, "mix": 0.8 }"#,
    // Very long tails that wash out over many seconds, boosted slightly, mostly wet.
    r#"{ "name": "Infinite Wash", "attack": 40.0, "decay": 45000.0,
         "fitting": 0.5, "freeze": 0, "gate": -66.0, "tailgain": 3.0, "mix": 0.75 }"#,
    // Freeze the captured spectrum into a held drone.
    r#"{ "name": "Freeze Drone", "attack": 8.0, "decay": 8000.0,
         "fitting": 0.2, "freeze": 1, "gate": -60.0, "tailgain": 0.0, "mix": 1.0 }"#,
    // Fast attack; lows fade quickly, highs shimmer on — a spectral gate-fade.
    r#"{ "name": "Spectral Gate-Fade", "attack": 3.0,
         "dec0": 120.0, "dec1": 160.0, "dec2": 220.0, "dec3": 350.0,
         "dec4": 700.0, "dec5": 1500.0, "dec6": 3500.0, "dec7": 6000.0,
         "fitting": 0.15, "freeze": 0, "gate": -42.0, "tailgain": 0.0, "mix": 1.0 }"#,
    // Heavy envelope fitting for a smeared "glue" wash.
    r#"{ "name": "Fitting Glue", "attack": 25.0, "decay": 600.0,
         "fitting": 0.85, "freeze": 0, "gate": -58.0, "tailgain": 0.0, "mix": 0.55 }"#,
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
}
