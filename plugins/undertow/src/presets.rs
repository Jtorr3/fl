//! UNDERTOW factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `strip`/`drive`/`size`/`tuneamt`/`duckdepth`/`width` are 0..1;
//! `decay` is RT60 seconds; `lpfreq` Hz; `lpres` Q; `tunenote` is the note index 0..35
//! (0 = C0 … 21 = A1/55 Hz … 35 = B2); `duckrel` ms; `rumble`/`dry`/`trim` are dB.

use crate::dsp::{db_to_gain, Settings};
use suite_core::presets::Preset;

/// Number of selectable tune notes: C0 (MIDI 12) .. B2 (MIDI 47) inclusive.
pub const NOTE_COUNT: i32 = 36;

/// Note index (0..35) → MIDI note (12..47).
#[inline]
pub fn note_index_to_midi(idx: i32) -> i32 {
    12 + idx.clamp(0, NOTE_COUNT - 1)
}

/// Note index (0..35) → fundamental frequency (Hz). Equal temperament, A4 = 440.
#[inline]
pub fn note_index_to_hz(idx: i32) -> f32 {
    let midi = note_index_to_midi(idx);
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

/// Note index → display name, e.g. `A1`, `C#2`.
pub fn note_name(idx: i32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let midi = note_index_to_midi(idx);
    let octave = midi / 12 - 1;
    format!("{}{}", NAMES[(midi % 12) as usize], octave)
}

/// Parse a display note name (e.g. `A1`, `c#2`) back to an index; falls back to a plain int.
pub fn note_name_to_index(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<i32>() {
        return Some(v.clamp(0, NOTE_COUNT - 1));
    }
    let up = t.to_ascii_uppercase();
    let bytes = up.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let letter = bytes[0] as char;
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut i = 1;
    let mut semi = base;
    if i < bytes.len() && (bytes[i] as char == '#') {
        semi += 1;
        i += 1;
    } else if i < bytes.len() && (bytes[i] as char == 'B') {
        semi -= 1;
        i += 1;
    }
    let octave: i32 = up[i..].parse().ok()?;
    let midi = (octave + 1) * 12 + semi;
    let idx = midi - 12;
    if (0..NOTE_COUNT).contains(&idx) {
        Some(idx)
    } else {
        None
    }
}

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Solid sustained bed under a warehouse kick; gentle duck, mostly mono.
    r#"{ "name": "Warehouse Bed", "strip": 0.5, "drive": 0.3, "size": 0.6, "decay": 1.2,
         "lpfreq": 160.0, "lpres": 1.2, "tunenote": 21, "tuneamt": 0.0, "duckdepth": 0.5,
         "duckrel": 170.0, "rumble": -3.0, "width": 0.4, "dry": 0.0, "trim": 0.0 }"#,
    // Long, tuned, hypnotic roll — melodic-techno low-end that hums a note.
    r#"{ "name": "Rolling Rumble", "strip": 0.55, "drive": 0.35, "size": 0.7, "decay": 1.6,
         "lpfreq": 130.0, "lpres": 1.6, "tunenote": 21, "tuneamt": 0.45, "duckdepth": 0.6,
         "duckrel": 210.0, "rumble": -2.0, "width": 0.3, "dry": 0.0, "trim": 0.0 }"#,
    // Tight, punchy, hard-techno: short tail, aggressive duck, strong strip, narrow.
    r#"{ "name": "Tight Modern Techno", "strip": 0.75, "drive": 0.45, "size": 0.35, "decay": 0.5,
         "lpfreq": 110.0, "lpres": 1.0, "tunenote": 21, "tuneamt": 0.0, "duckdepth": 0.8,
         "duckrel": 110.0, "rumble": -4.0, "width": 0.12, "dry": 0.0, "trim": 0.0 }"#,
    // Huge, dark, cavernous late field with a long recovery.
    r#"{ "name": "Cavern Floor", "strip": 0.45, "drive": 0.3, "size": 0.9, "decay": 2.6,
         "lpfreq": 210.0, "lpres": 1.1, "tunenote": 21, "tuneamt": 0.2, "duckdepth": 0.4,
         "duckrel": 270.0, "rumble": -1.0, "width": 0.55, "dry": 0.0, "trim": -1.0 }"#,
    // Melodic wash tuned to E1 with a strong resonant peak — sings under the kick.
    r#"{ "name": "Hypnotic Wash Low", "strip": 0.5, "drive": 0.35, "size": 0.65, "decay": 1.9,
         "lpfreq": 150.0, "lpres": 1.4, "tunenote": 16, "tuneamt": 0.7, "duckdepth": 0.5,
         "duckrel": 220.0, "rumble": -3.0, "width": 0.35, "dry": 0.0, "trim": 0.0 }"#,
    // Saturated drone bed: heavy drive, long tail, thickened body.
    r#"{ "name": "Distorted Drone Bed", "strip": 0.6, "drive": 0.85, "size": 0.6, "decay": 2.1,
         "lpfreq": 180.0, "lpres": 1.5, "tunenote": 21, "tuneamt": 0.3, "duckdepth": 0.45,
         "duckrel": 190.0, "rumble": -4.0, "width": 0.4, "dry": 0.0, "trim": -1.5 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
/// Converts dB gains → linear and the note index → Hz so the DSP core stays unit-agnostic.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let tune_idx = p.get("tunenote").map(|v| v.round() as i32).unwrap_or(21);
    Settings {
        strip: g("strip", d.strip),
        drive: g("drive", d.drive),
        size: g("size", d.size),
        decay: g("decay", d.decay),
        lp_cutoff: g("lpfreq", d.lp_cutoff),
        lp_res: g("lpres", d.lp_res),
        tune_hz: note_index_to_hz(tune_idx),
        tune_amount: g("tuneamt", d.tune_amount),
        duck_depth: g("duckdepth", d.duck_depth),
        duck_release_ms: g("duckrel", d.duck_release_ms),
        rumble_gain: db_to_gain(g("rumble", -2.0)),
        width: g("width", d.width),
        dry_gain: db_to_gain(g("dry", 0.0)),
        out_gain: db_to_gain(g("trim", 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.strip - d.strip).abs() > 1e-3 { diffs += 1; }
            if (s.drive - d.drive).abs() > 1e-3 { diffs += 1; }
            if (s.size - d.size).abs() > 1e-3 { diffs += 1; }
            if (s.decay - d.decay).abs() > 1e-3 { diffs += 1; }
            if (s.lp_cutoff - d.lp_cutoff).abs() > 1e-3 { diffs += 1; }
            if (s.tune_amount - d.tune_amount).abs() > 1e-3 { diffs += 1; }
            if (s.duck_depth - d.duck_depth).abs() > 1e-3 { diffs += 1; }
            if (s.width - d.width).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }

    #[test]
    fn note_index_a1_is_55hz() {
        assert!((note_index_to_hz(21) - 55.0).abs() < 0.1, "A1 should be 55 Hz");
        assert_eq!(note_name(21), "A1");
        assert_eq!(note_name(0), "C0");
        assert_eq!(note_name(35), "B2");
    }

    #[test]
    fn note_name_roundtrips() {
        for idx in 0..NOTE_COUNT {
            let name = note_name(idx);
            assert_eq!(note_name_to_index(&name), Some(idx), "roundtrip failed for {name}");
        }
    }
}
