//! SHAPESHIFT factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `x`/`y` XY point 0..1; `cA`..`cD` corner shaper index 0..7
//! (Tube, Tape, Diode, Hard, SineFold, TriFold, Cheby3, BitSoft); `gA`..`gD` per-corner gain dB;
//! `pre` pre-gain dB; `orbit` 0/1; `orate` Hz; `osync` 0/1; `odiv` 0..3 (1/2,1 bar,2 bar,4 bar);
//! `oradius` 0..0.5; `oshape` 0/1 (circle/figure-8); `ophase` 0..1; `postlp` Hz; `autogain` 0/1;
//! `mix` 0..1; `out` dB.

use crate::dsp::{Corner, OrbitShape, Settings, SyncDivision, NUM_CORNERS};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, build brief).
pub const PRESET_JSON: &[&str] = &[
    // Morph between warm tube/tape and sine/triangle folding across the pad.
    r#"{ "name": "Warm-Fold Morph", "category": "Morph",
         "x": 0.30, "y": 0.40,
         "cA": 0, "cB": 4, "cC": 1, "cD": 5,
         "gA": 0.0, "gB": 2.0, "gC": 0.0, "gD": 3.0,
         "pre": 8.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 14000.0, "autogain": 1, "mix": 1.0, "out": 0.0 }"#,
    // Asymmetric diode drive with a slow circular orbit for a breathing edge.
    r#"{ "name": "Diode Drive Orbit", "category": "Orbit",
         "x": 0.50, "y": 0.50,
         "cA": 2, "cB": 0, "cC": 2, "cD": 3,
         "gA": 2.0, "gB": 0.0, "gC": 2.0, "gD": 4.0,
         "pre": 12.0, "orbit": 1, "orate": 0.3, "osync": 0, "odiv": 1,
         "oradius": 0.35, "oshape": 0, "ophase": 0.0,
         "postlp": 12000.0, "autogain": 1, "mix": 1.0, "out": -1.0 }"#,
    // Chebyshev 3rd-harmonic shimmer with a slow figure-8 wander.
    r#"{ "name": "Cheby Shimmer", "category": "Morph",
         "x": 0.60, "y": 0.50,
         "cA": 6, "cB": 0, "cC": 4, "cD": 6,
         "gA": 0.0, "gB": 0.0, "gC": 1.0, "gD": 2.0,
         "pre": 10.0, "orbit": 1, "orate": 0.2, "osync": 0, "odiv": 2,
         "oradius": 0.3, "oshape": 1, "ophase": 0.0,
         "postlp": 18000.0, "autogain": 1, "mix": 0.8, "out": 0.0 }"#,
    // Soft-crush + fold digital grit, darker post filter.
    r#"{ "name": "Bit Edge", "category": "Digital",
         "x": 0.40, "y": 0.60,
         "cA": 7, "cB": 3, "cC": 7, "cD": 5,
         "gA": 0.0, "gB": 3.0, "gC": 0.0, "gD": 2.0,
         "pre": 9.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 8000.0, "autogain": 1, "mix": 1.0, "out": -1.0 }"#,
    // Gentle tape saturation parked near a warm corner.
    r#"{ "name": "Tape Corner", "category": "Warm",
         "x": 0.20, "y": 0.30,
         "cA": 1, "cB": 1, "cC": 0, "cD": 1,
         "gA": 0.0, "gB": 0.0, "gC": 0.0, "gD": 0.0,
         "pre": 6.0, "orbit": 0, "orate": 0.5, "osync": 0, "odiv": 1,
         "oradius": 0.3, "oshape": 0, "ophase": 0.0,
         "postlp": 16000.0, "autogain": 1, "mix": 0.9, "out": 0.0 }"#,
    // Everything harsh, fast tempo-free figure-8 — maximum morphing chaos.
    r#"{ "name": "Full Chaos Orbit", "category": "Extreme",
         "x": 0.50, "y": 0.50,
         "cA": 3, "cB": 6, "cC": 5, "cD": 7,
         "gA": 3.0, "gB": 2.0, "gC": 4.0, "gD": 1.0,
         "pre": 15.0, "orbit": 1, "orate": 2.0, "osync": 0, "odiv": 0,
         "oradius": 0.5, "oshape": 1, "ophase": 0.0,
         "postlp": 10000.0, "autogain": 1, "mix": 1.0, "out": -2.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.x - d.x).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.y - d.y).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.pre_db - d.pre_db).abs() > 1e-3 {
                diffs += 1;
            }
            if s.corner != d.corner {
                diffs += 1;
            }
            if s.orbit_on != d.orbit_on {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.post_lp_hz - d.post_lp_hz).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
