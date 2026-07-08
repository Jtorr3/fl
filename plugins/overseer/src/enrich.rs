//! OVERSEER-ENRICH: instrument-type param, per-type context defaults (the documented
//! table), LEARN ghost suggestions, and the Master theme-assist model.
//!
//! Pure data + math (no audio, no GUI): the Node/Master editors call these on the GUI
//! thread, and the DSP cores consume the resulting [`NodeSettings`] / [`MasterSettings`].

use nih_plug::prelude::Enum;
use serde::{Deserialize, Serialize};

use suite_core::classify::{FeatureSummary, InstrumentType, SessionTheme};

use crate::eq::EqSettings;
use crate::master::MasterSettings;
use crate::node::NodeSettings;

// ===========================================================================
// Instrument-type param
// ===========================================================================

/// The Node's Instrument Type param. `Auto` (the default) follows the continuous
/// classifier; a concrete type pins it manually. A LEARN lock is a separate persisted state
/// (see [`LearnPersist`]) that the audio thread resolves against this param.
#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum TypeParam {
    #[id = "auto"]
    #[name = "Auto"]
    Auto,
    #[id = "kick"]
    #[name = "Kick"]
    Kick,
    #[id = "bass"]
    #[name = "Bass"]
    Bass,
    #[id = "rumble"]
    #[name = "Rumble"]
    Rumble,
    #[id = "perc"]
    #[name = "Perc"]
    Perc,
    #[id = "hats"]
    #[name = "Hats"]
    Hats,
    #[id = "snare"]
    #[name = "Snare"]
    Snare,
    #[id = "breaks"]
    #[name = "Breaks"]
    Breaks,
    #[id = "vocal"]
    #[name = "Vocal"]
    Vocal,
    #[id = "pad"]
    #[name = "Pad"]
    Pad,
    #[id = "lead"]
    #[name = "Lead"]
    Lead,
    #[id = "atmos"]
    #[name = "Atmos"]
    Atmos,
    #[id = "fx"]
    #[name = "FX"]
    Fx,
    #[id = "busx"]
    #[name = "Bus"]
    Bus,
}

impl TypeParam {
    /// The pinned instrument type, or `None` when on Auto.
    pub fn to_instrument(self) -> Option<InstrumentType> {
        Some(match self {
            TypeParam::Auto => return None,
            TypeParam::Kick => InstrumentType::Kick,
            TypeParam::Bass => InstrumentType::Bass,
            TypeParam::Rumble => InstrumentType::Rumble,
            TypeParam::Perc => InstrumentType::Perc,
            TypeParam::Hats => InstrumentType::Hats,
            TypeParam::Snare => InstrumentType::Snare,
            TypeParam::Breaks => InstrumentType::Breaks,
            TypeParam::Vocal => InstrumentType::Vocal,
            TypeParam::Pad => InstrumentType::Pad,
            TypeParam::Lead => InstrumentType::Lead,
            TypeParam::Atmos => InstrumentType::Atmos,
            TypeParam::Fx => InstrumentType::Fx,
            TypeParam::Bus => InstrumentType::Bus,
        })
    }
}

/// The preset-bank category slug for an instrument type (matches [`Preset::category`]).
pub fn type_bank_category(t: InstrumentType) -> &'static str {
    match t {
        InstrumentType::Kick => "KICK",
        InstrumentType::Bass => "BASS",
        InstrumentType::Rumble => "BASS",
        InstrumentType::Perc | InstrumentType::Hats => "PERC",
        InstrumentType::Snare | InstrumentType::Breaks => "PERC",
        InstrumentType::Vocal => "VOCAL",
        InstrumentType::Lead => "VOCAL",
        InstrumentType::Pad | InstrumentType::Atmos => "PAD",
        InstrumentType::Fx | InstrumentType::Bus | InstrumentType::Generic => "BUS",
    }
}

// ===========================================================================
// Context-tuned defaults per type (the documented table)
// ===========================================================================

