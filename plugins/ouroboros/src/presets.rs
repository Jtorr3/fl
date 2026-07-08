//! OUROBOROS factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Value encodings (plain): `delay` ms; `feedback` 0..1.1; `decay` 0..1; `mix`/slot `*_amt`,
//! `*_param` 0..1; `out` dB; `sync`/`freeze` 0/1. Enum indices: `division` 0..6
//! (1/16,1/8,1/8·,1/4,1/4·,1/2,bar); `order` 0..5 (ABC,ACB,BAC,BCA,CAB,CBA); slot `*_type`
//! 0..8 (Off,Pitch,FilterLp,FilterHp,FilterBp,FreqShift,Saturate,Reverse,BitCrush).
//!
//! Note: no factory preset sets `freeze` — freeze is a live performance toggle that mutes the
//! input, so a from-scratch render with it on would be silent. The "Frozen Drone" preset
//! reaches a near-infinite sustain with 110 % feedback instead.

use crate::dsp::{Settings, SlotOrder, SlotSettings, SlotType, SyncDivision};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥5, PRD §1.4).
pub const PRESET_JSON: &[&str] = &[
    // Classic dub echo: filtered, lightly saturated repeats that darken as they recirculate.
    r#"{ "name": "Dub Tail", "delay": 420.0, "sync": 0, "division": 3, "feedback": 0.66,
         "decay": 1.0, "freeze": 0, "order": 0,
         "a_type": 2, "a_amt": 0.52, "a_param": 0.30,
         "b_type": 6, "b_amt": 0.22, "b_param": 0.10,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.42, "out": 0.0 }"#,
    // Each repeat pitches up ~1 semitone through a band-pass — an endless rising spiral.
    r#"{ "name": "Shifter Spiral", "delay": 300.0, "sync": 0, "division": 3, "feedback": 0.72,
         "decay": 1.0, "freeze": 0, "order": 0,
         "a_type": 1, "a_amt": 0.545, "a_param": 0.55,
         "b_type": 4, "b_amt": 0.55,  "b_param": 0.45,
         "c_type": 0, "c_amt": 0.5,   "c_param": 0.5,
         "mix": 0.5, "out": 0.0 }"#,
    // Bit-crushed, low-passed echoes: lo-fi digital decay.
    r#"{ "name": "Crushed Echoes", "delay": 260.0, "sync": 0, "division": 3, "feedback": 0.62,
         "decay": 1.0, "freeze": 0, "order": 0,
         "a_type": 8, "a_amt": 0.6,  "a_param": 0.4,
         "b_type": 2, "b_amt": 0.5,  "b_param": 0.25,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.5, "out": 0.0 }"#,
    // 110 % feedback into a filter + gentle saturator ⇒ a self-sustaining, near-infinite drone.
    r#"{ "name": "Frozen Drone", "delay": 520.0, "sync": 0, "division": 5, "feedback": 1.1,
         "decay": 1.0, "freeze": 0, "order": 0,
         "a_type": 2, "a_amt": 0.42, "a_param": 0.5,
         "b_type": 6, "b_amt": 0.28, "b_param": 0.15,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.7, "out": -1.0 }"#,
    // Reversed granules cascade through a high-pass — smeared, backwards tails.
    r#"{ "name": "Reverse Cascade", "delay": 360.0, "sync": 0, "division": 3, "feedback": 0.6,
         "decay": 1.0, "freeze": 0, "order": 0,
         "a_type": 7, "a_amt": 0.5,  "a_param": 0.2,
         "b_type": 3, "b_amt": 0.35, "b_param": 0.2,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.5, "out": 0.0 }"#,
    // Frequency-shifted repeats detune into inharmonic, bell-like clangor.
    r#"{ "name": "Frequency Clang", "delay": 210.0, "sync": 0, "division": 1, "feedback": 0.56,
         "decay": 1.0, "freeze": 0, "order": 0,
         "a_type": 5, "a_amt": 0.56, "a_param": 0.0,
         "b_type": 4, "b_amt": 0.5,  "b_param": 0.5,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.5, "out": 0.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let slot = |pfx: &str, df: &SlotSettings| SlotSettings {
        kind: SlotType::from_index(g(&format!("{pfx}_type"), 0.0) as usize),
        amount: g(&format!("{pfx}_amt"), df.amount),
        param: g(&format!("{pfx}_param"), df.param),
    };
    Settings {
        delay_ms: g("delay", d.delay_ms),
        sync: g("sync", 0.0) >= 0.5,
        division: SyncDivision::from_index(g("division", 3.0) as usize),
        tempo_bpm: 120.0,
        feedback: g("feedback", d.feedback),
        decay_scale: g("decay", d.decay_scale),
        freeze: g("freeze", 0.0) >= 0.5,
        freeze_mix: g("freezemix", d.freeze_mix),
        order: SlotOrder::from_index(g("order", 0.0) as usize),
        slots: [
            slot("a", &d.slots[0]),
            slot("b", &d.slots[1]),
            slot("c", &d.slots[2]),
        ],
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
        assert!(presets.len() >= 5, "need >= 5 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.delay_ms - d.delay_ms).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.feedback - d.feedback).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            if s.slots[0].kind != d.slots[0].kind {
                diffs += 1;
            }
            if s.slots[1].kind != d.slots[1].kind {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
