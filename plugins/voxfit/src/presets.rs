//! VOXFIT factory presets. Each is an embedded flat-JSON blob parsed by
//! `suite_core::presets`; the same list drives the GUI selector and the offline render tests.
//!
//! Keys (all plain numbers): `formant` = st (−5..5); `deess_thresh` = dB; `deess` = amount 0..1;
//! `listen` = 0/1; `harsh_thresh` = dB; `harsh` = amount 0..1; `tilt` = dB (−6..6, <0 dark);
//! `prox` = dB (−6..6); `air` = dB (−6..6); `sit` = macro 0..1; `mix` = 0..1; `out` = dB.

use crate::dsp::{Controls, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥6, PRD §1.4 + build brief).
pub const PRESET_JSON: &[&str] = &[
    // Sit In Dark Mix — the flagship SIT macro: drop a bright pop vocal into a dark production.
    r#"{ "name": "Sit In Dark Mix", "formant": 0.0, "deess_thresh": -24.0, "deess": 0.0,
         "listen": 0, "harsh_thresh": -24.0, "harsh": 0.0, "tilt": 0.0, "prox": 0.0, "air": 0.0,
         "sit": 0.8, "mix": 1.0, "out": 0.0 }"#,
    // De-Harsh Rip — tame a harsh/sibilant ripped acapella: strong de-ess + presence dip.
    r#"{ "name": "De-Harsh Rip", "formant": 0.0, "deess_thresh": -32.0, "deess": 0.8,
         "listen": 0, "harsh_thresh": -30.0, "harsh": 0.7, "tilt": -1.0, "prox": 0.0, "air": 1.0,
         "sit": 0.0, "mix": 1.0, "out": 0.0 }"#,
    // Radio Ghost — thin, mid-forward, bandlimited character (old-radio vocal bed).
    r#"{ "name": "Radio Ghost", "formant": 1.0, "deess_thresh": -28.0, "deess": 0.5,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.3, "tilt": 2.0, "prox": -4.0, "air": -4.0,
         "sit": 0.0, "mix": 1.0, "out": 1.0 }"#,
    // Deeper Voice — drop the formants for a bigger, deeper head without touching pitch.
    r#"{ "name": "Deeper Voice", "formant": -4.0, "deess_thresh": -26.0, "deess": 0.3,
         "listen": 0, "harsh_thresh": -24.0, "harsh": 0.2, "tilt": -1.0, "prox": 3.0, "air": 0.0,
         "sit": 0.0, "mix": 1.0, "out": -0.5 }"#,
    // Airy Feature — open, bright feature-vocal: air boost + a lift of formant, de-ess in check.
    r#"{ "name": "Airy Feature", "formant": 1.5, "deess_thresh": -30.0, "deess": 0.6,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.3, "tilt": 1.0, "prox": 1.0, "air": 5.0,
         "sit": 0.0, "mix": 1.0, "out": -1.0 }"#,
    // Neutral Cleanup — transparent housekeeping: gentle de-ess + mild harsh + a touch of air.
    r#"{ "name": "Neutral Cleanup", "formant": 0.0, "deess_thresh": -26.0, "deess": 0.35,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.25, "tilt": 0.0, "prox": 0.0, "air": 1.0,
         "sit": 0.0, "mix": 1.0, "out": 0.0 }"#,
];

/// Build [`Controls`] from a parsed preset, falling back to defaults for omitted keys.
pub fn controls_from_preset(p: &Preset) -> Controls {
    let d = Controls::default();
    let g = |k: &str, fb: f32| p.get(k).unwrap_or(fb);
    Controls {
        formant_st: g("formant", d.formant_st),
        deess_thresh_db: g("deess_thresh", d.deess_thresh_db),
        deess_amount: g("deess", d.deess_amount),
        deess_listen: g("listen", 0.0) >= 0.5,
        harsh_thresh_db: g("harsh_thresh", d.harsh_thresh_db),
        harsh_amount: g("harsh", d.harsh_amount),
        tilt_db: g("tilt", d.tilt_db),
        prox_db: g("prox", d.prox_db),
        air_db: g("air", d.air_db),
        sit: g("sit", d.sit),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
    }
}

/// Resolve a preset directly to effective [`Settings`].
pub fn settings_from_preset(p: &Preset) -> Settings {
    controls_from_preset(p).resolve()
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    /// The numeric fields we compare for the "differs meaningfully" quality gate.
    fn feature_vec(s: &Settings) -> [f32; 10] {
        [
            s.formant_ratio,
            s.deess_thresh,
            s.deess_amount,
            s.harsh_thresh,
            s.harsh_amount,
            s.tilt_db,
            s.prox_db,
            s.air_db,
            s.mix,
            s.out_gain,
        ]
    }

    #[test]
    fn all_presets_parse_and_differ() {
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        let d = feature_vec(&Settings::default());
        let feats: Vec<[f32; 10]> = presets.iter().map(|p| feature_vec(&settings_from_preset(p))).collect();

        // ≥4 params differ from default.
        for (p, f) in presets.iter().zip(&feats) {
            let diffs = (0..10).filter(|&i| (f[i] - d[i]).abs() > 1e-3).count();
            assert!(diffs >= 4, "preset '{}' differs from default in only {diffs} params", p.name);
        }
        // ≥2 params differ between every pair of presets.
        for i in 0..feats.len() {
            for j in (i + 1)..feats.len() {
                let diffs = (0..10).filter(|&k| (feats[i][k] - feats[j][k]).abs() > 1e-3).count();
                assert!(
                    diffs >= 2,
                    "presets '{}' and '{}' differ in only {diffs} params",
                    presets[i].name,
                    presets[j].name
                );
            }
        }
    }
}