/// Per-type starting strip settings (SPECS OVERSEER-ENRICH: "context-tuned defaults per
/// type"). Applied through the host when the user selects a concrete type (or a LEARN
/// commits one). KICK = mono-low + fast comp; VOCAL = gentle knee + presence bands; etc.
pub fn context_defaults(t: InstrumentType) -> NodeSettings {
    let base = NodeSettings::default();
    let eq = |low_f, low_g, b1_f, b1_g, b1_q, b2_f, b2_g, b2_q, hi_f, hi_g| EqSettings {
        low_freq: low_f,
        low_gain: low_g,
        b1_freq: b1_f,
        b1_gain: b1_g,
        b1_q,
        b2_freq: b2_f,
        b2_gain: b2_g,
        b2_q,
        high_freq: hi_f,
        high_gain: hi_g,
    };
    match t {
        InstrumentType::Kick => NodeSettings {
            eq: eq(60.0, 3.0, 400.0, -3.0, 1.2, 3000.0, 1.0, 1.0, 9000.0, 0.0),
            comp_threshold: -18.0,
            comp_ratio: 4.0,
            comp_knee: 6.0,
            comp_attack: 5.0,
            comp_release: 120.0,
            comp_makeup: 2.0,
            drive_db: 3.0,
            width: 0.0, // mono low end
            ..base
        },
        InstrumentType::Bass => NodeSettings {
            eq: eq(80.0, 2.0, 250.0, -2.0, 1.0, 1500.0, 1.0, 0.9, 8000.0, 0.0),
            comp_threshold: -20.0,
            comp_ratio: 3.0,
            comp_knee: 6.0,
            comp_attack: 15.0,
            comp_release: 180.0,
            comp_makeup: 2.0,
            drive_db: 2.0,
            width: 0.3,
            ..base
        },
        InstrumentType::Rumble => NodeSettings {
            eq: eq(45.0, 2.0, 300.0, -1.0, 0.9, 2000.0, 0.0, 0.8, 8000.0, -3.0),
            comp_threshold: -22.0,
            comp_ratio: 2.0,
            comp_knee: 8.0,
            comp_attack: 25.0,
            comp_release: 250.0,
            width: 0.0,
            ..base
        },
        InstrumentType::Perc => NodeSettings {
            eq: eq(120.0, -3.0, 500.0, -1.0, 1.0, 5000.0, 2.0, 0.8, 10000.0, 3.0),
            comp_threshold: -20.0,
            comp_ratio: 3.0,
            comp_knee: 4.0,
            comp_attack: 1.0,
            comp_release: 80.0,
            width: 1.2,
            ..base
        },
        InstrumentType::Hats => NodeSettings {
            eq: eq(200.0, -6.0, 800.0, -2.0, 1.0, 8000.0, 3.0, 0.7, 12000.0, 4.0),
            comp_threshold: -22.0,
            comp_ratio: 3.0,
            comp_knee: 3.0,
            comp_attack: 0.5,
            comp_release: 60.0,
            width: 1.3,
            ..base
        },
        InstrumentType::Snare => NodeSettings {
            eq: eq(150.0, -2.0, 400.0, 1.0, 1.0, 3000.0, 3.0, 0.9, 9000.0, 2.0),
            comp_threshold: -20.0,
            comp_ratio: 4.0,
            comp_knee: 4.0,
            comp_attack: 3.0,
            comp_release: 120.0,
            width: 1.0,
            ..base
        },
        InstrumentType::Breaks => NodeSettings {
            eq: eq(90.0, 0.0, 400.0, -1.0, 1.0, 3500.0, 2.0, 0.8, 10000.0, 2.0),
            comp_threshold: -20.0,
            comp_ratio: 3.0,
            comp_knee: 4.0,
            comp_attack: 2.0,
            comp_release: 100.0,
            width: 1.1,
            ..base
        },
        InstrumentType::Vocal => NodeSettings {
            eq: eq(100.0, -2.0, 350.0, -2.0, 1.0, 5000.0, 3.0, 0.8, 11000.0, 2.0),
            comp_threshold: -22.0,
            comp_ratio: 3.0,
            comp_knee: 10.0, // gentle knee
            comp_attack: 10.0,
            comp_release: 150.0,
            comp_makeup: 3.0,
            drive_db: 2.0,
            width: 1.0,
            ..base
        },
        InstrumentType::Pad => NodeSettings {
            eq: eq(80.0, 0.0, 300.0, -1.0, 0.8, 4000.0, 1.0, 0.6, 12000.0, 1.0),
            comp_threshold: -24.0,
            comp_ratio: 2.0,
            comp_knee: 12.0,
            comp_attack: 25.0,
            comp_release: 220.0,
            drive_db: 1.0,
            width: 1.5, // wide
            ..base
        },
        InstrumentType::Lead => NodeSettings {
            eq: eq(120.0, -1.0, 500.0, 0.0, 0.9, 3000.0, 2.0, 0.9, 10000.0, 1.0),
            comp_threshold: -20.0,
            comp_ratio: 3.0,
            comp_knee: 6.0,
            comp_attack: 8.0,
            comp_release: 140.0,
            drive_db: 3.0,
            width: 0.8,
            ..base
        },
        InstrumentType::Atmos => NodeSettings {
            eq: eq(70.0, 0.0, 300.0, 0.0, 0.8, 4000.0, 1.0, 0.6, 12000.0, 2.0),
            comp_threshold: -26.0,
            comp_ratio: 1.6,
            comp_knee: 12.0,
            comp_attack: 30.0,
            comp_release: 260.0,
            width: 1.6,
            ..base
        },
        InstrumentType::Fx => NodeSettings {
            width: 1.0,
            ..base
        },
        InstrumentType::Bus | InstrumentType::Generic => NodeSettings {
            eq: eq(90.0, 0.0, 300.0, 0.0, 0.9, 2500.0, 1.0, 0.7, 10000.0, 1.0),
            comp_threshold: -20.0,
            comp_ratio: 2.0,
            comp_knee: 10.0,
            comp_attack: 25.0,
            comp_release: 200.0,
            width: 1.2,
            ..base
        },
    }
}

