//! SEANCE factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Keys (all plain numbers): `pitch`/`formant` = semitones; `preserve` = 0/1; `pattern` =
//! chop pattern index 0..4; `rate` = chop division index 0..5; `chopdepth`/`size`/`shimmer`/
//! `wet`/`wash`/`duckdepth`/`ghost`/`drown`/`chopmacro`/`mix` = 0..1; `decay` = RT60 s;
//! `duckrel` = ms; `out` = dB.

use crate::dsp::{db_to_gain, RawControls, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Grief Pad Vox — soft octave-down-ish drift, big slow wash, gentle drown, no chop.
    r#"{ "name": "Grief Pad Vox", "pitch": -0.2, "formant": 2.0, "preserve": 1, "pattern": 0,
         "rate": 2, "chopdepth": 0.0, "size": 0.7, "decay": 3.2, "shimmer": 0.3, "wet": 0.5,
         "wash": 0.55, "duckdepth": 0.45, "duckrel": 320.0, "ghost": 0.25, "drown": 0.35,
         "chopmacro": 0.0, "mix": 0.6, "out": 0.0 }"#,
    // Drowned Lead — heavy drown macro, long verb, strong swell, mild pitch lift.
    r#"{ "name": "Drowned Lead", "pitch": 0.0, "formant": 1.0, "preserve": 1, "pattern": 0,
         "rate": 2, "chopdepth": 0.0, "size": 0.65, "decay": 3.8, "shimmer": 0.4, "wet": 0.45,
         "wash": 0.4, "duckdepth": 0.7, "duckrel": 280.0, "ghost": 0.1, "drown": 0.6,
         "chopmacro": 0.0, "mix": 0.65, "out": 0.0 }"#,
    // Whisper Choir — formant-up ghostly choir, airy wash, subtle shimmer, high mix.
    r#"{ "name": "Whisper Choir", "pitch": 7.0, "formant": 3.0, "preserve": 1, "pattern": 0,
         "rate": 2, "chopdepth": 0.0, "size": 0.75, "decay": 4.0, "shimmer": 0.5, "wet": 0.55,
         "wash": 0.5, "duckdepth": 0.4, "duckrel": 300.0, "ghost": 0.4, "drown": 0.3,
         "chopmacro": 0.0, "mix": 0.7, "out": -1.0 }"#,
    // Formant Ghost — pitch flat, formants pushed hard up (GHOST macro), preserve on.
    r#"{ "name": "Formant Ghost", "pitch": 0.0, "formant": 5.0, "preserve": 1, "pattern": 0,
         "rate": 2, "chopdepth": 0.0, "size": 0.6, "decay": 2.6, "shimmer": 0.35, "wet": 0.4,
         "wash": 0.45, "duckdepth": 0.35, "duckrel": 240.0, "ghost": 0.55, "drown": 0.2,
         "chopmacro": 0.0, "mix": 0.6, "out": 0.0 }"#,
    // Chopped Ether — 1/16 stutter gate, high chop macro, shimmer, medium verb.
    r#"{ "name": "Chopped Ether", "pitch": 5.0, "formant": 2.0, "preserve": 1, "pattern": 1,
         "rate": 4, "chopdepth": 0.6, "size": 0.55, "decay": 2.4, "shimmer": 0.45, "wet": 0.4,
         "wash": 0.35, "duckdepth": 0.3, "duckrel": 200.0, "ghost": 0.2, "drown": 0.15,
         "chopmacro": 0.5, "mix": 0.7, "out": 0.0 }"#,
    // Sunken Chorus — octave-up shimmer chorus, wide wow wash, drown swell.
    r#"{ "name": "Sunken Chorus", "pitch": 12.0, "formant": 0.0, "preserve": 1, "pattern": 0,
         "rate": 2, "chopdepth": 0.0, "size": 0.8, "decay": 4.5, "shimmer": 0.7, "wet": 0.55,
         "wash": 0.6, "duckdepth": 0.5, "duckrel": 340.0, "ghost": 0.3, "drown": 0.45,
         "chopmacro": 0.0, "mix": 0.72, "out": -1.5 }"#,
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
