//! SNAP factory presets — SOUND-PASS re-authoring (user directive 2026-07-08 PM:
//! "Some of the presets are completely useless... Especially for our kick and snare
//! generators"). The old bank was authored to a mechanical param-distance gate; the audition
//! (`tools/audition.py`) showed the claps piling energy into 200-800 Hz (MUD/BOXY — boxy, not
//! crisp) and several snares dull (HF 30-40 dB down — no crack). This bank is RE-AUTHORED
//! around use archetypes and judged preset-by-preset on the rendered OUTPUT AUDIO for the
//! target artists (Cynthoni atmospheric dnb·breakcore / KAS:ST dark techno / Akiaura wave).
//!
//! Each is an embedded flat-JSON blob parsed by `suite_core::presets`; the same list drives the
//! GUI selector (grouped by `"category"` into preset-bar sections) and the offline render tests.
//! Values are plain (un-normalized): Hz for `tune`, ms for `decay`/`spread`, 0..1 for the macros
//! (`mode`/`balance`/`snap`/`humanize`/`tone`/`drive`/`width`), integer `taps`, dB for `level`.
//! `mode` is the engine crossfade (0 = Snare .. 0.5 = Hybrid .. 1 = Clap).
//!
//! DESIGN RULES learned from the audition:
//!  * CLAPS use a brighter `tone` (0.55-0.7, was 0.4-0.5) so the clap band centre sits ~2 kHz —
//!    crisp, not the boxy 300-800 Hz pile the old claps had.
//!  * SNARES keep `balance` >= 0.45 and `snap` >= 0.65 so the 3 kHz rattle formant + 4.2 kHz
//!    click give real crack HF (the old dull snares were too body-heavy).
//!  * Snappy archetypes stay SHORT (decay 90-190 ms) so the near-silent tail doesn't ring the
//!    fixed rattle formants (the METALLIC_RINGING the audition caught on the long old snares).
//!
//! Categories (preset-bar sections): DnB Snare / Breakcore Snap / Wave Clap / Techno Rim /
//! Texture Layer. Names are purpose-driven in the user's vocabulary.

