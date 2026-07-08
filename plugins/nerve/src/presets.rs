//! NERVE factory presets (SPECS "PRESET-EXPANSION" deep bank). Keys are nih-plug **param
//! ids** with **plain** values, so the editor applies them through the generic
//! `suite_core::ui::apply_values` path (no per-key mapping). The same list drives the GUI
//! preset bar (grouped by the `"category"` tag into sections) and the offline harness.
//!
//! Encodings (see `dsp.rs`):
//!   * Division index (`lfoN_div`): 0=4bar 1=2bar 2=1bar 3=½ 4=¼ 5=1/8 6=1/16.
//!   * Shape index (`lfoN_shape`): 0 Sine 1 Tri 2 SawUp 3 SawDown 4 Square 5 S&H
//!     6 SmoothRnd 7 ExpPulse.
//!   * `lfoN_sync` is 0/1; depths/slew are 0..1; macros are bipolar -1..1; env atk/rel are
//!     ms; LFO/S&H rates are Hz.
//!
//! Only the keys a preset cares about are listed; every omitted key keeps its param default.
//! NERVE's audio is a bit-exact passthrough, so presets never touch the signal — they only
//! shape the 8 published modulation streams.
//!
//! Categories (preset-bar sections, in first-appearance order): Slow-Swells / Synced-Pumps /
//! Chaos-Random / Env-Follow / Macro-Desk. Names are purpose-driven and genre-aware (dark
//! techno / atmospheric dnb / Cynthoni-Sewerslvt texture) — never settings descriptions.

