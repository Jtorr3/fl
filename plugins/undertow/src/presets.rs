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

/// The factory presets, in menu order. PRESET-EXPANSION Batch A deep bank (SPECS target
/// 15–30 for a complex FX); the GUI groups them by the `"category"` tag into preset-bar
/// sections. Names are purpose-driven and genre-aware (dark techno / atmospheric dnb) —
/// never settings descriptions. Encodings: see the module-level note above.
///
/// Categories (preset-bar sections): Techno Rumble / DnB Sub / Ducked Bed / Tonal / Extreme.
pub const PRESET_JSON: &[&str] = &[
    // ---- Techno Rumble ----------------------------------------------------
    // Long, tuned, hypnotic roll — melodic-techno low-end that hums a note.
    r#"{ "name": "Rolling Rumble", "category": "Techno Rumble", "strip": 0.55, "drive": 0.35, "size": 0.7, "decay": 1.6,
         "lpfreq": 130.0, "lpres": 1.6, "tunenote": 21, "tuneamt": 0.45, "duckdepth": 0.6,
         "duckrel": 210.0, "rumble": -2.0, "width": 0.3, "dry": 0.0, "trim": 0.0 }"#,
    // Tight, punchy, hard-techno: short tail, aggressive duck, strong strip, narrow.
    r#"{ "name": "Tight Modern Techno", "category": "Techno Rumble", "strip": 0.75, "drive": 0.45, "size": 0.35, "decay": 0.5,
         "lpfreq": 110.0, "lpres": 1.0, "tunenote": 21, "tuneamt": 0.0, "duckdepth": 0.8,
         "duckrel": 110.0, "rumble": -4.0, "width": 0.12, "dry": 0.0, "trim": 0.0 }"#,
    // Classic dark-warehouse sub roll under an A1 kick; moderate everything.
    r#"{ "name": "Warehouse Rumble", "category": "Techno Rumble", "strip": 0.55, "drive": 0.4, "size": 0.65, "decay": 1.4,
         "lpfreq": 150.0, "lpres": 1.3, "tunenote": 21, "tuneamt": 0.2, "duckdepth": 0.6,
         "duckrel": 180.0, "rumble": -3.0, "width": 0.35, "dry": 0.0, "trim": 0.0 }"#,
    // Pressurised, dense, darker roller tuned down to G1 — hard-techno weight.
    r#"{ "name": "Basement Pressure", "category": "Techno Rumble", "strip": 0.6, "drive": 0.5, "size": 0.55, "decay": 1.1,
         "lpfreq": 120.0, "lpres": 1.5, "tunenote": 19, "tuneamt": 0.25, "duckdepth": 0.7,
         "duckrel": 150.0, "rumble": -3.5, "width": 0.25, "dry": 0.0, "trim": -1.0 }"#,
    // Driving peak-time roll: longer tail, stronger tune hum, wider recovery.
    r#"{ "name": "Peak Time Roller", "category": "Techno Rumble", "strip": 0.6, "drive": 0.45, "size": 0.7, "decay": 1.8,
         "lpfreq": 140.0, "lpres": 1.4, "tunenote": 21, "tuneamt": 0.4, "duckdepth": 0.65,
         "duckrel": 200.0, "rumble": -3.0, "width": 0.3, "dry": 0.0, "trim": 0.0 }"#,
    // ---- DnB Sub ----------------------------------------------------------
    // Atmospheric-dnb wash tuned to E1: washy, wide, strong resonant sing.
    r#"{ "name": "Sewerslvt Sub", "category": "DnB Sub", "strip": 0.45, "drive": 0.4, "size": 0.75, "decay": 2.0,
         "lpfreq": 165.0, "lpres": 1.6, "tunenote": 16, "tuneamt": 0.6, "duckdepth": 0.55,
         "duckrel": 230.0, "rumble": -3.0, "width": 0.45, "dry": 0.0, "trim": -0.5 }"#,
    // Half-time atmospheric bed tuned to D1: huge, long, wide, slow recovery.
    r#"{ "name": "Atmospheric Roller", "category": "DnB Sub", "strip": 0.5, "drive": 0.35, "size": 0.8, "decay": 2.3,
         "lpfreq": 155.0, "lpres": 1.3, "tunenote": 14, "tuneamt": 0.5, "duckdepth": 0.5,
         "duckrel": 250.0, "rumble": -2.5, "width": 0.5, "dry": 0.0, "trim": -0.5 }"#,
    // Smooth, clean liquid-dnb sub on F1: gentle drive, subtle duck.
    r#"{ "name": "Liquid Sub Bed", "category": "DnB Sub", "strip": 0.4, "drive": 0.25, "size": 0.6, "decay": 1.5,
         "lpfreq": 175.0, "lpres": 1.1, "tunenote": 17, "tuneamt": 0.45, "duckdepth": 0.45,
         "duckrel": 210.0, "rumble": -3.5, "width": 0.4, "dry": 0.0, "trim": 0.0 }"#,
    // ---- Ducked Bed -------------------------------------------------------
    // Solid sustained bed under a warehouse kick; gentle duck, mostly mono.
    r#"{ "name": "Warehouse Bed", "category": "Ducked Bed", "strip": 0.5, "drive": 0.3, "size": 0.6, "decay": 1.2,
         "lpfreq": 160.0, "lpres": 1.2, "tunenote": 21, "tuneamt": 0.0, "duckdepth": 0.5,
         "duckrel": 170.0, "rumble": -3.0, "width": 0.4, "dry": 0.0, "trim": 0.0 }"#,
    // Huge, dark, cavernous late field with a long recovery.
    r#"{ "name": "Cavern Floor", "category": "Ducked Bed", "strip": 0.45, "drive": 0.3, "size": 0.9, "decay": 2.6,
         "lpfreq": 210.0, "lpres": 1.1, "tunenote": 21, "tuneamt": 0.2, "duckdepth": 0.4,
         "duckrel": 270.0, "rumble": -1.0, "width": 0.55, "dry": 0.0, "trim": -1.0 }"#,
    // Low, subtle, ghostly bed that sits far under the kick; soft duck, quiet.
    r#"{ "name": "Ghost Bed", "category": "Ducked Bed", "strip": 0.5, "drive": 0.3, "size": 0.55, "decay": 1.3,
         "lpfreq": 130.0, "lpres": 1.2, "tunenote": 21, "tuneamt": 0.1, "duckdepth": 0.4,
         "duckrel": 190.0, "rumble": -5.0, "width": 0.35, "dry": 0.0, "trim": 0.0 }"#,
    // Pronounced pumping rhythm: deep, fast-recovering duck breathes with the kick.
    r#"{ "name": "Breathing Floor", "category": "Ducked Bed", "strip": 0.55, "drive": 0.35, "size": 0.6, "decay": 1.6,
         "lpfreq": 145.0, "lpres": 1.3, "tunenote": 21, "tuneamt": 0.2, "duckdepth": 0.85,
         "duckrel": 130.0, "rumble": -3.0, "width": 0.4, "dry": 0.0, "trim": 0.0 }"#,
    // ---- Tonal ------------------------------------------------------------
    // Melodic wash tuned to E1 with a strong resonant peak — sings under the kick.
    r#"{ "name": "Hypnotic Wash Low", "category": "Tonal", "strip": 0.5, "drive": 0.35, "size": 0.65, "decay": 1.9,
         "lpfreq": 150.0, "lpres": 1.4, "tunenote": 16, "tuneamt": 0.7, "duckdepth": 0.5,
         "duckrel": 220.0, "rumble": -3.0, "width": 0.35, "dry": 0.0, "trim": 0.0 }"#,
    // Pitched E1 rumble with a heavy tune peak — a low melodic note that rings.
    r#"{ "name": "Melodic Rumble", "category": "Tonal", "strip": 0.5, "drive": 0.35, "size": 0.6, "decay": 1.7,
         "lpfreq": 150.0, "lpres": 1.5, "tunenote": 16, "tuneamt": 0.8, "duckdepth": 0.5,
         "duckrel": 200.0, "rumble": -3.5, "width": 0.3, "dry": 0.0, "trim": -0.5 }"#,
    // Deep C1 drone: earthquake-low tonal hum, long tail, strong resonance.
    r#"{ "name": "Tectonic Hum", "category": "Tonal", "strip": 0.55, "drive": 0.45, "size": 0.7, "decay": 2.2,
         "lpfreq": 135.0, "lpres": 1.6, "tunenote": 12, "tuneamt": 0.7, "duckdepth": 0.45,
         "duckrel": 220.0, "rumble": -3.0, "width": 0.3, "dry": 0.0, "trim": -1.0 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Saturated drone bed: heavy drive, long tail, thickened body.
    r#"{ "name": "Distorted Drone Bed", "category": "Extreme", "strip": 0.6, "drive": 0.85, "size": 0.6, "decay": 2.1,
         "lpfreq": 180.0, "lpres": 1.5, "tunenote": 21, "tuneamt": 0.3, "duckdepth": 0.45,
         "duckrel": 190.0, "rumble": -4.0, "width": 0.4, "dry": 0.0, "trim": -1.5 }"#,
    // Colossal collapse: near-max size, longest tail, heavy drive, deep field.
    r#"{ "name": "Subterranean Collapse", "category": "Extreme", "strip": 0.6, "drive": 0.8, "size": 0.95, "decay": 2.8,
         "lpfreq": 200.0, "lpres": 1.5, "tunenote": 19, "tuneamt": 0.4, "duckdepth": 0.5,
         "duckrel": 260.0, "rumble": -4.0, "width": 0.5, "dry": 0.0, "trim": -2.0 }"#,
    // Fully overdriven abyss: max drive, strong strip, resonant and aggressive.
    r#"{ "name": "Overdriven Abyss", "category": "Extreme", "strip": 0.7, "drive": 0.95, "size": 0.65, "decay": 1.9,
         "lpfreq": 190.0, "lpres": 1.7, "tunenote": 21, "tuneamt": 0.35, "duckdepth": 0.6,
         "duckrel": 170.0, "rumble": -4.5, "width": 0.35, "dry": 0.0, "trim": -2.5 }"#,
    // Maximum-depth pump: full duck, fast recovery, narrow and relentless.
    r#"{ "name": "Total Duck", "category": "Extreme", "strip": 0.65, "drive": 0.5, "size": 0.5, "decay": 1.2,
         "lpfreq": 125.0, "lpres": 1.4, "tunenote": 21, "tuneamt": 0.0, "duckdepth": 1.0,
         "duckrel": 100.0, "rumble": -3.0, "width": 0.2, "dry": 0.0, "trim": -1.0 }"#,
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

    /// Count how many `Settings` fields differ between two presets (all fields are floats
    /// here — gains are already linear, the tune note is already Hz). Loose epsilon.
    /// Drives both the differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let fs = [
            (a.strip, b.strip), (a.drive, b.drive), (a.size, b.size),
            (a.decay, b.decay), (a.lp_cutoff, b.lp_cutoff), (a.lp_res, b.lp_res),
            (a.tune_hz, b.tune_hz), (a.tune_amount, b.tune_amount),
            (a.duck_depth, b.duck_depth), (a.duck_release_ms, b.duck_release_ms),
            (a.rumble_gain, b.rumble_gain), (a.width, b.width),
            (a.dry_gain, b.dry_gain), (a.out_gain, b.out_gain),
        ];
        let mut n = 0;
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 { n += 1; }
        }
        n
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 15, "UNDERTOW bank too small: {}", presets.len());

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
