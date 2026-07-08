//! VOXKEY factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`; the same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Keys (all plain numbers): `root` = 0..11 (C..B); `scale` = index 0..6 (see
//! [`crate::dsp::SCALE_NAMES`] — Chromatic/Major/Natural Minor/Harmonic Minor/Phrygian/
//! Dorian/Minor Pentatonic); `retune` = ms 0..400 (0 = hard snap); `amount` = 0..1;
//! `humanize` = peak cents drift 0..50; `formant` = semitones offset ±12; `gate` =
//! confidence 0..1; `midimode` = 0/1; `mix` = 0..1; `out` = dB.
//!
//! Categories (preset-bar sections): Natural / Hard-Tune / Scale-Locks / Character / Extreme.
//! Names are purpose-driven and genre-aware (atmospheric dnb / dark techno / Cynthoni-
//! Sewerslvt vocab) — never settings descriptions.

use crate::dsp::{Controls, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Natural — transparent correction, glides, partial amount ---------
    // Gentle Glide — natural correction: long glide, softened amount, tiny humanize, C major.
    r#"{ "name": "Gentle Glide", "category": "Natural", "root": 0, "scale": 1, "retune": 120.0, "amount": 0.8,
         "humanize": 4.0, "formant": 0.0, "gate": 0.55, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Subtle Live — transparent pitch nudge: slow glide, half amount, wider humanize, A major.
    r#"{ "name": "Subtle Live", "category": "Natural", "root": 9, "scale": 1, "retune": 200.0, "amount": 0.5,
         "humanize": 6.0, "formant": 0.0, "gate": 0.65, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Loose Natural Glide — G major, long glide keeping expression, generous humanize.
    r#"{ "name": "Loose Natural Glide", "category": "Natural", "root": 7, "scale": 1, "retune": 160.0, "amount": 0.65,
         "humanize": 8.0, "formant": 0.0, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Breathy Correction — D major, medium glide, high gate so breaths stay untouched.
    r#"{ "name": "Breathy Correction", "category": "Natural", "root": 2, "scale": 1, "retune": 90.0, "amount": 0.7,
         "humanize": 5.0, "formant": 0.0, "gate": 0.7, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Dorian Float — D dorian, airy long glide, a whisper of formant drop, gentle trim.
    r#"{ "name": "Dorian Float", "category": "Natural", "root": 2, "scale": 5, "retune": 140.0, "amount": 0.75,
         "humanize": 7.0, "formant": -0.5, "gate": 0.55, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,

    // ---- Hard-Tune — hard snap, full amount, genre-core robotic ----------
    // Hard Snap Am — classic autotune: hard snap, full amount, A natural minor, slight makeup.
    r#"{ "name": "Hard Snap Am", "category": "Hard-Tune", "root": 9, "scale": 2, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.0, "gate": 0.55, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Dark Pop Snap — C natural minor hard snap with formants dropped for a darker sheen.
    r#"{ "name": "Dark Pop Snap", "category": "Hard-Tune", "root": 0, "scale": 2, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": -1.0, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Hard Robotic Tune — F major hard snap, a touch of formant lift for plasticky edge.
    r#"{ "name": "Hard Robotic Tune", "category": "Hard-Tune", "root": 5, "scale": 1, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.5, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Trap Snap Key — G natural minor, near-hard 8 ms snap for a trap lead lock.
    r#"{ "name": "Trap Snap Key", "category": "Hard-Tune", "root": 7, "scale": 2, "retune": 8.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.0, "gate": 0.55, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Minor Key Lock — E natural minor dead-lock hard snap, lower gate to catch tails.
    r#"{ "name": "Minor Key Lock", "category": "Hard-Tune", "root": 4, "scale": 2, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.0, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,

    // ---- Scale-Locks — distinct scales/keys, medium glide -----------------
    // Phrygian Dark — E phrygian, medium glide, slight formant drop for a darker timbre.
    r#"{ "name": "Phrygian Dark", "category": "Scale-Locks", "root": 4, "scale": 4, "retune": 60.0, "amount": 1.0,
         "humanize": 3.0, "formant": -1.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Phrygian Ghost — A phrygian, dropped formants and a hint of dry blend for a hollow ghost.
    r#"{ "name": "Phrygian Ghost", "category": "Scale-Locks", "root": 9, "scale": 4, "retune": 45.0, "amount": 1.0,
         "humanize": 4.0, "formant": -2.0, "gate": 0.55, "midimode": 0, "mix": 0.9, "out": -1.0 }"#,
    // Harmonic Minor Veil — C harmonic minor, softened amount, faint formant drop, gentle trim.
    r#"{ "name": "Harmonic Minor Veil", "category": "Scale-Locks", "root": 0, "scale": 3, "retune": 70.0, "amount": 0.9,
         "humanize": 3.0, "formant": -0.5, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Pentatonic Lock — A minor pentatonic, hard-ish snap over a sparse tone set.
    r#"{ "name": "Pentatonic Lock", "category": "Scale-Locks", "root": 9, "scale": 6, "retune": 50.0, "amount": 1.0,
         "humanize": 2.0, "formant": 0.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,
    // Dorian Groove Lock — G dorian, medium glide with a sliver of expression left in.
    r#"{ "name": "Dorian Groove Lock", "category": "Scale-Locks", "root": 7, "scale": 5, "retune": 55.0, "amount": 0.95,
         "humanize": 3.0, "formant": 0.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Chromatic Drift — chromatic (nearest semitone), loose amount and wide humanize for drift.
    r#"{ "name": "Chromatic Drift", "category": "Scale-Locks", "root": 0, "scale": 0, "retune": 80.0, "amount": 0.6,
         "humanize": 10.0, "formant": 0.0, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": 0.0 }"#,

    // ---- Character — formant-driven timbre --------------------------------
    // Doll Formant — A minor snap with the formants pushed up for a small-headed doll voice.
    r#"{ "name": "Doll Formant", "category": "Character", "root": 9, "scale": 2, "retune": 30.0, "amount": 1.0,
         "humanize": 0.0, "formant": 4.0, "gate": 0.6, "midimode": 0, "mix": 1.0, "out": -1.0 }"#,
    // T-Pain Extreme — hard snap + a lift of formant for the plasticky robot-vocal sheen.
    r#"{ "name": "T-Pain Extreme", "category": "Character", "root": 0, "scale": 1, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 1.5, "gate": 0.5, "midimode": 0, "mix": 1.0, "out": -0.5 }"#,
    // Deep Throat Formant — C minor snap with formants dragged down for a monstrous body.
    r#"{ "name": "Deep Throat Formant", "category": "Character", "root": 0, "scale": 2, "retune": 25.0, "amount": 1.0,
         "humanize": 0.0, "formant": -5.0, "gate": 0.55, "midimode": 0, "mix": 1.0, "out": -1.0 }"#,
    // Cynthoni Ghost Vox — A phrygian, dropped formants, heavy humanize and a dry veil for dnb.
    r#"{ "name": "Cynthoni Ghost Vox", "category": "Character", "root": 4, "scale": 4, "retune": 55.0, "amount": 0.85,
         "humanize": 9.0, "formant": -3.0, "gate": 0.5, "midimode": 0, "mix": 0.85, "out": -1.5 }"#,

    // ---- Extreme — pushed formants / gates, showcase artifacts ------------
    // Total Robotic Snap — C minor hard snap with formants slammed up and a low gate.
    r#"{ "name": "Total Robotic Snap", "category": "Extreme", "root": 0, "scale": 2, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 6.0, "gate": 0.4, "midimode": 0, "mix": 1.0, "out": -2.0 }"#,
    // Sewerslvt Chipmunk — A pentatonic hard snap, formants shoved way up for a chipmunk shriek.
    r#"{ "name": "Sewerslvt Chipmunk", "category": "Extreme", "root": 9, "scale": 6, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": 7.0, "gate": 0.45, "midimode": 0, "mix": 1.0, "out": -1.5 }"#,
    // MIDI Puppet — held MIDI note drives the pitch (scale ignored), fast glide, low gate.
    r#"{ "name": "MIDI Puppet", "category": "Extreme", "root": 0, "scale": 1, "retune": 15.0, "amount": 1.0,
         "humanize": 0.0, "formant": 0.0, "gate": 0.5, "midimode": 1, "mix": 1.0, "out": -0.5 }"#,
    // Sub Monster Lock — C minor hard snap, formants dragged to the floor for a sub-monster.
    r#"{ "name": "Sub Monster Lock", "category": "Extreme", "root": 0, "scale": 2, "retune": 0.0, "amount": 1.0,
         "humanize": 0.0, "formant": -8.0, "gate": 0.45, "midimode": 0, "mix": 1.0, "out": -1.5 }"#,
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

    /// Count how many effective [`Settings`] fields differ between two presets
    /// (root/scale/midi_mode by equality, the rest by a loose float epsilon). Drives both
    /// the differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.root != b.root {
            n += 1;
        }
        if a.scale != b.scale {
            n += 1;
        }
        if a.midi_mode != b.midi_mode {
            n += 1;
        }
        let fs = [
            (a.retune_ms, b.retune_ms),
            (a.amount, b.amount),
            (a.humanize_cents, b.humanize_cents),
            (a.formant_ratio, b.formant_ratio),
            (a.conf_gate, b.conf_gate),
            (a.mix, b.mix),
            (a.out_gain, b.out_gain),
        ];
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
        // Deep bank: SPECS target 15+ for the expanded VOXKEY bank.
        assert!(presets.len() >= 15, "VOXKEY bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the
        // default in >= 4 params, and every preset is categorised.
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
                    presets[i].name,
                    presets[j].name
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