/// Embedded factory bank (menu order, tagged by category). `>= 13` presets.
pub const PRESET_JSON: &[&str] = &[
    // ---- Slow-Swells ------------------------------------------------------
    // Deep single-LFO drift on stream 1 — the classic slow "bus movement" source.
    r#"{ "name": "Tectonic Drift", "category": "Slow-Swells",
         "lfo1_rate": 0.05, "lfo1_shape": 0, "lfo1_depth": 1.0, "lfo1_sync": 0.0,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Two barely-detuned slow LFOs (sine + triangle) — a wide, breathing pad wash.
    r#"{ "name": "Glacier Breath", "category": "Slow-Swells",
         "lfo1_rate": 0.08, "lfo1_shape": 0, "lfo1_depth": 1.0, "lfo1_sync": 0.0,
         "lfo2_rate": 0.13, "lfo2_shape": 1, "lfo2_depth": 0.7, "lfo2_sync": 0.0,
         "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Slow smooth-random drift + a slow sine underneath — non-repeating atmospheric-dnb motion.
    r#"{ "name": "Underwater Cathedral", "category": "Slow-Swells",
         "lfo1_rate": 0.04, "lfo1_shape": 6, "lfo1_depth": 1.0, "lfo1_sync": 0.0,
         "lfo2_depth": 0.0,
         "lfo3_rate": 0.10, "lfo3_shape": 0, "lfo3_depth": 0.5, "lfo3_sync": 0.0,
         "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // ---- Synced-Pumps -----------------------------------------------------
    // Tempo-locked 1/4 downward saw — the dark-techno sidechain "pump" on stream 1.
    r#"{ "name": "Warehouse Sidechain", "category": "Synced-Pumps",
         "lfo1_rate": 2.0, "lfo1_shape": 3, "lfo1_depth": 1.0, "lfo1_sync": 1.0, "lfo1_div": 4,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Hard 1/16 square gate + a 1/8 saw pump underneath — a rhythmic two-source stab.
    r#"{ "name": "16th Gate Stab", "category": "Synced-Pumps",
         "lfo1_rate": 8.0, "lfo1_shape": 4, "lfo1_depth": 1.0, "lfo1_sync": 1.0, "lfo1_div": 6,
         "lfo2_rate": 4.0, "lfo2_shape": 3, "lfo2_depth": 0.6, "lfo2_sync": 1.0, "lfo2_div": 5,
         "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Slow half-bar synced sine — a wide, tempo-anchored swell that never drifts out of phase.
    r#"{ "name": "Half-Bar Heave", "category": "Synced-Pumps",
         "lfo1_rate": 0.5, "lfo1_shape": 0, "lfo1_depth": 1.0, "lfo1_sync": 1.0, "lfo1_div": 3,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // 1/8 exp-pulse ghost + a fluttering 1/16 saw — haunted, plucky synced motion.
    r#"{ "name": "Triplet Ghost Pulse", "category": "Synced-Pumps",
         "lfo1_rate": 4.0, "lfo1_shape": 7, "lfo1_depth": 0.9, "lfo1_sync": 1.0, "lfo1_div": 5,
         "lfo2_rate": 8.0, "lfo2_shape": 3, "lfo2_depth": 0.5, "lfo2_sync": 1.0, "lfo2_div": 6,
         "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // ---- Chaos-Random -----------------------------------------------------
    // Two fast, lightly-slewed S&H streams — jittery stepped wobble on streams 7 & 8.
    r#"{ "name": "Static Nerve", "category": "Chaos-Random",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "sh1_rate": 12.0, "sh1_slew": 0.05, "sh1_depth": 1.0,
         "sh2_rate": 7.0,  "sh2_slew": 0.20, "sh2_depth": 0.8,
         "env1_depth": 0.0, "env2_depth": 0.0 }"#,
    // Hard-stepped LFO (S&H shape) + a very fast raw S&H — Sewerslvt-style glitch modulation.
    r#"{ "name": "Datamosh", "category": "Chaos-Random",
         "lfo1_rate": 2.0, "lfo1_shape": 5, "lfo1_depth": 1.0, "lfo1_sync": 0.0,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "sh1_rate": 16.0, "sh1_slew": 0.0, "sh1_depth": 1.0,
         "sh2_depth": 0.0, "env1_depth": 0.0, "env2_depth": 0.0 }"#,
    // Smooth-random LFO + a heavily-slewed S&H glide — melting, decaying random contours.
    r#"{ "name": "Decaying Signal", "category": "Chaos-Random",
         "lfo1_rate": 0.6, "lfo1_shape": 6, "lfo1_depth": 1.0, "lfo1_sync": 0.0,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "sh1_rate": 3.0, "sh1_slew": 0.6, "sh1_depth": 0.7,
         "sh2_depth": 0.0, "env1_depth": 0.0, "env2_depth": 0.0 }"#,
    // ---- Env-Follow -------------------------------------------------------
    // A slow and a snappy follower of NERVE's own input — dual "breathing" streams 5 & 6.
    r#"{ "name": "Breathing Bus", "category": "Env-Follow",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_atk": 40.0, "env1_rel": 500.0, "env1_depth": 1.0,
         "env2_atk": 3.0,  "env2_rel": 90.0,  "env2_depth": 1.0,
         "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // A single razor-fast transient follower — a tight, ducking "ghost in the signal".
    r#"{ "name": "Ghost In The Signal", "category": "Env-Follow",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_atk": 1.0, "env1_rel": 60.0, "env1_depth": 1.0,
         "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Very slow swelling followers — long-arc dynamic movement for atmospheric-dnb pads.
    r#"{ "name": "Tidal Follow", "category": "Env-Follow",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_atk": 80.0, "env1_rel": 900.0, "env1_depth": 1.0,
         "env2_atk": 20.0, "env2_rel": 300.0, "env2_depth": 0.6,
         "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // ---- Macro-Desk -------------------------------------------------------
    // Four hand-ridden DC macros on streams 1..4 (LFOs off) — a manual modulation desk.
    r#"{ "name": "Hand On The Void", "category": "Macro-Desk",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "macro1": 0.6, "macro2": -0.4, "macro3": 0.9, "macro4": -0.2,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Hand macros with a slow sine bleeding into stream 1 — manual desk with a touch of drift.
    r#"{ "name": "Manual Override", "category": "Macro-Desk",
         "lfo1_rate": 0.2, "lfo1_shape": 0, "lfo1_depth": 0.4, "lfo1_sync": 0.0,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "macro1": 0.3, "macro2": 0.7, "macro3": -0.5, "macro4": 0.5,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
];

#[cfg(test)]
mod tests {
    use crate::dsp::{Division, EnvSet, LfoSet, Settings, ShSet, Shape, NUM_ENV, NUM_LFO, NUM_MACRO, NUM_SH};
    use suite_core::presets::{load_all, Preset};

