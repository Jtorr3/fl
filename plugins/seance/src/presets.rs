//! SEANCE factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`; the same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Keys (all plain numbers): `pitch`/`formant` = semitones (±12); `preserve` = 0/1; `pattern` =
//! chop pattern index 0..4 (Square/Stutter/Ramp/Double/Random); `rate` = chop division index
//! 0..5 (1/2 … 1/32); `chopdepth`/`size`/`shimmer`/`wet`/`wash`/`duckdepth`/`ghost`/`drown`/
//! `chopmacro`/`mix` = 0..1; `decay` = RT60 s (0.3..8); `duckrel` = ms (40..800); `out` = dB.
//!
//! Categories (preset-bar sections): Pitch-Shift / Chopped-Gated / Formant-Character /
//! Harmonizer / Drowned-Atmospheric / Extreme. Names are purpose-driven and genre-aware
//! (atmospheric dnb / dark techno / Cynthoni-Sewerslvt ghost-vocal vocabulary) — never
//! settings descriptions.

use crate::dsp::{db_to_gain, RawControls, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category (deep bank; SPECS PRESET-EXPANSION).
pub const PRESET_JSON: &[&str] = &[
    // ---- Pitch-Shift ------------------------------------------------------
    // Upfront Dark Pop — barely-there tune-up, vocal-forward, tight/dry, little verb.
    r#"{ "name": "Upfront Dark Pop", "category": "Pitch-Shift", "pitch": 0.3, "formant": 0.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.4, "decay": 1.6,
         "shimmer": 0.2, "wet": 0.25, "wash": 0.15, "duckdepth": 0.3, "duckrel": 180.0,
         "mix": 0.55, "out": 0.0 }"#,
    // Fifth Above Wraith — clean +7 harmony, formant preserved, medium ethereal tail.
    r#"{ "name": "Fifth Above Wraith", "category": "Pitch-Shift", "pitch": 7.0, "formant": 1.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.6, "decay": 2.8,
         "shimmer": 0.4, "wet": 0.4, "wash": 0.35, "duckdepth": 0.45, "duckrel": 260.0,
         "mix": 0.6, "out": -1.0 }"#,
    // Deep Vox Drop — down a fourth-ish, slightly darker formant, wider wash.
    r#"{ "name": "Deep Vox Drop", "category": "Pitch-Shift", "pitch": -5.0, "formant": -1.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.65, "decay": 2.6,
         "shimmer": 0.3, "wet": 0.35, "wash": 0.4, "duckdepth": 0.5, "duckrel": 300.0,
         "mix": 0.6, "out": 0.0 }"#,
    // Octave Down Golem — full -12 with natural formant kept, heavy and slow.
    r#"{ "name": "Octave Down Golem", "category": "Pitch-Shift", "pitch": -12.0, "formant": 0.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.7, "decay": 3.0,
         "shimmer": 0.25, "wet": 0.35, "wash": 0.45, "duckdepth": 0.5, "duckrel": 320.0,
         "mix": 0.65, "out": -1.0 }"#,
    // ---- Chopped-Gated ----------------------------------------------------
    // Chopped Ether — 1/16 stutter gate, high chop macro, shimmer, medium verb.
    r#"{ "name": "Chopped Ether", "category": "Chopped-Gated", "pitch": 5.0, "formant": 2.0,
         "preserve": 1, "pattern": 1, "rate": 4, "chopdepth": 0.6, "size": 0.55, "decay": 2.4,
         "shimmer": 0.45, "wet": 0.4, "wash": 0.35, "duckdepth": 0.3, "duckrel": 200.0,
         "ghost": 0.2, "drown": 0.15, "chopmacro": 0.5, "mix": 0.7, "out": 0.0 }"#,
    // Chopped Vox Stab — hard 1/8 square gate, dry punchy stabs, no pitch move.
    r#"{ "name": "Chopped Vox Stab", "category": "Chopped-Gated", "pitch": 0.0, "formant": 0.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.85, "size": 0.45, "decay": 1.8,
         "shimmer": 0.3, "wet": 0.3, "wash": 0.2, "duckdepth": 0.3, "duckrel": 160.0,
         "mix": 0.7, "out": 0.0 }"#,
    // Stutter Ghost Gate — 1/32 stutter, formant-lifted, glitchy ethereal stammer.
    r#"{ "name": "Stutter Ghost Gate", "category": "Chopped-Gated", "pitch": 3.0, "formant": 3.0,
         "preserve": 1, "pattern": 1, "rate": 5, "chopdepth": 0.75, "size": 0.5, "decay": 2.2,
         "shimmer": 0.5, "wet": 0.35, "wash": 0.3, "duckdepth": 0.4, "duckrel": 180.0,
         "mix": 0.72, "out": 0.0 }"#,
    // Ramp Tremolo Choir — 1/4 ramp gate (tremolo-down), formant choir, roomy tail.
    r#"{ "name": "Ramp Tremolo Choir", "category": "Chopped-Gated", "pitch": 0.0, "formant": 2.0,
         "preserve": 1, "pattern": 2, "rate": 1, "chopdepth": 0.6, "size": 0.7, "decay": 3.2,
         "shimmer": 0.4, "wet": 0.45, "wash": 0.4, "duckdepth": 0.4, "duckrel": 260.0,
         "mix": 0.65, "out": -1.0 }"#,
    // Random Glitch Vox — S&H random 1/32 gate, pitch down a third, deep chop.
    r#"{ "name": "Random Glitch Vox", "category": "Chopped-Gated", "pitch": -3.0, "formant": 1.0,
         "preserve": 1, "pattern": 4, "rate": 5, "chopdepth": 0.8, "size": 0.45, "decay": 1.8,
         "shimmer": 0.4, "wet": 0.35, "wash": 0.3, "duckdepth": 0.4, "duckrel": 150.0,
         "mix": 0.72, "out": 0.0 }"#,
    // ---- Formant-Character ------------------------------------------------
    // Formant Ghost — pitch flat, formants pushed hard up (GHOST macro), preserve on.
    r#"{ "name": "Formant Ghost", "category": "Formant-Character", "pitch": 0.0, "formant": 5.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.6, "decay": 2.6,
         "shimmer": 0.35, "wet": 0.4, "wash": 0.45, "duckdepth": 0.35, "duckrel": 240.0,
         "ghost": 0.55, "drown": 0.2, "chopmacro": 0.0, "mix": 0.6, "out": 0.0 }"#,
    // Chipmunk Ghost — bright pitch + formant lift, small shimmery space.
    r#"{ "name": "Chipmunk Ghost", "category": "Formant-Character", "pitch": 4.0, "formant": 6.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.5, "decay": 2.0,
         "shimmer": 0.45, "wet": 0.35, "wash": 0.25, "duckdepth": 0.35, "duckrel": 200.0,
         "mix": 0.65, "out": -1.0 }"#,
    // Demon Octave — -12 pitch with formants dragged down, monstrous & dark.
    r#"{ "name": "Demon Octave", "category": "Formant-Character", "pitch": -12.0, "formant": -5.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.6, "decay": 2.8,
         "shimmer": 0.25, "wet": 0.35, "wash": 0.4, "duckdepth": 0.5, "duckrel": 300.0,
         "mix": 0.65, "out": -1.0 }"#,
    // Hollow Throat — pitch flat, formants pulled down, dark hollowed vowel.
    r#"{ "name": "Hollow Throat", "category": "Formant-Character", "pitch": 0.0, "formant": -6.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.55, "decay": 2.4,
         "shimmer": 0.3, "wet": 0.35, "wash": 0.35, "duckdepth": 0.4, "duckrel": 240.0,
         "mix": 0.6, "out": 0.0 }"#,
    // Alien Formant Sit — small pitch lift, extreme formant-up, wide wash bed.
    r#"{ "name": "Alien Formant Sit", "category": "Formant-Character", "pitch": 2.0, "formant": 9.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.65, "decay": 3.0,
         "shimmer": 0.5, "wet": 0.45, "wash": 0.5, "duckdepth": 0.45, "duckrel": 280.0,
         "mix": 0.68, "out": -1.5 }"#,
    // ---- Harmonizer -------------------------------------------------------
    // Whisper Choir — formant-up ghostly choir, airy wash, subtle shimmer, high mix.
    r#"{ "name": "Whisper Choir", "category": "Harmonizer", "pitch": 7.0, "formant": 3.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.75, "decay": 4.0,
         "shimmer": 0.5, "wet": 0.55, "wash": 0.5, "duckdepth": 0.4, "duckrel": 300.0,
         "ghost": 0.4, "drown": 0.3, "chopmacro": 0.0, "mix": 0.7, "out": -1.0 }"#,
    // Sunken Chorus — octave-up shimmer chorus, wide wow wash, drown swell.
    r#"{ "name": "Sunken Chorus", "category": "Harmonizer", "pitch": 12.0, "formant": 0.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.8, "decay": 4.5,
         "shimmer": 0.7, "wet": 0.55, "wash": 0.6, "duckdepth": 0.5, "duckrel": 340.0,
         "ghost": 0.3, "drown": 0.45, "chopmacro": 0.0, "mix": 0.72, "out": -1.5 }"#,
    // Ghost Fifth Shimmer — +7 with heavy octave shimmer bloom, long ethereal tail.
    r#"{ "name": "Ghost Fifth Shimmer", "category": "Harmonizer", "pitch": 7.0, "formant": 0.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.7, "decay": 4.2,
         "shimmer": 0.75, "wet": 0.55, "wash": 0.45, "duckdepth": 0.45, "duckrel": 320.0,
         "mix": 0.68, "out": -1.5 }"#,
    // Octave Angel Choir — +12 with max shimmer + slight formant lift, huge and bright.
    r#"{ "name": "Octave Angel Choir", "category": "Harmonizer", "pitch": 12.0, "formant": 2.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.8, "decay": 5.0,
         "shimmer": 0.85, "wet": 0.6, "wash": 0.55, "duckdepth": 0.5, "duckrel": 340.0,
         "mix": 0.72, "out": -2.0 }"#,
    // Sub Octave Pad — -12 sub layer, low shimmer, big wet pad for stacking under leads.
    r#"{ "name": "Sub Octave Pad", "category": "Harmonizer", "pitch": -12.0, "formant": 1.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.75, "decay": 4.0,
         "shimmer": 0.3, "wet": 0.5, "wash": 0.5, "duckdepth": 0.5, "duckrel": 340.0,
         "mix": 0.65, "out": -1.0 }"#,
    // ---- Drowned-Atmospheric ----------------------------------------------
    // Grief Pad Vox — soft octave-down-ish drift, big slow wash, gentle drown, no chop.
    r#"{ "name": "Grief Pad Vox", "category": "Drowned-Atmospheric", "pitch": -0.2, "formant": 2.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.7, "decay": 3.2,
         "shimmer": 0.3, "wet": 0.5, "wash": 0.55, "duckdepth": 0.45, "duckrel": 320.0,
         "ghost": 0.25, "drown": 0.35, "chopmacro": 0.0, "mix": 0.6, "out": 0.0 }"#,
    // Drowned Lead — heavy drown macro, long verb, strong swell, mild formant lift.
    r#"{ "name": "Drowned Lead", "category": "Drowned-Atmospheric", "pitch": 0.0, "formant": 1.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.65, "decay": 3.8,
         "shimmer": 0.4, "wet": 0.45, "wash": 0.4, "duckdepth": 0.7, "duckrel": 280.0,
         "ghost": 0.1, "drown": 0.6, "chopmacro": 0.0, "mix": 0.65, "out": 0.0 }"#,
    // Drowned Ghost Sit — sunk-in-the-mix ghost, huge wash + swell, faint downward drift.
    r#"{ "name": "Drowned Ghost Sit", "category": "Drowned-Atmospheric", "pitch": -0.3, "formant": 2.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.85, "decay": 5.5,
         "shimmer": 0.4, "wet": 0.6, "wash": 0.7, "duckdepth": 0.75, "duckrel": 360.0,
         "drown": 0.3, "mix": 0.68, "out": -1.5 }"#,
    // Underwater Séance — pitch + formant down, near-black LP wash, cavernous 6 s tail.
    r#"{ "name": "Underwater Séance", "category": "Drowned-Atmospheric", "pitch": -2.0, "formant": -2.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.8, "decay": 6.0,
         "shimmer": 0.45, "wet": 0.6, "wash": 0.75, "duckdepth": 0.6, "duckrel": 400.0,
         "mix": 0.65, "out": -1.5 }"#,
    // Tape Choir Bed — low-mix background choir, warm wash + shimmer, gentle duck.
    r#"{ "name": "Tape Choir Bed", "category": "Drowned-Atmospheric", "pitch": 0.0, "formant": 1.0,
         "preserve": 1, "pattern": 0, "rate": 2, "chopdepth": 0.0, "size": 0.75, "decay": 4.5,
         "shimmer": 0.5, "wet": 0.4, "wash": 0.6, "duckdepth": 0.4, "duckrel": 320.0,
         "mix": 0.4, "out": -1.0 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Demon Choir Collapse — -12 demon formant, double-pulse gate, drowning verb, near-wet.
    r#"{ "name": "Demon Choir Collapse", "category": "Extreme", "pitch": -12.0, "formant": -6.0,
         "preserve": 1, "pattern": 3, "rate": 4, "chopdepth": 0.7, "size": 0.85, "decay": 6.0,
         "shimmer": 0.6, "wet": 0.6, "wash": 0.7, "duckdepth": 0.6, "duckrel": 300.0,
         "mix": 0.85, "out": -2.0 }"#,
    // Screaming Wraith — +12 pitch + extreme formant, max shimmer, 1/32 stutter shriek.
    r#"{ "name": "Screaming Wraith", "category": "Extreme", "pitch": 12.0, "formant": 9.0,
         "preserve": 1, "pattern": 1, "rate": 5, "chopdepth": 0.75, "size": 0.7, "decay": 4.0,
         "shimmer": 0.9, "wet": 0.6, "wash": 0.6, "duckdepth": 0.5, "duckrel": 200.0,
         "mix": 0.85, "out": -2.5 }"#,
    // Total Possession — pitch down, random 1/32 gate, high shimmer, black wash, near-full wet.
    r#"{ "name": "Total Possession", "category": "Extreme", "pitch": -7.0, "formant": -3.0,
         "preserve": 1, "pattern": 4, "rate": 5, "chopdepth": 0.8, "size": 0.8, "decay": 5.0,
         "shimmer": 0.7, "wet": 0.55, "wash": 0.65, "duckdepth": 0.6, "duckrel": 260.0,
         "mix": 0.9, "out": -2.5 }"#,
    // Glitch Abyss Vox — preserve OFF (raw shift), ramp gate, max wash, screaming shimmer.
    r#"{ "name": "Glitch Abyss Vox", "category": "Extreme", "pitch": 3.0, "formant": -4.0,
         "preserve": 0, "pattern": 2, "rate": 4, "chopdepth": 0.85, "size": 0.6, "decay": 3.5,
         "shimmer": 0.8, "wet": 0.55, "wash": 0.9, "duckdepth": 0.5, "duckrel": 180.0,
         "mix": 0.9, "out": -2.0 }"#,
];

