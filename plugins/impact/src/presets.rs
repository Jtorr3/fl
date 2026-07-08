//! IMPACT factory presets — SOUND-PASS re-authoring (user directive 2026-07-08 PM:
//! "Some of the presets are completely useless... Especially for our kick and snare
//! generators"). The old bank was authored to a mechanical param-distance gate, which
//! guaranteed variety but not *value* — the audition (`tools/audition.py`) showed a whole
//! Distorted category whose energy sat at 250-500 Hz with no sub (MUD/BOXY/DULL — honky
//! mid-kicks, not kicks) plus off-genre psy/clicky/tom filler. This bank is RE-AUTHORED
//! around use archetypes and judged preset-by-preset on the rendered OUTPUT AUDIO for the
//! target artists (KAS:ST dark techno / Cynthoni atmospheric dnb / Akiaura·agonyOST wave).
//!
//! Each is an embedded flat-JSON blob parsed by `suite_core::presets`; the same list drives
//! the GUI selector (grouped by `"category"` into preset-bar sections) and the offline render
//! tests. Values are plain (un-normalized): Hz for freqs, ms for times, 0..1 for levels/curves,
//! enum indices for `shape`/`trans`, 0/1 for `clip`, dB for `outgain`.
//!
//! `shape` is a `DriveShape` index (0=Tube 1=Tape 2=Fold 3=Hard). `trans` selects the embedded
//! PCM transient (0=off, 1..3 variant). Every kick keeps `fstart > fend` — a downward pitch
//! sweep — so the render §4 STFT assertion stays physical.
//!
//! DESIGN RULE learned from the audition: keep the low fundamental DOMINANT. Heavy Fold/Hard
//! drive on a high `fend` pushes all energy into 250-500 Hz and kills the sub (the old
//! "Distorted" failure). So the aggressive presets keep `fend` low (50-58 Hz), use Tube/Tape
//! (warm, symmetric — far less mid-honk than Fold), moderate the drive, and carry a sub layer
//! as a guaranteed low-end floor.
//!
//! Categories (preset-bar sections): Warehouse Rumble / Wave 808 / DnB Punch / Deep Sub Roller
//! / Character Drive. Names are purpose-driven in the user's vocabulary.

use crate::dsp::{DriveShape, Settings};
use suite_core::presets::Preset;

