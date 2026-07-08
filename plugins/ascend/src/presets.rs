//! ASCEND factory presets (SPECS list: Riser 8 Dark, Riser 16 Wide, Sub Boom Drop, Downlifter 8,
//! Noise Swell Short, Melodic Fifth Rise). Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//! Values are plain (un-normalized): int for key/octave/sync/bars, Hz for filter, 0..1 for macros,
//! semitones for rise, seconds for free length, dB for level, 0/1 for booleans.

use crate::dsp::{Settings, SyncTarget};
use suite_core::presets::Preset;

/// The six factory presets, in menu order.
pub const PRESET_JSON: &[&str] = &[
    // Riser 8 Dark — classic 8-bar techno riser, low root, pinky noise, moderate rise, impact drop.
    r#"{ "name": "Riser 8 Dark", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.35,
         "balance": 0.45, "color": 0.85, "wave": 0.4, "fstart": 150.0, "fend": 8000.0, "rise": 12.0,
         "width": 0.5, "impact": 1, "implevel": 0.85, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // Riser 16 Wide — long 16-bar build, wide bloom, bright top, softer log curve.
    r#"{ "name": "Riser 16 Wide", "key": 7, "octave": 2, "sync": 1, "bars": 16, "curve": 0.65,
         "balance": 0.4, "color": 0.7, "wave": 0.5, "fstart": 200.0, "fend": 11000.0, "rise": 10.0,
         "width": 0.85, "impact": 1, "implevel": 0.7, "autocut": 1, "downlifter": 0, "freelen": 6.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // Sub Boom Drop — tonal-leaning, big pitch rise, loud low impact, tight 8-bar.
    r#"{ "name": "Sub Boom Drop", "key": 0, "octave": 1, "sync": 0, "bars": 8, "curve": 0.25,
         "balance": 0.65, "color": 0.6, "wave": 0.25, "fstart": 120.0, "fend": 6000.0, "rise": 24.0,
         "width": 0.3, "impact": 1, "implevel": 1.0, "autocut": 1, "downlifter": 0, "freelen": 3.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Downlifter 8 — reversed: full at the boundary, falls away over 8 bars, impact on the drop.
    r#"{ "name": "Downlifter 8", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.6,
         "balance": 0.35, "color": 0.8, "wave": 0.45, "fstart": 300.0, "fend": 9000.0, "rise": 8.0,
         "width": 0.6, "impact": 1, "implevel": 0.9, "autocut": 0, "downlifter": 1, "freelen": 4.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // Noise Swell Short — all-noise atmospheric 8-bar swell, no impact, wide.
    r#"{ "name": "Noise Swell Short", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.5,
         "balance": 0.15, "color": 0.9, "wave": 0.5, "fstart": 300.0, "fend": 12000.0, "rise": 4.0,
         "width": 0.9, "impact": 0, "implevel": 0.5, "autocut": 1, "downlifter": 0, "freelen": 2.5,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Melodic Fifth Rise — tonal root+fifth foregrounded, sine-ish, exp curve, moderate rise.
    r#"{ "name": "Melodic Fifth Rise", "key": 9, "octave": 2, "sync": 0, "bars": 8, "curve": 0.3,
         "balance": 0.8, "color": 0.5, "wave": 0.7, "fstart": 250.0, "fend": 9000.0, "rise": 12.0,
         "width": 0.55, "impact": 1, "implevel": 0.65, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        key: g("key", d.key as f32) as usize,
        octave: g("octave", d.octave as f32) as i32,
        sync: SyncTarget::from_index(g("sync", 0.0) as usize),
        custom_bars: g("bars", d.custom_bars),
        curve: g("curve", d.curve),
        balance: g("balance", d.balance),
        color: g("color", d.color),
        wave: g("wave", d.wave),
        filter_start_hz: g("fstart", d.filter_start_hz),
        filter_end_hz: g("fend", d.filter_end_hz),
        rise_st: g("rise", d.rise_st),
        width: g("width", d.width),
        impact_on: g("impact", 1.0) >= 0.5,
        impact_level: g("implevel", d.impact_level),
        auto_cut: g("autocut", 1.0) >= 0.5,
        downlifter: g("downlifter", 0.0) >= 0.5,
        free_len_s: g("freelen", d.free_len_s),
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
            if s.key != d.key { diffs += 1; }
            if s.octave != d.octave { diffs += 1; }
            if s.sync != d.sync { diffs += 1; }
            if (s.curve - d.curve).abs() > 1e-3 { diffs += 1; }
            if (s.balance - d.balance).abs() > 1e-3 { diffs += 1; }
            if (s.color - d.color).abs() > 1e-3 { diffs += 1; }
            if (s.filter_end_hz - d.filter_end_hz).abs() > 1e-3 { diffs += 1; }
            if (s.rise_st - d.rise_st).abs() > 1e-3 { diffs += 1; }
            if (s.width - d.width).abs() > 1e-3 { diffs += 1; }
            if s.downlifter != d.downlifter { diffs += 1; }
            if s.impact_on != d.impact_on { diffs += 1; }
            assert!(diffs >= 4, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
