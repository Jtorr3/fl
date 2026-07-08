//! SWARM factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded flat-JSON
//! blob parsed by `suite_core::presets`; the same list drives the GUI selector (grouped by the
//! `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain): `density` grains/s (1..500); `size`/`spray` ms (10..500 / 0..500);
//! `scatter` st (0..24); `reverse`/`shimmer`/`width`/`mix` 0..1 (`shimmer` up to 1.1); `out` dB;
//! `quantize`/`sync`/`freeze` 0/1; `division` 0..6 (1/16,1/8,1/8·,1/4,1/4·,1/2,bar).
//!
//! Categories (preset-bar sections): Ambient Beds / Shimmer Cathedrals / Rhythmic Swarms /
//! Reverse & Smear / Extreme. Names are purpose-driven and genre-aware (dark techno /
//! atmospheric dnb / Sewerslvt-adjacent) — never settings descriptions.
//!
//! Note: no factory preset sets `freeze` — freeze is a live performance toggle that locks the
//! write head, so a from-scratch render with it on (empty buffer) would be silent. The
//! "cathedral" presets reach an evolving, near-static wash with huge grains + shimmer instead.
//! Every preset keeps `mix < 1` (dry always present) and `out <= 0 dB` so the render test's
//! `assert_universal` (finite, non-silent, ≤ 0 dBFS) holds even on the maxed-out chaos presets.

