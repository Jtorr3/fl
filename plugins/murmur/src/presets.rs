//! MURMUR factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain, un-normalized): `size`/`random`/`sens`/`width`/`mix` are 0..1;
//! `decay` is RT60 seconds (0.2–20); `color` is −1..1 (bright→dark). Presets never set
//! `freeze` (the input-duck would render silent from an empty buffer) — a long-`decay`
//! space emulates an eternal room instead; the freeze *button* is a live control.
//!
//! Categories (preset-bar sections): Halls & Naves / Chambers & Rooms / Drum Spaces /
//! Dark Textures / Extreme. Names are purpose-driven and genre-aware (dark techno /
//! atmospheric dnb / Cynthoni–Sewerslvt murk) — never settings descriptions. Levels are
//! kept conservative (mix ≤ 0.5, safety-clipped wet) so every preset renders finite,
//! non-silent, and ≤ 0 dBFS.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Halls & Naves ----------------------------------------------------
    // Big hall, high randomness — every hit lands in a different room.
    r#"{ "name": "Never The Same Hall", "category": "Halls & Naves", "size": 0.75,
         "decay": 4.0, "color": 0.1, "random": 0.9, "sens": 0.5, "width": 1.0, "mix": 0.4 }"#,
    // Vast, dark, long — a cavernous nave for slow techno chords and drones.
    r#"{ "name": "Cathedral Of Ash", "category": "Halls & Naves", "size": 0.9,
         "decay": 8.0, "color": 0.55, "random": 0.4, "sens": 0.4, "width": 1.0, "mix": 0.42 }"#,
    // Huge near-eternal nave (long decay stands in for freeze), bright and wide.
    r#"{ "name": "Frozen Nave", "category": "Halls & Naves", "size": 1.0,
         "decay": 18.0, "color": -0.1, "random": 0.3, "sens": 0.4, "width": 1.0, "mix": 0.5 }"#,
    // Immense airy vault, bright tail, moderate drift — cinematic atmospheric-dnb space.
    r#"{ "name": "Endless Vaults", "category": "Halls & Naves", "size": 0.95,
         "decay": 12.0, "color": -0.2, "random": 0.5, "sens": 0.45, "width": 1.0, "mix": 0.45 }"#,
    // ---- Chambers & Rooms -------------------------------------------------
    // Medium chamber, moderate drift — the room subtly shifts as you play.
    r#"{ "name": "Shifting Chamber", "category": "Chambers & Rooms", "size": 0.5,
         "decay": 2.0, "color": 0.0, "random": 0.6, "sens": 0.6, "width": 0.9, "mix": 0.35 }"#,
    // Boxy dark mid-room — tight concrete reflections for dark-techno stabs.
    r#"{ "name": "Concrete Antechamber", "category": "Chambers & Rooms", "size": 0.55,
         "decay": 2.6, "color": 0.35, "random": 0.45, "sens": 0.55, "width": 0.85, "mix": 0.33 }"#,
    // Tiny, bright, quirky — maximum randomness on short odd delays.
    r#"{ "name": "Small Odd Room", "category": "Chambers & Rooms", "size": 0.2,
         "decay": 0.8, "color": -0.3, "random": 1.0, "sens": 0.7, "width": 0.7, "mix": 0.3 }"#,
    // Close, warm, intimate — a small padded booth that hugs the source.
    r#"{ "name": "Velvet Booth", "category": "Chambers & Rooms", "size": 0.3,
         "decay": 1.0, "color": 0.25, "random": 0.35, "sens": 0.6, "width": 0.8, "mix": 0.28 }"#,
    // ---- Drum Spaces ------------------------------------------------------
    // Small snappy rooms, very responsive onset, big per-hit variety — for drums.
    r#"{ "name": "Percussion Rooms", "category": "Drum Spaces", "size": 0.35,
         "decay": 1.2, "color": -0.2, "random": 0.8, "sens": 0.85, "width": 0.8, "mix": 0.3 }"#,
    // Low damp cellar with a fast trigger — glue for chopped dnb breaks.
    r#"{ "name": "Breakbeat Cellar", "category": "Drum Spaces", "size": 0.4,
         "decay": 1.5, "color": 0.15, "random": 0.7, "sens": 0.9, "width": 0.85, "mix": 0.32 }"#,
    // Tight, bright, hair-trigger — snappy air on snares and rims.
    r#"{ "name": "Snare Chamber Snap", "category": "Drum Spaces", "size": 0.28,
         "decay": 0.9, "color": -0.35, "random": 0.6, "sens": 0.95, "width": 0.75, "mix": 0.27 }"#,
    // ---- Dark Textures ----------------------------------------------------
    // Dark, long, mournful — a heavily-damped space for grief pads.
    r#"{ "name": "Grief Space", "category": "Dark Textures", "size": 0.85,
         "decay": 6.0, "color": 0.7, "random": 0.5, "sens": 0.4, "width": 1.0, "mix": 0.45 }"#,
    // Murky, saturated-dark wash — Sewerslvt haze that swallows the transient.
    r#"{ "name": "Drowned In Static", "category": "Dark Textures", "size": 0.7,
         "decay": 5.0, "color": 0.85, "random": 0.65, "sens": 0.35, "width": 0.95, "mix": 0.5 }"#,
    // Deep, wide, slowly-drifting pad bed — nostalgic atmospheric-dnb undertow.
    r#"{ "name": "Submerged Memory", "category": "Dark Textures", "size": 0.8,
         "decay": 7.0, "color": 0.5, "random": 0.55, "sens": 0.4, "width": 1.0, "mix": 0.48 }"#,
    // Grey mid-length fog — soft dark diffusion for lonely keys and vox.
    r#"{ "name": "Mourning Fog", "category": "Dark Textures", "size": 0.65,
         "decay": 4.5, "color": 0.6, "random": 0.6, "sens": 0.45, "width": 0.95, "mix": 0.4 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Huge room, maximum room-to-room chaos — every hit is a new cathedral.
    r#"{ "name": "Chaos Bloom", "category": "Extreme", "size": 0.9,
         "decay": 10.0, "color": 0.2, "random": 1.0, "sens": 0.6, "width": 1.0, "mix": 0.5 }"#,
    // Enormous, near-eternal, dark and wildly unstable — a dissolving drone.
    r#"{ "name": "Dissociative Drift", "category": "Extreme", "size": 0.85,
         "decay": 15.0, "color": 0.75, "random": 0.85, "sens": 0.3, "width": 1.0, "mix": 0.5 }"#,
    // Tiny bright shards flung to full chaos — metallic, glitching, unstable.
    r#"{ "name": "Shattered Wavelength", "category": "Extreme", "size": 0.15,
         "decay": 3.5, "color": -0.6, "random": 1.0, "sens": 0.8, "width": 0.6, "mix": 0.45 }"#,
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
        freeze_mix: g("freezemix", d.freeze_mix),
        width: g("width", d.width),
        mix: g("mix", d.mix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (floats by a loose
    /// epsilon). Covers every field `settings_from_preset` actually sets from the blob;
    /// `freeze` is skipped because presets hardcode it to a fixed `false`. Drives both the
    /// differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let fs = [
            (a.size, b.size),
            (a.decay, b.decay),
            (a.color, b.color),
            (a.randomness, b.randomness),
            (a.sensitivity, b.sensitivity),
            (a.freeze_mix, b.freeze_mix),
            (a.width, b.width),
            (a.mix, b.mix),
        ];
        let mut n = 0;
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
        assert!(presets.len() >= 15, "MURMUR bank too small: {}", presets.len());

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
        // `render_tests::every_preset_renders_and_passes_universal` test in lib.rs.
    }
}