use crate::dsp::Settings;
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ==== DnB Snare (Cynthoni atmospheric dnb — sharp 200 Hz body + bright noise, short) ==
    r#"{ "name": "DnB Crack", "category": "DnB Snare", "mode": 0.28, "tune": 210.0, "balance": 0.55, "snap": 0.8,
         "decay": 175.0, "taps": 4, "spread": 16.0, "humanize": 0.3, "tone": 0.78, "drive": 0.4,
         "width": 0.38, "level": -1.0, "keytrack": 0 }"#,
    r#"{ "name": "Amen Snare", "category": "DnB Snare", "mode": 0.3, "tune": 200.0, "balance": 0.58, "snap": 0.72,
         "decay": 200.0, "taps": 4, "spread": 18.0, "humanize": 0.4, "tone": 0.72, "drive": 0.32,
         "width": 0.42, "level": -1.0, "keytrack": 0 }"#,
    r#"{ "name": "Neuro Snare", "category": "DnB Snare", "mode": 0.25, "tune": 220.0, "balance": 0.52, "snap": 0.85,
         "decay": 150.0, "taps": 4, "spread": 15.0, "humanize": 0.28, "tone": 0.82, "drive": 0.45,
         "width": 0.35, "level": -1.0, "keytrack": 0 }"#,
    r#"{ "name": "Bright Crack", "category": "DnB Snare", "mode": 0.22, "tune": 205.0, "balance": 0.5, "snap": 0.9,
         "decay": 160.0, "taps": 3, "spread": 14.0, "humanize": 0.25, "tone": 0.86, "drive": 0.3,
         "width": 0.4, "level": -1.0, "keytrack": 0 }"#,
    // ==== Breakcore Snap (tight, aggressive, driven, short) ================
    r#"{ "name": "Breakcore Snap", "category": "Breakcore Snap", "mode": 0.32, "tune": 225.0, "balance": 0.5, "snap": 0.9,
         "decay": 120.0, "taps": 3, "spread": 12.0, "humanize": 0.2, "tone": 0.75, "drive": 0.6,
         "width": 0.3, "level": -1.5, "keytrack": 0 }"#,
    r#"{ "name": "Sewer Snare", "category": "Breakcore Snap", "mode": 0.28, "tune": 195.0, "balance": 0.55, "snap": 0.82,
         "decay": 140.0, "taps": 3, "spread": 13.0, "humanize": 0.35, "tone": 0.68, "drive": 0.68,
         "width": 0.28, "level": -2.0, "keytrack": 0 }"#,
    r#"{ "name": "Crushed Snap", "category": "Breakcore Snap", "mode": 0.35, "tune": 240.0, "balance": 0.48, "snap": 0.95,
         "decay": 100.0, "taps": 3, "spread": 11.0, "humanize": 0.18, "tone": 0.8, "drive": 0.75,
         "width": 0.25, "level": -2.5, "keytrack": 0 }"#,
    // ==== Wave Clap (Akiaura wave — wide, bright, reverb-ish tail; NOT boxy) ===============
    r#"{ "name": "Techno Clap", "category": "Wave Clap", "mode": 0.92, "tune": 190.0, "balance": 0.8, "snap": 0.55,
         "decay": 300.0, "taps": 5, "spread": 26.0, "humanize": 0.55, "tone": 0.68, "drive": 0.2,
         "width": 0.7, "level": -1.5, "keytrack": 0 }"#,
    r#"{ "name": "Wave Clap Wide", "category": "Wave Clap", "mode": 1.0, "tune": 180.0, "balance": 0.82, "snap": 0.5,
         "decay": 360.0, "taps": 5, "spread": 28.0, "humanize": 0.62, "tone": 0.68, "drive": 0.18,
         "width": 0.82, "level": -2.0, "keytrack": 0 }"#,
    r#"{ "name": "Warehouse Clap", "category": "Wave Clap", "mode": 0.9, "tune": 195.0, "balance": 0.78, "snap": 0.58,
         "decay": 260.0, "taps": 4, "spread": 22.0, "humanize": 0.45, "tone": 0.66, "drive": 0.24,
         "width": 0.6, "level": -1.5, "keytrack": 0 }"#,
    // ==== Techno Rim (KAS:ST — high-tuned body-forward rimshot/perc, tight) ===============
    r#"{ "name": "Rimshot Knock", "category": "Techno Rim", "mode": 0.15, "tune": 300.0, "balance": 0.42, "snap": 0.75,
         "decay": 120.0, "taps": 3, "spread": 12.0, "humanize": 0.15, "tone": 0.72, "drive": 0.32,
         "width": 0.25, "level": -1.0, "keytrack": 0 }"#,
    r#"{ "name": "Steel Rim", "category": "Techno Rim", "mode": 0.18, "tune": 330.0, "balance": 0.46, "snap": 0.7,
         "decay": 140.0, "taps": 3, "spread": 13.0, "humanize": 0.2, "tone": 0.78, "drive": 0.28,
         "width": 0.3, "level": -1.5, "keytrack": 0 }"#,
    r#"{ "name": "Concrete Rim", "category": "Techno Rim", "mode": 0.12, "tune": 270.0, "balance": 0.4, "snap": 0.68,
         "decay": 105.0, "taps": 3, "spread": 11.0, "humanize": 0.12, "tone": 0.66, "drive": 0.35,
         "width": 0.2, "level": -1.0, "keytrack": 0 }"#,
    // ==== Texture Layer (rattle/noise tops to weld under an acoustic snare) ================
    r#"{ "name": "Snare Wire Rattle", "category": "Texture Layer", "mode": 0.35, "tune": 185.0, "balance": 0.9, "snap": 0.6,
         "decay": 170.0, "taps": 4, "spread": 16.0, "humanize": 0.4, "tone": 0.66, "drive": 0.22,
         "width": 0.5, "level": -2.0, "keytrack": 0 }"#,
    r#"{ "name": "Buzz Top Layer", "category": "Texture Layer", "mode": 0.3, "tune": 215.0, "balance": 0.88, "snap": 0.72,
         "decay": 130.0, "taps": 3, "spread": 14.0, "humanize": 0.3, "tone": 0.8, "drive": 0.3,
         "width": 0.42, "level": -2.5, "keytrack": 0 }"#,
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

    /// Quality gate (mechanical duplicate-guard). The SOUND-PASS re-authoring judges value on the
    /// rendered audio (see `render_tests`); these mechanical rules stay as guards only.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Curated bank: SOUND-PASS shrank the filler-laden 24 to a focused set of usable drums.
        assert!(presets.len() >= 12, "SNAP bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 2: every preset is categorised and differs from the default in >= 4 params.
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