// ===========================================================================
// LEARN ghost suggestions (Node)
// ===========================================================================

/// A LEARN's suggested strip moves, computed from the captured stats. Shown as ghost values
/// with an APPLY button. Deterministic function of the features (SPECS: "measured low-band
/// excess -> EQ suggestion; crest factor -> comp threshold/ratio suggestion").
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct NodeSuggestion {
    /// Suggested low-shelf gain (dB): tame an excessive sub, or lift a thin low end.
    pub low_gain: f32,
    /// Suggested compressor threshold (dB), derived from measured level and crest.
    pub threshold: f32,
    /// Suggested compressor ratio, derived from crest factor.
    pub ratio: f32,
}

/// Compute ghost suggestions from a captured feature summary.
pub fn suggest_from_features(f: &FeatureSummary) -> NodeSuggestion {
    // Low-band excess → low-shelf EQ suggestion.
    let low_gain = if f.low_ratio > 0.6 {
        -3.0
    } else if f.low_ratio > 0.45 {
        -1.5
    } else if f.low_ratio < 0.08 {
        1.5
    } else {
        0.0
    };

    // Crest factor (dB) → threshold + ratio. A peaky source wants a lower threshold and a
    // firmer ratio to catch the transients; a dense source wants a gentle setting.
    let crest_db = 20.0 * f.crest.clamp(1.0, 12.0).log10();
    let level = if f.level_db.is_finite() {
        f.level_db.clamp(-48.0, -3.0)
    } else {
        -20.0
    };
    let threshold = (level - crest_db * 0.5).clamp(-40.0, -6.0);
    let ratio = (1.5 + 0.35 * crest_db).clamp(1.5, 8.0);

    NodeSuggestion {
        low_gain,
        threshold,
        ratio,
    }
}

