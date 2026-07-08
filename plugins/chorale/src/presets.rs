//! CHORALE factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`; the same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests
//! (via [`settings_from_preset`]).
//!
//! Value encodings (plain, un-normalized): `source` 0/1/2 (Scale/MIDI/KeyDetect); `root`
//! 0..11 (C..B); `scale` 0..11 (mTriad/MTriad/m7/M7/sus2/sus4/5th/mPent/MPent/phryg/dorian/
//! octaves); `count` 12..24; `decay`/`damp`/`sympathetic`/`stereo`/`mix` 0..1; `spread` in
//! cents 0..50; `excite` linear 0..2; `wetsolo` 0/1; `out` dB (kept ≤ 0 for headroom).
//!
//! Categories (preset-bar sections): Drone / Choir / Body / Spectral / Extreme. Names are
//! purpose-driven and genre-aware (dark techno / atmospheric dnb / Cynthoni-Sewerslvt) —
//! never settings descriptions.

use crate::dsp::{Scale, Settings, TuningSource, MAX_RESONATORS};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Drone ------------------------------------------------------------
    // The reference sound: an A-minor bank singing sympathetically under the source.
    r#"{ "name": "Sympathetic Am", "category": "Drone",
         "source": 0, "root": 9, "scale": 0, "count": 18, "decay": 0.9, "damp": 0.35,
         "spread": 7.0, "sympathetic": 0.8, "excite": 1.0, "stereo": 0.6,
         "wetsolo": 0, "mix": 0.55, "out": 0.0 }"#,
    // Dark, slow Phrygian drone bed on E — long decay, damped, wide.
    r#"{ "name": "Phrygian Drone Bed", "category": "Drone",
         "source": 0, "root": 4, "scale": 9, "count": 20, "decay": 0.96, "damp": 0.62,
         "spread": 8.0, "sympathetic": 0.4, "excite": 0.9, "stereo": 0.75,
         "wetsolo": 0, "mix": 0.65, "out": -1.0 }"#,
    // Deep octave-stacked sub resonance on C — narrow, dark, powerful.
    r#"{ "name": "Sub Resonance", "category": "Drone",
         "source": 0, "root": 0, "scale": 11, "count": 12, "decay": 0.93, "damp": 0.7,
         "spread": 2.0, "sympathetic": 0.5, "excite": 1.1, "stereo": 0.3,
         "wetsolo": 0, "mix": 0.5, "out": 0.0 }"#,
    // Sunken minor-7 tar on D# — dense bank, heavy damping, slow crawl.
    r#"{ "name": "Black Tar Undertow", "category": "Drone",
         "source": 0, "root": 3, "scale": 2, "count": 22, "decay": 0.94, "damp": 0.75,
         "spread": 4.0, "sympathetic": 0.3, "excite": 1.0, "stereo": 0.45,
         "wetsolo": 0, "mix": 0.6, "out": -1.5 }"#,
    // ---- Choir ------------------------------------------------------------
    // Bright, ringing major-7 glass — full bank, low damping, big spread + stereo.
    r#"{ "name": "Glass Choir", "category": "Choir",
         "source": 0, "root": 0, "scale": 3, "count": 24, "decay": 0.85, "damp": 0.15,
         "spread": 11.0, "sympathetic": 0.6, "excite": 1.0, "stereo": 0.85,
         "wetsolo": 0, "mix": 0.55, "out": -1.5 }"#,
    // Lush, heavily-detuned sus2 shimmer — full bank, huge spread, very wide.
    r#"{ "name": "Wide Shimmer Strings", "category": "Choir",
         "source": 0, "root": 2, "scale": 4, "count": 24, "decay": 0.82, "damp": 0.3,
         "spread": 24.0, "sympathetic": 0.5, "excite": 1.0, "stereo": 0.95,
         "wetsolo": 0, "mix": 0.6, "out": -1.0 }"#,
    // Airy major-pentatonic frost on G — low damp, wide, gently spread.
    r#"{ "name": "Cathedral Frost", "category": "Choir",
         "source": 0, "root": 7, "scale": 8, "count": 22, "decay": 0.9, "damp": 0.22,
         "spread": 14.0, "sympathetic": 0.55, "excite": 0.95, "stereo": 0.9,
         "wetsolo": 0, "mix": 0.5, "out": -1.5 }"#,
    // Weightless major-triad shimmer on B — top-end sheen, hushed mix.
    r#"{ "name": "Angel Static", "category": "Choir",
         "source": 0, "root": 11, "scale": 1, "count": 24, "decay": 0.88, "damp": 0.18,
         "spread": 16.0, "sympathetic": 0.65, "excite": 1.0, "stereo": 0.8,
         "wetsolo": 0, "mix": 0.52, "out": -2.0 }"#,
    // ---- Body -------------------------------------------------------------
    // Short, resonant power-5 body on C — fast decay, tighter, wet-forward.
    r#"{ "name": "Tight Body", "category": "Body",
         "source": 0, "root": 0, "scale": 6, "count": 14, "decay": 0.38, "damp": 0.45,
         "spread": 3.0, "sympathetic": 0.7, "excite": 1.3, "stereo": 0.4,
         "wetsolo": 0, "mix": 0.7, "out": 0.0 }"#,
    // Woody sus4 knock on F — medium tail, mono-ish, adds shell to percussion.
    r#"{ "name": "Knock Chamber", "category": "Body",
         "source": 0, "root": 5, "scale": 5, "count": 14, "decay": 0.5, "damp": 0.5,
         "spread": 3.0, "sympathetic": 0.65, "excite": 1.2, "stereo": 0.5,
         "wetsolo": 0, "mix": 0.6, "out": 0.0 }"#,
    // Snappy power-5 rim resonance on D — very short, narrow, punchy.
    r#"{ "name": "Rimlock Resonance", "category": "Body",
         "source": 0, "root": 2, "scale": 6, "count": 12, "decay": 0.3, "damp": 0.55,
         "spread": 2.0, "sympathetic": 0.75, "excite": 1.4, "stereo": 0.35,
         "wetsolo": 0, "mix": 0.65, "out": -0.5 }"#,
    // ---- Spectral (key-detect / high sympathetic — the bank tracks the source) ----
    // Key-detect wash that sings only where the input has energy — full sympathetic.
    r#"{ "name": "Ghost In The Signal", "category": "Spectral",
         "source": 2, "root": 9, "scale": 0, "count": 18, "decay": 0.9, "damp": 0.4,
         "spread": 6.0, "sympathetic": 1.0, "excite": 1.1, "stereo": 0.65,
         "wetsolo": 0, "mix": 0.5, "out": -1.0 }"#,
    // Murky key-tracked choir, Phrygian fallback on E — damped, wide, haunted.
    r#"{ "name": "Sewer Choir", "category": "Spectral",
         "source": 2, "root": 4, "scale": 9, "count": 20, "decay": 0.93, "damp": 0.68,
         "spread": 9.0, "sympathetic": 0.9, "excite": 1.0, "stereo": 0.7,
         "wetsolo": 0, "mix": 0.6, "out": -1.5 }"#,
    // Dorian bank on G that resonates the room around the source — full sympathetic.
    r#"{ "name": "Sing The Room", "category": "Spectral",
         "source": 0, "root": 7, "scale": 10, "count": 20, "decay": 0.88, "damp": 0.35,
         "spread": 5.0, "sympathetic": 1.0, "excite": 1.1, "stereo": 0.6,
         "wetsolo": 0, "mix": 0.5, "out": -0.5 }"#,
    // Bright key-detect major wash on C — tracks the mix, wide and glassy.
    r#"{ "name": "Chromagram Wash", "category": "Spectral",
         "source": 2, "root": 0, "scale": 1, "count": 22, "decay": 0.91, "damp": 0.28,
         "spread": 7.0, "sympathetic": 0.85, "excite": 1.0, "stereo": 0.8,
         "wetsolo": 0, "mix": 0.55, "out": -1.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Wall-of-resonance wet solo, minor-7 on E — full bank, long, hard-panned.
    r#"{ "name": "Total Immersion", "category": "Extreme",
         "source": 0, "root": 4, "scale": 2, "count": 24, "decay": 0.97, "damp": 0.2,
         "spread": 20.0, "sympathetic": 0.6, "excite": 1.5, "stereo": 1.0,
         "wetsolo": 1, "mix": 1.0, "out": -2.0 }"#,
    // Screaming major-7 on B at maximum spread — bright, huge, unstable-sounding.
    r#"{ "name": "Nyquist Cathedral", "category": "Extreme",
         "source": 0, "root": 11, "scale": 3, "count": 24, "decay": 0.95, "damp": 0.12,
         "spread": 30.0, "sympathetic": 0.7, "excite": 1.4, "stereo": 0.95,
         "wetsolo": 0, "mix": 0.7, "out": -2.0 }"#,
    // Bottomless octave drone on C, near-max decay — wet solo, endless tail.
    r#"{ "name": "Decay Into Void", "category": "Extreme",
         "source": 0, "root": 0, "scale": 11, "count": 24, "decay": 0.99, "damp": 0.55,
         "spread": 10.0, "sympathetic": 0.5, "excite": 1.3, "stereo": 0.85,
         "wetsolo": 1, "mix": 1.0, "out": -3.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        source: TuningSource::from_index(g("source", 0.0) as usize),
        root_pc: g("root", d.root_pc as f32) as i32,
        scale: Scale::from_index(g("scale", 0.0) as usize),
        count: (g("count", d.count as f32) as usize).clamp(12, MAX_RESONATORS),
        decay: g("decay", d.decay),
        damp: g("damp", d.damp),
        spread_cents: g("spread", d.spread_cents),
        sympathetic: g("sympathetic", d.sympathetic),
        excite: g("excite", d.excite),
        stereo: g("stereo", d.stereo),
        wet_solo: g("wetsolo", 0.0) > 0.5,
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
        held: [f32::NAN; MAX_RESONATORS],
        held_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields (the ones `settings_from_preset` sets) differ between
    /// two presets: enums/bools/ints by equality, floats by a loose epsilon. The fixed
    /// constants (`held`/`held_count`) are skipped. Drives both the differ-from-default and
    /// pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.source != b.source { n += 1; }
        if a.scale != b.scale { n += 1; }
        if a.wet_solo != b.wet_solo { n += 1; }
        if a.root_pc != b.root_pc { n += 1; }
        if a.count != b.count { n += 1; }
        let fs = [
            (a.decay, b.decay), (a.damp, b.damp), (a.spread_cents, b.spread_cents),
            (a.sympathetic, b.sympathetic), (a.excite, b.excite), (a.stereo, b.stereo),
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
        // Deep bank: SPECS target 15-30 for a complex instrument.
        assert!(presets.len() >= 15, "CHORALE bank too small: {}", presets.len());

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
        // `presets_pass_universal` test in tests.rs.
    }
}
