//! FLYBY factory presets (SPECS "PRESET-EXPANSION" bank). Each is an embedded flat-JSON blob
//! parsed by `suite_core::presets`; the same list drives the GUI selector (grouped by the
//! `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain): `shape` 0..2 (Circle/Ellipse/Figure-8) — expanded into the node
//! coordinates by [`settings_from_preset`]; `nodes` node count 4..8; `speed` Hz (0.01..20);
//! `sync` 0/1; `division` 0..3 (½,1 bar,2 bar,4 bar); `size` scale 1..30; `doppler`/`air`/`mix`
//! 0..1; `width` 0..2; `itd` 0/1; `out` dB (kept ≤ 0 for headroom).
//!
//! Factory presets carry the path as a `shape` + `nodes` count (compact, readable); user presets
//! saved from the GUI store the explicit per-node `nXx`/`nXy` param values instead — both are the
//! "same flat JSON", applied through different code paths (see `suite_core::ui`).
//!
//! Categories (preset-bar sections, first-appearance order): Motion / Sync / Space / Texture /
//! Extreme. Names are purpose-driven and genre-aware (dark techno / atmospheric dnb) — never
//! settings descriptions.

use crate::dsp::{PathShape, Settings, SyncDivision, MAX_NODES};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Motion -----------------------------------------------------------
    // Slow, wide circular orbit — a lazy pass around the head with gentle pitch drift.
    r#"{ "name": "Slow Orbit", "category": "Motion",
         "shape": 0, "nodes": 6, "speed": 0.3, "sync": 0, "division": 1,
         "size": 9.0, "doppler": 0.6, "air": 0.45, "itd": 1, "width": 1.1,
         "mix": 1.0, "out": 0.0 }"#,
    // Tight, brisk circle close to the head — a spinning carousel with clear left/right travel.
    r#"{ "name": "Head Carousel", "category": "Motion",
         "shape": 0, "nodes": 5, "speed": 0.8, "sync": 0, "division": 1,
         "size": 6.0, "doppler": 0.55, "air": 0.3, "itd": 1, "width": 1.2,
         "mix": 1.0, "out": -0.5 }"#,
    // Big lemniscate — the source loops through two wide passes with sharp crossings near you.
    r#"{ "name": "Figure-8 Wide", "category": "Motion",
         "shape": 2, "nodes": 8, "speed": 0.5, "sync": 0, "division": 1,
         "size": 12.0, "doppler": 0.75, "air": 0.5, "itd": 1, "width": 1.2,
         "mix": 1.0, "out": 0.0 }"#,
    // Broad horizontal fly-past — a wide left→right arc that skims close at the bottom.
    r#"{ "name": "Lateral Drift", "category": "Motion",
         "shape": 1, "nodes": 6, "speed": 0.4, "sync": 0, "division": 1,
         "size": 7.0, "doppler": 0.5, "air": 0.35, "itd": 1, "width": 1.3,
         "mix": 0.9, "out": 0.0 }"#,
    // ---- Sync -------------------------------------------------------------
    // Tempo-locked half-note circle — a fast, rhythmic sweep that snaps to the groove.
    r#"{ "name": "Fast Circle 1/2", "category": "Sync",
         "shape": 0, "nodes": 6, "speed": 2.0, "sync": 1, "division": 0,
         "size": 7.0, "doppler": 0.8, "air": 0.35, "itd": 1, "width": 1.0,
         "mix": 1.0, "out": 0.0 }"#,
    // One-bar ellipse pulse — a locked left→right pass that breathes with the bar line.
    r#"{ "name": "Bar Sweep Pulse", "category": "Sync",
         "shape": 1, "nodes": 6, "speed": 1.0, "sync": 1, "division": 1,
         "size": 8.0, "doppler": 0.65, "air": 0.4, "itd": 1, "width": 1.1,
         "mix": 1.0, "out": 0.0 }"#,
    // Four-bar figure-8 — a slow, evolving loop that resolves over a full phrase.
    r#"{ "name": "Four-Bar Wander", "category": "Sync",
         "shape": 2, "nodes": 8, "speed": 0.5, "sync": 1, "division": 3,
         "size": 10.0, "doppler": 0.7, "air": 0.55, "itd": 1, "width": 1.15,
         "mix": 0.95, "out": 0.0 }"#,
    // ---- Space ------------------------------------------------------------
    // Distant fly-over — far, dark, and slow; strong air absorption and distance falloff.
    r#"{ "name": "Distant Flyover", "category": "Space",
         "shape": 1, "nodes": 6, "speed": 0.2, "sync": 0, "division": 2,
         "size": 20.0, "doppler": 0.9, "air": 0.85, "itd": 1, "width": 1.0,
         "mix": 0.85, "out": 0.0 }"#,
    // Subtle motion — a small, close, barely-there drift for width without obvious movement.
    r#"{ "name": "Subtle Motion", "category": "Space",
         "shape": 1, "nodes": 5, "speed": 0.15, "sync": 0, "division": 3,
         "size": 3.5, "doppler": 0.25, "air": 0.2, "itd": 0, "width": 1.05,
         "mix": 0.6, "out": 0.0 }"#,
    // Vast, glacial circle — a huge dark orbit that drags the source through deep air absorption.
    r#"{ "name": "Cathedral Pass", "category": "Space",
         "shape": 0, "nodes": 7, "speed": 0.18, "sync": 0, "division": 2,
         "size": 22.0, "doppler": 0.85, "air": 0.9, "itd": 1, "width": 1.0,
         "mix": 0.8, "out": 0.0 }"#,
    // Submerged, muffled sweep — heavy air makes every pass sound like it's underwater.
    r#"{ "name": "Underwater Bloom", "category": "Space",
         "shape": 1, "nodes": 6, "speed": 0.25, "sync": 0, "division": 2,
         "size": 16.0, "doppler": 0.6, "air": 0.95, "itd": 1, "width": 0.9,
         "mix": 0.75, "out": 0.0 }"#,
    // ---- Texture ----------------------------------------------------------
    // Smeared, wide figure-8 haze — ghostly doppler trails for atmospheric dnb pads.
    r#"{ "name": "Ghost Trails", "category": "Texture",
         "shape": 2, "nodes": 8, "speed": 0.6, "sync": 0, "division": 1,
         "size": 9.0, "doppler": 0.5, "air": 0.7, "itd": 1, "width": 1.6,
         "mix": 0.7, "out": -1.0 }"#,
    // Corroded, over-wide shimmer — a small close ellipse blown out to the edges of the field.
    r#"{ "name": "Rot Shimmer", "category": "Texture",
         "shape": 1, "nodes": 5, "speed": 0.35, "sync": 0, "division": 1,
         "size": 6.0, "doppler": 0.4, "air": 0.6, "itd": 0, "width": 1.8,
         "mix": 0.65, "out": -1.0 }"#,
    // Sparse four-node séance — a stark, close circle for haunted, half-present textures.
    r#"{ "name": "Static Seance", "category": "Texture",
         "shape": 0, "nodes": 4, "speed": 0.5, "sync": 0, "division": 1,
         "size": 5.0, "doppler": 0.45, "air": 0.5, "itd": 1, "width": 1.4,
         "mix": 0.6, "out": -0.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Vertigo — a fast figure-8 with heavy doppler; disorienting, seasick motion.
    r#"{ "name": "Vertigo", "category": "Extreme",
         "shape": 2, "nodes": 8, "speed": 4.0, "sync": 0, "division": 0,
         "size": 10.0, "doppler": 1.0, "air": 0.6, "itd": 1, "width": 1.4,
         "mix": 1.0, "out": -1.0 }"#,
    // Wild, over-wide figure-8 lurch — full doppler at a nauseating traversal rate.
    r#"{ "name": "Seasick", "category": "Extreme",
         "shape": 2, "nodes": 8, "speed": 6.0, "sync": 0, "division": 0,
         "size": 12.0, "doppler": 1.0, "air": 0.5, "itd": 1, "width": 1.7,
         "mix": 1.0, "out": -2.0 }"#,
    // Maximum-width blur spin — an eight-node circle whipped fast into a dizzying smear.
    r#"{ "name": "Nauseous Whirl", "category": "Extreme",
         "shape": 0, "nodes": 8, "speed": 8.0, "sync": 0, "division": 0,
         "size": 8.0, "doppler": 0.95, "air": 0.4, "itd": 1, "width": 2.0,
         "mix": 1.0, "out": -2.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys. The
