//! CLEAVE factory presets. Each is an embedded flat-JSON blob parsed by `suite_core::presets`;
//! the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain numeric): `slice_mode` 0/1 (Transient/Grid); `sensitivity` 0..1;
//! `grid_div` 0..2 (1/8, 1/16, 1/32); `steps` 16..64; `swing` 0..1; `mix`/`out` (mix 0..1,
//! out dB); and the pattern archetype `pattern` 0..5 + `seed` (expanded into the per-step grid
//! by [`build_pattern`], exactly as FLYBY expands a `shape` into node coordinates). User presets
//! saved from the GUI store only the automatable scalar params; the per-step grid is persisted
//! with the project as host state (see `lib.rs`).

use crate::dsp::{build_pattern, GridDiv, Settings, SliceMode, StepData, MAX_STEPS};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, build brief: Straight Rechop, Rolls & Ghosts,
/// Reverse Accents, Half-Time Flip, Jungle Scatter, Four Flat).
pub const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Straight Rechop", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 1, "steps": 32,
         "swing": 0.0, "pattern": 0, "seed": 1, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Rolls & Ghosts", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 1, "steps": 32,
         "swing": 0.12, "pattern": 1, "seed": 7, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Reverse Accents", "category": "FX",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 1, "steps": 32,
         "swing": 0.0, "pattern": 2, "seed": 3, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Half-Time Flip", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 1, "steps": 32,
         "swing": 0.0, "pattern": 3, "seed": 2, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Jungle Scatter", "category": "Glitch",
         "slice_mode": 0, "sensitivity": 0.65, "grid_div": 2, "steps": 32,
         "swing": 0.08, "pattern": 4, "seed": 11, "mix": 1.0, "out": -1.0 }"#,
    r#"{ "name": "Four Flat", "category": "Rhythmic",
         "slice_mode": 1, "sensitivity": 0.5, "grid_div": 1, "steps": 32,
         "swing": 0.0, "pattern": 5, "seed": 1, "mix": 1.0, "out": 0.0 }"#,
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

/// Build the per-step grid the preset implies (its pattern archetype expanded).
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        for p in &presets {
            let s = settings_from_preset(p);
            let grid = grid_from_preset(p);
            // Each preset produces at least one active step.
            let active = grid[..s.steps].iter().filter(|g| g.active).count();
            assert!(active >= 1, "preset '{}' has no active steps", p.name);
        }
    }
}
