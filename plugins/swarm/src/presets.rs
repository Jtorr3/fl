//! SWARM factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `density` grains/s; `size`/`spray` ms; `scatter` st; `reverse`,
//! `shimmer`, `width`, `mix` 0..1(.1); `out` dB; `quantize`/`sync`/`freeze` 0/1; `division`
//! 0..6 (1/16,1/8,1/8·,1/4,1/4·,1/2,bar).
//!
//! Note: no factory preset sets `freeze` — freeze is a live performance toggle that locks the
//! write head, so a from-scratch render with it on (empty buffer) would be silent. "Frozen
//! Cathedral" reaches an evolving, near-static wash with a huge grain size + shimmer instead.

use crate::dsp::{Settings, SyncDivision};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Dense, slow, wide bed of medium grains — an ambient pad-maker.
    r#"{ "name": "Texture Bed", "density": 60.0, "size": 180.0, "spray": 120.0,
         "scatter": 3.0, "quantize": 0, "reverse": 0.1, "shimmer": 0.0,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.8, "mix": 0.6, "out": 0.0 }"#,
    // Huge overlapping grains + shimmer ⇒ an evolving, near-frozen cathedral wash.
    r#"{ "name": "Frozen Cathedral", "density": 90.0, "size": 480.0, "spray": 300.0,
         "scatter": 5.0, "quantize": 0, "reverse": 0.15, "shimmer": 0.45,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.75, "out": -1.0 }"#,
    // Octave-up shimmer feedback blooms into a rising, angelic cloud.
    r#"{ "name": "Shimmer Bloom", "density": 70.0, "size": 220.0, "spray": 150.0,
         "scatter": 0.0, "quantize": 1, "reverse": 0.0, "shimmer": 0.8,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.9, "mix": 0.7, "out": -1.0 }"#,
    // Sparse, tempo-synced clusters of tiny grains — rhythmic granular dust.
    r#"{ "name": "Rhythmic Dust", "density": 24.0, "size": 45.0, "spray": 200.0,
         "scatter": 7.0, "quantize": 1, "reverse": 0.2, "shimmer": 0.0,
         "freeze": 0, "sync": 1, "division": 1, "width": 0.85, "mix": 0.6, "out": 0.0 }"#,
    // Mostly-reversed medium grains, wide — smeared backward swells.
    r#"{ "name": "Reverse Swell", "density": 50.0, "size": 300.0, "spray": 220.0,
         "scatter": 4.0, "quantize": 0, "reverse": 0.85, "shimmer": 0.2,
         "freeze": 0, "sync": 0, "division": 0, "width": 0.95, "mix": 0.65, "out": -1.0 }"#,
    // Everything cranked: high density, wide pitch scatter, reverse, shimmer — granular chaos.
    r#"{ "name": "Granular Chaos", "density": 260.0, "size": 90.0, "spray": 400.0,
         "scatter": 19.0, "quantize": 0, "reverse": 0.5, "shimmer": 0.5,
         "freeze": 0, "sync": 0, "division": 0, "width": 1.0, "mix": 0.7, "out": -2.0 }"#,
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

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.density - d.density).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.size_ms - d.size_ms).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.spray_ms - d.spray_ms).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.width - d.width).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
