//! OUROBOROS factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Value encodings (plain): `delay` ms; `feedback` 0..1.1; `decay` 0..1; `mix`/slot `*_amt`,
//! `*_param` 0..1; `out` dB; `sync`/`freeze` 0/1. Enum indices: `division` 0..6
//! (1/16,1/8,1/8·,1/4,1/4·,1/2,bar); `order` 0..5 (ABC,ACB,BAC,BCA,CAB,CBA); slot `*_type`
//! 0..8 (Off,Pitch,FilterLp,FilterHp,FilterBp,FreqShift,Saturate,Reverse,BitCrush).
//!
//! Categories (preset-bar sections): Dub & Echo / Rhythmic Sync / Pitch & Spiral /
//! Texture & Drone / Mangled. Names are purpose-driven and genre-aware (dark techno /
//! atmospheric dnb / Sewerslvt-adjacent lo-fi) — never settings descriptions.
//!
//! Safety: the final ±8.0 output clamp is only a runaway/NaN guard (not a level ceiling);
//! feedback past unity self-oscillates but the in-loop `tanh` limiter clamps it
//! to a bounded limit cycle. Levels are kept conservative (mix ≥ 0.35, out ≤ 0 dB) so no
//! preset renders silent or runs away.
//!
//! Note: no factory preset sets `freeze` — freeze mutes the input, so a from-scratch render
//! with it on would be silent. Drone presets reach near-infinite sustain with high feedback.