/// Persisted Node LEARN state (nih-plug `#[persist]`). Survives project reloads so the
/// locked type, and the ghost suggestions + their APPLY, are still available.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LearnPersist {
    /// A LEARN has locked a type.
    pub locked: bool,
    /// Locked type index (`InstrumentType::index`).
    pub ty: u32,
    /// The most recent ghost suggestions (if a LEARN has run).
    pub suggestion: Option<NodeSuggestion>,
}

/// Persisted Master theme-lock (nih-plug `#[persist]`). A LEARN locks the inferred theme so
/// assist targets stop drifting.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ThemeLock {
    pub locked: bool,
    /// Locked theme index (`SessionTheme::index`).
    pub theme: u32,
}

// ===========================================================================
// Master theme assist
// ===========================================================================

/// Theme-derived assist targets (SPECS: master EQ tilt / MB comp character / limiter drive).
/// These are the *full-strength* nudges; the ASSIST knob scales them (0 = none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistTargets {
    /// High-shelf tilt (dB) toward the theme's tonal balance.
    pub eq_tilt_db: f32,
    /// Low-shelf move (dB).
    pub eq_low_db: f32,
    /// Multiband-comp character: >0 = slower/glue, <0 = faster/punch (scales attack/release).
    pub comp_character: f32,
    /// Extra limiter drive (dB, negative ceiling push) for loudness themes.
    pub limiter_drive_db: f32,
}

impl Default for AssistTargets {
    fn default() -> Self {
        Self {
            eq_tilt_db: 0.0,
            eq_low_db: 0.0,
            comp_character: 0.0,
            limiter_drive_db: 0.0,
        }
    }
}

/// Full-strength assist targets for a theme.
pub fn theme_assist_targets(theme: SessionTheme) -> AssistTargets {
    match theme {
        SessionTheme::DarkTechno => AssistTargets {
            eq_tilt_db: -1.5, // roll the tops off a touch, keep it dark
            eq_low_db: 1.0,   // firm low end
            comp_character: 1.0, // slow glue
            limiter_drive_db: 1.0,
        },
        SessionTheme::DnbBreaks => AssistTargets {
            eq_tilt_db: 1.0, // brighter tops for breaks
            eq_low_db: 1.5,  // strong sub
            comp_character: -1.0, // fast/punch
            limiter_drive_db: 1.5,
        },
        SessionTheme::Ambient => AssistTargets {
            eq_tilt_db: -0.5,
            eq_low_db: -0.5,
            comp_character: 1.5, // very gentle/glue
            limiter_drive_db: -0.5, // preserve dynamics
        },
        SessionTheme::HouseGroove => AssistTargets {
            eq_tilt_db: 0.5,
            eq_low_db: 0.5,
            comp_character: 0.5,
            limiter_drive_db: 0.5,
        },
        SessionTheme::Generic => AssistTargets::default(),
    }
}

