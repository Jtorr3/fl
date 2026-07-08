//! NERVE factory presets. Keys are nih-plug **param ids** with **plain** values, so the
//! editor applies them through the generic `suite_core::ui::apply_values` path (no
//! per-key mapping). Division indices: 0=4bar 1=2bar 2=1bar 3=½ 4=¼ 5=1/8 6=1/16.
//! Shape indices: 0 Sine 1 Tri 2 SawUp 3 SawDown 4 Square 5 S&H 6 SmoothRnd 7 ExpPulse.

/// Embedded factory bank (≥6). Only the keys a preset cares about are listed; the rest keep
/// their defaults.
pub const PRESET_JSON: &[&str] = &[
    // Slow, deep single-LFO swell on stream 1 — the classic "bus movement" source.
    r#"{ "name": "Slow Swell Bus",
         "lfo1_rate": 0.1, "lfo1_shape": 0, "lfo1_depth": 1.0, "lfo1_sync": 0.0,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0,
         "macro1": 0.0, "macro2": 0.0, "macro3": 0.0, "macro4": 0.0 }"#,
    // Synced 1/16 downward saw — a rhythmic "pump" on stream 1.
    r#"{ "name": "16th Pump",
         "lfo1_rate": 4.0, "lfo1_shape": 3, "lfo1_depth": 1.0, "lfo1_sync": 1.0, "lfo1_div": 6,
         "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Two fast random S&H streams — chaotic wobble on streams 7 & 8.
    r#"{ "name": "Chaos Pair",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "sh1_rate": 8.0, "sh1_slew": 0.12, "sh1_depth": 1.0,
         "sh2_rate": 5.0, "sh2_slew": 0.35, "sh2_depth": 1.0,
         "env1_depth": 0.0, "env2_depth": 0.0 }"#,
    // Four hand-ridden DC macros on streams 1..4 (LFOs off) — a manual modulation desk.
    r#"{ "name": "Macro Desk",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "macro1": 0.5, "macro2": -0.3, "macro3": 0.8, "macro4": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Two env followers of the plugin's own input — "breathing" streams 5 & 6.
    r#"{ "name": "Breathe",
         "lfo1_depth": 0.0, "lfo2_depth": 0.0, "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_atk": 50.0, "env1_rel": 400.0, "env1_depth": 1.0,
         "env2_atk": 5.0,  "env2_rel": 120.0, "env2_depth": 1.0,
         "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
    // Synced 1/8 saw pump + a synced 1/8 square gate — a techno two-source combo.
    r#"{ "name": "Techno Pump 1/8",
         "lfo1_rate": 8.0, "lfo1_shape": 3, "lfo1_depth": 1.0, "lfo1_sync": 1.0, "lfo1_div": 5,
         "lfo2_rate": 8.0, "lfo2_shape": 4, "lfo2_depth": 0.85, "lfo2_sync": 1.0, "lfo2_div": 5,
         "lfo3_depth": 0.0, "lfo4_depth": 0.0,
         "env1_depth": 0.0, "env2_depth": 0.0, "sh1_depth": 0.0, "sh2_depth": 0.0 }"#,
];
