//! ASCEND factory presets (PRESET-EXPANSION deep bank). Each is an embedded flat-JSON blob parsed
//! by `suite_core::presets`; the same list drives the GUI selector (grouped by the `"category"` tag
//! into preset-bar sections) and the offline render tests. Values are plain (un-normalized): int for
//! key/octave/sync/bars, Hz for filter, 0..1 for macros, semitones for rise, seconds for free
//! length, dB for level, 0/1 for booleans.
//!
//! `sync` is an integer index (see `dsp::SyncTarget`: 0=Bars8 1=Bars16 2=Bars32 3=Custom; Custom
//! resolves to the `"bars"` value). Categories (preset-bar sections): Short-Risers / Long-Builds /
//! Noise-Sweeps / Tonal-Rises / Impacts-Drops. Names are purpose-driven and genre-aware (dark
//! techno / atmospheric dnb) — never settings descriptions.

use crate::dsp::{Settings, SyncTarget};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Short-Risers -----------------------------------------------------
    // Riser 8 Dark — classic 8-bar techno riser, low root, pinky noise, moderate rise, impact drop.
    r#"{ "name": "Riser 8 Dark", "category": "Short-Risers", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.35,
         "balance": 0.45, "color": 0.85, "wave": 0.4, "fstart": 150.0, "fend": 8000.0, "rise": 12.0,
         "width": 0.5, "impact": 1, "implevel": 0.85, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // Warehouse Drop Riser — sub-heavy dark-techno 8-bar, deep root, near-full pink noise, hard drop.
    r#"{ "name": "Warehouse Drop Riser", "category": "Short-Risers", "key": 0, "octave": 1, "sync": 0, "bars": 8, "curve": 0.32,
         "balance": 0.4, "color": 0.9, "wave": 0.3, "fstart": 140.0, "fend": 8500.0, "rise": 16.0,
         "width": 0.55, "impact": 1, "implevel": 0.95, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // 4-Bar Snap Rise — tight exponential 4-bar build for quick fills, bright top, punchy impact.
    r#"{ "name": "4-Bar Snap Rise", "category": "Short-Risers", "key": 0, "octave": 2, "sync": 3, "bars": 4, "curve": 0.3,
         "balance": 0.5, "color": 0.6, "wave": 0.4, "fstart": 160.0, "fend": 10000.0, "rise": 14.0,
         "width": 0.5, "impact": 1, "implevel": 0.9, "autocut": 1, "downlifter": 0, "freelen": 2.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // 8-Bar Tension — balanced neutral 8-bar tension builder, gentle rise, mid width.
    r#"{ "name": "8-Bar Tension", "category": "Short-Risers", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.45,
         "balance": 0.55, "color": 0.7, "wave": 0.35, "fstart": 200.0, "fend": 9500.0, "rise": 10.0,
         "width": 0.65, "impact": 1, "implevel": 0.8, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // Snap 2-Bar Fill — very short 2-bar snare-roll style lift, exp curve, hard drop.
    r#"{ "name": "Snap 2-Bar Fill", "category": "Short-Risers", "key": 0, "octave": 2, "sync": 3, "bars": 2, "curve": 0.25,
         "balance": 0.35, "color": 0.75, "wave": 0.45, "fstart": 220.0, "fend": 11000.0, "rise": 9.0,
         "width": 0.45, "impact": 1, "implevel": 0.9, "autocut": 1, "downlifter": 0, "freelen": 1.2,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // ---- Long-Builds ------------------------------------------------------
    // Riser 16 Wide — long 16-bar build, wide bloom, bright top, softer log curve.
    r#"{ "name": "Riser 16 Wide", "category": "Long-Builds", "key": 7, "octave": 2, "sync": 1, "bars": 16, "curve": 0.65,
         "balance": 0.4, "color": 0.7, "wave": 0.5, "fstart": 200.0, "fend": 11000.0, "rise": 10.0,
         "width": 0.85, "impact": 1, "implevel": 0.7, "autocut": 1, "downlifter": 0, "freelen": 6.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // 16-Bar Cathedral — soaring log 16-bar build, full width bloom, airy top, soft impact.
    r#"{ "name": "16-Bar Cathedral", "category": "Long-Builds", "key": 0, "octave": 2, "sync": 1, "bars": 16, "curve": 0.7,
         "balance": 0.45, "color": 0.75, "wave": 0.45, "fstart": 220.0, "fend": 12000.0, "rise": 8.0,
         "width": 0.9, "impact": 1, "implevel": 0.65, "autocut": 1, "downlifter": 0, "freelen": 6.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // 32-Bar Ascension — epic 32-bar main-stage build, wide, bright, patient log climb.
    r#"{ "name": "32-Bar Ascension", "category": "Long-Builds", "key": 7, "octave": 2, "sync": 2, "bars": 32, "curve": 0.6,
         "balance": 0.5, "color": 0.8, "wave": 0.4, "fstart": 200.0, "fend": 11000.0, "rise": 10.0,
         "width": 0.85, "impact": 1, "implevel": 0.6, "autocut": 1, "downlifter": 0, "freelen": 8.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Slow Log Build — 16-bar noise-forward slow bloom, very bright ceiling, gentle rise.
    r#"{ "name": "Slow Log Build", "category": "Long-Builds", "key": 0, "octave": 2, "sync": 1, "bars": 16, "curve": 0.85,
         "balance": 0.35, "color": 0.9, "wave": 0.5, "fstart": 260.0, "fend": 13000.0, "rise": 6.0,
         "width": 0.8, "impact": 1, "implevel": 0.7, "autocut": 1, "downlifter": 0, "freelen": 6.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // ---- Noise-Sweeps -----------------------------------------------------
    // Noise Swell Short — all-noise atmospheric 8-bar swell, no impact, wide.
    r#"{ "name": "Noise Swell Short", "category": "Noise-Sweeps", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.5,
         "balance": 0.15, "color": 0.9, "wave": 0.5, "fstart": 300.0, "fend": 12000.0, "rise": 4.0,
         "width": 0.9, "impact": 0, "implevel": 0.5, "autocut": 1, "downlifter": 0, "freelen": 2.5,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Noise Sweep To Impact — near all-noise 8-bar sweep culminating in a loud impact drop.
    r#"{ "name": "Noise Sweep To Impact", "category": "Noise-Sweeps", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.4,
         "balance": 0.1, "color": 0.85, "wave": 0.5, "fstart": 200.0, "fend": 12000.0, "rise": 2.0,
         "width": 0.85, "impact": 1, "implevel": 0.9, "autocut": 1, "downlifter": 0, "freelen": 3.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // White Wind Riser — bright white-noise wind sweep, no impact, very wide, high ceiling.
    r#"{ "name": "White Wind Riser", "category": "Noise-Sweeps", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.55,
         "balance": 0.2, "color": 0.05, "wave": 0.5, "fstart": 400.0, "fend": 14000.0, "rise": 3.0,
         "width": 0.9, "impact": 0, "implevel": 0.5, "autocut": 1, "downlifter": 0, "freelen": 3.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Pink Storm Swell — dense pink-noise storm build, maximal width, impact drop.
    r#"{ "name": "Pink Storm Swell", "category": "Noise-Sweeps", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.5,
         "balance": 0.15, "color": 1.0, "wave": 0.5, "fstart": 150.0, "fend": 10000.0, "rise": 4.0,
         "width": 0.95, "impact": 1, "implevel": 0.7, "autocut": 1, "downlifter": 0, "freelen": 3.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Ghost Rise — atmospheric dnb ghost swell, noise-led, log curve, no impact, spacious.
    r#"{ "name": "Ghost Rise", "category": "Noise-Sweeps", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.65,
         "balance": 0.25, "color": 0.9, "wave": 0.6, "fstart": 300.0, "fend": 11000.0, "rise": 5.0,
         "width": 0.85, "impact": 0, "implevel": 0.5, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // ---- Tonal-Rises ------------------------------------------------------
    // Melodic Fifth Rise — tonal root+fifth foregrounded, sine-ish, exp curve, moderate rise.
    r#"{ "name": "Melodic Fifth Rise", "category": "Tonal-Rises", "key": 9, "octave": 2, "sync": 0, "bars": 8, "curve": 0.3,
         "balance": 0.8, "color": 0.5, "wave": 0.7, "fstart": 250.0, "fend": 9000.0, "rise": 12.0,
         "width": 0.55, "impact": 1, "implevel": 0.65, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // Tonal Fifth Climb — bright sine-led fifth climb in G, exp attack, tight width.
    r#"{ "name": "Tonal Fifth Climb", "category": "Tonal-Rises", "key": 7, "octave": 2, "sync": 0, "bars": 8, "curve": 0.35,
         "balance": 0.75, "color": 0.5, "wave": 0.6, "fstart": 250.0, "fend": 9500.0, "rise": 12.0,
         "width": 0.55, "impact": 1, "implevel": 0.65, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // Sub Drop Build — deep sub-octave tonal build, huge pitch rise, loud low impact.
    r#"{ "name": "Sub Drop Build", "category": "Tonal-Rises", "key": 0, "octave": 1, "sync": 0, "bars": 8, "curve": 0.28,
         "balance": 0.7, "color": 0.6, "wave": 0.25, "fstart": 110.0, "fend": 7000.0, "rise": 20.0,
         "width": 0.35, "impact": 1, "implevel": 1.0, "autocut": 1, "downlifter": 0, "freelen": 3.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Minor Third Riser — darker tonal riser in E-flat, saw/sine blend, moderate width.
    r#"{ "name": "Minor Third Riser", "category": "Tonal-Rises", "key": 3, "octave": 2, "sync": 0, "bars": 8, "curve": 0.4,
         "balance": 0.72, "color": 0.55, "wave": 0.55, "fstart": 230.0, "fend": 9800.0, "rise": 15.0,
         "width": 0.5, "impact": 1, "implevel": 0.7, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // Saw Lead Rise — aggressive saw-lead octave-up tonal build, big rise, bright.
    r#"{ "name": "Saw Lead Rise", "category": "Tonal-Rises", "key": 0, "octave": 3, "sync": 0, "bars": 8, "curve": 0.45,
         "balance": 0.8, "color": 0.5, "wave": 0.05, "fstart": 300.0, "fend": 11000.0, "rise": 18.0,
         "width": 0.6, "impact": 1, "implevel": 0.6, "autocut": 1, "downlifter": 0, "freelen": 4.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // ---- Impacts-Drops ----------------------------------------------------
    // Sub Boom Drop — tonal-leaning, big pitch rise, loud low impact, tight 8-bar.
    r#"{ "name": "Sub Boom Drop", "category": "Impacts-Drops", "key": 0, "octave": 1, "sync": 0, "bars": 8, "curve": 0.25,
         "balance": 0.65, "color": 0.6, "wave": 0.25, "fstart": 120.0, "fend": 6000.0, "rise": 24.0,
         "width": 0.3, "impact": 1, "implevel": 1.0, "autocut": 1, "downlifter": 0, "freelen": 3.0,
         "level": -4.0, "keytrack": 0, "trigger": 0 }"#,
    // Downlifter 8 — reversed: full at the boundary, falls away over 8 bars, impact on the drop.
    r#"{ "name": "Downlifter 8", "category": "Impacts-Drops", "key": 0, "octave": 2, "sync": 0, "bars": 8, "curve": 0.6,
         "balance": 0.35, "color": 0.8, "wave": 0.45, "fstart": 300.0, "fend": 9000.0, "rise": 8.0,
         "width": 0.6, "impact": 1, "implevel": 0.9, "autocut": 0, "downlifter": 1, "freelen": 4.0,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
    // Downlifter 16 Fall — long 16-bar reversed fall, wide, log curve, impact on the drop.
    r#"{ "name": "Downlifter 16 Fall", "category": "Impacts-Drops", "key": 0, "octave": 2, "sync": 1, "bars": 16, "curve": 0.6,
         "balance": 0.4, "color": 0.8, "wave": 0.45, "fstart": 320.0, "fend": 10000.0, "rise": 8.0,
         "width": 0.65, "impact": 1, "implevel": 0.9, "autocut": 0, "downlifter": 1, "freelen": 6.0,
         "level": -3.5, "keytrack": 0, "trigger": 0 }"#,
    // Impact Only Slam — minimal 2-bar lead-in, all about the loud slam impact, narrow.
    r#"{ "name": "Impact Only Slam", "category": "Impacts-Drops", "key": 0, "octave": 2, "sync": 3, "bars": 2, "curve": 0.5,
         "balance": 0.5, "color": 0.7, "wave": 0.35, "fstart": 160.0, "fend": 8000.0, "rise": 8.0,
         "width": 0.4, "impact": 1, "implevel": 1.0, "autocut": 1, "downlifter": 0, "freelen": 1.2,
         "level": -3.0, "keytrack": 0, "trigger": 0 }"#,
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

    /// Count how many `Settings` fields differ between two presets (enums/ints/bools by equality,
    /// floats by a loose epsilon). Drives both the differ-from-default and pairwise-distinctness
    /// quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.key != b.key { n += 1; }
        if a.octave != b.octave { n += 1; }
        if a.sync != b.sync { n += 1; }
        if a.impact_on != b.impact_on { n += 1; }
        if a.auto_cut != b.auto_cut { n += 1; }
        if a.downlifter != b.downlifter { n += 1; }
        let fs = [
            (a.custom_bars, b.custom_bars), (a.curve, b.curve), (a.balance, b.balance),
            (a.color, b.color), (a.wave, b.wave), (a.filter_start_hz, b.filter_start_hz),
            (a.filter_end_hz, b.filter_end_hz), (a.rise_st, b.rise_st), (a.width, b.width),
            (a.impact_level, b.impact_level), (a.free_len_s, b.free_len_s), (a.level_db, b.level_db),
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
        // Deep bank: PRESET-EXPANSION target 15-24 for a complex tension FX.
        assert!(presets.len() >= 15, "ASCEND bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the default in
        // >= 4 params. Every preset is categorised.
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
        // `render_tests::every_preset_renders_and_passes_universal` test.
    }
}