use crate::dsp::{Settings, SyncDivision};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Ambient Beds -----------------------------------------------------
    r#"{ "name": "Silt Garden", "category": "Ambient Beds", "density": 30.0, "size": 160.0,
         "spray": 90.0, "scatter": 2.0, "quantize": 0, "reverse": 0.05, "shimmer": 0.0,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.75, "mix": 0.45, "out": 0.0 }"#,
    r#"{ "name": "Tidal Ash", "category": "Ambient Beds", "density": 55.0, "size": 220.0,
         "spray": 140.0, "scatter": 3.0, "quantize": 0, "reverse": 0.1, "shimmer": 0.1,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.85, "mix": 0.55, "out": -0.5 }"#,
    r#"{ "name": "Pale Horizon", "category": "Ambient Beds", "density": 45.0, "size": 300.0,
         "spray": 180.0, "scatter": 5.0, "quantize": 0, "reverse": 0.15, "shimmer": 0.05,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.9, "mix": 0.6, "out": -1.0 }"#,
    r#"{ "name": "Drowned Meadow", "category": "Ambient Beds", "density": 70.0, "size": 260.0,
         "spray": 200.0, "scatter": 4.0, "quantize": 0, "reverse": 0.2, "shimmer": 0.15,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.62, "out": -1.0 }"#,
    // ---- Shimmer Cathedrals -----------------------------------------------
    r#"{ "name": "Ascension Vapor", "category": "Shimmer Cathedrals", "density": 70.0,
         "size": 220.0, "spray": 150.0, "scatter": 0.0, "quantize": 1, "reverse": 0.0,
         "shimmer": 0.7, "freeze": 0, "sync": 0, "division": 0, "width": 0.9, "mix": 0.7,
         "out": -1.5 }"#,
    r#"{ "name": "Gilded Rot", "category": "Shimmer Cathedrals", "density": 90.0, "size": 300.0,
         "spray": 180.0, "scatter": 3.0, "quantize": 0, "reverse": 0.1, "shimmer": 0.55,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.68, "out": -1.5 }"#,
    r#"{ "name": "Chapel of Static", "category": "Shimmer Cathedrals", "density": 80.0,
         "size": 420.0, "spray": 260.0, "scatter": 5.0, "quantize": 0, "reverse": 0.15,
         "shimmer": 0.6, "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.72,
         "out": -2.0 }"#,
    r#"{ "name": "Halo Bleed", "category": "Shimmer Cathedrals", "density": 60.0, "size": 200.0,
         "spray": 120.0, "scatter": 0.0, "quantize": 1, "reverse": 0.0, "shimmer": 0.85,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.85, "mix": 0.66, "out": -2.0 }"#,
    // ---- Rhythmic Swarms --------------------------------------------------
    r#"{ "name": "Clockwork Dust", "category": "Rhythmic Swarms", "density": 24.0, "size": 45.0,
         "spray": 200.0, "scatter": 7.0, "quantize": 1, "reverse": 0.2, "shimmer": 0.0,
         "freeze": 0, "sync": 1, "division": 1, "width": 0.85, "mix": 0.6, "out": 0.0 }"#,
    r#"{ "name": "Ritual Pulse", "category": "Rhythmic Swarms", "density": 40.0, "size": 60.0,
         "spray": 120.0, "scatter": 5.0, "quantize": 0, "reverse": 0.1, "shimmer": 0.0,
         "freeze": 0, "sync": 1, "division": 3, "width": 0.8, "mix": 0.6, "out": -0.5 }"#,
    r#"{ "name": "Stutter Cathedra", "category": "Rhythmic Swarms", "density": 60.0, "size": 30.0,
         "spray": 150.0, "scatter": 9.0, "quantize": 1, "reverse": 0.15, "shimmer": 0.2,
         "freeze": 0, "sync": 1, "division": 0, "width": 0.9, "mix": 0.62, "out": -1.0 }"#,
    r#"{ "name": "Broken Metronome", "category": "Rhythmic Swarms", "density": 18.0, "size": 90.0,
         "spray": 250.0, "scatter": 6.0, "quantize": 0, "reverse": 0.3, "shimmer": 0.0,
         "freeze": 0, "sync": 1, "division": 5, "width": 0.9, "mix": 0.58, "out": -0.5 }"#,
    // ---- Reverse & Smear --------------------------------------------------
    r#"{ "name": "Backward Requiem", "category": "Reverse & Smear", "density": 50.0,
         "size": 300.0, "spray": 220.0, "scatter": 4.0, "quantize": 0, "reverse": 0.85,
         "shimmer": 0.2, "freeze": 0, "sync": 0, "division": 0, "width": 0.95, "mix": 0.65,
         "out": -1.0 }"#,
    r#"{ "name": "Undertow", "category": "Reverse & Smear", "density": 40.0, "size": 350.0,
         "spray": 260.0, "scatter": 3.0, "quantize": 0, "reverse": 0.7, "shimmer": 0.1,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.9, "mix": 0.6, "out": -1.0 }"#,
    r#"{ "name": "Memory Reversed", "category": "Reverse & Smear", "density": 65.0, "size": 180.0,
         "spray": 180.0, "scatter": 6.0, "quantize": 0, "reverse": 0.6, "shimmer": 0.3,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.64, "out": -1.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    r#"{ "name": "Swarm Collapse", "category": "Extreme", "density": 260.0, "size": 90.0,
         "spray": 400.0, "scatter": 19.0, "quantize": 0, "reverse": 0.5, "shimmer": 0.5,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.7, "out": -2.0 }"#,
    r#"{ "name": "Locust Bible", "category": "Extreme", "density": 400.0, "size": 40.0,
         "spray": 500.0, "scatter": 24.0, "quantize": 0, "reverse": 0.4, "shimmer": 0.6,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.72, "out": -3.0 }"#,
    r#"{ "name": "Event Horizon", "category": "Extreme", "density": 500.0, "size": 500.0,
         "spray": 500.0, "scatter": 24.0, "quantize": 0, "reverse": 0.5, "shimmer": 1.1,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.75, "out": -3.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        density: g("density", d.density),
        size_ms: g("size", d.size_ms),
        spray_ms: g("spray", d.spray_ms),
        scatter_st: g("scatter", d.scatter_st),
        quantize: g("quantize", 0.0) >= 0.5,
        reverse_prob: g("reverse", d.reverse_prob),
        shimmer: g("shimmer", d.shimmer),
        freeze: g("freeze", 0.0) >= 0.5,
        freeze_mix: g("freezemix", d.freeze_mix),
        sync: g("sync", 0.0) >= 0.5,
        division: SyncDivision::from_index(g("division", 0.0) as usize),
        tempo_bpm: 120.0,
        width: g("width", d.width),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/bools by equality,
    /// floats by a loose epsilon). `tempo_bpm` is a fixed constant (`settings_from_preset` hard-
    /// codes 120), so it is excluded. Drives both the differ-from-default and pairwise-
    /// distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.quantize != b.quantize { n += 1; }
        if a.freeze != b.freeze { n += 1; }
        if a.sync != b.sync { n += 1; }
        if a.division != b.division { n += 1; }
        let fs = [
            (a.density, b.density), (a.size_ms, b.size_ms), (a.spray_ms, b.spray_ms),
            (a.scatter_st, b.scatter_st), (a.reverse_prob, b.reverse_prob),
            (a.shimmer, b.shimmer), (a.freeze_mix, b.freeze_mix),
            (a.width, b.width), (a.mix, b.mix), (a.out_db, b.out_db),
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
        assert!(presets.len() >= 15, "SWARM bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the default
        // in >= 4 params. Every preset is categorised.
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
