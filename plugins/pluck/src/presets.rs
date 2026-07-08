//! PLUCK factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`; the same list drives the GUI selector
//! (grouped by the `"category"` tag into preset-bar sections) and the offline render tests
//! (via [`settings_from_preset`]).
//!
//! Value encodings (plain): `source` 0/1/2 (Chord/MIDI/KeyDetect); `root` 0..11 (C..B);
//! `chord` 0..5 (m/m7/sus2/m9/5th/sus4); `dir` 0..2 (Up/Down/Alt); `decay`/`damp`/
//! `velbright`/`body`/`mix` 0..1; `strum` ms (5..80); `exgain` linear (0..2); `cont`/
//! `wetsolo` 0/1; `spread` cents (0..50); `stereoalt` 0..1; `out` dB.
//!
//! Categories (preset-bar sections), in menu order by articulation/use:
//! Chord Voices / Sympathetic Drones / Techno Stabs / Wide Atmospheres / Extremes.
//! Names are purpose-driven and genre-aware (dark techno / atmospheric dnb /
//! Cynthoni-Sewerslvt taste) — never settings descriptions.

use crate::dsp::{Chord, Settings, StrumDir, TuningSource, MAX_STRINGS};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Chord Voices -----------------------------------------------------
    // Warm, close, soft nylon-guitar minor chord — the reference sound.
    r#"{ "name": "Dark Nylon", "category": "Chord Voices",
         "source": 0, "root": 0, "chord": 0, "decay": 0.6, "damp": 0.6, "strum": 35.0,
         "dir": 0, "exgain": 1.0, "cont": 0, "velbright": 0.4, "body": 0.5,
         "spread": 5.0, "stereoalt": 0.4, "wetsolo": 0, "mix": 1.0, "out": 0.0 }"#,
    // Rounded A-minor7 with a little more body and a slower upstrum.
    r#"{ "name": "Ashen Minor Seven", "category": "Chord Voices",
         "source": 0, "root": 9, "chord": 1, "decay": 0.65, "damp": 0.55, "strum": 30.0,
         "dir": 0, "exgain": 1.1, "cont": 0, "velbright": 0.45, "body": 0.55,
         "spread": 6.0, "stereoalt": 0.45, "wetsolo": 0, "mix": 1.0, "out": -0.5 }"#,
    // Hollow D-sus4 downstrum — open, questioning, mid-decay.
    r#"{ "name": "Grave Suspension", "category": "Chord Voices",
         "source": 0, "root": 2, "chord": 5, "decay": 0.55, "damp": 0.5, "strum": 40.0,
         "dir": 1, "exgain": 1.0, "cont": 0, "velbright": 0.35, "body": 0.45,
         "spread": 4.0, "stereoalt": 0.5, "wetsolo": 0, "mix": 0.95, "out": -0.5 }"#,
    // Deep, slow F-minor9 harp — long decay, dark damping, big body.
    r#"{ "name": "Cellar Harp", "category": "Chord Voices",
         "source": 0, "root": 5, "chord": 3, "decay": 0.9, "damp": 0.7, "strum": 60.0,
         "dir": 0, "exgain": 1.0, "cont": 0, "velbright": 0.3, "body": 0.6,
         "spread": 3.0, "stereoalt": 0.5, "wetsolo": 0, "mix": 0.9, "out": -1.0 }"#,
    // Play-me-a-chord: MIDI-held source (falls back to Chord when nothing is held),
    // set up as a soft comping voice for a MIDI track feeding PLUCK.
    r#"{ "name": "Midi Comp Strings", "category": "Chord Voices",
         "source": 1, "root": 0, "chord": 1, "decay": 0.7, "damp": 0.5, "strum": 28.0,
         "dir": 2, "exgain": 1.1, "cont": 0, "velbright": 0.5, "body": 0.5,
         "spread": 7.0, "stereoalt": 0.55, "wetsolo": 0, "mix": 1.0, "out": -0.5 }"#,
    // ---- Sympathetic Drones ----------------------------------------------
    // Key-tracked sympathetic resonance wash — follows the input's key, under the dry.
    r#"{ "name": "Sympathetic Wash", "category": "Sympathetic Drones",
         "source": 2, "root": 0, "chord": 0, "decay": 0.95, "damp": 0.5, "strum": 25.0,
         "dir": 0, "exgain": 0.8, "cont": 1, "velbright": 0.2, "body": 0.7,
         "spread": 8.0, "stereoalt": 0.6, "wetsolo": 0, "mix": 0.6, "out": 0.0 }"#,
    // Continuous key-detect drift, wide alternate strums — a haunted choir under a mix.
    r#"{ "name": "Ghost Choir Drift", "category": "Sympathetic Drones",
         "source": 2, "root": 7, "chord": 0, "decay": 0.92, "damp": 0.6, "strum": 35.0,
         "dir": 2, "exgain": 0.7, "cont": 1, "velbright": 0.2, "body": 0.65,
         "spread": 10.0, "stereoalt": 0.7, "wetsolo": 0, "mix": 0.5, "out": -1.0 }"#,
    // Dark m9 undertow: continuous drive, heavy damping, low blend — a resonant floor.
    r#"{ "name": "Undertow Resonance", "category": "Sympathetic Drones",
         "source": 0, "root": 0, "chord": 3, "decay": 0.88, "damp": 0.75, "strum": 50.0,
         "dir": 0, "exgain": 0.6, "cont": 1, "velbright": 0.15, "body": 0.7,
         "spread": 6.0, "stereoalt": 0.55, "wetsolo": 0, "mix": 0.55, "out": -1.0 }"#,
    // Wide, murky key-tracked sus2 bloom — Cynthoni-Sewerslvt sympathetic haze.
    r#"{ "name": "Sewer Bloom", "category": "Sympathetic Drones",
         "source": 2, "root": 10, "chord": 2, "decay": 0.9, "damp": 0.55, "strum": 45.0,
         "dir": 2, "exgain": 0.7, "cont": 1, "velbright": 0.25, "body": 0.6,
         "spread": 14.0, "stereoalt": 0.75, "wetsolo": 0, "mix": 0.45, "out": -1.0 }"#,
    // ---- Techno Stabs -----------------------------------------------------
    // Tight, fast, short-decay stab machine — staccato sus4 downstrums.
    r#"{ "name": "Staccato Machine", "category": "Techno Stabs",
         "source": 0, "root": 0, "chord": 5, "decay": 0.2, "damp": 0.4, "strum": 8.0,
         "dir": 1, "exgain": 1.5, "cont": 0, "velbright": 0.6, "body": 0.25,
         "spread": 2.0, "stereoalt": 0.5, "wetsolo": 0, "mix": 1.0, "out": 0.0 }"#,
    // Dark, dry power-5th pluck — tight warehouse-techno pluck hit.
    r#"{ "name": "Warehouse Pluck", "category": "Techno Stabs",
         "source": 0, "root": 0, "chord": 4, "decay": 0.35, "damp": 0.45, "strum": 10.0,
         "dir": 0, "exgain": 1.4, "cont": 0, "velbright": 0.55, "body": 0.3,
         "spread": 3.0, "stereoalt": 0.4, "wetsolo": 0, "mix": 1.0, "out": -0.5 }"#,
    // Fast G power-5th downstab, dry and percussive — a bunker rhythm stab.
    r#"{ "name": "Bunker Stab", "category": "Techno Stabs",
         "source": 0, "root": 7, "chord": 4, "decay": 0.3, "damp": 0.5, "strum": 7.0,
         "dir": 1, "exgain": 1.5, "cont": 0, "velbright": 0.6, "body": 0.2,
         "spread": 2.0, "stereoalt": 0.45, "wetsolo": 0, "mix": 1.0, "out": -0.5 }"#,
    // ---- Wide Atmospheres -------------------------------------------------
    // Wide, heavily detuned sus2 dream — slow alternate strums, huge spread + stereo.
    r#"{ "name": "Detuned Dream", "category": "Wide Atmospheres",
         "source": 0, "root": 0, "chord": 2, "decay": 0.8, "damp": 0.45, "strum": 45.0,
         "dir": 2, "exgain": 1.0, "cont": 0, "velbright": 0.3, "body": 0.55,
         "spread": 30.0, "stereoalt": 0.8, "wetsolo": 0, "mix": 0.85, "out": 0.0 }"#,
    // Bright E-sus2 shimmer spread wide — glassy, rain-on-window atmospheric dnb.
    r#"{ "name": "Rain On Glass", "category": "Wide Atmospheres",
         "source": 0, "root": 4, "chord": 2, "decay": 0.75, "damp": 0.35, "strum": 50.0,
         "dir": 2, "exgain": 0.9, "cont": 0, "velbright": 0.3, "body": 0.5,
         "spread": 22.0, "stereoalt": 0.85, "wetsolo": 0, "mix": 0.8, "out": -1.0 }"#,
    // Slow B-minor7 alternate strums, wide and mournful — distant-sirens pad.
    r#"{ "name": "Distant Sirens", "category": "Wide Atmospheres",
         "source": 0, "root": 11, "chord": 1, "decay": 0.82, "damp": 0.5, "strum": 55.0,
         "dir": 2, "exgain": 0.9, "cont": 0, "velbright": 0.35, "body": 0.55,
         "spread": 26.0, "stereoalt": 0.8, "wetsolo": 0, "mix": 0.8, "out": -1.0 }"#,
    // Eb-minor9 upstrum, slow and hazy — a nocturne drift for atmospheric dnb beds.
    r#"{ "name": "Nocturne Drift", "category": "Wide Atmospheres",
         "source": 0, "root": 3, "chord": 3, "decay": 0.85, "damp": 0.55, "strum": 60.0,
         "dir": 0, "exgain": 0.85, "cont": 0, "velbright": 0.3, "body": 0.6,
         "spread": 18.0, "stereoalt": 0.7, "wetsolo": 0, "mix": 0.75, "out": -1.0 }"#,
    // ---- Extremes ---------------------------------------------------------
    // Bright, ringing power-5th cloud driven continuously — shimmery and metallic.
    r#"{ "name": "Metallic Cloud", "category": "Extremes",
         "source": 0, "root": 0, "chord": 4, "decay": 0.85, "damp": 0.15, "strum": 15.0,
         "dir": 2, "exgain": 1.2, "cont": 1, "velbright": 0.5, "body": 0.3,
         "spread": 12.0, "stereoalt": 0.7, "wetsolo": 0, "mix": 0.8, "out": -1.0 }"#,
    // Wet-solo m9 cathedral: long decay, big body, slow wide strums — pure resonance.
    r#"{ "name": "Endless Cathedral", "category": "Extremes",
         "source": 0, "root": 0, "chord": 3, "decay": 0.93, "damp": 0.6, "strum": 70.0,
         "dir": 2, "exgain": 0.8, "cont": 0, "velbright": 0.25, "body": 0.75,
         "spread": 16.0, "stereoalt": 0.65, "wetsolo": 1, "mix": 1.0, "out": -2.0 }"#,
    // Extreme bright power-5th, max spread and stereo, wet-solo — screaming ghost strings.
    r#"{ "name": "Nyquist Ghost Strings", "category": "Extremes",
         "source": 0, "root": 0, "chord": 4, "decay": 0.9, "damp": 0.1, "strum": 20.0,
         "dir": 2, "exgain": 1.0, "cont": 0, "velbright": 0.6, "body": 0.35,
         "spread": 40.0, "stereoalt": 0.9, "wetsolo": 1, "mix": 1.0, "out": -2.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        source: TuningSource::from_index(g("source", 0.0) as usize),
        root_pc: g("root", 0.0) as i32,
        chord: Chord::from_index(g("chord", 0.0) as usize),
        decay: g("decay", d.decay),
        damp: g("damp", d.damp),
        strum_ms: g("strum", d.strum_ms),
        dir: StrumDir::from_index(g("dir", 0.0) as usize),
        exciter_gain: g("exgain", d.exciter_gain),
        continuous: g("cont", 0.0) > 0.5,
        vel_bright: g("velbright", d.vel_bright),
        body: g("body", d.body),
        spread_cents: g("spread", d.spread_cents),
        stereo_alt: g("stereoalt", d.stereo_alt),
        wet_solo: g("wetsolo", 0.0) > 0.5,
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
        held: [f32::NAN; MAX_STRINGS],
        held_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enums/bools by
    /// equality, floats by a loose epsilon). Covers every field `settings_from_preset`
    /// sets; the fixed `held`/`held_count` constants are skipped. Drives both the
    /// differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.source != b.source { n += 1; }
        if a.root_pc != b.root_pc { n += 1; }
        if a.chord != b.chord { n += 1; }
        if a.dir != b.dir { n += 1; }
        if a.continuous != b.continuous { n += 1; }
        if a.wet_solo != b.wet_solo { n += 1; }
        let fs = [
            (a.decay, b.decay), (a.damp, b.damp), (a.strum_ms, b.strum_ms),
            (a.exciter_gain, b.exciter_gain), (a.vel_bright, b.vel_bright),
            (a.body, b.body), (a.spread_cents, b.spread_cents),
            (a.stereo_alt, b.stereo_alt), (a.mix, b.mix), (a.out_db, b.out_db),
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
        // Deep bank: SPECS target 15-30 for a complex instrument.
        assert!(presets.len() >= 15, "PLUCK bank too small: {}", presets.len());

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
        // `presets_pass_universal` test in tests.rs.
    }
}
