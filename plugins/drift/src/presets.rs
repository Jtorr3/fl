//! DRIFT factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//! Values are plain (un-normalized): Hz for rate/range, dB for depth/out, 0..1 for
//! mix/stereo offset, integer indices for the enums, and 0/1 for the sync toggle.
//!
//! Enum encodings: `direction` 0 = Up, 1 = Down. `division` 0 = 4 bars, 1 = 2 bars,
//! 2 = 1 bar, 3 = 1/2, 4 = 1/4, 5 = 1/8, 6 = 1/16 (see `dsp::SyncDivision::from_index`).
//!
//! Categories (preset-bar sections): Risers / Descents / Tempo-Sync / Ambient Motion /
//! Extreme. Names are purpose-driven and genre-aware (dark techno / atmospheric dnb /
//! Sewerslvt taste) — never settings descriptions. Levels stay conservative (out at/below
//! 0 dB, negative on hot presets) so every preset renders finite, non-silent, and <= 0 dBFS.

use crate::dsp::{Direction, Settings, SyncDivision};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Risers -----------------------------------------------------------
    // The classic ever-rising Shepard filter — six filters ~1 octave apart, endless climb.
    r#"{ "name": "Endless Ascent", "category": "Risers", "rate": 0.15, "sync": 0,
         "division": 2, "direction": 0, "resonance": 3.5, "range_lo": 40.0,
         "range_hi": 2560.0, "peaks": 6, "stereo_offset": 0.25, "depth": 14.0,
         "mix": 1.0, "out": -5.75 }"#,
    // Eight gentle filters over seven octaves, hard L/R split — a vast, slow lift.
    r#"{ "name": "Cathedral Lift", "category": "Risers", "rate": 0.08, "sync": 0,
         "division": 2, "direction": 0, "resonance": 2.5, "range_lo": 60.0,
         "range_hi": 7680.0, "peaks": 8, "stereo_offset": 0.5, "depth": 10.0,
         "mix": 1.0, "out": -4.25 }"#,
    // Atmospheric-dnb parallel riser — motion woven under the source rather than over it.
    r#"{ "name": "Vapor Climb", "category": "Risers", "rate": 0.12, "sync": 0,
         "division": 2, "direction": 0, "resonance": 4.0, "range_lo": 120.0,
         "range_hi": 4800.0, "peaks": 5, "stereo_offset": 0.30, "depth": 11.0,
         "mix": 0.75, "out": -2.0 }"#,
    // ---- Descents ---------------------------------------------------------
    // The mirror illusion: an unbroken, very slow fall.
    r#"{ "name": "Slow Descent", "category": "Descents", "rate": 0.06, "sync": 0,
         "division": 2, "direction": 1, "resonance": 3.0, "range_lo": 50.0,
         "range_hi": 3200.0, "peaks": 6, "stereo_offset": 0.30, "depth": 13.0,
         "mix": 1.0, "out": -6.5 }"#,
    // Dark-techno sinking sweep — low, resonant, dragging the mix downward forever.
    r#"{ "name": "Sinking Feeling", "category": "Descents", "rate": 0.05, "sync": 0,
         "division": 2, "direction": 1, "resonance": 5.0, "range_lo": 30.0,
         "range_hi": 1920.0, "peaks": 6, "stereo_offset": 0.35, "depth": 15.0,
         "mix": 1.0, "out": -7.25 }"#,
    // A glacial seven-octave plunge, wide and gentle — the floor dropping out.
    r#"{ "name": "Abyssal Drop", "category": "Descents", "rate": 0.04, "sync": 0,
         "division": 2, "direction": 1, "resonance": 2.5, "range_lo": 40.0,
         "range_hi": 5120.0, "peaks": 7, "stereo_offset": 0.45, "depth": 10.0,
         "mix": 0.9, "out": -3.5 }"#,
    // ---- Tempo-Sync -------------------------------------------------------
    // Tempo-locked hypnotic sweep, one full glide per beat.
    r#"{ "name": "Hypnotic Quarter", "category": "Tempo-Sync", "rate": 0.5, "sync": 1,
         "division": 4, "direction": 0, "resonance": 5.0, "range_lo": 200.0,
         "range_hi": 3200.0, "peaks": 4, "stereo_offset": 0.25, "depth": 16.0,
         "mix": 1.0, "out": -8.5 }"#,
    // Rolling half-note pump for techno risers — glides once every two beats.
    r#"{ "name": "Rolling Half", "category": "Tempo-Sync", "rate": 0.5, "sync": 1,
         "division": 3, "direction": 0, "resonance": 4.0, "range_lo": 100.0,
         "range_hi": 6400.0, "peaks": 6, "stereo_offset": 0.35, "depth": 13.0,
         "mix": 0.9, "out": -5.5 }"#,
    // Fast falling eighth-note shiver — a tight, nervous downward strobe.
    r#"{ "name": "Eighth-Note Shiver", "category": "Tempo-Sync", "rate": 0.5, "sync": 1,
         "division": 5, "direction": 1, "resonance": 6.0, "range_lo": 300.0,
         "range_hi": 4800.0, "peaks": 4, "stereo_offset": 0.20, "depth": 14.0,
         "mix": 1.0, "out": -6.0 }"#,
    // A slow two-bar swell that evolves across the whole phrase.
    r#"{ "name": "Bar-Long Swell", "category": "Tempo-Sync", "rate": 0.5, "sync": 1,
         "division": 1, "direction": 0, "resonance": 3.5, "range_lo": 60.0,
         "range_hi": 3840.0, "peaks": 6, "stereo_offset": 0.28, "depth": 12.0,
         "mix": 0.85, "out": -3.0 }"#,
    // ---- Ambient Motion ---------------------------------------------------
    // A parallel shimmer that barely moves — motion you feel more than hear.
    r#"{ "name": "Subtle Motion", "category": "Ambient Motion", "rate": 0.08, "sync": 0,
         "division": 2, "direction": 0, "resonance": 2.0, "range_lo": 300.0,
         "range_hi": 4800.0, "peaks": 5, "stereo_offset": 0.20, "depth": 6.0,
         "mix": 0.6, "out": 0.0 }"#,
    // A slow, wide breathing pad utility — gentle life under sustained sources.
    r#"{ "name": "Breathing Pad", "category": "Ambient Motion", "rate": 0.03, "sync": 0,
         "division": 2, "direction": 0, "resonance": 2.2, "range_lo": 200.0,
         "range_hi": 6400.0, "peaks": 5, "stereo_offset": 0.22, "depth": 7.0,
         "mix": 0.5, "out": 0.0 }"#,
    // Sewerslvt-woozy underwater drift — hard-split, slow-sinking, half-wet.
    r#"{ "name": "Underwater Drift", "category": "Ambient Motion", "rate": 0.05, "sync": 0,
         "division": 2, "direction": 1, "resonance": 3.0, "range_lo": 150.0,
         "range_hi": 3600.0, "peaks": 6, "stereo_offset": 0.5, "depth": 8.0,
         "mix": 0.65, "out": -0.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    // High-Q, deep, all eight filters — a searing resonant climb.
    r#"{ "name": "Resonant Screamer", "category": "Extreme", "rate": 0.3, "sync": 0,
         "division": 2, "direction": 0, "resonance": 12.0, "range_lo": 80.0,
         "range_hi": 8000.0, "peaks": 8, "stereo_offset": 0.4, "depth": 20.0,
         "mix": 0.9, "out": -5.0 }"#,
    // Fast, deep, full-width downward chaos — the room spinning out.
    r#"{ "name": "Total Vertigo", "category": "Extreme", "rate": 1.5, "sync": 0,
         "division": 2, "direction": 1, "resonance": 8.0, "range_lo": 40.0,
         "range_hi": 12000.0, "peaks": 8, "stereo_offset": 0.5, "depth": 24.0,
         "mix": 1.0, "out": -13.25 }"#,
    // A shrieking high-band haze pushing toward Nyquist — bright, cutting, extreme.
    r#"{ "name": "Nyquist Haze", "category": "Extreme", "rate": 0.2, "sync": 0,
         "division": 2, "direction": 0, "resonance": 6.0, "range_lo": 500.0,
         "range_hi": 16000.0, "peaks": 7, "stereo_offset": 0.45, "depth": 18.0,
         "mix": 0.85, "out": -9.75 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for any key
