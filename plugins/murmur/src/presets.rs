//! MURMUR factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `size`/`random`/`sens`/`width`/`mix` are 0..1; `decay` is RT60
//! seconds; `color` is −1..1 (bright→dark). Presets never set `freeze` (the input-duck would
//! render silent from an empty buffer, like SWARM's note) — a long-`decay` "Frozen Nave"
//! emulates an eternal space instead; the freeze *button* is a live control.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Big hall, high randomness — every hit lands in a different room.
    r#"{ "name": "Never The Same Hall", "size": 0.75, "decay": 4.0, "color": 0.1,
         "random": 0.9, "sens": 0.5, "width": 1.0, "mix": 0.4 }"#,
    // Medium chamber, moderate drift — the room subtly shifts as you play.
    r#"{ "name": "Shifting Chamber", "size": 0.5, "decay": 2.0, "color": 0.0,
         "random": 0.6, "sens": 0.6, "width": 0.9, "mix": 0.35 }"#,
    // Dark, long, mournful — a heavy damped space for grief pads.
    r#"{ "name": "Grief Space", "size": 0.85, "decay": 6.0, "color": 0.7,
         "random": 0.5, "sens": 0.4, "width": 1.0, "mix": 0.45 }"#,
    // Small snappy rooms, very responsive onset, big per-hit variety — for drums.
    r#"{ "name": "Percussion Rooms", "size": 0.35, "decay": 1.2, "color": -0.2,
         "random": 0.8, "sens": 0.85, "width": 0.8, "mix": 0.3 }"#,
    // Huge near-eternal nave (long decay stands in for freeze), bright and wide.
    r#"{ "name": "Frozen Nave", "size": 1.0, "decay": 18.0, "color": -0.1,
         "random": 0.3, "sens": 0.4, "width": 1.0, "mix": 0.5 }"#,
    // Tiny, bright, quirky — maximum randomness on short odd delays.
    r#"{ "name": "Small Odd Room", "size": 0.2, "decay": 0.8, "color": -0.3,
         "random": 1.0, "sens": 0.7, "width": 0.7, "mix": 0.3 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        size: g("size", d.size),
        decay: g("decay", d.decay),
        color: g("color", d.color),
        randomness: g("random", d.randomness),
        sensitivity: g("sens", d.sensitivity),
        freeze: false,
        width: g("width", d.width),
        mix: g("mix", d.mix),
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
            if (s.size - d.size).abs() > 1e-3 { diffs += 1; }
            if (s.decay - d.decay).abs() > 1e-3 { diffs += 1; }
            if (s.color - d.color).abs() > 1e-3 { diffs += 1; }
            if (s.randomness - d.randomness).abs() > 1e-3 { diffs += 1; }
            if (s.sensitivity - d.sensitivity).abs() > 1e-3 { diffs += 1; }
            if (s.width - d.width).abs() > 1e-3 { diffs += 1; }
            if (s.mix - d.mix).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
