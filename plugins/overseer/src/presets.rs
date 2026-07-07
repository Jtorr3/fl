//! OVERSEER factory presets — a set for the Node strip and a set for the Master bus
//! (≥5 across the pair, PRD §1.4 step 6). Flat-JSON blobs parsed by `suite_core::presets`;
//! the same lists drive the GUI selectors and the offline render tests. Values are plain
//! (dB, Hz, ratio, 0..1 mix).

use suite_core::presets::Preset;

use crate::eq::EqSettings;
use crate::master::{BandComp, MasterSettings};
use crate::node::NodeSettings;

/// Node-strip presets (menu order).
pub const NODE_PRESET_JSON: &[&str] = &[
    r#"{ "name": "Kick Strip",
         "low_freq": 60.0, "low_gain": 3.0, "b1_freq": 400.0, "b1_gain": -3.0, "b1_q": 1.2,
         "b2_freq": 3000.0, "b2_gain": 2.0, "b2_q": 1.0, "high_freq": 9000.0, "high_gain": 0.0,
         "threshold": -18.0, "ratio": 4.0, "knee": 6.0, "attack": 5.0, "release": 120.0,
         "makeup": 3.0, "drive": 4.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Vocal Strip",
         "low_freq": 100.0, "low_gain": -2.0, "b1_freq": 350.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 5000.0, "b2_gain": 3.0, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 2.0,
         "threshold": -22.0, "ratio": 3.0, "knee": 8.0, "attack": 10.0, "release": 150.0,
         "makeup": 3.0, "drive": 2.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Bus Glue",
         "low_freq": 90.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 2500.0, "b2_gain": 1.0, "b2_q": 0.7, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -20.0, "ratio": 2.0, "knee": 10.0, "attack": 25.0, "release": 200.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.2, "trim": 0.0, "mix": 1.0 }"#,
];

/// Master-bus presets (menu order).
pub const MASTER_PRESET_JSON: &[&str] = &[
    r#"{ "name": "Techno Master",
         "low_freq": 50.0, "low_gain": 1.5, "b1_freq": 300.0, "b1_gain": -1.5, "b1_q": 1.0,
         "b2_freq": 3500.0, "b2_gain": 1.5, "b2_q": 0.8, "high_freq": 10000.0, "high_gain": 2.0,
         "xo_low": 150.0, "xo_high": 2800.0,
         "b1_thr": -20.0, "b1_ratio": 2.5, "b1_makeup": 2.0,
         "b2_thr": -18.0, "b2_ratio": 2.0, "b2_makeup": 1.5,
         "b3_thr": -16.0, "b3_ratio": 2.0, "b3_makeup": 1.5,
         "knee": 6.0, "attack": 10.0, "release": 140.0,
         "ceiling": -1.0, "lim_release": 80.0, "mix": 1.0 }"#,
    r#"{ "name": "Gentle Master",
         "low_freq": 60.0, "low_gain": 0.5, "b1_freq": 400.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 4000.0, "b2_gain": 0.5, "b2_q": 0.7, "high_freq": 12000.0, "high_gain": 1.0,
         "xo_low": 200.0, "xo_high": 3000.0,
         "b1_thr": -22.0, "b1_ratio": 1.6, "b1_makeup": 1.0,
         "b2_thr": -22.0, "b2_ratio": 1.6, "b2_makeup": 1.0,
         "b3_thr": -22.0, "b3_ratio": 1.6, "b3_makeup": 1.0,
         "knee": 10.0, "attack": 25.0, "release": 220.0,
         "ceiling": -1.0, "lim_release": 150.0, "mix": 1.0 }"#,
    r#"{ "name": "Loud & Proud",
         "low_freq": 45.0, "low_gain": 2.0, "b1_freq": 500.0, "b1_gain": -2.0, "b1_q": 1.1,
         "b2_freq": 3000.0, "b2_gain": 2.5, "b2_q": 0.8, "high_freq": 9000.0, "high_gain": 3.0,
         "xo_low": 120.0, "xo_high": 2500.0,
         "b1_thr": -26.0, "b1_ratio": 3.0, "b1_makeup": 4.0,
         "b2_thr": -24.0, "b2_ratio": 3.0, "b2_makeup": 4.0,
         "b3_thr": -22.0, "b3_ratio": 3.0, "b3_makeup": 4.0,
         "knee": 4.0, "attack": 5.0, "release": 100.0,
         "ceiling": -0.3, "lim_release": 60.0, "mix": 1.0 }"#,
];