/// Build [`RawControls`] from a parsed preset, falling back to defaults for omitted keys.
pub fn raw_from_preset(p: &Preset) -> RawControls {
    let d = RawControls::default();
    let g = |k: &str, fb: f32| p.get(k).unwrap_or(fb);
    RawControls {
        pitch_st: g("pitch", d.pitch_st),
        formant_st: g("formant", d.formant_st),
        preserve: g("preserve", 1.0) >= 0.5,
        chop_pattern: g("pattern", 0.0).round() as usize,
        chop_rate: g("rate", 2.0).round() as usize,
        chop_depth: g("chopdepth", d.chop_depth),
        verb_size: g("size", d.verb_size),
        verb_decay: g("decay", d.verb_decay),
        verb_shimmer: g("shimmer", d.verb_shimmer),
        verb_wet: g("wet", d.verb_wet),
        wash: g("wash", d.wash),
        duck_depth: g("duckdepth", d.duck_depth),
        duck_release_ms: g("duckrel", d.duck_release_ms),
        ghost: g("ghost", 0.0),
        drown: g("drown", 0.0),
        chop_macro: g("chopmacro", 0.0),
        mix: g("mix", d.mix),
        out_gain: db_to_gain(g("out", 0.0)),
        tempo_bpm: 120.0,
    }
}

