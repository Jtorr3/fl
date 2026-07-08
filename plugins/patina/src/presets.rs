//! PATINA factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded flat-JSON
//! blob parsed by `suite_core::presets`. The same list drives the GUI selector (grouped by the
//! `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain numeric, all 0..1 unless noted): `wow` wow depth; `wowrate` rate trim
//! (0.25..4); `flut` flutter depth; `sat` saturation drive; `bump` head-bump amount; `bumpf`
//! shelf corner Hz (60..120); `azim` azimuth; `droprate`/`dropdep` dropout rate/depth;
//! `hiss`/`hum`/`crackle` noise levels; `hum60` 1 = 60 Hz else 50 Hz; `key` noise key amount;
//! `age` AGE macro; `mix` 0..1; `out` dB.
//!
//! Categories (preset-bar sections): Cassette / Vinyl / Broadcast / Console / Subtle Glue /
//! Destroyed. Names are purpose-driven and genre-aware (dark techno / atmospheric dnb /
//! Cynthoni-Sewerslvt taste) — never settings descriptions. Levels stay conservative (hot banks
//! trimmed with negative `out`) so every render passes the universal assertions.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Cassette ---------------------------------------------------------
    r#"{ "name": "Bedroom Dub Deck", "category": "Cassette",
         "wow": 0.3, "wowrate": 1.0, "flut": 0.35, "sat": 0.35, "bump": 0.4, "bumpf": 90.0,
         "azim": 0.3, "droprate": 0.15, "dropdep": 0.3, "hiss": 0.3, "hum": 0.05, "crackle": 0.15,
         "hum60": 1.0, "key": 0.6, "age": 0.25, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Ferric Memory", "category": "Cassette",
         "wow": 0.45, "wowrate": 0.7, "flut": 0.25, "sat": 0.45, "bump": 0.5, "bumpf": 85.0,
         "azim": 0.35, "droprate": 0.2, "dropdep": 0.35, "hiss": 0.35, "hum": 0.1, "crackle": 0.2,
         "hum60": 1.0, "key": 0.5, "age": 0.35, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Walkman Flutter", "category": "Cassette",
         "wow": 0.25, "wowrate": 1.5, "flut": 0.6, "sat": 0.3, "bump": 0.3, "bumpf": 95.0,
         "azim": 0.5, "droprate": 0.25, "dropdep": 0.4, "hiss": 0.4, "hum": 0.0, "crackle": 0.1,
         "hum60": 1.0, "key": 0.7, "age": 0.3, "mix": 1.0, "out": -0.5 }"#,
    // ---- Vinyl ------------------------------------------------------------
    r#"{ "name": "Sunday Dust", "category": "Vinyl",
         "wow": 0.15, "wowrate": 0.9, "flut": 0.08, "sat": 0.2, "bump": 0.25, "bumpf": 80.0,
         "azim": 0.1, "droprate": 0.1, "dropdep": 0.2, "hiss": 0.12, "hum": 0.03, "crackle": 0.55,
         "hum60": 1.0, "key": 0.3, "age": 0.15, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Locked Groove Rain", "category": "Vinyl",
         "wow": 0.2, "wowrate": 0.8, "flut": 0.12, "sat": 0.25, "bump": 0.2, "bumpf": 75.0,
         "azim": 0.15, "droprate": 0.3, "dropdep": 0.45, "hiss": 0.2, "hum": 0.08, "crackle": 0.75,
         "hum60": 1.0, "key": 0.25, "age": 0.3, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Warped 78", "category": "Vinyl",
         "wow": 0.5, "wowrate": 0.6, "flut": 0.2, "sat": 0.3, "bump": 0.3, "bumpf": 70.0,
         "azim": 0.2, "droprate": 0.2, "dropdep": 0.3, "hiss": 0.25, "hum": 0.05, "crackle": 0.6,
         "hum60": 0.0, "key": 0.35, "age": 0.35, "mix": 1.0, "out": -0.5 }"#,
    // ---- Broadcast --------------------------------------------------------
    r#"{ "name": "Numbers Station", "category": "Broadcast",
         "wow": 0.35, "wowrate": 1.2, "flut": 0.4, "sat": 0.35, "bump": 0.15, "bumpf": 110.0,
         "azim": 0.6, "droprate": 0.35, "dropdep": 0.5, "hiss": 0.4, "hum": 0.3, "crackle": 0.25,
         "hum60": 0.0, "key": 0.5, "age": 0.35, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Shortwave Ghost", "category": "Broadcast",
         "wow": 0.4, "wowrate": 1.4, "flut": 0.5, "sat": 0.4, "bump": 0.2, "bumpf": 120.0,
         "azim": 0.5, "droprate": 0.5, "dropdep": 0.6, "hiss": 0.5, "hum": 0.25, "crackle": 0.3,
         "hum60": 0.0, "key": 0.45, "age": 0.45, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Pirate Transmission", "category": "Broadcast",
         "wow": 0.3, "wowrate": 1.1, "flut": 0.45, "sat": 0.5, "bump": 0.25, "bumpf": 100.0,
         "azim": 0.55, "droprate": 0.4, "dropdep": 0.5, "hiss": 0.45, "hum": 0.35, "crackle": 0.35,
         "hum60": 0.0, "key": 0.5, "age": 0.4, "mix": 1.0, "out": -1.0 }"#,
    // ---- Console ----------------------------------------------------------
    r#"{ "name": "Tube Console Glue", "category": "Console",
         "wow": 0.05, "wowrate": 1.0, "flut": 0.05, "sat": 0.35, "bump": 0.45, "bumpf": 100.0,
         "azim": 0.1, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.15, "hum": 0.4, "crackle": 0.0,
         "hum60": 1.0, "key": 0.2, "age": 0.1, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Studio B Hum", "category": "Console",
         "wow": 0.08, "wowrate": 1.0, "flut": 0.06, "sat": 0.3, "bump": 0.4, "bumpf": 110.0,
         "azim": 0.12, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.2, "hum": 0.55, "crackle": 0.02,
         "hum60": 1.0, "key": 0.0, "age": 0.1, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Iron Oxide Bus", "category": "Console",
         "wow": 0.12, "wowrate": 1.0, "flut": 0.1, "sat": 0.45, "bump": 0.5, "bumpf": 90.0,
         "azim": 0.15, "droprate": 0.05, "dropdep": 0.2, "hiss": 0.2, "hum": 0.15, "crackle": 0.05,
         "hum60": 1.0, "key": 0.4, "age": 0.2, "mix": 0.9, "out": -1.0 }"#,
    // ---- Subtle Glue ------------------------------------------------------
    r#"{ "name": "Barely There", "category": "Subtle Glue",
         "wow": 0.08, "wowrate": 1.0, "flut": 0.06, "sat": 0.18, "bump": 0.2, "bumpf": 85.0,
         "azim": 0.08, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.06, "hum": 0.0, "crackle": 0.0,
         "hum60": 1.0, "key": 0.8, "age": 0.12, "mix": 0.5, "out": 0.0 }"#,
    r#"{ "name": "Analog Sheen", "category": "Subtle Glue",
         "wow": 0.1, "wowrate": 1.0, "flut": 0.08, "sat": 0.25, "bump": 0.28, "bumpf": 95.0,
         "azim": 0.12, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.1, "hum": 0.02, "crackle": 0.03,
         "hum60": 1.0, "key": 0.7, "age": 0.15, "mix": 0.7, "out": 0.0 }"#,
    r#"{ "name": "Master Patina", "category": "Subtle Glue",
         "wow": 0.06, "wowrate": 1.0, "flut": 0.05, "sat": 0.2, "bump": 0.22, "bumpf": 80.0,
         "azim": 0.05, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.05, "hum": 0.0, "crackle": 0.0,
         "hum60": 1.0, "key": 0.9, "age": 0.1, "mix": 0.6, "out": 0.0 }"#,
    // ---- Destroyed --------------------------------------------------------
    r#"{ "name": "Sewer Transmission", "category": "Destroyed",
         "wow": 0.75, "wowrate": 1.6, "flut": 0.8, "sat": 0.65, "bump": 0.5, "bumpf": 95.0,
         "azim": 0.8, "droprate": 0.7, "dropdep": 0.75, "hiss": 0.6, "hum": 0.4, "crackle": 0.65,
         "hum60": 1.0, "key": 0.25, "age": 0.85, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Melted Reel", "category": "Destroyed",
         "wow": 0.95, "wowrate": 2.2, "flut": 0.7, "sat": 0.7, "bump": 0.55, "bumpf": 100.0,
         "azim": 0.7, "droprate": 0.6, "dropdep": 0.7, "hiss": 0.55, "hum": 0.35, "crackle": 0.5,
         "hum60": 1.0, "key": 0.2, "age": 0.9, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Total Decay", "category": "Destroyed",
         "wow": 0.9, "wowrate": 1.8, "flut": 0.9, "sat": 0.8, "bump": 0.6, "bumpf": 90.0,
         "azim": 0.9, "droprate": 0.85, "dropdep": 0.85, "hiss": 0.7, "hum": 0.5, "crackle": 0.75,
         "hum60": 1.0, "key": 0.15, "age": 1.0, "mix": 1.0, "out": -3.0 }"#,
];

