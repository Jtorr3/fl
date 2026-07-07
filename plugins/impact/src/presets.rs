//! IMPACT factory presets (SPECS list: 808 long, techno rumble kick, psy snap, house punch,
//! hardstyle distorted). Each is an embedded flat-JSON blob parsed by `suite_core::presets`;
//! the same list drives the GUI selector and the offline render tests. Values are plain
//! (un-normalized): Hz for freqs, ms for times, 0..1 for levels/curves, enum indices for
//! `shape`/`trans`, 0/1 for `clip`/`keytrack`, dB for `outgain`.

use crate::dsp::{DriveShape, Settings};
use suite_core::presets::Preset;

/// The five factory presets, in menu order.
pub const PRESET_JSON: &[&str] = &[
    // Deep, long, sub-heavy 808 — soft clip, minimal click.
    r#"{ "name": "808 Long", "fstart": 160.0, "fend": 45.0, "pdecay": 60.0, "pcurve": 0.5,
         "length": 2.0, "adecay": 900.0, "acurve": 0.5, "tone": 0.0, "drive": 0.1, "shape": 0,
         "clip": 1, "clicklvl": 0.1, "clickdecay": 8.0, "clickfreq": 3000.0, "trans": 0,
         "translvl": 0.4, "sublvl": 0.4, "subratio": 0.5, "keytrack": 0, "outgain": 0.0 }"#,
    // Techno rumble kick — tape drive, woody knock transient, rumbling tail.
    r#"{ "name": "Techno Rumble Kick", "fstart": 300.0, "fend": 50.0, "pdecay": 30.0, "pcurve": 0.4,
         "length": 1.2, "adecay": 500.0, "acurve": 0.6, "tone": 0.1, "drive": 0.35, "shape": 1,
         "clip": 1, "clicklvl": 0.25, "clickdecay": 12.0, "clickfreq": 3500.0, "trans": 3,
         "translvl": 0.4, "sublvl": 0.2, "subratio": 0.5, "keytrack": 0, "outgain": 0.0 }"#,
    // Psy snap — very short, fast pitch drop, bright tick, hard clipped.
    r#"{ "name": "Psy Snap", "fstart": 500.0, "fend": 60.0, "pdecay": 12.0, "pcurve": 0.3,
         "length": 0.5, "adecay": 180.0, "acurve": 0.4, "tone": 0.2, "drive": 0.5, "shape": 3,
         "clip": 0, "clicklvl": 0.4, "clickdecay": 6.0, "clickfreq": 5000.0, "trans": 1,
         "translvl": 0.7, "sublvl": 0.0, "subratio": 0.5, "keytrack": 0, "outgain": -1.0 }"#,
    // House punch — tight, snap transient, moderate tube drive.
    r#"{ "name": "House Punch", "fstart": 240.0, "fend": 55.0, "pdecay": 25.0, "pcurve": 0.45,
         "length": 0.9, "adecay": 320.0, "acurve": 0.5, "tone": 0.0, "drive": 0.2, "shape": 0,
         "clip": 1, "clicklvl": 0.3, "clickdecay": 10.0, "clickfreq": 4000.0, "trans": 2,
         "translvl": 0.5, "sublvl": 0.1, "subratio": 0.5, "keytrack": 0, "outgain": 0.0 }"#,
    // Hardstyle distorted — heavy wavefold drive, long body, knock transient.
    r#"{ "name": "Hardstyle Distorted", "fstart": 380.0, "fend": 70.0, "pdecay": 40.0, "pcurve": 0.6,
         "length": 1.4, "adecay": 700.0, "acurve": 0.6, "tone": 0.15, "drive": 0.85, "shape": 2,
         "clip": 0, "clicklvl": 0.3, "clickdecay": 15.0, "clickfreq": 4500.0, "trans": 3,
         "translvl": 0.5, "sublvl": 0.15, "subratio": 0.5, "keytrack": 0, "outgain": -2.0 }"#,
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
}
