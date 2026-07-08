//! IMPACT factory presets (SPECS "PRESET-EXPANSION" deep bank for the kick-synth
//! instrument). Each is an embedded flat-JSON blob parsed by `suite_core::presets`; the
//! same list drives the GUI selector (grouped by the `"category"` tag into preset-bar
//! sections) and the offline render tests. Values are plain (un-normalized): Hz for freqs,
//! ms for times, 0..1 for levels/curves, enum indices for `shape`/`trans`, 0/1 for `clip`,
//! dB for `outgain`.
//!
//! `shape` is a `DriveShape` index (0=Tube 1=Tape 2=Fold 3=Hard). `trans` selects the
//! embedded PCM transient (0=off, 1..3 variant). Every kick keeps `fstart > fend` — a
//! downward pitch sweep — so the render harness §4 STFT assertion (f0 starts within 10% of
//! f_start, ends within 5% of f_end) stays physical.
//!
//! Categories (preset-bar sections): Techno Kicks / DnB & Sub / Punchy / Clicky-Forward /
//! Distorted / Tonal-Toms. Names are purpose-driven and genre-aware (dark techno /
//! atmospheric dnb) — never settings descriptions.

use crate::dsp::{DriveShape, Settings};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ---- Techno Kicks -----------------------------------------------------
    // Techno rumble kick — tape drive, woody knock transient, rumbling tail.
    r#"{ "name": "Techno Rumble Kick", "category": "Techno Kicks", "fstart": 300.0, "fend": 50.0,
         "pdecay": 30.0, "pcurve": 0.4, "length": 1.2, "adecay": 500.0, "acurve": 0.6, "tone": 0.1,
         "drive": 0.35, "shape": 1, "clip": 1, "clicklvl": 0.25, "clickdecay": 12.0, "clickfreq": 3500.0,
         "trans": 3, "translvl": 0.4, "sublvl": 0.2, "subratio": 0.5, "outgain": 0.0 }"#,
    r#"{ "name": "Warehouse Thump", "category": "Techno Kicks", "fstart": 260.0, "fend": 48.0,
         "pdecay": 28.0, "pcurve": 0.45, "length": 1.5, "adecay": 650.0, "acurve": 0.55, "tone": 0.05,
         "drive": 0.3, "shape": 1, "clip": 1, "clicklvl": 0.15, "clickdecay": 14.0, "clickfreq": 3000.0,
         "trans": 3, "translvl": 0.35, "sublvl": 0.3, "subratio": 0.5, "outgain": -0.5 }"#,
    r#"{ "name": "Rumble Bed Glue", "category": "Techno Kicks", "fstart": 220.0, "fend": 44.0,
         "pdecay": 22.0, "pcurve": 0.4, "length": 2.2, "adecay": 1100.0, "acurve": 0.6, "tone": 0.0,
         "drive": 0.45, "shape": 1, "clip": 1, "clicklvl": 0.1, "clickdecay": 18.0, "clickfreq": 2500.0,
         "trans": 0, "translvl": 0.4, "sublvl": 0.35, "subratio": 0.5, "outgain": -1.0 }"#,
    r#"{ "name": "Berghain Floor", "category": "Techno Kicks", "fstart": 340.0, "fend": 52.0,
         "pdecay": 35.0, "pcurve": 0.35, "length": 1.3, "adecay": 560.0, "acurve": 0.65, "tone": 0.15,
         "drive": 0.4, "shape": 3, "clip": 0, "clicklvl": 0.3, "clickdecay": 10.0, "clickfreq": 3800.0,
         "trans": 3, "translvl": 0.5, "sublvl": 0.15, "subratio": 0.5, "outgain": -1.0 }"#,
    r#"{ "name": "Detroit Stomp", "category": "Techno Kicks", "fstart": 280.0, "fend": 58.0,
         "pdecay": 26.0, "pcurve": 0.5, "length": 1.1, "adecay": 460.0, "acurve": 0.5, "tone": 0.1,
         "drive": 0.28, "shape": 0, "clip": 1, "clicklvl": 0.2, "clickdecay": 12.0, "clickfreq": 3200.0,
         "trans": 2, "translvl": 0.45, "sublvl": 0.1, "subratio": 0.5, "outgain": 0.0 }"#,
    // ---- DnB & Sub --------------------------------------------------------
    // Deep, long, sub-heavy 808 — soft clip, minimal click.
    r#"{ "name": "808 Long", "category": "DnB & Sub", "fstart": 160.0, "fend": 45.0, "pdecay": 60.0,
         "pcurve": 0.5, "length": 2.0, "adecay": 900.0, "acurve": 0.5, "tone": 0.0, "drive": 0.1,
         "shape": 0, "clip": 1, "clicklvl": 0.1, "clickdecay": 8.0, "clickfreq": 3000.0, "trans": 0,
         "translvl": 0.4, "sublvl": 0.4, "subratio": 0.5, "outgain": 0.0 }"#,
    r#"{ "name": "Sewer Sub", "category": "DnB & Sub", "fstart": 140.0, "fend": 40.0, "pdecay": 70.0,
         "pcurve": 0.55, "length": 2.5, "adecay": 1300.0, "acurve": 0.45, "tone": 0.0, "drive": 0.15,
         "shape": 1, "clip": 1, "clicklvl": 0.05, "clickdecay": 10.0, "clickfreq": 2200.0, "trans": 0,
         "translvl": 0.3, "sublvl": 0.55, "subratio": 0.5, "outgain": -1.0 }"#,
    r#"{ "name": "Sub Drop Foundation", "category": "DnB & Sub", "fstart": 180.0, "fend": 38.0,
         "pdecay": 80.0, "pcurve": 0.6, "length": 2.8, "adecay": 1500.0, "acurve": 0.5, "tone": 0.0,
         "drive": 0.2, "shape": 0, "clip": 1, "clicklvl": 0.08, "clickdecay": 9.0, "clickfreq": 2600.0,
         "trans": 0, "translvl": 0.3, "sublvl": 0.6, "subratio": 0.5, "outgain": -1.5 }"#,
    r#"{ "name": "Atmos DnB Boom", "category": "DnB & Sub", "fstart": 200.0, "fend": 46.0,
         "pdecay": 55.0, "pcurve": 0.45, "length": 2.2, "adecay": 1000.0, "acurve": 0.55, "tone": 0.05,
         "drive": 0.25, "shape": 1, "clip": 1, "clicklvl": 0.12, "clickdecay": 12.0, "clickfreq": 2800.0,
         "trans": 0, "translvl": 0.4, "sublvl": 0.45, "subratio": 1.0, "outgain": -0.5 }"#,
    // ---- Punchy -----------------------------------------------------------
    // House punch — tight, snap transient, moderate tube drive.
    r#"{ "name": "House Punch", "category": "Punchy", "fstart": 240.0, "fend": 55.0, "pdecay": 25.0,
         "pcurve": 0.45, "length": 0.9, "adecay": 320.0, "acurve": 0.5, "tone": 0.0, "drive": 0.2,
         "shape": 0, "clip": 1, "clicklvl": 0.3, "clickdecay": 10.0, "clickfreq": 4000.0, "trans": 2,
         "translvl": 0.5, "sublvl": 0.1, "subratio": 0.5, "outgain": 0.0 }"#,
    r#"{ "name": "Tight Live Punch", "category": "Punchy", "fstart": 260.0, "fend": 60.0, "pdecay": 20.0,
         "pcurve": 0.4, "length": 0.7, "adecay": 260.0, "acurve": 0.45, "tone": 0.1, "drive": 0.18,
         "shape": 0, "clip": 1, "clicklvl": 0.35, "clickdecay": 9.0, "clickfreq": 4200.0, "trans": 2,
         "translvl": 0.55, "sublvl": 0.05, "subratio": 0.5, "outgain": 0.0 }"#,
    r#"{ "name": "909 Ghost", "category": "Punchy", "fstart": 300.0, "fend": 62.0, "pdecay": 18.0,
         "pcurve": 0.4, "length": 0.8, "adecay": 300.0, "acurve": 0.5, "tone": 0.05, "drive": 0.22,
         "shape": 1, "clip": 1, "clicklvl": 0.28, "clickdecay": 11.0, "clickfreq": 4500.0, "trans": 1,
         "translvl": 0.5, "sublvl": 0.08, "subratio": 1.0, "outgain": -0.5 }"#,
    r#"{ "name": "Garage Knocker", "category": "Punchy", "fstart": 280.0, "fend": 58.0, "pdecay": 22.0,
         "pcurve": 0.45, "length": 0.85, "adecay": 340.0, "acurve": 0.55, "tone": 0.15, "drive": 0.25,
         "shape": 0, "clip": 1, "clicklvl": 0.32, "clickdecay": 8.0, "clickfreq": 5000.0, "trans": 2,
         "translvl": 0.6, "sublvl": 0.0, "subratio": 0.5, "outgain": 0.0 }"#,
    // ---- Clicky-Forward ---------------------------------------------------
    // Psy snap — very short, fast pitch drop, bright tick, hard clipped.
    r#"{ "name": "Psy Snap", "category": "Clicky-Forward", "fstart": 500.0, "fend": 60.0, "pdecay": 12.0,
         "pcurve": 0.3, "length": 0.5, "adecay": 180.0, "acurve": 0.4, "tone": 0.2, "drive": 0.5,
         "shape": 3, "clip": 0, "clicklvl": 0.4, "clickdecay": 6.0, "clickfreq": 5000.0, "trans": 1,
         "translvl": 0.7, "sublvl": 0.0, "subratio": 0.5, "outgain": -1.0 }"#,
    r#"{ "name": "Psy Click Forward", "category": "Clicky-Forward", "fstart": 600.0, "fend": 58.0,
         "pdecay": 10.0, "pcurve": 0.25, "length": 0.45, "adecay": 160.0, "acurve": 0.35, "tone": 0.25,
         "drive": 0.55, "shape": 3, "clip": 0, "clicklvl": 0.5, "clickdecay": 5.0, "clickfreq": 6000.0,
         "trans": 1, "translvl": 0.75, "sublvl": 0.0, "subratio": 0.5, "outgain": -1.5 }"#,
    r#"{ "name": "Tick Top Kick", "category": "Clicky-Forward", "fstart": 420.0, "fend": 64.0,
         "pdecay": 14.0, "pcurve": 0.35, "length": 0.6, "adecay": 220.0, "acurve": 0.45, "tone": 0.15,
         "drive": 0.35, "shape": 2, "clip": 0, "clicklvl": 0.55, "clickdecay": 7.0, "clickfreq": 6500.0,
         "trans": 1, "translvl": 0.6, "sublvl": 0.0, "subratio": 0.5, "outgain": -1.0 }"#,
    r#"{ "name": "Beater Attack", "category": "Clicky-Forward", "fstart": 360.0, "fend": 66.0,
         "pdecay": 16.0, "pcurve": 0.4, "length": 0.65, "adecay": 240.0, "acurve": 0.5, "tone": 0.1,
         "drive": 0.3, "shape": 0, "clip": 1, "clicklvl": 0.6, "clickdecay": 9.0, "clickfreq": 5500.0,
         "trans": 2, "translvl": 0.65, "sublvl": 0.0, "subratio": 0.5, "outgain": -0.5 }"#,
    // ---- Distorted --------------------------------------------------------
    // Hardstyle distorted — heavy wavefold drive, long body, knock transient.
    r#"{ "name": "Hardstyle Distorted", "category": "Distorted", "fstart": 380.0, "fend": 70.0,
         "pdecay": 40.0, "pcurve": 0.6, "length": 1.4, "adecay": 700.0, "acurve": 0.6, "tone": 0.15,
         "drive": 0.85, "shape": 2, "clip": 0, "clicklvl": 0.3, "clickdecay": 15.0, "clickfreq": 4500.0,
         "trans": 3, "translvl": 0.5, "sublvl": 0.15, "subratio": 0.5, "outgain": -2.0 }"#,
    r#"{ "name": "Gabber Crunch", "category": "Distorted", "fstart": 450.0, "fend": 75.0, "pdecay": 30.0,
         "pcurve": 0.55, "length": 1.0, "adecay": 500.0, "acurve": 0.6, "tone": 0.2, "drive": 0.95,
         "shape": 3, "clip": 0, "clicklvl": 0.35, "clickdecay": 12.0, "clickfreq": 5000.0, "trans": 3,
         "translvl": 0.55, "sublvl": 0.1, "subratio": 0.5, "outgain": -3.0 }"#,
    r#"{ "name": "Industrial Fold", "category": "Distorted", "fstart": 320.0, "fend": 60.0,
         "pdecay": 35.0, "pcurve": 0.5, "length": 1.1, "adecay": 520.0, "acurve": 0.55, "tone": 0.3,
         "drive": 0.75, "shape": 2, "clip": 0, "clicklvl": 0.3, "clickdecay": 20.0, "clickfreq": 3800.0,
         "trans": 3, "translvl": 0.45, "sublvl": 0.2, "subratio": 0.5, "outgain": -2.5 }"#,
    r#"{ "name": "Fold Abyss Kick", "category": "Distorted", "fstart": 300.0, "fend": 52.0,
         "pdecay": 45.0, "pcurve": 0.65, "length": 1.3, "adecay": 620.0, "acurve": 0.6, "tone": 0.25,
         "drive": 0.8, "shape": 2, "clip": 0, "clicklvl": 0.2, "clickdecay": 18.0, "clickfreq": 3200.0,
         "trans": 0, "translvl": 0.4, "sublvl": 0.25, "subratio": 0.5, "outgain": -2.5 }"#,
    // ---- Tonal-Toms -------------------------------------------------------
    r#"{ "name": "Deep Floor Tom", "category": "Tonal-Toms", "fstart": 180.0, "fend": 90.0, "pdecay": 90.0,
         "pcurve": 0.6, "length": 1.2, "adecay": 550.0, "acurve": 0.55, "tone": 0.6, "drive": 0.15,
         "shape": 0, "clip": 1, "clicklvl": 0.2, "clickdecay": 14.0, "clickfreq": 3000.0, "trans": 2,
         "translvl": 0.4, "sublvl": 0.0, "subratio": 0.5, "outgain": 0.0 }"#,
    r#"{ "name": "Melodic Sub Tom", "category": "Tonal-Toms", "fstart": 160.0, "fend": 82.0, "pdecay": 110.0,
         "pcurve": 0.65, "length": 1.5, "adecay": 700.0, "acurve": 0.6, "tone": 0.7, "drive": 0.1,
         "shape": 0, "clip": 1, "clicklvl": 0.12, "clickdecay": 12.0, "clickfreq": 2800.0, "trans": 0,
         "translvl": 0.35, "sublvl": 0.2, "subratio": 1.0, "outgain": 0.0 }"#,
    r#"{ "name": "Rototom Snap", "category": "Tonal-Toms", "fstart": 260.0, "fend": 130.0, "pdecay": 50.0,
         "pcurve": 0.5, "length": 0.8, "adecay": 380.0, "acurve": 0.5, "tone": 0.5, "drive": 0.2,
         "shape": 0, "clip": 1, "clicklvl": 0.3, "clickdecay": 10.0, "clickfreq": 4000.0, "trans": 2,
         "translvl": 0.5, "sublvl": 0.0, "subratio": 0.5, "outgain": -0.5 }"#,
    r#"{ "name": "Tuned Body Knock", "category": "Tonal-Toms", "fstart": 300.0, "fend": 110.0, "pdecay": 40.0,
         "pcurve": 0.45, "length": 0.9, "adecay": 420.0, "acurve": 0.55, "tone": 0.4, "drive": 0.25,
         "shape": 0, "clip": 1, "clicklvl": 0.35, "clickdecay": 11.0, "clickfreq": 4500.0, "trans": 2,
         "translvl": 0.55, "sublvl": 0.0, "subratio": 0.5, "outgain": -0.5 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for omitted keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        f_start: g("fstart", d.f_start),
        f_end: g("fend", d.f_end),
        pitch_decay_ms: g("pdecay", d.pitch_decay_ms),
        pitch_curve: g("pcurve", d.pitch_curve),
        length: g("length", d.length),
        amp_decay_ms: g("adecay", d.amp_decay_ms),
        amp_curve: g("acurve", d.amp_curve),
        tone: g("tone", d.tone),
        drive: g("drive", d.drive),
        shape: DriveShape::from_index(g("shape", 0.0) as usize),
        clip_soft: g("clip", 1.0) >= 0.5,
        click_level: g("clicklvl", d.click_level),
        click_decay_ms: g("clickdecay", d.click_decay_ms),
        click_freq: g("clickfreq", d.click_freq),
        transient: g("trans", 0.0) as usize,
        transient_level: g("translvl", d.transient_level),
        sub_level: g("sublvl", d.sub_level),
        sub_ratio: g("subratio", d.sub_ratio),
        out_gain_db: g("outgain", d.out_gain_db),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// Count how many `Settings` fields differ between two presets (enum/usize/bool by
    /// equality, floats by a loose epsilon). Drives both the differ-from-default and the
    /// pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.shape != b.shape { n += 1; }
        if a.clip_soft != b.clip_soft { n += 1; }
        if a.transient != b.transient { n += 1; }
        let fs = [
            (a.f_start, b.f_start), (a.f_end, b.f_end),
            (a.pitch_decay_ms, b.pitch_decay_ms), (a.pitch_curve, b.pitch_curve),
            (a.length, b.length), (a.amp_decay_ms, b.amp_decay_ms),
            (a.amp_curve, b.amp_curve), (a.tone, b.tone), (a.drive, b.drive),
            (a.click_level, b.click_level), (a.click_decay_ms, b.click_decay_ms),
            (a.click_freq, b.click_freq), (a.transient_level, b.transient_level),
            (a.sub_level, b.sub_level), (a.sub_ratio, b.sub_ratio),
            (a.out_gain_db, b.out_gain_db),
        ];
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 { n += 1; }
        }
        n
    }

    #[test]
    fn all_presets_parse_and_differ_from_default() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5, "need >= 5 presets, got {}", presets.len());
        let d = Settings::default();
        for p in &presets {
            let s = settings_from_preset(p);
            let mut diffs = 0;
            if (s.f_start - d.f_start).abs() > 1e-3 { diffs += 1; }
            if (s.f_end - d.f_end).abs() > 1e-3 { diffs += 1; }
            if (s.amp_decay_ms - d.amp_decay_ms).abs() > 1e-3 { diffs += 1; }
            if (s.length - d.length).abs() > 1e-3 { diffs += 1; }
            if (s.drive - d.drive).abs() > 1e-3 { diffs += 1; }
            if s.shape != d.shape { diffs += 1; }
            if (s.sub_level - d.sub_level).abs() > 1e-3 { diffs += 1; }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank for an instrument: SPECS target aims high (20-30).
        assert!(presets.len() >= 20, "IMPACT bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised and
        // differs from the default in >= 4 params. Every kick is a real downward sweep.
        for (p, s) in presets.iter().zip(&settings) {
            assert!(p.category.is_some(), "preset '{}' has no category", p.name);
            assert!(
                s.f_start > s.f_end,
                "preset '{}' is not a downward pitch sweep (fstart {} <= fend {})",
                p.name, s.f_start, s.f_end
            );
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
        // `render_tests::every_preset_renders_and_passes_universal` test in lib.rs.
    }
}
