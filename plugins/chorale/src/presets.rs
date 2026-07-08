//! CHORALE factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests
//! (via [`settings_from_preset`]).
//!
//! Value encodings (plain): `source` 0/1/2 (Scale/MIDI/KeyDetect); `root` 0..11 (C..B);
//! `scale` 0..11 (mTriad/MTriad/m7/M7/sus2/sus4/5th/mPent/MPent/phryg/dorian/octaves);
//! `count` 12..24; `decay`/`damp`/`spread(ct)`/`sympathetic`/`stereo`/`mix` 0..1 (spread in
//! cents 0..50); `excite` linear 0..2; `wetsolo` 0/1; `out` dB.

use crate::dsp::{Scale, Settings, TuningSource, MAX_RESONATORS};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, build brief).
pub const PRESET_JSON: &[&str] = &[
    // The reference sound: an A-minor bank singing sympathetically under the source.
    r#"{ "name": "Sympathetic Am", "category": "Resonator",
         "source": 0, "root": 9, "scale": 0, "count": 16, "decay": 0.9, "damp": 0.4,
         "spread": 6.0, "sympathetic": 0.8, "excite": 1.0, "stereo": 0.6,
         "wetsolo": 0, "mix": 0.5, "out": 0.0 }"#,
    // Dark, slow Phrygian drone bed on E — long decay, damped, wide.
    r#"{ "name": "Phrygian Drone Bed", "category": "Drone",
         "source": 0, "root": 4, "scale": 9, "count": 20, "decay": 0.96, "damp": 0.62,
         "spread": 8.0, "sympathetic": 0.4, "excite": 0.9, "stereo": 0.75,
         "wetsolo": 0, "mix": 0.65, "out": -1.0 }"#,
    // Bright, ringing major-7 glass — full bank, low damping, big spread + stereo.
    r#"{ "name": "Glass Choir", "category": "Texture",
         "source": 0, "root": 0, "scale": 3, "count": 24, "decay": 0.85, "damp": 0.15,
         "spread": 11.0, "sympathetic": 0.6, "excite": 1.0, "stereo": 0.85,
         "wetsolo": 0, "mix": 0.55, "out": -1.5 }"#,
    // Deep octave-stacked sub resonance on C — narrow, dark, powerful.
    r#"{ "name": "Sub Resonance", "category": "Drone",
         "source": 0, "root": 0, "scale": 11, "count": 12, "decay": 0.93, "damp": 0.7,
         "spread": 2.0, "sympathetic": 0.5, "excite": 1.1, "stereo": 0.3,
         "wetsolo": 0, "mix": 0.5, "out": 0.0 }"#,
    // Lush, heavily-detuned sus2 shimmer — full bank, huge spread, very wide.
    r#"{ "name": "Wide Shimmer Strings", "category": "Texture",
         "source": 0, "root": 2, "scale": 4, "count": 24, "decay": 0.82, "damp": 0.3,
         "spread": 24.0, "sympathetic": 0.5, "excite": 1.0, "stereo": 0.95,
         "wetsolo": 0, "mix": 0.6, "out": -1.0 }"#,
    // Short, resonant power-5 body — fast decay, tighter, wet-forward.
    r#"{ "name": "Tight Body", "category": "Resonator",
         "source": 0, "root": 0, "scale": 6, "count": 14, "decay": 0.38, "damp": 0.45,
         "spread": 3.0, "sympathetic": 0.7, "excite": 1.3, "stereo": 0.4,
         "wetsolo": 0, "mix": 0.7, "out": 0.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        source: TuningSource::from_index(g("source", 0.0) as usize),
        root_pc: g("root", d.root_pc as f32) as i32,
        scale: Scale::from_index(g("scale", 0.0) as usize),
        count: (g("count", d.count as f32) as usize).clamp(12, MAX_RESONATORS),
        decay: g("decay", d.decay),
        damp: g("damp", d.damp),
        spread_cents: g("spread", d.spread_cents),
        sympathetic: g("sympathetic", d.sympathetic),
        excite: g("excite", d.excite),
        stereo: g("stereo", d.stereo),
        wet_solo: g("wetsolo", 0.0) > 0.5,
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
        held: [f32::NAN; MAX_RESONATORS],
        held_count: 0,
    }
}
