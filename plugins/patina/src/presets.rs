//! PATINA factory presets — flat-JSON blobs parsed by `suite_core::presets`. The same list
//! drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain numeric, all 0..1 unless noted): `wow` wow depth; `wowrate` rate
//! trim (0.25..4); `flut` flutter depth; `sat` saturation drive; `bump` head-bump amount;
//! `bumpf` shelf corner Hz; `azim` azimuth; `droprate`/`dropdep` dropout rate/depth;
//! `hiss`/`hum`/`crackle` noise levels; `hum60` 1 = 60 Hz else 50 Hz; `key` noise key amount;
//! `age` AGE macro; `mix` 0..1; `out` dB.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order (≥6).
pub const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Worn Cassette", "category": "TAPE",
         "wow": 0.35, "wowrate": 1.0, "flut": 0.4, "sat": 0.3, "bump": 0.35, "bumpf": 90.0,
         "azim": 0.25, "droprate": 0.15, "dropdep": 0.3, "hiss": 0.3, "hum": 0.0, "crackle": 0.1,
         "hum60": 1.0, "key": 0.6, "age": 0.25, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Dusty Vinyl", "category": "VINYL",
         "wow": 0.2, "wowrate": 0.8, "flut": 0.1, "sat": 0.2, "bump": 0.2, "bumpf": 80.0,
         "azim": 0.15, "droprate": 0.1, "dropdep": 0.2, "hiss": 0.15, "hum": 0.05, "crackle": 0.6,
         "hum60": 1.0, "key": 0.3, "age": 0.2, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Old Console Hum", "category": "GEAR",
         "wow": 0.05, "wowrate": 1.0, "flut": 0.05, "sat": 0.35, "bump": 0.4, "bumpf": 100.0,
         "azim": 0.1, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.2, "hum": 0.5, "crackle": 0.0,
         "hum60": 1.0, "key": 0.0, "age": 0.1, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Broadcast Ghost", "category": "RADIO",
         "wow": 0.5, "wowrate": 1.3, "flut": 0.5, "sat": 0.4, "bump": 0.15, "bumpf": 110.0,
         "azim": 0.5, "droprate": 0.4, "dropdep": 0.5, "hiss": 0.4, "hum": 0.25, "crackle": 0.25,
         "hum60": 0.0, "key": 0.5, "age": 0.4, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Gentle Glue Age", "category": "MIX",
         "wow": 0.1, "wowrate": 1.0, "flut": 0.08, "sat": 0.2, "bump": 0.25, "bumpf": 85.0,
         "azim": 0.1, "droprate": 0.0, "dropdep": 0.0, "hiss": 0.08, "hum": 0.0, "crackle": 0.0,
         "hum60": 1.0, "key": 0.8, "age": 0.15, "mix": 0.6, "out": 0.0 }"#,
    r#"{ "name": "Destroyed Tape", "category": "FX",
         "wow": 0.9, "wowrate": 1.6, "flut": 0.9, "sat": 0.7, "bump": 0.5, "bumpf": 95.0,
         "azim": 0.8, "droprate": 0.8, "dropdep": 0.8, "hiss": 0.6, "hum": 0.4, "crackle": 0.7,
         "hum60": 1.0, "key": 0.2, "age": 0.9, "mix": 1.0, "out": -1.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "want >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            let checks = [
                (s.wow_depth, d.wow_depth),
                (s.flutter, d.flutter),
                (s.sat_drive, d.sat_drive),
                (s.bump_amount, d.bump_amount),
                (s.azimuth, d.azimuth),
                (s.dropout_depth, d.dropout_depth),
                (s.hiss, d.hiss),
                (s.hum, d.hum),
                (s.crackle, d.crackle),
                (s.age, d.age),
            ];
            for (a, b) in checks {
                if (a - b).abs() > 0.01 {
                    diffs += 1;
                }
            }
            assert!(diffs >= 3, "preset '{}' differs from default in only {diffs} params", p.name);
        }
    }
}
