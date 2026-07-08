//! CHAMBER factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `w`/`d`/`h` metres; `sx..sz`/`lx..lz` metres (source/listener);
//! `matw`/`matf`/`matc` material indices (0 concrete, 1 wood, 2 curtain, 3 glass); `order` ER
//! order (1/2/3); `balance` ER↔late (0..1); `distance` rolloff exaggeration; `predelay` seconds;
//! `rt60` override seconds (0 = Sabine auto); `width`/`mix` 0..1(-2); `out` dB.

use crate::dsp::{Material, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Tiny absorptive vocal booth — dead, ER-forward, negligible tail.
    r#"{ "name": "Small Dead Booth", "w": 2.5, "d": 3.0, "h": 2.4,
         "sx": 0.8, "sy": 0.8, "sz": 1.5, "lx": 1.7, "ly": 2.2, "lz": 1.5,
         "matw": 2, "matf": 1, "matc": 2, "order": 3, "balance": 0.3,
         "distance": 1.0, "predelay": 0.0, "rt60": 0.0, "width": 0.7, "mix": 0.3, "out": 0.0 }"#,
    // Warm mid wooden room — natural, balanced early+late.
    r#"{ "name": "Wood Room", "w": 5.0, "d": 6.5, "h": 3.2,
         "sx": 1.5, "sy": 1.8, "sz": 1.6, "lx": 3.5, "ly": 4.8, "lz": 1.6,
         "matw": 1, "matf": 1, "matc": 1, "order": 3, "balance": 0.45,
         "distance": 1.0, "predelay": 0.008, "rt60": 0.0, "width": 1.0, "mix": 0.35, "out": 0.0 }"#,
    // Big live concrete warehouse — long, bright, roomy.
    r#"{ "name": "Warehouse", "w": 22.0, "d": 32.0, "h": 8.5,
         "sx": 6.0, "sy": 6.0, "sz": 1.8, "lx": 15.0, "ly": 24.0, "lz": 1.8,
         "matw": 0, "matf": 0, "matc": 0, "order": 3, "balance": 0.6,
         "distance": 1.2, "predelay": 0.02, "rt60": 0.0, "width": 1.3, "mix": 0.4, "out": -1.0 }"#,
    // Cathedral-ish stone/glass — very long, huge, late-dominated.
    r#"{ "name": "Cathedral-ish", "w": 18.0, "d": 40.0, "h": 18.0,
         "sx": 5.0, "sy": 6.0, "sz": 2.0, "lx": 12.0, "ly": 30.0, "lz": 1.6,
         "matw": 0, "matf": 0, "matc": 3, "order": 3, "balance": 0.72,
         "distance": 1.3, "predelay": 0.04, "rt60": 0.0, "width": 1.5, "mix": 0.45, "out": -1.5 }"#,
    // Tight punchy drum room — small, ER-forward, snappy tail.
    r#"{ "name": "Tight Drum Room", "w": 4.2, "d": 5.0, "h": 3.0,
         "sx": 1.2, "sy": 1.2, "sz": 1.5, "lx": 2.8, "ly": 3.6, "lz": 1.5,
         "matw": 1, "matf": 0, "matc": 1, "order": 3, "balance": 0.35,
         "distance": 1.4, "predelay": 0.0, "rt60": 0.0, "width": 0.9, "mix": 0.3, "out": 0.0 }"#,
    // Distant hall — listener far from source, late-heavy, exaggerated distance.
    r#"{ "name": "Distant Hall", "w": 16.0, "d": 24.0, "h": 10.0,
         "sx": 3.0, "sy": 3.0, "sz": 1.8, "lx": 13.0, "ly": 20.0, "lz": 1.8,
         "matw": 1, "matf": 0, "matc": 0, "order": 3, "balance": 0.8,
         "distance": 2.0, "predelay": 0.05, "rt60": 0.0, "width": 1.4, "mix": 0.5, "out": -1.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let mat = |k: &str, fallback: Material| {
        p.get(k)
            .map(|v| Material::from_index(v as usize))
            .unwrap_or(fallback)
    };
    Settings {
        w: g("w", d.w),
        d: g("d", d.d),
        h: g("h", d.h),
        src_x: g("sx", d.src_x),
        src_y: g("sy", d.src_y),
        src_z: g("sz", d.src_z),
        lis_x: g("lx", d.lis_x),
        lis_y: g("ly", d.lis_y),
        lis_z: g("lz", d.lis_z),
        mat_walls: mat("matw", d.mat_walls),
        mat_floor: mat("matf", d.mat_floor),
        mat_ceiling: mat("matc", d.mat_ceiling),
        er_order: g("order", d.er_order as f32) as usize,
        er_late: g("balance", d.er_late),
        distance: g("distance", d.distance),
        predelay: g("predelay", d.predelay),
        rt60_override: g("rt60", d.rt60_override),
        width: g("width", d.width),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}