    /// Build a DSP [`Settings`] from a parsed preset, exactly as the live editor would (same
    /// param-id keys, plain values), falling back to the param defaults for any omitted key.
    /// Test-only: the plugin itself applies presets through `suite_core::ui::apply_values`.
    fn settings_from_preset(p: &Preset) -> Settings {
        let d = Settings::default();
        let g = |k: &str, fb: f32| p.get(k).unwrap_or(fb);
        let lfo = |i: usize, dl: LfoSet| LfoSet {
            rate_hz: g(&format!("lfo{i}_rate"), dl.rate_hz),
            synced: g(&format!("lfo{i}_sync"), if dl.synced { 1.0 } else { 0.0 }) >= 0.5,
            div: Division::from_index(g(&format!("lfo{i}_div"), 2.0) as usize),
            shape: Shape::from_index(g(&format!("lfo{i}_shape"), 0.0) as usize),
            depth: g(&format!("lfo{i}_depth"), dl.depth),
        };
        let env = |i: usize, de: EnvSet| EnvSet {
            attack_ms: g(&format!("env{i}_atk"), de.attack_ms),
            release_ms: g(&format!("env{i}_rel"), de.release_ms),
            depth: g(&format!("env{i}_depth"), de.depth),
        };
        let sh = |i: usize, ds: ShSet| ShSet {
            rate_hz: g(&format!("sh{i}_rate"), ds.rate_hz),
            slew: g(&format!("sh{i}_slew"), ds.slew),
            depth: g(&format!("sh{i}_depth"), ds.depth),
        };
        Settings {
            lfo: [lfo(1, d.lfo[0]), lfo(2, d.lfo[1]), lfo(3, d.lfo[2]), lfo(4, d.lfo[3])],
            macros: [
                g("macro1", d.macros[0]),
                g("macro2", d.macros[1]),
                g("macro3", d.macros[2]),
                g("macro4", d.macros[3]),
            ],
            env: [env(1, d.env[0]), env(2, d.env[1])],
            sh: [sh(1, d.sh[0]), sh(2, d.sh[1])],
            tempo: d.tempo,
            beats: d.beats,
            playing: d.playing,
        }
    }

    /// Count how many modulation-shaping `Settings` fields differ between two presets
    /// (enums/bools by equality, floats by a loose epsilon). Fixed transport constants
    /// (tempo/beats/playing) are not preset-controlled and are skipped. Drives both the
    /// differ-from-default and pairwise-distinctness quality gates.
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        for k in 0..NUM_LFO {
            if a.lfo[k].synced != b.lfo[k].synced { n += 1; }
            if a.lfo[k].div != b.lfo[k].div { n += 1; }
            if a.lfo[k].shape != b.lfo[k].shape { n += 1; }
            if (a.lfo[k].rate_hz - b.lfo[k].rate_hz).abs() > 1e-3 { n += 1; }
            if (a.lfo[k].depth - b.lfo[k].depth).abs() > 1e-3 { n += 1; }
        }
        for k in 0..NUM_MACRO {
            if (a.macros[k] - b.macros[k]).abs() > 1e-3 { n += 1; }
        }
        for k in 0..NUM_ENV {
            if (a.env[k].attack_ms - b.env[k].attack_ms).abs() > 1e-3 { n += 1; }
            if (a.env[k].release_ms - b.env[k].release_ms).abs() > 1e-3 { n += 1; }
            if (a.env[k].depth - b.env[k].depth).abs() > 1e-3 { n += 1; }
        }
        for k in 0..NUM_SH {
            if (a.sh[k].rate_hz - b.sh[k].rate_hz).abs() > 1e-3 { n += 1; }
            if (a.sh[k].slew - b.sh[k].slew).abs() > 1e-3 { n += 1; }
            if (a.sh[k].depth - b.sh[k].depth).abs() > 1e-3 { n += 1; }
        }
        n
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(super::PRESET_JSON);
        // Expanded bank: SPECS minimum for NERVE is 12.
        assert!(presets.len() >= 12, "NERVE bank too small: {}", presets.len());

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
    }
}
