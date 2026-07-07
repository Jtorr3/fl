//! TRACER factory presets (SPECS list). Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`. The same list drives the GUI selector and the offline render
//! tests. Values are plain (un-normalized): dB for gains, Hz for fixed cutoffs, octaves
//! for smart-freq, 0..1 for mix, and integer indices for the enums (see `dsp`).

use crate::dsp::{PitchMode, Settings, ShapeKind, XoMode};
use suite_core::presets::Preset;

/// The five factory presets, in menu order.
pub const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Sliding 808 Grit", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 200.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 14.0, "b2_drive": 9.0, "b3_drive": 5.0, "b4_drive": 3.0,
         "b1_shape": 0, "b2_shape": 3, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -2.0, "b3_level": -4.0, "b4_level": 0.0,
         "slew": 120.0, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Vocal Fundamental Warmth", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 1,
         "xo1_hz": 300.0, "xo2_hz": 1200.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 8.0, "b2_drive": 5.0, "b3_drive": 3.0, "b4_drive": 2.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -3.0, "b4_level": 0.0,
         "slew": 60.0, "mix": 0.6, "out": 0.0 }"#,
    r#"{ "name": "Lead Bite", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 250.0, "xo2_hz": 1500.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 2.0,
         "b1_drive": 6.0, "b2_drive": 12.0, "b3_drive": 16.0, "b4_drive": 8.0,
         "b1_shape": 0, "b2_shape": 0, "b3_shape": 2, "b4_shape": 3,
         "b1_level": -1.0, "b2_level": 0.0, "b3_level": -1.0, "b4_level": -3.0,
         "slew": 200.0, "mix": 0.85, "out": -1.0 }"#,
    r#"{ "name": "Bass Harmonic Push", "pitch_mode": 0, "bands": 2, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 180.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 16.0, "b2_drive": 6.0, "b3_drive": 0.0, "b4_drive": 0.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 0, "b4_shape": 0,
         "b1_level": 0.0, "b2_level": -2.0, "b3_level": 0.0, "b4_level": 0.0,
         "slew": 90.0, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Fixed-Band Bus Saturator", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 1, "xo2_mode": 1, "xo3_mode": 1,
         "xo1_hz": 120.0, "xo2_hz": 800.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 5.0, "b2_drive": 4.0, "b3_drive": 4.0, "b4_drive": 3.0,
         "b1_shape": 1, "b2_shape": 1, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": 0.0, "b3_level": 0.0, "b4_level": -1.0,
         "slew": 200.0, "mix": 0.75, "out": 0.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for any key
/// the blob omits.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let midi_hz = if PitchMode::from_index(g("pitch_mode", 0.0) as usize) == PitchMode::Midi {
        Some(220.0)
    } else {
        None
    };
    Settings {
        pitch_mode: PitchMode::from_index(g("pitch_mode", 0.0) as usize),
        midi_note_hz: midi_hz,
        band_count: (g("bands", 3.0) as usize).clamp(2, 4),
        smart_freq_oct: g("smart_freq", d.smart_freq_oct),
        xo_mode: [
            XoMode::from_index(g("xo1_mode", 0.0) as usize),
            XoMode::from_index(g("xo2_mode", 0.0) as usize),
            XoMode::from_index(g("xo3_mode", 0.0) as usize),
        ],
        xo_fixed_hz: [
            g("xo1_hz", d.xo_fixed_hz[0]),
            g("xo2_hz", d.xo_fixed_hz[1]),
            g("xo3_hz", d.xo_fixed_hz[2]),
        ],
        const_color: g("const_color", 1.0) >= 0.5,
        trim_db: g("trim", d.trim_db),
        band_drive_db: [
            g("b1_drive", d.band_drive_db[0]),
            g("b2_drive", d.band_drive_db[1]),
            g("b3_drive", d.band_drive_db[2]),
            g("b4_drive", d.band_drive_db[3]),
        ],
        band_shape: [
            ShapeKind::from_index(g("b1_shape", 0.0) as usize),
            ShapeKind::from_index(g("b2_shape", 0.0) as usize),
            ShapeKind::from_index(g("b3_shape", 0.0) as usize),
            ShapeKind::from_index(g("b4_shape", 0.0) as usize),
        ],
        band_level_db: [
            g("b1_level", d.band_level_db[0]),
            g("b2_level", d.band_level_db[1]),
            g("b3_level", d.band_level_db[2]),
            g("b4_level", d.band_level_db[3]),
        ],
        slew_hz_per_ms: g("slew", d.slew_hz_per_ms),
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
            if s.band_count != d.band_count {
                diffs += 1;
            }
            if s.const_color != d.const_color {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.out_db - d.out_db).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.trim_db - d.trim_db).abs() > 1e-3 {
                diffs += 1;
            }
            for b in 0..4 {
                if (s.band_drive_db[b] - d.band_drive_db[b]).abs() > 1e-3 {
                    diffs += 1;
                }
                if s.band_shape[b] != d.band_shape[b] {
                    diffs += 1;
                }
            }
            if s.xo_mode != d.xo_mode {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
