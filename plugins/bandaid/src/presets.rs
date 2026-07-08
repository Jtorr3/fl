//! BANDAID factory presets — flat-JSON blobs parsed by `suite_core::presets`. The same list
//! drives the GUI selector and the offline render tests. Presets set the crossover +
//! per-band attack/sustain shaping; per-band solo is a live audition toggle and is never
//! stored (like a listen button).
//!
//! Value encodings (plain numeric): `xlow`/`xhigh` Hz; `latk`/`lsus`/`matk`/`msus`/`hatk`/`hsus`
//! per-band attack/sustain dB (±12); `det` detector time scale; `mix` 0..1; `out` dB.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order (≥6): Punchier Kick, Tighter Room, Soften Hats,
/// Drum Bus Snap, Pad Bloom, Full Squash-Reverse.
pub const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Punchier Kick", "category": "KICK",
         "xlow": 120.0, "xhigh": 2500.0,
         "latk": 6.0, "lsus": -2.0, "matk": 3.0, "msus": 0.0, "hatk": 2.0, "hsus": -1.0,
         "det": 1.0, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Tighter Room", "category": "BUS",
         "xlow": 180.0, "xhigh": 2200.0,
         "latk": 0.0, "lsus": -4.0, "matk": 1.0, "msus": -6.0, "hatk": 0.0, "hsus": -5.0,
         "det": 1.2, "mix": 1.0, "out": 1.0 }"#,
    r#"{ "name": "Soften Hats", "category": "HATS",
         "xlow": 200.0, "xhigh": 4000.0,
         "latk": 0.0, "lsus": 0.0, "matk": -2.0, "msus": 1.0, "hatk": -7.0, "hsus": 2.0,
         "det": 0.8, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Drum Bus Snap", "category": "BUS",
         "xlow": 140.0, "xhigh": 3000.0,
         "latk": 4.0, "lsus": -2.0, "matk": 5.0, "msus": -3.0, "hatk": 4.0, "hsus": -2.0,
         "det": 0.9, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Pad Bloom", "category": "PAD",
         "xlow": 250.0, "xhigh": 2000.0,
         "latk": -2.0, "lsus": 3.0, "matk": -3.0, "msus": 6.0, "hatk": -2.0, "hsus": 5.0,
         "det": 1.5, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Full Squash-Reverse", "category": "FX",
         "xlow": 160.0, "xhigh": 2600.0,
         "latk": -9.0, "lsus": 9.0, "matk": -9.0, "msus": 9.0, "hatk": -9.0, "hsus": 9.0,
         "det": 1.0, "mix": 1.0, "out": -2.0 }"#,
];

/// Build the DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        xover_low: g("xlow", d.xover_low),
        xover_high: g("xhigh", d.xover_high),
        attack_db: [g("latk", 0.0), g("matk", 0.0), g("hatk", 0.0)],
        sustain_db: [g("lsus", 0.0), g("msus", 0.0), g("hsus", 0.0)],
        solo: [false; 3],
        det_scale: g("det", d.det_scale),
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
        assert!(presets.len() >= 6, "want >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            // At least the shaping should differ from the flat default in several params.
            let mut diffs = 0;
            for b in 0..3 {
                if (s.attack_db[b] - d.attack_db[b]).abs() > 0.01 {
                    diffs += 1;
                }
                if (s.sustain_db[b] - d.sustain_db[b]).abs() > 0.01 {
                    diffs += 1;
                }
            }
            if (s.xover_low - d.xover_low).abs() > 0.01 {
                diffs += 1;
            }
            if (s.xover_high - d.xover_high).abs() > 0.01 {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs from default in only {diffs} params", p.name);
        }
    }
}