/// the blob omits.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        rate_hz: g("rate", d.rate_hz),
        sync: g("sync", 0.0) >= 0.5,
        division: SyncDivision::from_index(g("division", 2.0) as usize),
        tempo_bpm: 120.0,
        direction: Direction::from_index(g("direction", 0.0) as usize),
        resonance: g("resonance", d.resonance),
        range_lo: g("range_lo", d.range_lo),
        range_hi: g("range_hi", d.range_hi),
        peaks: (g("peaks", 6.0) as usize).clamp(2, crate::dsp::MAX_PEAKS),
        stereo_offset: g("stereo_offset", d.stereo_offset),
        depth_db: g("depth", d.depth_db),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/bools/peaks by
    /// equality, floats by a loose epsilon). Drives both the differ-from-default and
    /// pairwise-distinctness quality gates. The fixed `tempo_bpm` field is constant across
    /// all presets and deliberately excluded.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.sync != b.sync {
            n += 1;
        }
        if a.division != b.division {
            n += 1;
        }
        if a.direction != b.direction {
            n += 1;
        }
        if a.peaks != b.peaks {
            n += 1;
        }
        let fs = [
            (a.rate_hz, b.rate_hz),
            (a.resonance, b.resonance),
            (a.range_lo, b.range_lo),
            (a.range_hi, b.range_hi),
            (a.stereo_offset, b.stereo_offset),
            (a.depth_db, b.depth_db),
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
        // Expanded utility bank: SPECS target >= 12 for a simpler plugin.
        assert!(presets.len() >= 12, "DRIFT bank too small: {}", presets.len());

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
        // `render_tests::every_preset_renders_and_passes_universal` test in lib.rs.
    }
}
