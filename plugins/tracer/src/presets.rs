//! TRACER factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render
//! tests. Values are plain (un-normalized): dB for gains, Hz for fixed cutoffs, octaves
//! for smart-freq, 0..1 for mix, and integer indices for the enums (see `dsp`):
//!   pitch_mode 0=Detect 1=MIDI; xoN_mode 0=Track(harmonic × f0) 1=Fixed(Hz);
//!   bN_shape 0=Tube 1=Tape 2=Fold 3=Hard; const_color 0/1.
//!
//! Categories (preset-bar sections): Bass-Reese / Vocal-Formant / Movement / Static-EQ /
//! Extreme. Names are purpose-driven and genre-aware (dark techno / atmospheric dnb /
//! Cynthoni-Sewerslvt) — never settings descriptions.

use crate::dsp::{PitchMode, Settings, ShapeKind, XoMode};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Bass-Reese -------------------------------------------------------
    r#"{ "name": "Sliding 808 Grit", "category": "Bass-Reese", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 200.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 14.0, "b2_drive": 9.0, "b3_drive": 5.0, "b4_drive": 3.0,
         "b1_shape": 0, "b2_shape": 3, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -2.0, "b3_level": -4.0, "b4_level": 0.0,
         "slew": 120.0, "mix": 1.0, "out": -6.3 }"#,
    r#"{ "name": "Bass Harmonic Push", "category": "Bass-Reese", "pitch_mode": 0, "bands": 2, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 180.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 16.0, "b2_drive": 6.0, "b3_drive": 0.0, "b4_drive": 0.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 0, "b4_shape": 0,
         "b1_level": 0.0, "b2_level": -2.0, "b3_level": 0.0, "b4_level": 0.0,
         "slew": 90.0, "mix": 1.0, "out": -5.2 }"#,
    r#"{ "name": "Reese Tracker", "category": "Bass-Reese", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 200.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 12.0, "b2_drive": 15.0, "b3_drive": 8.0, "b4_drive": 4.0,
         "b1_shape": 0, "b2_shape": 2, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -3.0, "b4_level": 0.0,
         "slew": 150.0, "mix": 1.0, "out": -7.6 }"#,
    r#"{ "name": "Sub Formant Drive", "category": "Bass-Reese", "pitch_mode": 0, "bands": 2, "smart_freq": -1.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 200.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 18.0, "b2_drive": 7.0, "b3_drive": 0.0, "b4_drive": 0.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 0, "b4_shape": 0,
         "b1_level": 0.0, "b2_level": -3.0, "b3_level": 0.0, "b4_level": 0.0,
         "slew": 100.0, "mix": 0.9, "out": -5.6 }"#,
    // ---- Vocal-Formant ----------------------------------------------------
    r#"{ "name": "Vocal Fundamental Warmth", "category": "Vocal-Formant", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 1,
         "xo1_hz": 300.0, "xo2_hz": 1200.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 8.0, "b2_drive": 5.0, "b3_drive": 3.0, "b4_drive": 2.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -3.0, "b4_level": 0.0,
         "slew": 60.0, "mix": 0.6, "out": -0.4 }"#,
    r#"{ "name": "Vowel Ghost", "category": "Vocal-Formant", "pitch_mode": 0, "bands": 4, "smart_freq": 1.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 300.0, "xo2_hz": 1200.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 4.0, "b2_drive": 8.0, "b3_drive": 10.0, "b4_drive": 6.0,
         "b1_shape": 1, "b2_shape": 0, "b3_shape": 2, "b4_shape": 1,
         "b1_level": -2.0, "b2_level": 0.0, "b3_level": -1.0, "b4_level": -3.0,
         "slew": 250.0, "mix": 0.5, "out": 0.0 }"#,
    r#"{ "name": "Formant Drift", "category": "Vocal-Formant", "pitch_mode": 0, "bands": 3, "smart_freq": 0.5,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 300.0, "xo2_hz": 1200.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 6.0, "b2_drive": 9.0, "b3_drive": 7.0, "b4_drive": 3.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 2, "b4_shape": 1,
         "b1_level": -1.0, "b2_level": 0.0, "b3_level": -2.0, "b4_level": 0.0,
         "slew": 40.0, "mix": 0.6, "out": -0.4 }"#,
    r#"{ "name": "Choir Haze", "category": "Vocal-Formant", "pitch_mode": 1, "bands": 3, "smart_freq": 1.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 300.0, "xo2_hz": 1200.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 4.0, "b2_drive": 5.0, "b3_drive": 6.0, "b4_drive": 3.0,
         "b1_shape": 1, "b2_shape": 1, "b3_shape": 2, "b4_shape": 1,
         "b1_level": -2.0, "b2_level": -1.0, "b3_level": -3.0, "b4_level": 0.0,
         "slew": 200.0, "mix": 0.45, "out": 0.0 }"#,
    // ---- Movement ---------------------------------------------------------
    r#"{ "name": "Lead Bite", "category": "Movement", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 250.0, "xo2_hz": 1500.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 2.0,
         "b1_drive": 6.0, "b2_drive": 12.0, "b3_drive": 16.0, "b4_drive": 8.0,
         "b1_shape": 0, "b2_shape": 0, "b3_shape": 2, "b4_shape": 3,
         "b1_level": -1.0, "b2_level": 0.0, "b3_level": -1.0, "b4_level": -3.0,
         "slew": 200.0, "mix": 0.85, "out": -7.1 }"#,
    r#"{ "name": "Harmonic Climber", "category": "Movement", "pitch_mode": 0, "bands": 4, "smart_freq": 2.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 250.0, "xo2_hz": 1500.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 5.0, "b2_drive": 10.0, "b3_drive": 13.0, "b4_drive": 9.0,
         "b1_shape": 0, "b2_shape": 0, "b3_shape": 2, "b4_shape": 3,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -1.0, "b4_level": -2.0,
         "slew": 300.0, "mix": 0.85, "out": -4.0 }"#,
    r#"{ "name": "Glide Chorus Sat", "category": "Movement", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 250.0, "xo2_hz": 1500.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 7.0, "b2_drive": 7.0, "b3_drive": 6.0, "b4_drive": 5.0,
         "b1_shape": 1, "b2_shape": 1, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -1.0, "b4_level": -2.0,
         "slew": 500.0, "mix": 0.7, "out": -1.2 }"#,
    r#"{ "name": "Pitch Rider", "category": "Movement", "pitch_mode": 0, "bands": 3, "smart_freq": -0.5,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 250.0, "xo2_hz": 1500.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 8.0, "b2_drive": 11.0, "b3_drive": 9.0, "b4_drive": 4.0,
         "b1_shape": 0, "b2_shape": 2, "b3_shape": 2, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -2.0, "b4_level": 0.0,
         "slew": 800.0, "mix": 0.9, "out": -8.0 }"#,
    // ---- Static-EQ (fixed crossovers, no pitch tracking — utility/mix) -----
    r#"{ "name": "Fixed-Band Bus Saturator", "category": "Static-EQ", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 1, "xo2_mode": 1, "xo3_mode": 1,
         "xo1_hz": 120.0, "xo2_hz": 800.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 0.0,
         "b1_drive": 5.0, "b2_drive": 4.0, "b3_drive": 4.0, "b4_drive": 3.0,
         "b1_shape": 1, "b2_shape": 1, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": 0.0, "b3_level": 0.0, "b4_level": -1.0,
         "slew": 200.0, "mix": 0.75, "out": -3.1 }"#,
    // (CUT: "Mix Glue Bands" — a mix-0.5 fixed-band all-Tape saturator that duplicated the
    //  purpose of Fixed-Band Bus Saturator + Static Warmth EQ without adding a distinct voice.
    //  SOUND-PASS prune of filler; the two survivors cover gentle bus glue better.)
    r#"{ "name": "Static Warmth EQ", "category": "Static-EQ", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 1, "xo2_mode": 1, "xo3_mode": 1,
         "xo1_hz": 150.0, "xo2_hz": 1200.0, "xo3_hz": 5000.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 5.0, "b2_drive": 4.0, "b3_drive": 3.0, "b4_drive": 2.0,
         "b1_shape": 0, "b2_shape": 0, "b3_shape": 1, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": 0.0, "b3_level": -1.0, "b4_level": 0.0,
         "slew": 200.0, "mix": 0.6, "out": 0.0 }"#,
    r#"{ "name": "Broadcast Band", "category": "Static-EQ", "pitch_mode": 0, "bands": 3, "smart_freq": 0.0,
         "xo1_mode": 1, "xo2_mode": 1, "xo3_mode": 1,
         "xo1_hz": 200.0, "xo2_hz": 1500.0, "xo3_hz": 4500.0, "const_color": 1, "trim": 1.0,
         "b1_drive": 4.0, "b2_drive": 8.0, "b3_drive": 6.0, "b4_drive": 3.0,
         "b1_shape": 0, "b2_shape": 1, "b3_shape": 3, "b4_shape": 1,
         "b1_level": -3.0, "b2_level": 0.0, "b3_level": -1.0, "b4_level": 0.0,
         "slew": 200.0, "mix": 0.7, "out": -1.0 }"#,
    r#"{ "name": "Fixed Fold Texture", "category": "Static-EQ", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 1, "xo2_mode": 1, "xo3_mode": 1,
         "xo1_hz": 180.0, "xo2_hz": 1000.0, "xo3_hz": 3500.0, "const_color": 0, "trim": 0.0,
         "b1_drive": 4.0, "b2_drive": 9.0, "b3_drive": 11.0, "b4_drive": 6.0,
         "b1_shape": 0, "b2_shape": 2, "b3_shape": 2, "b4_shape": 1,
         "b1_level": -1.0, "b2_level": -1.0, "b3_level": -2.0, "b4_level": -3.0,
         "slew": 200.0, "mix": 0.55, "out": -1.0 }"#,
    // ---- Extreme ----------------------------------------------------------
    r#"{ "name": "Spectral Fog", "category": "Extreme", "pitch_mode": 0, "bands": 4, "smart_freq": 1.5,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 250.0, "xo2_hz": 1500.0, "xo3_hz": 5000.0, "const_color": 0, "trim": 0.0,
         "b1_drive": 10.0, "b2_drive": 14.0, "b3_drive": 16.0, "b4_drive": 10.0,
         "b1_shape": 2, "b2_shape": 2, "b3_shape": 2, "b4_shape": 3,
         "b1_level": -2.0, "b2_level": -2.0, "b3_level": -3.0, "b4_level": -4.0,
         "slew": 60.0, "mix": 0.6, "out": -2.0 }"#,
    r#"{ "name": "Sewer Reese Mangler", "category": "Extreme", "pitch_mode": 0, "bands": 3, "smart_freq": -1.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 200.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 0, "trim": 3.0,
         "b1_drive": 18.0, "b2_drive": 20.0, "b3_drive": 14.0, "b4_drive": 6.0,
         "b1_shape": 3, "b2_shape": 2, "b3_shape": 3, "b4_shape": 1,
         "b1_level": 0.0, "b2_level": -1.0, "b3_level": -2.0, "b4_level": 0.0,
         "slew": 90.0, "mix": 1.0, "out": -9.3 }"#,
    r#"{ "name": "Total Harmonic Collapse", "category": "Extreme", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 0, "xo2_mode": 0, "xo3_mode": 0,
         "xo1_hz": 200.0, "xo2_hz": 1000.0, "xo3_hz": 4000.0, "const_color": 0, "trim": 4.0,
         "b1_drive": 24.0, "b2_drive": 24.0, "b3_drive": 20.0, "b4_drive": 16.0,
         "b1_shape": 3, "b2_shape": 3, "b3_shape": 3, "b4_shape": 3,
         "b1_level": -1.0, "b2_level": -2.0, "b3_level": -3.0, "b4_level": -4.0,
         "slew": 120.0, "mix": 1.0, "out": -9.9 }"#,
    r#"{ "name": "Nyquist Formant Screech", "category": "Extreme", "pitch_mode": 0, "bands": 4, "smart_freq": 0.0,
         "xo1_mode": 1, "xo2_mode": 1, "xo3_mode": 1,
         "xo1_hz": 300.0, "xo2_hz": 3000.0, "xo3_hz": 9000.0, "const_color": 1, "trim": 2.0,
         "b1_drive": 6.0, "b2_drive": 16.0, "b3_drive": 20.0, "b4_drive": 12.0,
         "b1_shape": 0, "b2_shape": 2, "b3_shape": 3, "b4_shape": 2,
         "b1_level": -2.0, "b2_level": -1.0, "b3_level": -2.0, "b4_level": -4.0,
         "slew": 200.0, "mix": 0.85, "out": -7.6 }"#,
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

    /// Count how many `Settings` fields differ between two presets (enums/bools by
    /// equality, floats by a loose epsilon). Drives both the differ-from-default and
    /// pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.pitch_mode != b.pitch_mode {
            n += 1;
        }
        if a.band_count != b.band_count {
            n += 1;
        }
        if a.const_color != b.const_color {
            n += 1;
        }
        for i in 0..3 {
            if a.xo_mode[i] != b.xo_mode[i] {
                n += 1;
            }
        }
        for b_idx in 0..4 {
            if a.band_shape[b_idx] != b.band_shape[b_idx] {
                n += 1;
            }
        }
        let mut fs = vec![
            (a.smart_freq_oct, b.smart_freq_oct),
            (a.trim_db, b.trim_db),
            (a.slew_hz_per_ms, b.slew_hz_per_ms),
            (a.mix, b.mix),
            (a.out_db, b.out_db),
        ];
        for i in 0..3 {
            fs.push((a.xo_fixed_hz[i], b.xo_fixed_hz[i]));
        }
        for b_idx in 0..4 {
            fs.push((a.band_drive_db[b_idx], b.band_drive_db[b_idx]));
            fs.push((a.band_level_db[b_idx], b.band_level_db[b_idx]));
        }
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 {
                n += 1;
            }
        }
        n
    }

    /// Original gate kept intact: every preset parses and differs from the default.
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

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 15, "TRACER bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the
        // default in >= 4 params. Every preset is categorised.
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
