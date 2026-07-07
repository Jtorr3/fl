//! DRIFT factory presets (SPECS/brief list). Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`. The same list drives the GUI selector and the offline render
//! tests. Values are plain (un-normalized): Hz for rate/range, dB for depth/out, 0..1 for
//! mix/stereo offset, integer indices for the enums, and 0/1 for the sync toggle.

use crate::dsp::{Direction, Settings, SyncDivision};
use suite_core::presets::Preset;

/// The factory presets, in menu order.
///
/// Enum encodings: `direction` 0 = Up, 1 = Down. `division` 0 = 4 bars, 1 = 2 bars,
/// 2 = 1 bar, 3 = 1/2, 4 = 1/4, 5 = 1/8, 6 = 1/16.
pub const PRESET_JSON: &[&str] = &[
    // A classic ever-rising Shepard filter — six filters ~1 octave apart over six octaves.
    r#"{ "name": "Endless Riser", "rate": 0.15, "sync": 0, "division": 2, "direction": 0,
         "resonance": 3.5, "range_lo": 40.0, "range_hi": 2560.0, "peaks": 6,
         "stereo_offset": 0.25, "depth": 14.0, "mix": 1.0, "out": -2.0 }"#,
    // The mirror illusion: an unbroken slow fall.
    r#"{ "name": "Slow Descent", "rate": 0.06, "sync": 0, "division": 2, "direction": 1,
         "resonance": 3.0, "range_lo": 50.0, "range_hi": 3200.0, "peaks": 6,
         "stereo_offset": 0.30, "depth": 12.0, "mix": 1.0, "out": -2.0 }"#,
    // Tempo-locked hypnotic sweep, one full glide per beat.
    r#"{ "name": "Hypnotic Sweep 1/4", "rate": 0.5, "sync": 1, "division": 4, "direction": 0,
         "resonance": 5.0, "range_lo": 200.0, "range_hi": 3200.0, "peaks": 4,
         "stereo_offset": 0.25, "depth": 16.0, "mix": 1.0, "out": -3.0 }"#,
    // Eight gentle filters over seven octaves, hard L/R phase split for a wide, woozy drift.
    r#"{ "name": "Wide Drift", "rate": 0.1, "sync": 0, "division": 2, "direction": 0,
         "resonance": 2.5, "range_lo": 60.0, "range_hi": 7680.0, "peaks": 8,
         "stereo_offset": 0.5, "depth": 10.0, "mix": 1.0, "out": -1.0 }"#,
    // A parallel-mix shimmer that barely moves — motion you feel more than hear.
    r#"{ "name": "Subtle Motion", "rate": 0.08, "sync": 0, "division": 2, "direction": 0,
         "resonance": 2.0, "range_lo": 300.0, "range_hi": 4800.0, "peaks": 5,
         "stereo_offset": 0.20, "depth": 6.0, "mix": 0.6, "out": 0.0 }"#,
    // Tempo-locked descent, one glide every two beats.
    r#"{ "name": "Falling Half-Note", "rate": 0.5, "sync": 1, "division": 3, "direction": 1,
         "resonance": 4.0, "range_lo": 100.0, "range_hi": 6400.0, "peaks": 6,
         "stereo_offset": 0.35, "depth": 14.0, "mix": 0.9, "out": -2.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5, "need >= 5 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if s.peaks != d.peaks {
                diffs += 1;
            }
            if s.sync != d.sync {
                diffs += 1;
            }
            if s.direction != d.direction {
                diffs += 1;
            }
            if (s.rate_hz - d.rate_hz).abs() > 1e-4 {
                diffs += 1;
            }
            if (s.resonance - d.resonance).abs() > 1e-4 {
                diffs += 1;
            }
            if (s.range_lo - d.range_lo).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.range_hi - d.range_hi).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.depth_db - d.depth_db).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.stereo_offset - d.stereo_offset).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
