//! CLEAVE factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded flat-JSON
//! blob parsed by `suite_core::presets`; the same list drives the GUI selector (grouped by the
//! `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain numeric): `slice_mode` 0/1 (Transient/Grid); `sensitivity` 0..1;
//! `grid_div` 0..2 (1/8, 1/16, 1/32); `steps` 16..64; `swing` 0..1; `mix` 0..1; `out` dB.
//!
//! **Step-grid caveat.** CLEAVE's per-step grid (which slice each step plays, gate, reverse,
//! pitch, roll, probability, level) is **persisted host state**, NOT automatable params (16–64
//! steps × 8 lanes is far too many to expose to the automation tree — see `docs/CLEAVE.md`).
//! `settings_from_preset` therefore covers only the **param space** (slice mode, sensitivity,
//! grid division, steps, swing, mix, out). These presets vary that param space. As a convenience
//! each preset also carries a compact `pattern` archetype + `seed` that [`build_pattern`] expands
//! into a starter grid on load (so a freshly-loaded factory preset is audible), but that grid is
//! not itself part of the preset's automatable state — a user's edited grid is saved with the
//! project separately. The `PRESET-EXPANSION` quality gate below scores presets on the param
//! space only, exactly as `settings_from_preset` reads them.
//!
//! Categories (preset-bar sections): Rhythmic / Breakbeat / Glitch / Texture / Extreme. Names are
//! purpose-driven and genre-aware (dark techno / atmospheric dnb / breakcore) — never settings
//! descriptions.

use crate::dsp::{build_pattern, GridDiv, Settings, SliceMode, StepData, MAX_STEPS};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Rhythmic ---------------------------------------------------------
    r#"{ "name": "Warehouse Rechop", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.42, "grid_div": 0, "steps": 16,
         "swing": 0.10, "pattern": 0, "seed": 1, "mix": 1.0, "out": -0.5 }"#,
    r#"{ "name": "Peak-Time Stutter", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.6, "grid_div": 1, "steps": 48,
         "swing": 0.04, "pattern": 0, "seed": 5, "mix": 0.9, "out": -1.0 }"#,
    r#"{ "name": "Concrete Four-Flat", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.35, "grid_div": 0, "steps": 24,
         "swing": 0.0, "pattern": 5, "seed": 1, "mix": 0.92, "out": -1.5 }"#,
    r#"{ "name": "Halftime Pressure", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.55, "grid_div": 2, "steps": 40,
         "swing": 0.14, "pattern": 3, "seed": 2, "mix": 1.0, "out": -1.0 }"#,
    // ---- Breakbeat --------------------------------------------------------
    r#"{ "name": "Amen Recut", "category": "Breakbeat",
         "slice_mode": 0, "sensitivity": 0.7, "grid_div": 1, "steps": 48,
         "swing": 0.06, "pattern": 4, "seed": 11, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Rollage Ghosts", "category": "Breakbeat",
         "slice_mode": 1, "sensitivity": 0.45, "grid_div": 2, "steps": 64,
         "swing": 0.12, "pattern": 1, "seed": 7, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Think Break Scatter", "category": "Breakbeat",
         "slice_mode": 0, "sensitivity": 0.8, "grid_div": 1, "steps": 40,
         "swing": 0.05, "pattern": 4, "seed": 23, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Liquid Rollers", "category": "Breakbeat",
         "slice_mode": 0, "sensitivity": 0.6, "grid_div": 1, "steps": 32,
         "swing": 0.08, "pattern": 1, "seed": 13, "mix": 0.8, "out": -1.0 }"#,
    // ---- Glitch -----------------------------------------------------------
    r#"{ "name": "Breakcore Mince", "category": "Glitch",
         "slice_mode": 1, "sensitivity": 0.65, "grid_div": 2, "steps": 64,
         "swing": 0.03, "pattern": 4, "seed": 99, "mix": 1.0, "out": -2.0 }"#,
    r#"{ "name": "Stutter Gate", "category": "Glitch",
         "slice_mode": 1, "sensitivity": 0.4, "grid_div": 2, "steps": 48,
         "swing": 0.16, "pattern": 3, "seed": 4, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Reverse Shard", "category": "Glitch",
         "slice_mode": 0, "sensitivity": 0.75, "grid_div": 1, "steps": 40,
         "swing": 0.05, "pattern": 2, "seed": 3, "mix": 1.0, "out": -1.5 }"#,
    r#"{ "name": "Sewer Glitch Roll", "category": "Glitch",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 2, "steps": 64,
         "swing": 0.10, "pattern": 1, "seed": 77, "mix": 0.85, "out": -2.0 }"#,
    // ---- Texture ----------------------------------------------------------
    r#"{ "name": "Drowned Pad Gate", "category": "Texture",
         "slice_mode": 1, "sensitivity": 0.4, "grid_div": 0, "steps": 16,
         "swing": 0.0, "pattern": 3, "seed": 6, "mix": 0.5, "out": -1.5 }"#,
    r#"{ "name": "Cynthoni Wash", "category": "Texture",
         "slice_mode": 0, "sensitivity": 0.3, "grid_div": 1, "steps": 24,
         "swing": 0.0, "pattern": 3, "seed": 14, "mix": 0.45, "out": -2.0 }"#,
    r#"{ "name": "Ghost Bloom", "category": "Texture",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 0, "steps": 16,
         "swing": 0.20, "pattern": 1, "seed": 21, "mix": 0.4, "out": -2.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    r#"{ "name": "Total Rechop", "category": "Extreme",
         "slice_mode": 1, "sensitivity": 0.9, "grid_div": 2, "steps": 64,
         "swing": 0.02, "pattern": 4, "seed": 255, "mix": 1.0, "out": -3.0 }"#,
    r#"{ "name": "Nyquist Mangler", "category": "Extreme",
         "slice_mode": 0, "sensitivity": 1.0, "grid_div": 1, "steps": 64,
         "swing": 0.04, "pattern": 4, "seed": 137, "mix": 1.0, "out": -3.0 }"#,
    r#"{ "name": "Fold Abyss Chop", "category": "Extreme",
         "slice_mode": 0, "sensitivity": 0.95, "grid_div": 2, "steps": 56,
         "swing": 0.18, "pattern": 4, "seed": 42, "mix": 0.9, "out": -3.0 }"#,
];

