//! SHAPESHIFT factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain, un-normalized): `x`/`y` XY point 0..1; `cA`..`cD` corner shaper index
//! 0..7 (0 Tube, 1 Tape, 2 Diode, 3 Hard, 4 SineFold, 5 TriFold, 6 Cheby3, 7 BitSoft);
//! `gA`..`gD` per-corner input gain dB (±24); `pre` pre-gain dB (−12..+36); `orbit` 0/1;
//! `orate` Hz (0.01..20); `osync` 0/1; `odiv` 0..3 (½/1/2/4 bar); `oradius` 0..0.5;
//! `oshape` 0/1 (circle/figure-8); `ophase` 0..1; `postlp` Hz (200..20k); `autogain` 0/1;
//! `mix` 0..1; `out` dB (±24).
//!
//! Categories (preset-bar sections, first-appearance order): Warm / Morph / Orbit / Digital /
//! Extreme. Names are purpose-driven and genre-aware (dark-techno / atmospheric-dnb /
//! Cynthoni-Sewerslvt taste) — never a settings description. Auto-gain rides on every preset and
//! output trims sit at or below 0 dB (negative on the hot ones) so each render stays finite,
//! non-silent, and under the 0 dBFS ceiling.

use crate::dsp::{Corner, OrbitShape, Settings, SyncDivision, NUM_CORNERS};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Warm -------------------------------------------------------------
    r#"{ "name": "Analog Sunrise Bed", "category": "Warm",
         "x": 0.25, "y": 0.30,
         "cA": 0, "cB": 1, "cC": 1, "cD": 0,
         "gA": 0.0, "gB": 1.0, "gC": 0.0, "gD": 1.5,
         "pre": 5.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 15000.0, "autogain": 1, "mix": 0.85, "out": 0.0 }"#,
    r#"{ "name": "Ferrous Lull", "category": "Warm",
         "x": 0.20, "y": 0.55,
         "cA": 1, "cB": 1, "cC": 2, "cD": 1,
         "gA": 0.0, "gB": 0.0, "gC": 2.0, "gD": 0.0,
         "pre": 7.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 12000.0, "autogain": 1, "mix": 0.9, "out": -0.5 }"#,
    r#"{ "name": "Dust On The Reel", "category": "Warm",
         "x": 0.35, "y": 0.25,
         "cA": 1, "cB": 2, "cC": 0, "cD": 1,
         "gA": 1.0, "gB": 2.0, "gC": 0.0, "gD": 1.0,
         "pre": 8.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 10000.0, "autogain": 1, "mix": 0.8, "out": 0.0 }"#,
    // ---- Morph ------------------------------------------------------------
    r#"{ "name": "Cynthoni Haze", "category": "Morph",
         "x": 0.45, "y": 0.55,
         "cA": 0, "cB": 4, "cC": 6, "cD": 5,
         "gA": 0.0, "gB": 2.0, "gC": 1.0, "gD": 3.0,
         "pre": 10.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 14000.0, "autogain": 1, "mix": 0.9, "out": -1.0 }"#,
    r#"{ "name": "Glass Membrane", "category": "Morph",
         "x": 0.60, "y": 0.40,
         "cA": 6, "cB": 0, "cC": 4, "cD": 6,
         "gA": 0.0, "gB": 0.0, "gC": 1.0, "gD": 2.0,
         "pre": 9.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 16000.0, "autogain": 1, "mix": 0.85, "out": 0.0 }"#,
    r#"{ "name": "Velvet Sinew", "category": "Morph",
         "x": 0.40, "y": 0.65,
         "cA": 0, "cB": 1, "cC": 5, "cD": 4,
         "gA": 1.0, "gB": 0.0, "gC": 2.0, "gD": 2.0,
         "pre": 11.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 13000.0, "autogain": 1, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Fold Cathedral", "category": "Morph",
         "x": 0.55, "y": 0.60,
         "cA": 4, "cB": 5, "cC": 4, "cD": 5,
         "gA": 0.0, "gB": 2.0, "gC": 1.0, "gD": 2.0,
         "pre": 12.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 11000.0, "autogain": 1, "mix": 0.9, "out": -1.5 }"#,
    // ---- Orbit ------------------------------------------------------------
    r#"{ "name": "Tidal Drift", "category": "Orbit",
         "x": 0.50, "y": 0.50,
         "cA": 0, "cB": 1, "cC": 6, "cD": 4,
         "gA": 0.0, "gB": 1.0, "gC": 1.0, "gD": 2.0,
         "pre": 9.0, "orbit": 1, "orate": 0.15, "osync": 0, "odiv": 1,
         "oradius": 0.30, "oshape": 0, "ophase": 0.0,
         "postlp": 14000.0, "autogain": 1, "mix": 0.9, "out": -1.0 }"#,
    r#"{ "name": "Sewer Current", "category": "Orbit",
         "x": 0.50, "y": 0.45,
         "cA": 2, "cB": 3, "cC": 0, "cD": 5,
         "gA": 2.0, "gB": 1.0, "gC": 0.0, "gD": 3.0,
         "pre": 12.0, "orbit": 1, "orate": 0.25, "osync": 0, "odiv": 1,
         "oradius": 0.35, "oshape": 1, "ophase": 0.0,
         "postlp": 9000.0, "autogain": 1, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Barbed Pendulum", "category": "Orbit",
         "x": 0.50, "y": 0.50,
         "cA": 3, "cB": 6, "cC": 2, "cD": 4,
         "gA": 2.0, "gB": 0.0, "gC": 2.0, "gD": 3.0,
         "pre": 13.0, "orbit": 1, "orate": 0.5, "osync": 1, "odiv": 2,
         "oradius": 0.40, "oshape": 1, "ophase": 0.0,
         "postlp": 10000.0, "autogain": 1, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Lunar Wobble", "category": "Orbit",
         "x": 0.50, "y": 0.55,
         "cA": 1, "cB": 4, "cC": 5, "cD": 0,
         "gA": 0.0, "gB": 2.0, "gC": 2.0, "gD": 0.0,
         "pre": 10.0, "orbit": 1, "orate": 0.5, "osync": 1, "odiv": 3,
         "oradius": 0.45, "oshape": 0, "ophase": 0.25,
         "postlp": 12000.0, "autogain": 1, "mix": 0.9, "out": -1.0 }"#,
    // ---- Digital ----------------------------------------------------------
    r#"{ "name": "Chlorine Bloom", "category": "Digital",
         "x": 0.40, "y": 0.60,
         "cA": 7, "cB": 3, "cC": 7, "cD": 5,
         "gA": 0.0, "gB": 3.0, "gC": 0.0, "gD": 2.0,
         "pre": 9.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 8000.0, "autogain": 1, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Datamosh Ritual", "category": "Digital",
         "x": 0.55, "y": 0.50,
         "cA": 6, "cB": 7, "cC": 3, "cD": 7,
         "gA": 0.0, "gB": 2.0, "gC": 3.0, "gD": 1.0,
         "pre": 11.0, "orbit": 1, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.25, "oshape": 0, "ophase": 0.0,
         "postlp": 7000.0, "autogain": 1, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Sub Rosa Static", "category": "Digital",
         "x": 0.35, "y": 0.40,
         "cA": 2, "cB": 7, "cC": 3, "cD": 3,
         "gA": 1.0, "gB": 2.0, "gC": 0.0, "gD": 2.0,
         "pre": 12.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 6000.0, "autogain": 1, "mix": 0.95, "out": -1.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    r#"{ "name": "Total Erosion", "category": "Extreme",
         "x": 0.50, "y": 0.50,
         "cA": 3, "cB": 5, "cC": 6, "cD": 7,
         "gA": 3.0, "gB": 2.0, "gC": 4.0, "gD": 1.0,
         "pre": 18.0, "orbit": 1, "orate": 2.0, "osync": 0, "odiv": 1,
         "oradius": 0.50, "oshape": 1, "ophase": 0.0,
         "postlp": 10000.0, "autogain": 1, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Rusted Godhead", "category": "Extreme",
         "x": 0.50, "y": 0.50,
         "cA": 5, "cB": 3, "cC": 5, "cD": 3,
         "gA": 4.0, "gB": 2.0, "gC": 4.0, "gD": 2.0,
         "pre": 20.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 8000.0, "autogain": 1, "mix": 1.0, "out": -3.0 }"#,
    r#"{ "name": "Screech Liturgy", "category": "Extreme",
         "x": 0.60, "y": 0.50,
         "cA": 6, "cB": 5, "cC": 6, "cD": 5,
         "gA": 2.0, "gB": 3.0, "gC": 2.0, "gD": 3.0,
         "pre": 22.0, "orbit": 1, "orate": 6.0, "osync": 0, "odiv": 1,
         "oradius": 0.40, "oshape": 0, "ophase": 0.0,
         "postlp": 16000.0, "autogain": 1, "mix": 0.9, "out": -2.5 }"#,
    r#"{ "name": "The Drowning Machine", "category": "Extreme",
         "x": 0.50, "y": 0.50,
         "cA": 2, "cB": 3, "cC": 3, "cD": 7,
         "gA": 4.0, "gB": 4.0, "gC": 3.0, "gD": 2.0,
         "pre": 24.0, "orbit": 1, "orate": 0.5, "osync": 1, "odiv": 0,
         "oradius": 0.50, "oshape": 1, "ophase": 0.0,
         "postlp": 5000.0, "autogain": 1, "mix": 1.0, "out": -3.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    let corner = [
        Corner::from_index(g("cA", 0.0) as usize),
        Corner::from_index(g("cB", 1.0) as usize),
        Corner::from_index(g("cC", 6.0) as usize),
        Corner::from_index(g("cD", 3.0) as usize),
    ];
    let gain_db = [g("gA", 0.0), g("gB", 0.0), g("gC", 0.0), g("gD", 0.0)];
    debug_assert_eq!(corner.len(), NUM_CORNERS);

    Settings {
        x: g("x", d.x),
        y: g("y", d.y),
        corner,
        gain_db,
        pre_db: g("pre", d.pre_db),
        orbit_on: g("orbit", 0.0) >= 0.5,
        orbit_rate_hz: g("orate", d.orbit_rate_hz),
        orbit_sync: g("osync", 0.0) >= 0.5,
        orbit_div: SyncDivision::from_index(g("odiv", 1.0) as usize),
        orbit_radius: g("oradius", d.orbit_radius),
        orbit_shape: OrbitShape::from_index(g("oshape", 0.0) as usize),
        orbit_phase0: g("ophase", 0.0),
        tempo_bpm: 120.0,
        post_lp_hz: g("postlp", d.post_lp_hz),
        auto_gain: g("autogain", 0.0) >= 0.5,
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/bools by equality,
    /// floats by a loose epsilon). Each of the four corner shapers and four corner gains is
    /// counted individually; `tempo_bpm` is a fixed constant and is skipped. Drives both the
    /// differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.orbit_on != b.orbit_on { n += 1; }
        if a.orbit_sync != b.orbit_sync { n += 1; }
        if a.orbit_div != b.orbit_div { n += 1; }
        if a.orbit_shape != b.orbit_shape { n += 1; }
        if a.auto_gain != b.auto_gain { n += 1; }
        for i in 0..NUM_CORNERS {
            if a.corner[i] != b.corner[i] { n += 1; }
            if (a.gain_db[i] - b.gain_db[i]).abs() > 1e-3 { n += 1; }
        }
        let fs = [
            (a.x, b.x), (a.y, b.y), (a.pre_db, b.pre_db),
            (a.orbit_rate_hz, b.orbit_rate_hz), (a.orbit_radius, b.orbit_radius),
            (a.orbit_phase0, b.orbit_phase0), (a.post_lp_hz, b.post_lp_hz),
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
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 15, "SHAPESHIFT bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised and differs
        // from the default in >= 4 params.
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