use crate::dsp::{Settings, SlotOrder, SlotSettings, SlotType, SyncDivision};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Dub & Echo -------------------------------------------------------
    // Classic dub echo: low-passed, lightly saturated repeats that darken as they recirculate.
    r#"{ "name": "Basement Dub", "category": "Dub & Echo", "delay": 440.0, "sync": 0, "division": 3,
         "feedback": 0.62, "decay": 1.0, "order": 0,
         "a_type": 2, "a_amt": 0.48, "a_param": 0.25,
         "b_type": 6, "b_amt": 0.22, "b_param": 0.10,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.42, "out": 0.0 }"#,
    // Long, clean, sparse echoes — a wide dub throw for space and dread.
    r#"{ "name": "Rooftop Delay", "category": "Dub & Echo", "delay": 560.0, "sync": 0, "division": 3,
         "feedback": 0.52, "decay": 1.0, "order": 0,
         "a_type": 2, "a_amt": 0.62, "a_param": 0.15,
         "b_type": 0, "b_amt": 0.5,  "b_param": 0.5,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.36, "out": 0.0 }"#,
    // High-passed, tube-driven repeats — thin, smoky, pushed-into-the-red echo.
    r#"{ "name": "Smoke Echo", "category": "Dub & Echo", "delay": 380.0, "sync": 0, "division": 3,
         "feedback": 0.60, "decay": 1.0, "order": 2,
         "a_type": 6, "a_amt": 0.30, "a_param": 0.20,
         "b_type": 3, "b_amt": 0.22, "b_param": 0.20,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.40, "out": -0.5 }"#,
    // Band-passed, gritty repeats bouncing down a narrow corridor.
    r#"{ "name": "Concrete Alley", "category": "Dub & Echo", "delay": 300.0, "sync": 0, "division": 3,
         "feedback": 0.68, "decay": 1.0, "order": 0,
         "a_type": 4, "a_amt": 0.42, "a_param": 0.40,
         "b_type": 6, "b_amt": 0.25, "b_param": 0.15,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.45, "out": -0.5 }"#,
    // ---- Rhythmic Sync ----------------------------------------------------
    // Tempo-locked 1/16 stutters — dark-techno ghost notes tucked behind the beat.
    r#"{ "name": "Techno Ghost Notes", "category": "Rhythmic Sync", "delay": 250.0, "sync": 1, "division": 0,
         "feedback": 0.55, "decay": 1.0, "order": 0,
         "a_type": 2, "a_amt": 0.50, "a_param": 0.20,
         "b_type": 0, "b_amt": 0.5,  "b_param": 0.5,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.40, "out": 0.0 }"#,
    // Dotted-eighth throw with saturated low-pass tails — the syncopated dub-techno pulse.
    r#"{ "name": "Dotted Pulse", "category": "Rhythmic Sync", "delay": 300.0, "sync": 1, "division": 2,
         "feedback": 0.60, "decay": 1.0, "order": 0,
         "a_type": 6, "a_amt": 0.24, "a_param": 0.15,
         "b_type": 2, "b_amt": 0.55, "b_param": 0.25,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.42, "out": -0.5 }"#,
    // Half-note dubbed-out rumble, decay pulled back so it breathes with the track.
    r#"{ "name": "Halfstep Rumble", "category": "Rhythmic Sync", "delay": 300.0, "sync": 1, "division": 5,
         "feedback": 0.74, "decay": 0.9, "order": 0,
         "a_type": 2, "a_amt": 0.35, "a_param": 0.30,
         "b_type": 6, "b_amt": 0.28, "b_param": 0.10,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.45, "out": -1.0 }"#,
    // ---- Pitch & Spiral ---------------------------------------------------
    // Each repeat pitches up ~1 st through a band-pass — an endless rising spiral.
    r#"{ "name": "Ascension Spiral", "category": "Pitch & Spiral", "delay": 300.0, "sync": 0, "division": 3,
         "feedback": 0.70, "decay": 1.0, "order": 0,
         "a_type": 1, "a_amt": 0.545, "a_param": 0.55,
         "b_type": 4, "b_amt": 0.55,  "b_param": 0.40,
         "c_type": 0, "c_amt": 0.5,   "c_param": 0.5,
         "mix": 0.50, "out": 0.0 }"#,
    // Each repeat sinks a semitone through a closing low-pass — a bottomless descent.
    r#"{ "name": "Descent Well", "category": "Pitch & Spiral", "delay": 340.0, "sync": 0, "division": 3,
         "feedback": 0.66, "decay": 1.0, "order": 0,
         "a_type": 1, "a_amt": 0.44, "a_param": 0.50,
         "b_type": 2, "b_amt": 0.50, "b_param": 0.20,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.48, "out": 0.0 }"#,
    // Frequency-shifted repeats detune into inharmonic, bell-like clangor.
    r#"{ "name": "Detune Clang", "category": "Pitch & Spiral", "delay": 210.0, "sync": 0, "division": 1,
         "feedback": 0.56, "decay": 1.0, "order": 0,
         "a_type": 5, "a_amt": 0.56, "a_param": 0.0,
         "b_type": 4, "b_amt": 0.50, "b_param": 0.50,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.50, "out": 0.0 }"#,
    // Pitch-up feeding a slow frequency shift into a low-pass — a metallic chrome riser.
    r#"{ "name": "Chrome Riser", "category": "Pitch & Spiral", "delay": 260.0, "sync": 0, "division": 3,
         "feedback": 0.62, "decay": 1.0, "order": 0,
         "a_type": 1, "a_amt": 0.53, "a_param": 0.60,
         "b_type": 5, "b_amt": 0.54, "b_param": 0.30,
         "c_type": 2, "c_amt": 0.60, "c_param": 0.20,
         "mix": 0.50, "out": -0.5 }"#,
    // ---- Texture & Drone --------------------------------------------------
    // Long saturated low-pass wash — an atmospheric-dnb pad of grief that never quite resolves.
    r#"{ "name": "Grief Wash", "category": "Texture & Drone", "delay": 600.0, "sync": 0, "division": 3,
         "feedback": 0.80, "decay": 0.95, "order": 0,
         "a_type": 6, "a_amt": 0.35, "a_param": 0.20,
         "b_type": 2, "b_amt": 0.50, "b_param": 0.35,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.50, "out": -1.0 }"#,
    // 110 % feedback into filter + saturator ⇒ a self-sustaining, near-infinite cathedral drone.
    r#"{ "name": "Frozen Cathedral", "category": "Texture & Drone", "delay": 520.0, "sync": 0, "division": 5,
         "feedback": 1.1, "decay": 1.0, "order": 0,
         "a_type": 2, "a_amt": 0.42, "a_param": 0.50,
         "b_type": 6, "b_amt": 0.28, "b_param": 0.15,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.70, "out": -1.5 }"#,
    // Slight pitch drift through a warm low-pass and gentle tube — a fog of degraded tape.
    r#"{ "name": "Tape Fog", "category": "Texture & Drone", "delay": 480.0, "sync": 0, "division": 3,
         "feedback": 0.75, "decay": 1.0, "order": 0,
         "a_type": 1, "a_amt": 0.51, "a_param": 0.40,
         "b_type": 2, "b_amt": 0.55, "b_param": 0.20,
         "c_type": 6, "c_amt": 0.20, "c_param": 0.10,
         "mix": 0.50, "out": -1.0 }"#,
    // Band-passed reversed granules smear into a slow, tidal undertow.
    r#"{ "name": "Ambient Undertow", "category": "Texture & Drone", "delay": 700.0, "sync": 0, "division": 3,
         "feedback": 0.72, "decay": 1.0, "order": 0,
         "a_type": 4, "a_amt": 0.45, "a_param": 0.30,
         "b_type": 7, "b_amt": 0.50, "b_param": 0.40,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.45, "out": -1.0 }"#,
    // ---- Mangled ----------------------------------------------------------
    // Bit-crushed, low-passed echoes rotting into lo-fi sludge — Sewerslvt-grade decay.
    r#"{ "name": "Sewer Crush", "category": "Mangled", "delay": 300.0, "sync": 0, "division": 3,
         "feedback": 0.62, "decay": 1.0, "order": 0,
         "a_type": 8, "a_amt": 0.60, "a_param": 0.40,
         "b_type": 2, "b_amt": 0.50, "b_param": 0.25,
         "c_type": 0, "c_amt": 0.5,  "c_param": 0.5,
         "mix": 0.50, "out": -1.0 }"#,
    // Reversed granules high-passed and frequency-shifted into backwards, seasick smear.
    r#"{ "name": "Reverse Nightmare", "category": "Mangled", "delay": 360.0, "sync": 0, "division": 3,
         "feedback": 0.66, "decay": 1.0, "order": 1,
         "a_type": 7, "a_amt": 0.55, "a_param": 0.20,
         "b_type": 3, "b_amt": 0.35, "b_param": 0.20,
         "c_type": 5, "c_amt": 0.53, "c_param": 0.50,
         "mix": 0.50, "out": -1.0 }"#,
    // Near-unity feedback through saturate → band-pass → crush: a bounded self-oscillating collapse.
    r#"{ "name": "Total Collapse", "category": "Mangled", "delay": 180.0, "sync": 0, "division": 3,
         "feedback": 1.05, "decay": 1.0, "order": 0,
         "a_type": 6, "a_amt": 0.50, "a_param": 0.35,
         "b_type": 4, "b_amt": 0.45, "b_param": 0.50,
         "c_type": 8, "c_amt": 0.40, "c_param": 0.30,
         "mix": 0.60, "out": -2.0 }"#,
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

    /// Count how many `Settings` fields differ between two presets (enums/bools by equality,
    /// floats by a loose epsilon). Drives both the differ-from-default and pairwise-distinctness
    /// gates. Skips `tempo_bpm` (a hardcoded 120 constant), `freeze` and `freeze_mix` (no factory
    /// preset varies them), which are fixed across the whole bank.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.sync != b.sync { n += 1; }
        if a.division != b.division { n += 1; }
        if a.order != b.order { n += 1; }
        for i in 0..3 {
            if a.slots[i].kind != b.slots[i].kind { n += 1; }
        }
        let mut fs = vec![
            (a.delay_ms, b.delay_ms),
            (a.feedback, b.feedback),
            (a.decay_scale, b.decay_scale),
            (a.mix, b.mix),
            (a.out_db, b.out_db),
        ];
        for i in 0..3 {
            fs.push((a.slots[i].amount, b.slots[i].amount));
            fs.push((a.slots[i].param, b.slots[i].param));
        }
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 { n += 1; }
        }
        n
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank: SPECS target 15-30 for a complex FX.
        assert!(presets.len() >= 15, "OUROBOROS bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the default
        // in >= 4 params, and every preset is categorised.
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