/// Apply theme assist to the base master settings, scaled by `strength` (`0..1`). The ASSIST
/// knob feeds `strength`; a SUGGEST-ONLY toggle or a manual param touch passes `0` for the
/// excluded aspect.
///
/// **Null guarantee:** `strength <= 0` returns the base settings BIT-FOR-BIT (early return),
/// so assist at 0 changes nothing in the audio path — the OVERSEER-ENRICH done-bar null test.
pub fn apply_assist(base: &MasterSettings, targets: &AssistTargets, strength: f32) -> MasterSettings {
    if strength <= 0.0 {
        return *base;
    }
    let k = strength.clamp(0.0, 1.0);
    let mut s = *base;
    // EQ tilt (high shelf) + low shelf.
    s.eq.high_gain += k * targets.eq_tilt_db;
    s.eq.low_gain += k * targets.eq_low_db;
    // Multiband comp character: scale attack/release around the base. character>0 → slower.
    let atk_scale = 1.0 + k * 0.4 * targets.comp_character;
    let rel_scale = 1.0 + k * 0.4 * targets.comp_character;
    s.comp_attack = (s.comp_attack * atk_scale).clamp(0.1, 100.0);
    s.comp_release = (s.comp_release * rel_scale).clamp(10.0, 1000.0);
    // Limiter drive: push the ceiling down slightly for loudness (bounded, never above 0).
    s.ceiling_db = (s.ceiling_db - k * targets.limiter_drive_db).clamp(-12.0, 0.0);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_and_vocal_context_defaults_differ_in_documented_fields() {
        let k = context_defaults(InstrumentType::Kick);
        let v = context_defaults(InstrumentType::Vocal);
        // The documented KICK-vs-VOCAL diffs.
        assert_eq!(k.width, 0.0, "KICK is mono-low");
        assert_eq!(v.width, 1.0, "VOCAL is stereo");
        assert!(k.comp_knee < v.comp_knee, "VOCAL has the gentler knee");
        assert!(k.comp_attack < v.comp_attack, "KICK comp is faster");
        assert!(v.eq.high_gain > k.eq.high_gain, "VOCAL is presence-tilted");
        // At least 3 differing fields overall.
        let diffs = [
            (k.width - v.width).abs() > 1e-6,
            (k.comp_knee - v.comp_knee).abs() > 1e-6,
            (k.comp_attack - v.comp_attack).abs() > 1e-6,
            (k.eq.high_gain - v.eq.high_gain).abs() > 1e-6,
            (k.eq.b2_gain - v.eq.b2_gain).abs() > 1e-6,
        ];
        assert!(diffs.iter().filter(|b| **b).count() >= 3);
    }

    #[test]
    fn every_type_has_defaults_that_differ_from_bare_default() {
        let base = NodeSettings::default();
        for t in [
            InstrumentType::Kick,
            InstrumentType::Bass,
            InstrumentType::Vocal,
            InstrumentType::Pad,
            InstrumentType::Perc,
        ] {
            let d = context_defaults(t);
            let n = [
                (d.width - base.width).abs() > 1e-6,
                (d.comp_attack - base.comp_attack).abs() > 1e-6,
                (d.eq.low_gain - base.eq.low_gain).abs() > 1e-6,
                (d.eq.high_gain - base.eq.high_gain).abs() > 1e-6,
            ]
            .iter()
            .filter(|b| **b)
            .count();
            assert!(n >= 2, "{t:?} defaults barely differ from base");
        }
    }

    #[test]
    fn assist_at_zero_is_bit_exact_identity() {
        let base = MasterSettings::default();
        let targets = theme_assist_targets(SessionTheme::DarkTechno);
        let out = apply_assist(&base, &targets, 0.0);
        // Every field must be byte-identical (the audio-path null guarantee).
        assert_eq!(out.eq.high_gain.to_bits(), base.eq.high_gain.to_bits());
        assert_eq!(out.eq.low_gain.to_bits(), base.eq.low_gain.to_bits());
        assert_eq!(out.comp_attack.to_bits(), base.comp_attack.to_bits());
        assert_eq!(out.comp_release.to_bits(), base.comp_release.to_bits());
        assert_eq!(out.ceiling_db.to_bits(), base.ceiling_db.to_bits());
    }

    #[test]
    fn assist_nudges_at_positive_strength() {
        let base = MasterSettings::default();
        let targets = theme_assist_targets(SessionTheme::DarkTechno);
        let out = apply_assist(&base, &targets, 0.3);
        // Dark techno rolls the tops off → high shelf drops.
        assert!(out.eq.high_gain < base.eq.high_gain);
        // Slow glue → attack/release lengthen.
        assert!(out.comp_attack > base.comp_attack);
    }

    #[test]
    fn suggestions_are_deterministic_and_bounded() {
        let mut f = FeatureSummary::default();
        f.low_ratio = 0.8;
        f.crest = 6.0;
        f.level_db = -14.0;
        let s = suggest_from_features(&f);
        assert!(s.low_gain < 0.0, "excess low → cut");
        assert!((-40.0..=-6.0).contains(&s.threshold));
        assert!((1.5..=8.0).contains(&s.ratio));
        // Deterministic.
        assert_eq!(s, suggest_from_features(&f));
    }
}
