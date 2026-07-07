//! SNAP factory presets (SPECS list: Rimshot Knock, Wet Techno Clap, DnB Crack, Gunshot Layer,
//! 90s Machine Clap, Airy Top Snare). Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//! Values are plain (un-normalized): Hz for tune, ms for decay/spread, 0..1 for macros,
//! integer for taps, dB for level.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// The six factory presets, in menu order.
pub const PRESET_JSON: &[&str] = &[
    // Rimshot Knock — snare-leaning, tight body knock, minimal noise, short.
    r#"{ "name": "Rimshot Knock", "mode": 0.1, "tune": 260.0, "balance": 0.35, "snap": 0.6,
         "decay": 140.0, "taps": 3, "spread": 12.0, "humanize": 0.15, "tone": 0.6, "drive": 0.25,
         "width": 0.2, "level": -1.0, "keytrack": 0 }"#,
    // Wet Techno Clap — clap-leaning, wide, longer tail, mid tone.
    r#"{ "name": "Wet Techno Clap", "mode": 0.9, "tune": 180.0, "balance": 0.8, "snap": 0.55,
         "decay": 320.0, "taps": 5, "spread": 26.0, "humanize": 0.5, "tone": 0.45, "drive": 0.2,
         "width": 0.75, "level": -1.5, "keytrack": 0 }"#,
    // DnB Crack — hybrid, bright, fast, aggressive drive.
    r#"{ "name": "DnB Crack", "mode": 0.45, "tune": 220.0, "balance": 0.6, "snap": 0.8,
         "decay": 180.0, "taps": 4, "spread": 16.0, "humanize": 0.35, "tone": 0.75, "drive": 0.45,
         "width": 0.4, "level": -1.0, "keytrack": 0 }"#,
    // Gunshot Layer — big hybrid slap, heavy drive, long tail, wide.
    r#"{ "name": "Gunshot Layer", "mode": 0.6, "tune": 150.0, "balance": 0.7, "snap": 0.5,
         "decay": 500.0, "taps": 5, "spread": 30.0, "humanize": 0.7, "tone": 0.4, "drive": 0.7,
         "width": 0.6, "level": -2.0, "keytrack": 0 }"#,
    // 90s Machine Clap — clap, tight machine spread, low humanize, dry-ish.
    r#"{ "name": "90s Machine Clap", "mode": 0.85, "tune": 200.0, "balance": 0.85, "snap": 0.45,
         "decay": 240.0, "taps": 4, "spread": 18.0, "humanize": 0.1, "tone": 0.55, "drive": 0.15,
         "width": 0.35, "level": -1.5, "keytrack": 0 }"#,
    // Airy Top Snare — snare, noisy/airy, high tone, moderate length, wide.
    r#"{ "name": "Airy Top Snare", "mode": 0.25, "tune": 240.0, "balance": 0.75, "snap": 0.65,
         "decay": 260.0, "taps": 3, "spread": 14.0, "humanize": 0.25, "tone": 0.85, "drive": 0.1,
         "width": 0.55, "level": -1.0, "keytrack": 0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        mode: g("mode", d.mode),
        tune: g("tune", d.tune),
        balance: g("balance", d.balance),
        snap: g("snap", d.snap),
        decay_ms: g("decay", d.decay_ms),
        taps: g("taps", d.taps as f32) as usize,
        spread_ms: g("spread", d.spread_ms),
        humanize: g("humanize", d.humanize),
        tone: g("tone", d.tone),
        drive: g("drive", d.drive),
        width: g("width", d.width),
        level_db: g("level", d.level_db),
    }
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
            if (s.mode - d.mode).abs() > 1e-3 { diffs += 1; }
            if (s.tune - d.tune).abs() > 1e-3 { diffs += 1; }
            if (s.decay_ms - d.decay_ms).abs() > 1e-3 { diffs += 1; }
            if (s.balance - d.balance).abs() > 1e-3 { diffs += 1; }
            if (s.drive - d.drive).abs() > 1e-3 { diffs += 1; }
            if s.taps != d.taps { diffs += 1; }
            if (s.width - d.width).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
