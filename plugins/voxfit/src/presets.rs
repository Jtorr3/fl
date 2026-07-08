//! VOXFIT factory presets (PRESET-EXPANSION Batch A). Each is an embedded flat-JSON blob parsed
//! by `suite_core::presets`; the same list drives the GUI selector (grouped by the `"category"`
//! tag into preset-bar sections) and the offline render tests.
//!
//! Keys (all plain numbers): `formant` = st (−5..5); `deess_thresh` = dB (−60..0); `deess` = amount
//! 0..1; `listen` = 0/1; `harsh_thresh` = dB (−60..0); `harsh` = amount 0..1; `tilt` = dB (−6..6,
//! <0 dark); `prox` = dB (−6..6); `air` = dB (−6..6); `sit` = macro 0..1; `mix` = 0..1; `out` = dB
//! (−24..12). Omitted keys fall back to [`Controls::default`].
//!
//! Categories (preset-bar sections): Natural-Fit / Character / Backing-Beds / Upfront / Extreme.
//! Names are purpose-driven and genre-aware (atmospheric dnb / dark techno / the drowned-vocal
//! Cynthoni–Sewerslvt palette) — never settings descriptions.

use crate::dsp::{Controls, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category (≥15, PRESET-EXPANSION Batch A).
pub const PRESET_JSON: &[&str] = &[
    // ---- Natural-Fit ------------------------------------------------------
    // Sit In Dark Mix — the flagship SIT macro: drop a bright pop vocal into a dark production.
    r#"{ "name": "Sit In Dark Mix", "category": "Natural-Fit", "formant": 0.0, "deess_thresh": -24.0, "deess": 0.0,
         "listen": 0, "harsh_thresh": -24.0, "harsh": 0.0, "tilt": 0.0, "prox": 0.0, "air": 0.0,
         "sit": 0.8, "mix": 1.0, "out": 0.0 }"#,
    // Sit In The Mix — a lighter conform macro: tuck the vocal in without going fully dark.
    r#"{ "name": "Sit In The Mix", "category": "Natural-Fit", "formant": 0.0, "deess_thresh": -24.0, "deess": 0.1,
         "listen": 0, "harsh_thresh": -24.0, "harsh": 0.1, "tilt": 0.0, "prox": 0.0, "air": 0.0,
         "sit": 0.45, "mix": 1.0, "out": 0.0 }"#,
    // De-Harsh Rip — tame a harsh/sibilant ripped acapella: strong de-ess + presence dip.
    r#"{ "name": "De-Harsh Rip", "category": "Natural-Fit", "formant": 0.0, "deess_thresh": -32.0, "deess": 0.8,
         "listen": 0, "harsh_thresh": -30.0, "harsh": 0.7, "tilt": -1.0, "prox": 0.0, "air": 1.0,
         "sit": 0.0, "mix": 1.0, "out": 0.0 }"#,
    // Neutral Cleanup — transparent housekeeping: gentle de-ess + mild harsh + a touch of air.
    r#"{ "name": "Neutral Cleanup", "category": "Natural-Fit", "formant": 0.0, "deess_thresh": -26.0, "deess": 0.35,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.25, "tilt": 0.0, "prox": 0.0, "air": 1.0,
         "sit": 0.0, "mix": 1.0, "out": 0.0 }"#,
    // Tuck Behind The Beat — a gentle recessed fit: dark tilt, a hair of de-ess, sit down slightly.
    r#"{ "name": "Tuck Behind The Beat", "category": "Natural-Fit", "formant": -0.5, "deess_thresh": -28.0, "deess": 0.4,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.35, "tilt": -2.0, "prox": 1.0, "air": -1.0,
         "sit": 0.2, "mix": 0.95, "out": 0.0 }"#,
    // ---- Character --------------------------------------------------------
    // Radio Ghost — thin, mid-forward, bandlimited character (old-radio vocal bed).
    r#"{ "name": "Radio Ghost", "category": "Character", "formant": 1.0, "deess_thresh": -28.0, "deess": 0.5,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.3, "tilt": 2.0, "prox": -4.0, "air": -4.0,
         "sit": 0.0, "mix": 1.0, "out": 1.0 }"#,
    // Deeper Voice — drop the formants for a bigger, deeper head without touching pitch.
    r#"{ "name": "Deeper Voice", "category": "Character", "formant": -4.0, "deess_thresh": -26.0, "deess": 0.3,
         "listen": 0, "harsh_thresh": -24.0, "harsh": 0.2, "tilt": -1.0, "prox": 3.0, "air": 0.0,
         "sit": 0.0, "mix": 1.0, "out": -0.5 }"#,
    // Chest Voice Weight — a lower, chestier head: formant down, proximity + darker top.
    r#"{ "name": "Chest Voice Weight", "category": "Character", "formant": -3.0, "deess_thresh": -24.0, "deess": 0.25,
         "listen": 0, "harsh_thresh": -24.0, "harsh": 0.15, "tilt": -2.0, "prox": 4.0, "air": 1.0,
         "sit": 0.0, "mix": 1.0, "out": -1.0 }"#,
    // Helium Sprite — small, bright, airy head: formant up hard, air lift, thinned low-mids.
    r#"{ "name": "Helium Sprite", "category": "Character", "formant": 4.0, "deess_thresh": -30.0, "deess": 0.55,
         "listen": 0, "harsh_thresh": -28.0, "harsh": 0.4, "tilt": 2.0, "prox": -3.0, "air": 3.0,
         "sit": 0.0, "mix": 1.0, "out": -1.0 }"#,
    // Old Cassette Vox — worn, dull, mid-heavy tape character: dark tilt, air rolled off.
    r#"{ "name": "Old Cassette Vox", "category": "Character", "formant": -1.0, "deess_thresh": -26.0, "deess": 0.4,
         "listen": 0, "harsh_thresh": -25.0, "harsh": 0.45, "tilt": -2.5, "prox": 2.0, "air": -4.0,
         "sit": 0.0, "mix": 1.0, "out": 0.0 }"#,
    // ---- Backing-Beds -----------------------------------------------------
    // Ghost Backing Bed — a recessed, hazy vocal bed to sit under the lead.
    r#"{ "name": "Ghost Backing Bed", "category": "Backing-Beds", "formant": -1.0, "deess_thresh": -30.0, "deess": 0.45,
         "listen": 0, "harsh_thresh": -28.0, "harsh": 0.4, "tilt": -3.0, "prox": -2.0, "air": -3.0,
         "sit": 0.0, "mix": 0.85, "out": -1.5 }"#,
    // Drowned Choir Bed — a submerged, dark choir wash (Sewerslvt-style backing haze).
    r#"{ "name": "Drowned Choir Bed", "category": "Backing-Beds", "formant": -2.0, "deess_thresh": -34.0, "deess": 0.6,
         "listen": 0, "harsh_thresh": -30.0, "harsh": 0.55, "tilt": -4.0, "prox": 2.0, "air": -5.0,
         "sit": 0.0, "mix": 0.9, "out": -1.0 }"#,
    // Distant Vocal Fog — pushed far back and thinned: low body cut, dull top, heavy de-ess.
    r#"{ "name": "Distant Vocal Fog", "category": "Backing-Beds", "formant": 0.0, "deess_thresh": -32.0, "deess": 0.55,
         "listen": 0, "harsh_thresh": -28.0, "harsh": 0.35, "tilt": -2.0, "prox": -4.0, "air": -2.0,
         "sit": 0.15, "mix": 0.8, "out": -1.0 }"#,
    // Warped Sampled Choir — a pitched-up, wobbly sampled-choir pad: formant up, dark, tamed.
    r#"{ "name": "Warped Sampled Choir", "category": "Backing-Beds", "formant": 2.0, "deess_thresh": -30.0, "deess": 0.3,
         "listen": 0, "harsh_thresh": -27.0, "harsh": 0.5, "tilt": -2.0, "prox": 1.0, "air": -1.0,
         "sit": 0.0, "mix": 0.9, "out": -1.0 }"#,
    // ---- Upfront ----------------------------------------------------------
    // Airy Feature — open, bright feature-vocal: air boost + a lift of formant, de-ess in check.
    r#"{ "name": "Airy Feature", "category": "Upfront", "formant": 1.5, "deess_thresh": -30.0, "deess": 0.6,
         "listen": 0, "harsh_thresh": -26.0, "harsh": 0.3, "tilt": 1.0, "prox": 1.0, "air": 5.0,
         "sit": 0.0, "mix": 1.0, "out": -1.0 }"#,
    // Upfront Dark Pop — present but not bright: body + controlled top, dark tilt, firm de-ess.
    r#"{ "name": "Upfront Dark Pop", "category": "Upfront", "formant": -1.0, "deess_thresh": -32.0, "deess": 0.55,
         "listen": 0, "harsh_thresh": -28.0, "harsh": 0.45, "tilt": -2.0, "prox": 2.0, "air": 2.0,
         "sit": 0.0, "mix": 1.0, "out": -0.5 }"#,
    // Radio Vox — an up-front bandlimited lead: mid-forward, thinned, bright tilt but no air.
    r#"{ "name": "Radio Vox", "category": "Upfront", "formant": 1.0, "deess_thresh": -30.0, "deess": 0.4,
         "listen": 0, "harsh_thresh": -28.0, "harsh": 0.35, "tilt": 2.0, "prox": -3.0, "air": -3.0,
         "sit": 0.0, "mix": 1.0, "out": 1.0 }"#,
    // Bright Lead Conform — a shiny top-line lead: air + presence, tilt bright, de-ess held.
    r#"{ "name": "Bright Lead Conform", "category": "Upfront", "formant": 1.0, "deess_thresh": -34.0, "deess": 0.6,
         "listen": 0, "harsh_thresh": -27.0, "harsh": 0.4, "tilt": 2.0, "prox": 1.0, "air": 4.0,
         "sit": 0.0, "mix": 1.0, "out": -1.5 }"#,
    // ---- Extreme ----------------------------------------------------------
    // Sewer Choir — fully drowned, formant collapsed down, black tilt, air killed.
    r#"{ "name": "Sewer Choir", "category": "Extreme", "formant": -5.0, "deess_thresh": -36.0, "deess": 0.65,
         "listen": 0, "harsh_thresh": -32.0, "harsh": 0.8, "tilt": -6.0, "prox": 4.0, "air": -6.0,
         "sit": 0.0, "mix": 0.95, "out": -2.0 }"#,
    // Crushed Angel — tiny, glassy, over-bright and hyper de-essed (shattered-vocal texture).
    r#"{ "name": "Crushed Angel", "category": "Extreme", "formant": 5.0, "deess_thresh": -42.0, "deess": 0.9,
         "listen": 0, "harsh_thresh": -34.0, "harsh": 0.7, "tilt": 3.0, "prox": -4.0, "air": 5.0,
         "sit": 0.0, "mix": 1.0, "out": -2.0 }"#,
    // Total Formant Collapse — the deepest possible head, maximum darkness and body.
    r#"{ "name": "Total Formant Collapse", "category": "Extreme", "formant": -5.0, "deess_thresh": -30.0, "deess": 0.7,
         "listen": 0, "harsh_thresh": -34.0, "harsh": 0.9, "tilt": -5.0, "prox": 5.0, "air": -4.0,
         "sit": 0.0, "mix": 1.0, "out": -1.5 }"#,
    // Nyquist Sibilance Kill — a brutal de-ess / harsh clamp for the most savage ripped tops.
    r#"{ "name": "Nyquist Sibilance Kill", "category": "Extreme", "formant": 0.0, "deess_thresh": -45.0, "deess": 1.0,
         "listen": 0, "harsh_thresh": -40.0, "harsh": 0.9, "tilt": 1.0, "prox": 0.0, "air": 3.0,
         "sit": 0.0, "mix": 1.0, "out": -1.0 }"#,
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

    /// Count how many effective [`Settings`] params differ between two presets (bools by equality,
    /// floats by a loose epsilon). Drives both the differ-from-default and pairwise-distinctness
    /// quality gates. Mirrors grit's `count_diffs` over VOXFIT's resolved fields.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.deess_listen != b.deess_listen {
            n += 1;
        }
        let fa = feature_vec(a);
        let fb = feature_vec(b);
        for i in 0..fa.len() {
            if (fa[i] - fb[i]).abs() > 1e-3 {
                n += 1;
            }
        }
        n
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

    /// PRESET-EXPANSION Batch A quality gate (mechanical): all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank target for the expanded factory set.
        assert!(presets.len() >= 15, "VOXFIT bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset is categorised and differs
        // from the default in >= 4 params.
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

        // Names must be unique.
        for i in 0..presets.len() {
            for j in (i + 1)..presets.len() {
                assert_ne!(presets[i].name, presets[j].name, "duplicate preset name");
            }
        }
        // Rule 4 (render passes universal assertions) is enforced by
        // `tests::every_preset_renders_and_passes_universal`.
    }
}