/// Build the DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        slice_mode: SliceMode::from_index(g("slice_mode", 1.0) as usize),
        sensitivity: g("sensitivity", d.sensitivity),
        grid_div: GridDiv::from_index(g("grid_div", 1.0) as usize),
        steps: (g("steps", d.steps as f32) as usize).clamp(crate::dsp::MIN_STEPS, MAX_STEPS),
        swing: g("swing", d.swing),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

/// Build the per-step grid the preset implies (its pattern archetype expanded). This is a
/// starter grid only — the persisted, user-editable grid is host state (see the module docs).
pub fn grid_from_preset(p: &Preset) -> [StepData; MAX_STEPS] {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let steps = (g("steps", 32.0) as usize).clamp(crate::dsp::MIN_STEPS, MAX_STEPS);
    let pattern = g("pattern", 0.0) as usize;
    let seed = g("seed", 1.0) as u32;
    build_pattern(pattern, steps, seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/int by equality, floats
    /// by a loose epsilon). Covers exactly the fields `settings_from_preset` sets — the automatable
    /// param space; the persisted step-grid is out of scope by design. Drives both the
    /// differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.slice_mode != b.slice_mode {
            n += 1;
        }
        if a.grid_div != b.grid_div {
            n += 1;
        }
        if a.steps != b.steps {
            n += 1;
        }
        let fs = [
            (a.sensitivity, b.sensitivity),
            (a.swing, b.swing),
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
        assert!(presets.len() >= 15, "CLEAVE bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the default in
        // >= 4 params and is categorised (and its starter grid is non-silent).
        for (p, s) in presets.iter().zip(&settings) {
            assert!(p.category.is_some(), "preset '{}' has no category", p.name);
            let diffs = count_diffs(s, &d);
            assert!(diffs >= 4, "preset '{}' differs from default in only {diffs} params", p.name);
            let grid = grid_from_preset(p);
            let active = grid[..s.steps].iter().filter(|g| g.active).count();
            assert!(active >= 1, "preset '{}' has no active steps", p.name);
        }

        // Rule 3 (no near-duplicates): every preset differs from EVERY other in >= 2.
        for i in 0..settings.len() {
            for j in (i + 1)..settings.len() {
                let diffs = count_diffs(&settings[i], &settings[j]);
                assert!(
                    diffs >= 2,
                    "presets '{}' and '{}' differ in only {diffs} params (near-duplicate)",
                    presets[i].name,
                    presets[j].name
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
        // `every_preset_renders_and_passes_universal` test in tests.rs.
    }
}