/// path is generated from the `shape` + `nodes` fields.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
    let count = g("nodes", d.node_count as f32) as usize;
    // If the preset carries explicit node coordinates (user preset), use them; else expand a shape.
    let has_explicit = p.get("n0x").is_some();
    let node_count = if has_explicit {
        let c = count.clamp(crate::dsp::MIN_NODES, MAX_NODES);
        for (i, slot) in nodes.iter_mut().enumerate() {
            *slot = (
                p.get(&format!("n{i}x")).unwrap_or(0.0),
                p.get(&format!("n{i}y")).unwrap_or(0.0),
            );
        }
        c
    } else {
        PathShape::from_index(g("shape", 0.0) as usize).layout(&mut nodes, count)
    };

    Settings {
        nodes,
        node_count,
        speed_hz: g("speed", d.speed_hz),
        sync: g("sync", 0.0) >= 0.5,
        division: SyncDivision::from_index(g("division", 1.0) as usize),
        tempo_bpm: 120.0,
        size: g("size", d.size),
        doppler: g("doppler", d.doppler),
        air: g("air", d.air),
        itd: g("itd", 1.0) >= 0.5,
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
    /// floats by a loose epsilon; the path counts as one field when the used node positions
    /// differ). `tempo_bpm` is a fixed constant (120) so it is intentionally skipped. Drives both
    /// the differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.node_count != b.node_count {
            n += 1;
        }
        if a.sync != b.sync {
            n += 1;
        }
        if a.itd != b.itd {
            n += 1;
        }
        if a.division != b.division {
            n += 1;
        }
        // Path shape: the node positions actually used differ (a different layout/count).
        let an = a.node_count.min(MAX_NODES);
        let bn = b.node_count.min(MAX_NODES);
        if a.nodes[..an] != b.nodes[..bn] {
            n += 1;
        }
        let fs = [
            (a.speed_hz, b.speed_hz),
            (a.size, b.size),
            (a.doppler, b.doppler),
            (a.air, b.air),
            (a.width, b.width),
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
        // Expanded bank for a simpler utility FX: SPECS target ≥ 12.
        assert!(presets.len() >= 12, "FLYBY bank too small: {}", presets.len());

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
