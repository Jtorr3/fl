//! SNAP factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded flat-JSON
//! blob parsed by `suite_core::presets`; the same list drives the GUI selector (grouped by the
//! `"category"` tag into preset-bar sections) and the offline render tests.
//!
//! Values are plain (un-normalized): Hz for `tune`, ms for `decay`/`spread`, 0..1 for the
//! macros (`mode`/`balance`/`snap`/`humanize`/`tone`/`drive`/`width`), integer for `taps`, dB
//! for `level`. `mode` is the engine crossfade (0 = Snare .. 0.5 = Hybrid .. 1 = Clap).
//!
//! Categories (preset-bar sections): Snares / Claps / Rattle-Layers / Clicks / Extreme. Names are
//! purpose-driven and genre-aware (dark techno / atmospheric dnb) — never settings descriptions.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Snares -----------------------------------------------------------
    // Rimshot Knock — snare-leaning, tight body knock, minimal noise, short.
    r#"{ "name": "Rimshot Knock", "category": "Snares", "mode": 0.1, "tune": 260.0, "balance": 0.35, "snap": 0.6,
         "decay": 140.0, "taps": 3, "spread": 12.0, "humanize": 0.15, "tone": 0.6, "drive": 0.25,
         "width": 0.2, "level": -1.0, "keytrack": 0 }"#,
    // DnB Crack — hybrid, bright, fast, aggressive drive.
    r#"{ "name": "DnB Crack", "category": "Snares", "mode": 0.45, "tune": 220.0, "balance": 0.6, "snap": 0.8,
         "decay": 180.0, "taps": 4, "spread": 16.0, "humanize": 0.35, "tone": 0.75, "drive": 0.45,
         "width": 0.4, "level": -1.0, "keytrack": 0 }"#,
    // Airy Top Snare — snare, noisy/airy, high tone, moderate length, wide.
    r#"{ "name": "Airy Top Snare", "category": "Snares", "mode": 0.25, "tune": 240.0, "balance": 0.75, "snap": 0.65,
         "decay": 260.0, "taps": 3, "spread": 14.0, "humanize": 0.25, "tone": 0.85, "drive": 0.1,
         "width": 0.55, "level": -1.0, "keytrack": 0 }"#,
    // Concrete Snap — dark-techno snare, tight and punchy, low-ish shell, snappy.
    r#"{ "name": "Concrete Snap", "category": "Snares", "mode": 0.2, "tune": 200.0, "balance": 0.4, "snap": 0.7,
         "decay": 130.0, "taps": 3, "spread": 12.0, "humanize": 0.15, "tone": 0.55, "drive": 0.3,
         "width": 0.25, "level": -1.0, "keytrack": 0 }"#,
    // Rimshot Ghost — quiet ghost rimshot, very short, dry, for busy dnb rolls.
    r#"{ "name": "Rimshot Ghost", "category": "Snares", "mode": 0.15, "tune": 280.0, "balance": 0.3, "snap": 0.5,
         "decay": 90.0, "taps": 3, "spread": 10.0, "humanize": 0.2, "tone": 0.5, "drive": 0.1,
         "width": 0.15, "level": -4.0, "keytrack": 0 }"#,
    // Steel Shell Snare — bright metallic body, high tune, mid-length.
    r#"{ "name": "Steel Shell Snare", "category": "Snares", "mode": 0.3, "tune": 320.0, "balance": 0.5, "snap": 0.6,
         "decay": 170.0, "taps": 3, "spread": 14.0, "humanize": 0.25, "tone": 0.7, "drive": 0.2,
         "width": 0.4, "level": -1.0, "keytrack": 0 }"#,
    // Sub Snare Knock — low-tuned, body-heavy shell knock, warm tone.
    r#"{ "name": "Sub Snare Knock", "category": "Snares", "mode": 0.1, "tune": 130.0, "balance": 0.25, "snap": 0.4,
         "decay": 200.0, "taps": 3, "spread": 12.0, "humanize": 0.2, "tone": 0.35, "drive": 0.25,
         "width": 0.2, "level": -1.5, "keytrack": 0 }"#,
    // Break Rattle — atmospheric-dnb break snare, rattle-forward, wide.
    r#"{ "name": "Break Rattle", "category": "Snares", "mode": 0.4, "tune": 210.0, "balance": 0.7, "snap": 0.6,
         "decay": 230.0, "taps": 4, "spread": 18.0, "humanize": 0.45, "tone": 0.6, "drive": 0.3,
         "width": 0.5, "level": -1.0, "keytrack": 0 }"#,
    // ---- Claps ------------------------------------------------------------
    // Wet Techno Clap — clap-leaning, wide, longer tail, mid tone.
    r#"{ "name": "Wet Techno Clap", "category": "Claps", "mode": 0.9, "tune": 180.0, "balance": 0.8, "snap": 0.55,
         "decay": 320.0, "taps": 5, "spread": 26.0, "humanize": 0.5, "tone": 0.45, "drive": 0.2,
         "width": 0.75, "level": -1.5, "keytrack": 0 }"#,
    // 90s Machine Clap — clap, tight machine spread, low humanize, dry-ish.
    r#"{ "name": "90s Machine Clap", "category": "Claps", "mode": 0.85, "tune": 200.0, "balance": 0.85, "snap": 0.45,
         "decay": 240.0, "taps": 4, "spread": 18.0, "humanize": 0.1, "tone": 0.55, "drive": 0.15,
         "width": 0.35, "level": -1.5, "keytrack": 0 }"#,
    // Clap Layer Dark — dark-techno clap layer, wide, low tone, long-ish tail.
    r#"{ "name": "Clap Layer Dark", "category": "Claps", "mode": 0.9, "tune": 170.0, "balance": 0.8, "snap": 0.5,
         "decay": 300.0, "taps": 5, "spread": 24.0, "humanize": 0.5, "tone": 0.4, "drive": 0.2,
         "width": 0.7, "level": -1.5, "keytrack": 0 }"#,
    // Cavern Clap — big reverberant clap, very wide, long tail, humanized.
    r#"{ "name": "Cavern Clap", "category": "Claps", "mode": 1.0, "tune": 160.0, "balance": 0.85, "snap": 0.45,
         "decay": 600.0, "taps": 5, "spread": 28.0, "humanize": 0.65, "tone": 0.45, "drive": 0.15,
         "width": 0.85, "level": -2.0, "keytrack": 0 }"#,
    // Analog Clap Stack — drum-machine clap stack, even spread, low humanize.
    r#"{ "name": "Analog Clap Stack", "category": "Claps", "mode": 0.85, "tune": 210.0, "balance": 0.8, "snap": 0.4,
         "decay": 220.0, "taps": 4, "spread": 20.0, "humanize": 0.15, "tone": 0.6, "drive": 0.25,
         "width": 0.45, "level": -1.5, "keytrack": 0 }"#,
    // Warehouse Clap — huge techno clap, max spread, heavy width, driven.
    r#"{ "name": "Warehouse Clap", "category": "Claps", "mode": 0.9, "tune": 150.0, "balance": 0.9, "snap": 0.55,
         "decay": 420.0, "taps": 5, "spread": 30.0, "humanize": 0.6, "tone": 0.5, "drive": 0.35,
         "width": 0.75, "level": -2.0, "keytrack": 0 }"#,
    // ---- Rattle-Layers ----------------------------------------------------
    // Snare Wire Rattle — noise-forward wire layer to stack under an acoustic snare.
    r#"{ "name": "Snare Wire Rattle", "category": "Rattle-Layers", "mode": 0.35, "tune": 180.0, "balance": 0.95, "snap": 0.6,
         "decay": 190.0, "taps": 4, "spread": 16.0, "humanize": 0.35, "tone": 0.6, "drive": 0.2,
         "width": 0.5, "level": -1.5, "keytrack": 0 }"#,
    // Buzz Layer — buzzy mid-high rattle top, snappy, tighter.
    r#"{ "name": "Buzz Layer", "category": "Rattle-Layers", "mode": 0.3, "tune": 220.0, "balance": 0.9, "snap": 0.7,
         "decay": 150.0, "taps": 3, "spread": 14.0, "humanize": 0.3, "tone": 0.75, "drive": 0.3,
         "width": 0.4, "level": -2.0, "keytrack": 0 }"#,
    // Tin Rattle Top — thin high-tone tin rattle, short, wide air layer.
    r#"{ "name": "Tin Rattle Top", "category": "Rattle-Layers", "mode": 0.25, "tune": 260.0, "balance": 0.85, "snap": 0.65,
         "decay": 130.0, "taps": 3, "spread": 12.0, "humanize": 0.25, "tone": 0.9, "drive": 0.15,
         "width": 0.55, "level": -2.0, "keytrack": 0 }"#,
    // ---- Clicks -----------------------------------------------------------
    // Transient Tick — clicky top-layer transient, very short, snap-forward.
    r#"{ "name": "Transient Tick", "category": "Clicks", "mode": 0.4, "tune": 200.0, "balance": 0.5, "snap": 0.95,
         "decay": 80.0, "taps": 3, "spread": 10.0, "humanize": 0.1, "tone": 0.7, "drive": 0.2,
         "width": 0.2, "level": -2.0, "keytrack": 0 }"#,
    // Snap Click Layer — max-snap click to weld under any drum, dry and mono-ish.
    r#"{ "name": "Snap Click Layer", "category": "Clicks", "mode": 0.35, "tune": 190.0, "balance": 0.45, "snap": 1.0,
         "decay": 100.0, "taps": 3, "spread": 10.0, "humanize": 0.15, "tone": 0.65, "drive": 0.25,
         "width": 0.25, "level": -3.0, "keytrack": 0 }"#,
    // Needle Click — needle-thin bright click, ultra-short, tightest spread.
    r#"{ "name": "Needle Click", "category": "Clicks", "mode": 0.2, "tune": 240.0, "balance": 0.4, "snap": 0.9,
         "decay": 60.0, "taps": 3, "spread": 8.0, "humanize": 0.1, "tone": 0.85, "drive": 0.15,
         "width": 0.15, "level": -3.0, "keytrack": 0 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Gunshot Layer — big hybrid slap, heavy drive, long tail, wide.
    r#"{ "name": "Gunshot Layer", "category": "Extreme", "mode": 0.6, "tune": 150.0, "balance": 0.7, "snap": 0.5,
         "decay": 500.0, "taps": 5, "spread": 30.0, "humanize": 0.7, "tone": 0.4, "drive": 0.7,
         "width": 0.6, "level": -2.0, "keytrack": 0 }"#,
    // Detroit Detonator — heavy driven hybrid slam, long tail, wide.
    r#"{ "name": "Detroit Detonator", "category": "Extreme", "mode": 0.55, "tune": 140.0, "balance": 0.7, "snap": 0.6,
         "decay": 480.0, "taps": 5, "spread": 28.0, "humanize": 0.6, "tone": 0.45, "drive": 0.85,
         "width": 0.6, "level": -3.0, "keytrack": 0 }"#,
    // Distorted Crack Wide — near-max drive, bright crack, extreme width.
    r#"{ "name": "Distorted Crack Wide", "category": "Extreme", "mode": 0.6, "tune": 200.0, "balance": 0.65, "snap": 0.85,
         "decay": 260.0, "taps": 4, "spread": 20.0, "humanize": 0.4, "tone": 0.7, "drive": 0.9,
         "width": 0.9, "level": -3.0, "keytrack": 0 }"#,
    // Blown Rimshot — fully overdriven rimshot, high tune, short and mean.
    r#"{ "name": "Blown Rimshot", "category": "Extreme", "mode": 0.3, "tune": 250.0, "balance": 0.5, "snap": 0.8,
         "decay": 140.0, "taps": 3, "spread": 12.0, "humanize": 0.2, "tone": 0.8, "drive": 1.0,
         "width": 0.35, "level": -4.0, "keytrack": 0 }"#,
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

    /// Count how many `Settings` fields differ between two presets (`taps` by equality, floats by
    /// a loose epsilon). Drives both the differ-from-default and pairwise-distinctness gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.taps != b.taps {
            n += 1;
        }
        let fs = [
            (a.mode, b.mode),
            (a.tune, b.tune),
            (a.balance, b.balance),
            (a.snap, b.snap),
            (a.decay_ms, b.decay_ms),
            (a.spread_ms, b.spread_ms),
            (a.humanize, b.humanize),
            (a.tone, b.tone),
            (a.drive, b.drive),
            (a.width, b.width),
            (a.level_db, b.level_db),
        ];
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 {
                n += 1;
            }
        }
        n
    }

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

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank: SPECS target 15-24 for the expanded SNAP factory bank.
        assert!(presets.len() >= 15, "SNAP bank too small: {}", presets.len());

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
        // `render_tests::every_preset_renders_and_passes_universal` test.
    }
}