fn eq_from(p: &Preset, d: &EqSettings) -> EqSettings {
    let g = |k: &str, f: f32| p.get(k).unwrap_or(f);
    EqSettings {
        low_freq: g("low_freq", d.low_freq),
        low_gain: g("low_gain", d.low_gain),
        b1_freq: g("b1_freq", d.b1_freq),
        b1_gain: g("b1_gain", d.b1_gain),
        b1_q: g("b1_q", d.b1_q),
        b2_freq: g("b2_freq", d.b2_freq),
        b2_gain: g("b2_gain", d.b2_gain),
        b2_q: g("b2_q", d.b2_q),
        high_freq: g("high_freq", d.high_freq),
        high_gain: g("high_gain", d.high_gain),
    }
}

/// Build [`NodeSettings`] from a parsed Node preset (defaults fill omitted keys).
pub fn node_settings_from_preset(p: &Preset) -> NodeSettings {
    let d = NodeSettings::default();
    let g = |k: &str, f: f32| p.get(k).unwrap_or(f);
    NodeSettings {
        eq: eq_from(p, &d.eq),
        comp_threshold: g("threshold", d.comp_threshold),
        comp_ratio: g("ratio", d.comp_ratio),
        comp_knee: g("knee", d.comp_knee),
        comp_attack: g("attack", d.comp_attack),
        comp_release: g("release", d.comp_release),
        comp_makeup: g("makeup", d.comp_makeup),
        drive_db: g("drive", d.drive_db),
        width: g("width", d.width),
        trim_db: g("trim", d.trim_db),
        mix: g("mix", d.mix),
    }
}

/// Build [`MasterSettings`] from a parsed Master preset.
pub fn master_settings_from_preset(p: &Preset) -> MasterSettings {
    let d = MasterSettings::default();
    let g = |k: &str, f: f32| p.get(k).unwrap_or(f);
    MasterSettings {
        eq: eq_from(p, &d.eq),
        xo_low: g("xo_low", d.xo_low),
        xo_high: g("xo_high", d.xo_high),
        bands: [
            BandComp {
                threshold: g("b1_thr", d.bands[0].threshold),
                ratio: g("b1_ratio", d.bands[0].ratio),
                makeup: g("b1_makeup", d.bands[0].makeup),
            },
            BandComp {
                threshold: g("b2_thr", d.bands[1].threshold),
                ratio: g("b2_ratio", d.bands[1].ratio),
                makeup: g("b2_makeup", d.bands[1].makeup),
            },
            BandComp {
                threshold: g("b3_thr", d.bands[2].threshold),
                ratio: g("b3_ratio", d.bands[2].ratio),
                makeup: g("b3_makeup", d.bands[2].makeup),
            },
        ],
        comp_knee: g("knee", d.comp_knee),
        comp_attack: g("attack", d.comp_attack),
        comp_release: g("release", d.comp_release),
        ceiling_db: g("ceiling", d.ceiling_db),
        limiter_release: g("lim_release", d.limiter_release),
        mix: g("mix", d.mix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    #[test]
    fn at_least_five_presets_across_the_pair() {
        let n = load_all(NODE_PRESET_JSON).len();
        let m = load_all(MASTER_PRESET_JSON).len();
        assert!(n + m >= 5, "need >=5 presets across the pair, got {}", n + m);
    }

    #[test]
    fn node_presets_differ_from_default() {
        let d = NodeSettings::default();
        for p in load_all(NODE_PRESET_JSON).iter() {
            let s = node_settings_from_preset(p);
            let mut diffs = 0;
            if (s.comp_threshold - d.comp_threshold).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.comp_ratio - d.comp_ratio).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.drive_db - d.drive_db).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.width - d.width).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.eq.low_gain - d.eq.low_gain).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "node preset '{}' differs in only {diffs}", p.name);
        }
    }

    #[test]
    fn master_presets_differ_from_default() {
        let d = MasterSettings::default();
        for p in load_all(MASTER_PRESET_JSON).iter() {
            let s = master_settings_from_preset(p);
            let mut diffs = 0;
            if (s.xo_low - d.xo_low).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.bands[0].ratio - d.bands[0].ratio).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.ceiling_db - d.ceiling_db).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.eq.high_gain - d.eq.high_gain).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "master preset '{}' differs in only {diffs}", p.name);
        }
    }
}
