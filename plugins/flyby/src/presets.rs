//! FLYBY factory presets. Each is an embedded flat-JSON blob parsed by `suite_core::presets`;
//! the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `shape` 0..2 (Circle/Ellipse/Figure-8) — expanded into the node
//! coordinates by [`settings_from_preset`]; `nodes` node count 4..8; `speed` Hz; `sync` 0/1;
//! `division` 0..3 (1/2,1 bar,2 bar,4 bar); `size` scale; `doppler`/`air`/`width`/`mix` 0..1
//! (width up to 2); `itd` 0/1; `out` dB.
//!
//! Factory presets carry the path as a `shape` + `nodes` count (compact, readable); user presets
//! saved from the GUI store the explicit per-node `nXx`/`nXy` param values instead — both are the
//! "same flat JSON", applied through different code paths (see `suite_core::ui`).

use crate::dsp::{PathShape, Settings, SyncDivision, MAX_NODES};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, build brief).
pub const PRESET_JSON: &[&str] = &[
    // Slow, wide circular orbit — a lazy pass around the head with gentle pitch drift.
    r#"{ "name": "Slow Orbit", "category": "Motion",
         "shape": 0, "nodes": 6, "speed": 0.3, "sync": 0, "division": 1,
         "size": 9.0, "doppler": 0.6, "air": 0.45, "itd": 1, "width": 1.1,
         "mix": 1.0, "out": 0.0 }"#,
    // Tempo-locked half-note circle — a fast, rhythmic sweep that snaps to the groove.
    r#"{ "name": "Fast Circle 1/2", "category": "Sync",
         "shape": 0, "nodes": 6, "speed": 2.0, "sync": 1, "division": 0,
         "size": 7.0, "doppler": 0.8, "air": 0.35, "itd": 1, "width": 1.0,
         "mix": 1.0, "out": 0.0 }"#,
    // Big lemniscate — the source loops through two wide passes with sharp crossings near you.
    r#"{ "name": "Figure-8 Wide", "category": "Motion",
         "shape": 2, "nodes": 8, "speed": 0.5, "sync": 0, "division": 1,
         "size": 12.0, "doppler": 0.75, "air": 0.5, "itd": 1, "width": 1.2,
         "mix": 1.0, "out": 0.0 }"#,
    // Distant fly-over — far, dark, and slow; strong air absorption and distance falloff.
    r#"{ "name": "Distant Flyover", "category": "Space",
         "shape": 1, "nodes": 6, "speed": 0.2, "sync": 0, "division": 2,
         "size": 20.0, "doppler": 0.9, "air": 0.85, "itd": 1, "width": 1.0,
         "mix": 0.85, "out": 1.0 }"#,
    // Subtle motion — a small, close, barely-there drift for width without obvious movement.
    r#"{ "name": "Subtle Motion", "category": "Space",
         "shape": 1, "nodes": 5, "speed": 0.15, "sync": 0, "division": 3,
         "size": 3.5, "doppler": 0.25, "air": 0.2, "itd": 0, "width": 1.05,
         "mix": 0.6, "out": 0.0 }"#,
    // Vertigo — a fast figure-8 with heavy doppler; disorienting, seasick motion.
    r#"{ "name": "Vertigo", "category": "Extreme",
         "shape": 2, "nodes": 8, "speed": 4.0, "sync": 0, "division": 0,
         "size": 10.0, "doppler": 1.0, "air": 0.6, "itd": 1, "width": 1.4,
         "mix": 1.0, "out": -1.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.speed_hz - d.speed_hz).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.size - d.size).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.doppler - d.doppler).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.air - d.air).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.width - d.width).abs() > 1e-3 {
                diffs += 1;
            }
            if s.node_count != d.node_count {
                diffs += 1;
            }
            if s.sync != d.sync {
                diffs += 1;
            }
            // Path shape difference: any node position differs from the default layout.
            if s.nodes[..s.node_count] != d.nodes[..d.node_count] {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
