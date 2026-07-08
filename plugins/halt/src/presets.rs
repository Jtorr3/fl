//! HALT factory presets — flat-JSON blobs parsed by `suite_core::presets`. The same list
//! drives the GUI selector and the offline render tests. Presets set the *character* knobs;
//! the four momentary mode buttons are live performance state and are never stored.
//!
//! Value encodings (plain numeric): `stutdiv` 0..4 (1/4,1/8,1/16,1/32,1/64); `decay` 0..1;
//! `pitchstep` −12..12 st; `tapesync` 0..4 (Free,1beat,1/2bar,1bar,2bar); `tapefree` s;
//! `tapecurve` 0..1; `taperel` 0/1 (Ramp/Instant); `quant` 0..3 (Off,1/16,1/8,1/4);
//! `mix` 0..1; `out` dB.

use crate::dsp::{QuantDiv, Settings, StutterDiv, TapeRelease, TapeSync};
use suite_core::presets::Preset;

/// Factory presets, in menu order (≥6): Classic Stop 1 Bar, Fast Brake, Stutter 16th Decay,
/// Reverse Sweep, Half Time Groove, DJ Kill.
pub const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Classic Stop 1 Bar", "category": "FX",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 0, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Fast Brake", "category": "FX",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 1, "tapefree": 0.25, "tapecurve": 0.15, "taperel": 1,
         "quant": 0, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Stutter 16th Decay", "category": "Rhythmic",
         "stutdiv": 2, "decay": 0.35, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 2, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Reverse Sweep", "category": "FX",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.7, "taperel": 0,
         "quant": 0, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "Half Time Groove", "category": "Rhythmic",
         "stutdiv": 1, "decay": 0.0, "pitchstep": 0,
         "tapesync": 3, "tapefree": 1.0, "tapecurve": 0.5, "taperel": 1,
         "quant": 0, "mix": 1.0, "out": 0.0 }"#,
    r#"{ "name": "DJ Kill", "category": "Glitch",
         "stutdiv": 3, "decay": 0.6, "pitchstep": 7,
         "tapesync": 1, "tapefree": 0.35, "tapecurve": 0.1, "taperel": 1,
         "quant": 1, "mix": 1.0, "out": 0.0 }"#,
];

/// Build the DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        stutter_div: StutterDiv::from_index(g("stutdiv", 1.0) as usize),
        stutter_decay: g("decay", d.stutter_decay),
        stutter_pitch: g("pitchstep", d.stutter_pitch as f32) as i32,
        tape_sync: TapeSync::from_index(g("tapesync", 3.0) as usize),
        tape_free_s: g("tapefree", d.tape_free_s),
        tape_curve: g("tapecurve", d.tape_curve),
        tape_release: TapeRelease::from_index(g("taperel", 1.0) as usize),
        quantize: QuantDiv::from_index(g("quant", 0.0) as usize),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    #[test]
    fn all_presets_parse() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        for p in &presets {
            let _ = settings_from_preset(p);
        }
    }
}
