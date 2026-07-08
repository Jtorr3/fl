//! VOXKEY factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Keys (all plain numbers): `root` = 0..11 (C..B); `scale` = index 0..6 (see
//! [`crate::dsp::SCALE_NAMES`]); `retune` = ms (0 = hard snap); `amount` = 0..1;
//! `humanize` = peak cents drift; `formant` = semitones offset; `gate` = confidence 0..1;
//! `midimode` = 0/1; `mix` = 0..1; `out` = dB.

use crate::dsp::{Controls, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Hard Snap Am — classic autotune: retune 0, full amount, A natural minor.
    r#"{ "name": "Hard Snap Am", "root": 9, "scale": 2, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Gentle Glide — natural correction: long glide, softened amount, tiny humanize, C major.
    r#"{ "name": "Gentle Glide", "root": 0, "scale": 1, "retune": 120.0, "amount": 0.8,
         "humanize": 4.0, "formant": 0.0, "gate": 0.55, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Phrygian Dark — E phrygian, medium glide, slight formant drop for a darker timbre.
    r#"{ "name": "Phrygian Dark", "root": 4, "scale": 4, "retune": 60.0, "amount": 1.0,
         "humanize": 3.0, "formant": -1.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // T-Pain Extreme — hard snap + a lift of formant for the plasticky robot-vocal sheen.
    r#"{ "name": "T-Pain Extreme", "root": 0, "scale": 1, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 1.5, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Subtle Live — transparent pitch nudge: slow glide, half amount, wider humanize, A major.
    r#"{ "name": "Subtle Live", "root": 9, "scale": 1, "retune": 200.0, "amount": 0.5,
         "humanize": 6.0, "formant": 0.0, "gate": 0.65, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // MIDI Puppet — held MIDI note drives the pitch (scale ignored), fast glide.
    r#"{ "name": "MIDI Puppet", "root": 0, "scale": 1, "retune": 20.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.0, "gate": 0.5, "midimode": 1, "mix": 1.0, "out": 0.0 }"#,
    // Doll Formant — A minor snap with the formants pushed up for a small-headed doll voice.
    r#"{ "name": "Doll Formant", "root": 9, "scale": 2, "retune": 30.0, "amount": 1.0,
         "humanize": 0.0, "formant": 4.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": -1.0 }"#,
];

/// Build [`Controls`] from a parsed preset, falling back to defaults for omitted keys.
pub fn controls_from_preset(p: &Preset) -> Controls {
    let d = Controls::default();
    let g = |k: &str, fb: f32| p.get(k).unwrap_or(fb);
    Controls {
        root: g("root", d.root as f32).round().clamp(0.0, 11.0) as usize,
        scale: g("scale", d.scale as f32).round().max(0.0) as usize,
        retune_ms: g("retune", d.retune_ms),
        amount: g("amount", d.amount),
        humanize_cents: g("humanize", d.humanize_cents),
        formant_st: g("formant", d.formant_st),
        conf_gate: g("gate", d.conf_gate),
        midi_mode: g("midimode", 0.0) >= 0.5,
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

/// Resolve a preset directly to effective [`Settings`] (no live MIDI note).
pub fn settings_from_preset(p: &Preset) -> Settings {
    controls_from_preset(p).resolve(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// A vector of the numeric fields we compare for the "differs meaningfully" quality gate.
    fn feature_vec(s: &Settings) -> [f32; 10] {
        [
            s.root as f32,
            s.scale as f32,
            s.retune_ms,
            s.amount,
            s.humanize_cents,
            s.formant_ratio,
            s.conf_gate,
            if s.midi_mode { 1.0 } else { 0.0 },
            s.mix,
            s.out_gain,
        ]
    }

    #[test]
    fn all_presets_parse_and_differ() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = feature_vec(&Settings::default());
        let feats: Vec<[f32; 10]> = presets.iter().map(|p| feature_vec(&settings_from_preset(p))).collect();

        // ≥3 params differ from default.
        for (p, f) in presets.iter().zip(&feats) {
            let diffs = (0..10).filter(|&i| (f[i] - d[i]).abs() > 1e-3).count();
            assert!(diffs >= 3, "preset '{}' differs from default in only {diffs} params", p.name);
        }
        // ≥2 params differ between every pair of presets.
        for i in 0..feats.len() {
            for j in (i + 1)..feats.len() {
                let diffs = (0..10).filter(|&k| (feats[i][k] - feats[j][k]).abs() > 1e-3).count();
                assert!(
                    diffs >= 2,
                    "presets '{}' and '{}' differ in only {diffs} params",
                    presets[i].name,
                    presets[j].name
                );
            }
        }
    }
}
