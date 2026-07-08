//! PLUCK factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render
//! tests (via [`settings_from_preset`]).
//!
//! Value encodings (plain): `source` 0/1/2 (Chord/MIDI/KeyDetect); `root` 0..11 (C..B);
//! `chord` 0..5 (m/m7/sus2/m9/5th/sus4); `dir` 0..2 (Up/Down/Alt); `decay`/`damp`/
//! `velbright`/`body`/`mix` 0..1; `strum` ms (5..80); `exgain` linear (0..2); `cont`/
//! `wetsolo` 0/1; `spread` cents (0..50); `stereoalt` 0..1; `out` dB.

use crate::dsp::{Chord, Settings, StrumDir, TuningSource, MAX_STRINGS};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, build brief).
pub const PRESET_JSON: &[&str] = &[
    // Warm, close, soft nylon-guitar chord — the reference sound.
    r#"{ "name": "Dark Nylon", "category": "Pluck",
         "source": 0, "root": 0, "chord": 0, "decay": 0.6, "damp": 0.6, "strum": 35.0,
         "dir": 0, "exgain": 1.0, "cont": 0, "velbright": 0.4, "body": 0.5,
         "spread": 5.0, "stereoalt": 0.4, "wetsolo": 0, "mix": 1.0, "out": 0.0 }"#,
    // Bright, ringing power-string cloud driven continuously — shimmery and metallic.
    r#"{ "name": "Metallic Cloud", "category": "Texture",
         "source": 0, "root": 0, "chord": 4, "decay": 0.85, "damp": 0.15, "strum": 15.0,
         "dir": 2, "exgain": 1.2, "cont": 1, "velbright": 0.5, "body": 0.3,
         "spread": 12.0, "stereoalt": 0.7, "wetsolo": 0, "mix": 0.8, "out": -1.0 }"#,
    // Deep, slow m9 harp — long decay, dark damping, big body.
    r#"{ "name": "Sub Harp", "category": "Pluck",
         "source": 0, "root": 0, "chord": 3, "decay": 0.9, "damp": 0.7, "strum": 60.0,
         "dir": 0, "exgain": 1.0, "cont": 0, "velbright": 0.3, "body": 0.6,
         "spread": 3.0, "stereoalt": 0.5, "wetsolo": 0, "mix": 0.9, "out": 0.0 }"#,
    // Key-tracked sympathetic resonance wash — follows the input's key, blended under dry.
    r#"{ "name": "Sympathetic Wash", "category": "Resonator",
         "source": 2, "root": 0, "chord": 0, "decay": 0.95, "damp": 0.5, "strum": 25.0,
         "dir": 0, "exgain": 0.8, "cont": 1, "velbright": 0.2, "body": 0.7,
         "spread": 8.0, "stereoalt": 0.6, "wetsolo": 0, "mix": 0.6, "out": 0.0 }"#,
    // Tight, fast, short-decay stab machine — staccato downstrums.
    r#"{ "name": "Staccato Machine", "category": "Pluck",
         "source": 0, "root": 0, "chord": 5, "decay": 0.2, "damp": 0.4, "strum": 8.0,
         "dir": 1, "exgain": 1.5, "cont": 0, "velbright": 0.6, "body": 0.25,
         "spread": 2.0, "stereoalt": 0.5, "wetsolo": 0, "mix": 1.0, "out": 0.0 }"#,
    // Wide, heavily detuned sus2 dream — slow alternate strums, huge spread + stereo.
    r#"{ "name": "Detuned Dream", "category": "Texture",
         "source": 0, "root": 0, "chord": 2, "decay": 0.8, "damp": 0.45, "strum": 45.0,
         "dir": 2, "exgain": 1.0, "cont": 0, "velbright": 0.3, "body": 0.55,
         "spread": 30.0, "stereoalt": 0.8, "wetsolo": 0, "mix": 0.85, "out": 0.0 }"#,
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