/// Resolve a preset directly to effective [`Settings`].
pub fn settings_from_preset(p: &Preset) -> Settings {
    raw_from_preset(p).resolve()
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many *effective* (macro-resolved) `Settings` fields differ between two
    /// presets (enums/bools by equality, floats by a loose epsilon). Drives both the
    /// differ-from-default and pairwise-distinctness quality gates. `tempo_bpm` is fixed
    /// (120) for every factory preset and is intentionally excluded.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.preserve != b.preserve { n += 1; }
        if a.chop_pattern != b.chop_pattern { n += 1; }
        if a.chop_rate != b.chop_rate { n += 1; }
        let fs = [
            (a.pitch_ratio, b.pitch_ratio), (a.formant_ratio, b.formant_ratio),
            (a.chop_depth, b.chop_depth), (a.verb_size, b.verb_size),
            (a.verb_decay, b.verb_decay), (a.verb_shimmer, b.verb_shimmer),
            (a.verb_wet, b.verb_wet), (a.wash, b.wash),
            (a.duck_depth, b.duck_depth), (a.duck_release_ms, b.duck_release_ms),
            (a.mix, b.mix), (a.out_gain, b.out_gain),
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
        // Deep bank: an expressive vocal instrument (SPECS PRESET-EXPANSION target).
        assert!(presets.len() >= 18, "SEANCE bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised and
        // differs from the default in >= 4 effective params.
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
        // `every_preset_renders_and_passes_universal` test in tests.rs.
    }

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.pitch_ratio - d.pitch_ratio).abs() > 1e-3 { diffs += 1; }
            if (s.formant_ratio - d.formant_ratio).abs() > 1e-3 { diffs += 1; }
            if (s.verb_size - d.verb_size).abs() > 1e-3 { diffs += 1; }
            if (s.verb_decay - d.verb_decay).abs() > 1e-3 { diffs += 1; }
            if (s.verb_wet - d.verb_wet).abs() > 1e-3 { diffs += 1; }
            if (s.wash - d.wash).abs() > 1e-3 { diffs += 1; }
            if (s.duck_depth - d.duck_depth).abs() > 1e-3 { diffs += 1; }
            if (s.mix - d.mix).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
