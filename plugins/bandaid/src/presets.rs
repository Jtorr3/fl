//! BANDAID factory presets (SPECS "PRESET-EXPANSION" bank). Each is an embedded flat-JSON
//! blob parsed by `suite_core::presets`. The same list drives the GUI selector (grouped by
//! the `"category"` tag into preset-bar sections) and the offline render tests. Presets set
//! the crossover + per-band attack/sustain shaping; per-band solo is a live audition toggle
//! and is never stored (like a listen button).
//!
//! Value encodings (plain numeric): `xlow`/`xhigh` Hz (LR4 low↔mid / mid↔high crossovers);
//! `latk`/`lsus`/`matk`/`msus`/`hatk`/`hsus` per-band attack/sustain gain (dB, ±12);
//! `det` detector time scale (0.25..4, 1.0 = 1 ms fast / 50 ms slow); `mix` 0..1; `out` dB.
//!
//! Categories (preset-bar sections, first-appearance menu order): Drums/Kick / Bus-Snap /
//! Softeners / Pad-Bloom / Extreme. Names are purpose-driven and genre-aware (dark-techno /
//! atmospheric-dnb) — never settings descriptions. Boosts stay moderate and `out` goes
//! negative on the aggressive presets so every render leaves headroom under 0 dBFS.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Drums/Kick -------------------------------------------------------
    r#"{ "name": "Warehouse Kick Punch", "category": "Drums/Kick",
         "xlow": 110.0, "xhigh": 2400.0,
         "latk": 6.0, "lsus": -2.0, "matk": 3.0, "msus": 0.0, "hatk": 1.0, "hsus": -1.0,
         "det": 0.9, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Basement Sub Thud", "category": "Drums/Kick",
         "xlow": 90.0, "xhigh": 1800.0,
         "latk": 5.0, "lsus": 1.0, "matk": 1.0, "msus": -2.0, "hatk": 0.0, "hsus": -3.0,
         "det": 1.3, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Break Snap Reglue", "category": "Drums/Kick",
         "xlow": 150.0, "xhigh": 3200.0,
         "latk": 3.0, "lsus": -3.0, "matk": 5.0, "msus": -4.0, "hatk": 4.0, "hsus": -2.0,
         "det": 0.7, "mix": 1.0, "out": -0.5 }"#,
    // Hardest attack-boost preset — a tight, hot slam kick that pushes the onset well over
    // 0 dBFS float on a real bus to prove the boosted transient PUNCHES (crest up) instead of
    // digitally clipping. Tight low (lsus −4, no boom) + present top (hatk +5) keep it clean.
    r#"{ "name": "Peak Slam Kick", "category": "Drums/Kick",
         "xlow": 95.0, "xhigh": 2600.0,
         "latk": 8.0, "lsus": -4.0, "matk": 5.0, "msus": -2.0, "hatk": 5.0, "hsus": -1.0,
         "det": 0.65, "mix": 1.0, "out": -1.5 }"#,
    // ---- Bus-Snap ---------------------------------------------------------
    r#"{ "name": "Drum Bus Snap", "category": "Bus-Snap",
         "xlow": 140.0, "xhigh": 3000.0,
         "latk": 4.0, "lsus": -2.0, "matk": 5.0, "msus": -3.0, "hatk": 4.0, "hsus": -2.0,
         "det": 0.9, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Tighter Room", "category": "Bus-Snap",
         "xlow": 180.0, "xhigh": 2200.0,
         "latk": 0.0, "lsus": -4.0, "matk": 1.0, "msus": -6.0, "hatk": 0.0, "hsus": -5.0,
         "det": 1.2, "mix": 1.0, "out": 0.5 }"#,
    r#"{ "name": "Parallel Glue Slam", "category": "Bus-Snap",
         "xlow": 160.0, "xhigh": 2600.0,
         "latk": 3.0, "lsus": -1.0, "matk": 3.0, "msus": -2.0, "hatk": 2.0, "hsus": -1.0,
         "det": 1.0, "mix": 0.6, "out": 0.0 }"#,
    // ---- Softeners --------------------------------------------------------
    r#"{ "name": "Soften Hats", "category": "Softeners",
         "xlow": 200.0, "xhigh": 4000.0,
         "latk": 0.0, "lsus": 0.0, "matk": -2.0, "msus": 1.0, "hatk": -7.0, "hsus": 2.0,
         "det": 0.8, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "De-Click Foley", "category": "Softeners",
         "xlow": 250.0, "xhigh": 5000.0,
         "latk": -4.0, "lsus": 1.0, "matk": -5.0, "msus": 2.0, "hatk": -6.0, "hsus": 1.0,
         "det": 0.7, "mix": 1.0, "out": 0.5 }"#,
    r#"{ "name": "Velvet Snare Tame", "category": "Softeners",
         "xlow": 180.0, "xhigh": 2800.0,
         "latk": -1.0, "lsus": 0.0, "matk": -6.0, "msus": 2.0, "hatk": -4.0, "hsus": 1.0,
         "det": 0.85, "mix": 0.8, "out": 0.0 }"#,
    // ---- Pad-Bloom --------------------------------------------------------
    r#"{ "name": "Pad Bloom", "category": "Pad-Bloom",
         "xlow": 250.0, "xhigh": 2000.0,
         "latk": -2.0, "lsus": 3.0, "matk": -3.0, "msus": 6.0, "hatk": -2.0, "hsus": 5.0,
         "det": 1.5, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Atmos DnB Wash", "category": "Pad-Bloom",
         "xlow": 220.0, "xhigh": 2300.0,
         "latk": -1.0, "lsus": 4.0, "matk": -2.0, "msus": 5.0, "hatk": -3.0, "hsus": 6.0,
         "det": 2.0, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Sewer Reverb Bloom", "category": "Pad-Bloom",
         "xlow": 300.0, "xhigh": 1600.0,
         "latk": -3.0, "lsus": 5.0, "matk": -4.0, "msus": 7.0, "hatk": -1.0, "hsus": 4.0,
         "det": 2.5, "mix": 1.0, "out": -2.0 }"#,
    // ---- Extreme ----------------------------------------------------------
    // (Pruned "Full Squash-Reverse" — its all-band attack-down/sustain-up squash was a
    // near-duplicate of "Inverted Timestretch" and rendered muddy with crest *below* the dry;
    // the reference bar wants tight punchy transients, so the redundant softener was cut.)
    r#"{ "name": "Gated Void Slam", "category": "Extreme",
         "xlow": 130.0, "xhigh": 2900.0,
         "latk": 9.0, "lsus": -12.0, "matk": 8.0, "msus": -12.0, "hatk": 7.0, "hsus": -11.0,
         "det": 0.5, "mix": 1.0, "out": -3.0 }"#,
    r#"{ "name": "Inverted Timestretch", "category": "Extreme",
         "xlow": 200.0, "xhigh": 2100.0,
         "latk": -12.0, "lsus": 8.0, "matk": -11.0, "msus": 10.0, "hatk": -12.0, "hsus": 9.0,
         "det": 3.0, "mix": 1.0, "out": -2.5 }"#,
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

    /// Count how many `Settings` fields differ between two presets (all params
    /// `settings_from_preset` sets; floats by a loose epsilon). Solo is a live audition
    /// toggle, never stored, so it is excluded. Drives both the differ-from-default and
    /// pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        let fs = [
            (a.xover_low, b.xover_low),
            (a.xover_high, b.xover_high),
            (a.attack_db[0], b.attack_db[0]),
            (a.attack_db[1], b.attack_db[1]),
            (a.attack_db[2], b.attack_db[2]),
            (a.sustain_db[0], b.sustain_db[0]),
            (a.sustain_db[1], b.sustain_db[1]),
            (a.sustain_db[2], b.sustain_db[2]),
            (a.det_scale, b.det_scale),
            (a.mix, b.mix),
            (a.out_db, b.out_db),
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
        // Curated bank (SOUND-PASS): 15 presets across 5 categories after pruning one
        // redundant Extreme softener and adding a hard-punch "Peak Slam Kick".
        assert!(presets.len() >= 14, "BANDAID bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the
        // default in >= 4 params, and every preset is categorised.
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
        // `presets_pass_universal_assertions` test in tests.rs.
    }
}