/// Factory presets, in menu order, tagged by category.
pub const PRESET_JSON: &[&str] = &[
    // ==== Warehouse Rumble (KAS:ST dark techno) ============================
    // 50-55 Hz fundamentals, controlled saturated sub tail, muted knock. Sub layer at the
    // fundamental (ratio 1.0) thickens without going to 25 Hz mud. Tape drive = warm body.
    r#"{ "name": "Warehouse Thump", "category": "Warehouse Rumble", "fstart": 240.0, "fend": 52.0,
         "pdecay": 30.0, "pcurve": 0.42, "length": 1.4, "adecay": 620.0, "acurve": 0.58, "tone": 0.06,
         "drive": 0.34, "shape": 1, "clip": 1, "clicklvl": 0.16, "clickdecay": 13.0, "clickfreq": 3200.0,
         "trans": 3, "translvl": 0.28, "sublvl": 0.26, "subratio": 1.0, "outgain": -1.5 }"#,
    r#"{ "name": "Basement Rumble", "category": "Warehouse Rumble", "fstart": 210.0, "fend": 50.0,
         "pdecay": 34.0, "pcurve": 0.4, "length": 1.9, "adecay": 900.0, "acurve": 0.6, "tone": 0.03,
         "drive": 0.4, "shape": 1, "clip": 1, "clicklvl": 0.12, "clickdecay": 16.0, "clickfreq": 2800.0,
         "trans": 0, "translvl": 0.3, "sublvl": 0.3, "subratio": 1.0, "outgain": -1.5 }"#,
    r#"{ "name": "Last Train Home", "category": "Warehouse Rumble", "fstart": 260.0, "fend": 54.0,
         "pdecay": 28.0, "pcurve": 0.44, "length": 1.5, "adecay": 700.0, "acurve": 0.56, "tone": 0.08,
         "drive": 0.3, "shape": 0, "clip": 1, "clicklvl": 0.18, "clickdecay": 12.0, "clickfreq": 3400.0,
         "trans": 3, "translvl": 0.32, "sublvl": 0.22, "subratio": 1.0, "outgain": -1.0 }"#,
    r#"{ "name": "Concrete Floor", "category": "Warehouse Rumble", "fstart": 280.0, "fend": 55.0,
         "pdecay": 26.0, "pcurve": 0.46, "length": 1.2, "adecay": 540.0, "acurve": 0.54, "tone": 0.05,
         "drive": 0.32, "shape": 1, "clip": 1, "clicklvl": 0.2, "clickdecay": 11.0, "clickfreq": 3600.0,
         "trans": 2, "translvl": 0.34, "sublvl": 0.18, "subratio": 1.0, "outgain": -1.0 }"#,
    // ==== Wave 808 (Akiaura / agonyOST hard wave, tuned 808-kicks) =========
    // Tuned, long, warm-distorted, deep sub (ratio 0.5). Tube drive keeps it musical.
    r#"{ "name": "808 Cathedral", "category": "Wave 808", "fstart": 200.0, "fend": 58.0,
         "pdecay": 45.0, "pcurve": 0.55, "length": 2.1, "adecay": 1000.0, "acurve": 0.5, "tone": 0.08,
         "drive": 0.45, "shape": 0, "clip": 1, "clicklvl": 0.14, "clickdecay": 12.0, "clickfreq": 3000.0,
         "trans": 0, "translvl": 0.3, "sublvl": 0.34, "subratio": 0.5, "outgain": -1.5 }"#,
    r#"{ "name": "Memphis Sub 808", "category": "Wave 808", "fstart": 230.0, "fend": 55.0,
         "pdecay": 40.0, "pcurve": 0.54, "length": 2.0, "adecay": 900.0, "acurve": 0.5, "tone": 0.06,
         "drive": 0.38, "shape": 0, "clip": 1, "clicklvl": 0.16, "clickdecay": 11.0, "clickfreq": 2900.0,
         "trans": 2, "translvl": 0.32, "sublvl": 0.28, "subratio": 0.5, "outgain": -1.5 }"#,
    r#"{ "name": "Wave Tuned Kick", "category": "Wave 808", "fstart": 240.0, "fend": 62.0,
         "pdecay": 38.0, "pcurve": 0.5, "length": 1.7, "adecay": 820.0, "acurve": 0.52, "tone": 0.12,
         "drive": 0.42, "shape": 1, "clip": 1, "clicklvl": 0.2, "clickdecay": 12.0, "clickfreq": 3400.0,
         "trans": 3, "translvl": 0.34, "sublvl": 0.3, "subratio": 0.5, "outgain": -1.5 }"#,
    // ==== DnB Punch (Cynthoni atmospheric dnb) ============================
    // Short, tight, defined knock, tight controlled sub. Fast pitch drop = punch.
    r#"{ "name": "DnB Punch", "category": "DnB Punch", "fstart": 320.0, "fend": 60.0,
         "pdecay": 20.0, "pcurve": 0.42, "length": 0.85, "adecay": 320.0, "acurve": 0.5, "tone": 0.06,
         "drive": 0.26, "shape": 0, "clip": 1, "clicklvl": 0.3, "clickdecay": 9.0, "clickfreq": 3800.0,
         "trans": 2, "translvl": 0.42, "sublvl": 0.12, "subratio": 1.0, "outgain": -1.0 }"#,
    r#"{ "name": "Neurofunk Knock", "category": "DnB Punch", "fstart": 300.0, "fend": 58.0,
         "pdecay": 18.0, "pcurve": 0.4, "length": 0.75, "adecay": 280.0, "acurve": 0.48, "tone": 0.1,
         "drive": 0.3, "shape": 0, "clip": 1, "clicklvl": 0.34, "clickdecay": 8.0, "clickfreq": 4200.0,
         "trans": 2, "translvl": 0.46, "sublvl": 0.1, "subratio": 1.0, "outgain": -1.0 }"#,
    r#"{ "name": "Rolling Punch", "category": "DnB Punch", "fstart": 340.0, "fend": 56.0,
         "pdecay": 22.0, "pcurve": 0.44, "length": 0.95, "adecay": 380.0, "acurve": 0.52, "tone": 0.04,
         "drive": 0.24, "shape": 1, "clip": 1, "clicklvl": 0.26, "clickdecay": 10.0, "clickfreq": 3600.0,
         "trans": 1, "translvl": 0.4, "sublvl": 0.14, "subratio": 1.0, "outgain": -1.0 }"#,
    // ==== Deep Sub Roller (near-sine layer to sit under a click/top) ======
    // Clean, minimal click/drive, near-sine, long. Low crest by design — a layer, not a hit.
    r#"{ "name": "Sub Foundation", "category": "Deep Sub Roller", "fstart": 130.0, "fend": 50.0,
         "pdecay": 55.0, "pcurve": 0.55, "length": 2.3, "adecay": 1100.0, "acurve": 0.5, "tone": 0.0,
         "drive": 0.1, "shape": 0, "clip": 1, "clicklvl": 0.06, "clickdecay": 8.0, "clickfreq": 2400.0,
         "trans": 0, "translvl": 0.3, "sublvl": 0.5, "subratio": 1.0, "outgain": -2.0 }"#,
    r#"{ "name": "Sine Sub Layer", "category": "Deep Sub Roller", "fstart": 100.0, "fend": 47.0,
         "pdecay": 40.0, "pcurve": 0.5, "length": 2.6, "adecay": 1300.0, "acurve": 0.48, "tone": 0.0,
         "drive": 0.06, "shape": 0, "clip": 1, "clicklvl": 0.04, "clickdecay": 8.0, "clickfreq": 2200.0,
         "trans": 0, "translvl": 0.3, "sublvl": 0.55, "subratio": 1.0, "outgain": -2.0 }"#,
    r#"{ "name": "Deep Roller", "category": "Deep Sub Roller", "fstart": 150.0, "fend": 52.0,
         "pdecay": 60.0, "pcurve": 0.58, "length": 2.0, "adecay": 950.0, "acurve": 0.52, "tone": 0.02,
         "drive": 0.14, "shape": 1, "clip": 1, "clicklvl": 0.08, "clickdecay": 10.0, "clickfreq": 2600.0,
         "trans": 0, "translvl": 0.3, "sublvl": 0.44, "subratio": 1.0, "outgain": -2.0 }"#,
    // ==== Character Drive (hard-wave aggression WITH the sub kept) =========
    // Aggressive body but fend stays low + a sub floor guarantees the low end survives the
    // drive. Tube/Tape (not Fold) to avoid the honky 250-500 collapse the audition caught.
    r#"{ "name": "Distorted Warehouse", "category": "Character Drive", "fstart": 260.0, "fend": 54.0,
         "pdecay": 32.0, "pcurve": 0.5, "length": 1.3, "adecay": 640.0, "acurve": 0.58, "tone": 0.14,
         "drive": 0.62, "shape": 0, "clip": 1, "clicklvl": 0.2, "clickdecay": 14.0, "clickfreq": 3400.0,
         "trans": 3, "translvl": 0.34, "sublvl": 0.22, "subratio": 0.5, "outgain": -2.0 }"#,
    r#"{ "name": "Hardwave Slam", "category": "Character Drive", "fstart": 300.0, "fend": 56.0,
         "pdecay": 28.0, "pcurve": 0.52, "length": 1.1, "adecay": 520.0, "acurve": 0.56, "tone": 0.18,
         "drive": 0.7, "shape": 1, "clip": 1, "clicklvl": 0.24, "clickdecay": 12.0, "clickfreq": 3800.0,
         "trans": 3, "translvl": 0.4, "sublvl": 0.26, "subratio": 0.5, "outgain": -2.5 }"#,
    r#"{ "name": "Industrial Thud", "category": "Character Drive", "fstart": 220.0, "fend": 50.0,
         "pdecay": 36.0, "pcurve": 0.48, "length": 1.5, "adecay": 720.0, "acurve": 0.6, "tone": 0.1,
         "drive": 0.66, "shape": 0, "clip": 0, "clicklvl": 0.16, "clickdecay": 18.0, "clickfreq": 3000.0,
         "trans": 2, "translvl": 0.3, "sublvl": 0.24, "subratio": 0.5, "outgain": -2.5 }"#,
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

    /// Quality gate (mechanical duplicate-guard). The SOUND-PASS re-authoring judges value on
    /// the rendered audio (see `render_tests`); these mechanical rules stay as guards only.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Curated bank: SOUND-PASS shrank the filler-laden 25 to ~16 usable kicks.
        assert!(presets.len() >= 14, "IMPACT bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 2: every preset is categorised, is a real downward sweep, and differs from the
        // default in >= 4 params.
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