/// Build the DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        wow_depth: g("wow", d.wow_depth),
        wow_rate: g("wowrate", d.wow_rate),
        flutter: g("flut", d.flutter),
        sat_drive: g("sat", d.sat_drive),
        bump_amount: g("bump", d.bump_amount),
        bump_freq: g("bumpf", d.bump_freq),
        azimuth: g("azim", d.azimuth),
        dropout_rate: g("droprate", d.dropout_rate),
        dropout_depth: g("dropdep", d.dropout_depth),
        hiss: g("hiss", d.hiss),
        hum: g("hum", d.hum),
        crackle: g("crackle", d.crackle),
        hum_60: g("hum60", 1.0) > 0.5,
        key_amount: g("key", d.key_amount),
        age: g("age", d.age),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/bools by equality,
    /// floats by a loose epsilon). Drives both the differ-from-default and pairwise-distinctness
    /// quality gates. Covers every field `settings_from_preset` sets.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.hum_60 != b.hum_60 {
            n += 1;
        }
        let fs = [
            (a.wow_depth, b.wow_depth),
            (a.wow_rate, b.wow_rate),
            (a.flutter, b.flutter),
            (a.sat_drive, b.sat_drive),
            (a.bump_amount, b.bump_amount),
            (a.bump_freq, b.bump_freq),
            (a.azimuth, b.azimuth),
            (a.dropout_rate, b.dropout_rate),
            (a.dropout_depth, b.dropout_depth),
            (a.hiss, b.hiss),
            (a.hum, b.hum),
            (a.crackle, b.crackle),
            (a.key_amount, b.key_amount),
            (a.age, b.age),
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
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 15, "PATINA bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the default
        // in >= 4 params, and every preset is categorised.
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
