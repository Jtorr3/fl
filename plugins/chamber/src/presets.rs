//! CHAMBER factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain): `w`/`d`/`h` metres (room, 2–40 / 2–40 / 2–20); `sx..sz`/`lx..lz`
//! metres (source / listener, clamped inside the room); `matw`/`matf`/`matc` material indices
//! (0 concrete, 1 wood, 2 curtain, 3 glass); `order` ER order (1/2/3); `balance` ER↔late (0..1,
//! 0 = only early reflections, 1 = only late field); `distance` inverse-distance rolloff
//! exaggeration (0.5..3); `predelay` seconds (0..0.2); `rt60` override seconds (0 = Sabine auto,
//! else 0.1..12); `width` 0..2; `mix` dry/wet 0..1; `out` dB (kept ≤ 0 for headroom — the wet
//! path is safety-clipped and levels are conservative, so every preset stays ≤ 0 dBFS; the
//! final ±8.0 output clamp is only a runaway/NaN guard, not a level ceiling).
//!
//! Categories (preset-bar sections, first-appearance order): Rooms & Booths / Halls & Chambers /
//! Cavernous / Textural / Broken. Names are purpose-driven and evocative (dark-techno /
//! atmospheric-dnb space taste) — never settings descriptions.

use crate::dsp::{Material, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Rooms & Booths ---------------------------------------------------
    r#"{ "name": "Vocal Isolation", "category": "Rooms & Booths",
         "w": 2.6, "d": 3.2, "h": 2.4, "sx": 0.8, "sy": 0.8, "sz": 1.5,
         "lx": 1.8, "ly": 2.4, "lz": 1.5, "matw": 2, "matf": 2, "matc": 2,
         "order": 3, "balance": 0.25, "distance": 1.0, "predelay": 0.0, "rt60": 0.0,
         "width": 0.7, "mix": 0.28, "out": 0.0 }"#,
    r#"{ "name": "Rehearsal Corner", "category": "Rooms & Booths",
         "w": 4.5, "d": 5.5, "h": 3.0, "sx": 1.4, "sy": 1.6, "sz": 1.6,
         "lx": 3.0, "ly": 4.2, "lz": 1.6, "matw": 1, "matf": 1, "matc": 1,
         "order": 3, "balance": 0.4, "distance": 1.0, "predelay": 0.006, "rt60": 0.0,
         "width": 0.95, "mix": 0.32, "out": 0.0 }"#,
    r#"{ "name": "Live Drum Room", "category": "Rooms & Booths",
         "w": 4.2, "d": 5.0, "h": 3.2, "sx": 1.2, "sy": 1.2, "sz": 1.5,
         "lx": 2.8, "ly": 3.6, "lz": 1.5, "matw": 1, "matf": 0, "matc": 1,
         "order": 3, "balance": 0.35, "distance": 1.4, "predelay": 0.0, "rt60": 0.0,
         "width": 0.9, "mix": 0.32, "out": 0.0 }"#,
    r#"{ "name": "Tile Bathroom", "category": "Rooms & Booths",
         "w": 2.8, "d": 3.4, "h": 2.6, "sx": 0.9, "sy": 1.0, "sz": 1.5,
         "lx": 1.9, "ly": 2.6, "lz": 1.5, "matw": 3, "matf": 0, "matc": 3,
         "order": 3, "balance": 0.45, "distance": 1.1, "predelay": 0.0, "rt60": 0.0,
         "width": 1.0, "mix": 0.35, "out": -0.5 }"#,
    // ---- Halls & Chambers -------------------------------------------------
    r#"{ "name": "Chamber Strings", "category": "Halls & Chambers",
         "w": 9.0, "d": 12.0, "h": 5.0, "sx": 2.6, "sy": 2.4, "sz": 1.7,
         "lx": 6.0, "ly": 9.0, "lz": 1.7, "matw": 1, "matf": 1, "matc": 1,
         "order": 3, "balance": 0.5, "distance": 1.0, "predelay": 0.012, "rt60": 0.0,
         "width": 1.1, "mix": 0.35, "out": -0.5 }"#,
    r#"{ "name": "Concert Stage", "category": "Halls & Chambers",
         "w": 16.0, "d": 20.0, "h": 9.0, "sx": 4.0, "sy": 4.0, "sz": 1.8,
         "lx": 11.0, "ly": 15.0, "lz": 1.8, "matw": 0, "matf": 1, "matc": 0,
         "order": 3, "balance": 0.6, "distance": 1.1, "predelay": 0.02, "rt60": 0.0,
         "width": 1.3, "mix": 0.4, "out": -1.0 }"#,
    r#"{ "name": "Stone Chapel", "category": "Halls & Chambers",
         "w": 12.0, "d": 22.0, "h": 12.0, "sx": 3.2, "sy": 4.0, "sz": 1.8,
         "lx": 8.0, "ly": 16.0, "lz": 1.6, "matw": 0, "matf": 0, "matc": 3,
         "order": 3, "balance": 0.65, "distance": 1.2, "predelay": 0.03, "rt60": 0.0,
         "width": 1.4, "mix": 0.42, "out": -1.0 }"#,
    r#"{ "name": "Marble Foyer", "category": "Halls & Chambers",
         "w": 10.0, "d": 14.0, "h": 6.0, "sx": 2.8, "sy": 3.0, "sz": 1.7,
         "lx": 6.5, "ly": 10.0, "lz": 1.7, "matw": 3, "matf": 3, "matc": 3,
         "order": 3, "balance": 0.55, "distance": 1.1, "predelay": 0.015, "rt60": 0.0,
         "width": 1.25, "mix": 0.38, "out": -1.0 }"#,
    // ---- Cavernous --------------------------------------------------------
    r#"{ "name": "Cathedral Vault", "category": "Cavernous",
         "w": 18.0, "d": 40.0, "h": 18.0, "sx": 5.0, "sy": 6.0, "sz": 2.0,
         "lx": 12.0, "ly": 30.0, "lz": 1.6, "matw": 0, "matf": 0, "matc": 3,
         "order": 3, "balance": 0.72, "distance": 1.3, "predelay": 0.04, "rt60": 0.0,
         "width": 1.5, "mix": 0.45, "out": -1.5 }"#,
    r#"{ "name": "Abandoned Reservoir", "category": "Cavernous",
         "w": 30.0, "d": 38.0, "h": 14.0, "sx": 6.0, "sy": 6.0, "sz": 1.8,
         "lx": 22.0, "ly": 30.0, "lz": 1.8, "matw": 0, "matf": 0, "matc": 0,
         "order": 3, "balance": 0.8, "distance": 1.5, "predelay": 0.05, "rt60": 8.0,
         "width": 1.4, "mix": 0.48, "out": -2.0 }"#,
    r#"{ "name": "Cavern Deep", "category": "Cavernous",
         "w": 24.0, "d": 34.0, "h": 16.0, "sx": 4.0, "sy": 4.0, "sz": 1.8,
         "lx": 18.0, "ly": 28.0, "lz": 1.8, "matw": 1, "matf": 0, "matc": 0,
         "order": 3, "balance": 0.82, "distance": 2.2, "predelay": 0.06, "rt60": 0.0,
         "width": 1.45, "mix": 0.5, "out": -1.5 }"#,
    // ---- Textural ---------------------------------------------------------
    r#"{ "name": "Fog Bank", "category": "Textural",
         "w": 14.0, "d": 20.0, "h": 8.0, "sx": 3.5, "sy": 4.0, "sz": 1.7,
         "lx": 10.0, "ly": 15.0, "lz": 1.7, "matw": 2, "matf": 2, "matc": 1,
         "order": 3, "balance": 0.9, "distance": 1.3, "predelay": 0.05, "rt60": 6.0,
         "width": 1.6, "mix": 0.5, "out": -2.0 }"#,
    r#"{ "name": "Distant Choir", "category": "Textural",
         "w": 16.0, "d": 24.0, "h": 10.0, "sx": 3.0, "sy": 3.0, "sz": 1.8,
         "lx": 13.0, "ly": 20.0, "lz": 1.8, "matw": 1, "matf": 0, "matc": 2,
         "order": 3, "balance": 0.85, "distance": 2.5, "predelay": 0.05, "rt60": 0.0,
         "width": 1.55, "mix": 0.5, "out": -1.5 }"#,
    r#"{ "name": "Drifting Ambience", "category": "Textural",
         "w": 11.0, "d": 16.0, "h": 6.0, "sx": 3.0, "sy": 3.5, "sz": 1.7,
         "lx": 7.0, "ly": 11.0, "lz": 1.7, "matw": 1, "matf": 1, "matc": 2,
         "order": 3, "balance": 0.78, "distance": 1.4, "predelay": 0.03, "rt60": 0.0,
         "width": 1.8, "mix": 0.45, "out": -1.0 }"#,
    r#"{ "name": "Ghost Pad", "category": "Textural",
         "w": 13.0, "d": 18.0, "h": 7.0, "sx": 3.2, "sy": 3.5, "sz": 1.7,
         "lx": 8.5, "ly": 13.5, "lz": 1.7, "matw": 2, "matf": 2, "matc": 2,
         "order": 3, "balance": 0.88, "distance": 1.6, "predelay": 0.04, "rt60": 7.5,
         "width": 1.7, "mix": 0.48, "out": -2.0 }"#,
    // ---- Broken -----------------------------------------------------------
    r#"{ "name": "Sewer Runoff", "category": "Broken",
         "w": 8.0, "d": 30.0, "h": 5.0, "sx": 2.0, "sy": 3.0, "sz": 1.4,
         "lx": 5.0, "ly": 24.0, "lz": 1.4, "matw": 2, "matf": 0, "matc": 0,
         "order": 3, "balance": 0.7, "distance": 2.8, "predelay": 0.08, "rt60": 9.0,
         "width": 1.2, "mix": 0.55, "out": -2.5 }"#,
    r#"{ "name": "Concrete Nightmare", "category": "Broken",
         "w": 40.0, "d": 40.0, "h": 20.0, "sx": 6.0, "sy": 6.0, "sz": 2.0,
         "lx": 30.0, "ly": 34.0, "lz": 2.0, "matw": 0, "matf": 0, "matc": 3,
         "order": 3, "balance": 0.75, "distance": 3.0, "predelay": 0.12, "rt60": 12.0,
         "width": 2.0, "mix": 0.55, "out": -3.0 }"#,
    r#"{ "name": "Collapsing Space", "category": "Broken",
         "w": 3.0, "d": 6.0, "h": 18.0, "sx": 1.0, "sy": 1.5, "sz": 2.0,
         "lx": 2.0, "ly": 4.5, "lz": 15.0, "matw": 3, "matf": 3, "matc": 3,
         "order": 3, "balance": 0.6, "distance": 2.5, "predelay": 0.02, "rt60": 0.0,
         "width": 2.0, "mix": 0.5, "out": -2.0 }"#,
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

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums / order / bools by
    /// equality, floats by a loose epsilon). Covers EVERY field `settings_from_preset` sets, so
    /// it drives both the differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.mat_walls != b.mat_walls { n += 1; }
        if a.mat_floor != b.mat_floor { n += 1; }
        if a.mat_ceiling != b.mat_ceiling { n += 1; }
        if a.er_order != b.er_order { n += 1; }
        let fs = [
            (a.w, b.w), (a.d, b.d), (a.h, b.h),
            (a.src_x, b.src_x), (a.src_y, b.src_y), (a.src_z, b.src_z),
            (a.lis_x, b.lis_x), (a.lis_y, b.lis_y), (a.lis_z, b.lis_z),
            (a.er_late, b.er_late), (a.distance, b.distance), (a.predelay, b.predelay),
            (a.rt60_override, b.rt60_override), (a.width, b.width), (a.mix, b.mix),
            (a.out_db, b.out_db),
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
        assert!(presets.len() >= 15, "CHAMBER bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised and
        // differs from the default in >= 4 params.
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
        // `presets_pass_universal` test in tests.rs.
    }
}
