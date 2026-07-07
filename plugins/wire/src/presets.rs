//! WIRE factory presets. Each is an embedded flat-JSON blob parsed by `suite_core::presets`.
//! The same list drives the GUI selector and the offline render tests. Values are plain:
//! kbps for bitrate, ms for regen delay, dB for out, 0..1 for crunch/mix, 0..2 for width,
//! 0/1 for fec, and integer indices for the enums.
//!
//! Enum encodings: `mode` 0 = Voice, 1 = Music. `bandwidth` 0 = Narrow, 1 = Medium,
//! 2 = Wide, 3 = Superwide, 4 = Full.

use crate::dsp::{BandwidthSel, Mode, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order (≥5, PRD §1.4).
pub const PRESET_JSON: &[&str] = &[
    // Crunchy, glitchy low-bitrate voice with intermittent dropouts — a VoIP ghost.
    r#"{ "name": "Discord Ghost", "bitrate": 16.0, "mode": 0, "bandwidth": 2, "fec": 1,
         "loss": 12.0, "crunch": 0.15, "regen_delay": 60.0, "regen_amount": 0.0,
         "width": 1.0, "mix": 1.0, "out": 0.0 }"#,
    // A buffering, falling-apart stream: very low bitrate, heavy packet loss, FEC fighting it.
    r#"{ "name": "Dying Stream", "bitrate": 8.0, "mode": 0, "bandwidth": 1, "fec": 1,
         "loss": 35.0, "crunch": 0.25, "regen_delay": 100.0, "regen_amount": 0.0,
         "width": 0.8, "mix": 1.0, "out": 1.0 }"#,
    // Muffled telephone hold-music: narrowband, low bitrate, no loss, collapsed width.
    r#"{ "name": "Hold Music", "bitrate": 12.0, "mode": 0, "bandwidth": 0, "fec": 0,
         "loss": 0.0, "crunch": 0.1, "regen_delay": 40.0, "regen_amount": 0.0,
         "width": 0.4, "mix": 1.0, "out": 2.0 }"#,
    // Tape-style generation loss: re-encoding feedback compounds the artifacts each pass.
    r#"{ "name": "Generation Loss", "bitrate": 24.0, "mode": 1, "bandwidth": 3, "fec": 0,
         "loss": 3.0, "crunch": 0.2, "regen_delay": 180.0, "regen_amount": 0.7,
         "width": 1.1, "mix": 1.0, "out": -1.0 }"#,
    // A barely-there digital sheen for glue on a full mix — parallel, high bitrate.
    r#"{ "name": "Subtle Digital", "bitrate": 96.0, "mode": 1, "bandwidth": 4, "fec": 0,
         "loss": 0.0, "crunch": 0.05, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 1.0, "mix": 0.4, "out": 0.0 }"#,
    // Everything at once — bitcrushed, starved, feeding back into the void.
    r#"{ "name": "Bitcrushed Void", "bitrate": 6.0, "mode": 1, "bandwidth": 1, "fec": 0,
         "loss": 20.0, "crunch": 0.75, "regen_delay": 90.0, "regen_amount": 0.6,
         "width": 1.4, "mix": 1.0, "out": 0.0 }"#,
];

/// Build a DSP [`Settings`] from a parsed preset, falling back to defaults for missing keys.
pub fn settings_from_preset(p: &Preset) -> Settings {
    let d = Settings::default();
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    Settings {
        bitrate_kbps: g("bitrate", d.bitrate_kbps),
        mode: Mode::from_index(g("mode", 1.0) as usize),
        bandwidth: BandwidthSel::from_index(g("bandwidth", 4.0) as usize),
        fec: g("fec", 0.0) >= 0.5,
        loss_pct: g("loss", d.loss_pct),
        crunch: g("crunch", d.crunch),
        regen_delay_ms: g("regen_delay", d.regen_delay_ms),
        regen_amount: g("regen_amount", d.regen_amount),
        width: g("width", d.width),
        mix: g("mix", d.mix),
        out_db: g("out", d.out_db),
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
            if (s.bitrate_kbps - d.bitrate_kbps).abs() > 1e-3 {
                diffs += 1;
            }
            if s.mode != d.mode {
                diffs += 1;
            }
            if s.bandwidth != d.bandwidth {
                diffs += 1;
            }
            if s.fec != d.fec {
                diffs += 1;
            }
            if (s.loss_pct - d.loss_pct).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.crunch - d.crunch).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.regen_amount - d.regen_amount).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.width - d.width).abs() > 1e-3 {
                diffs += 1;
            }
            if (s.mix - d.mix).abs() > 1e-3 {
                diffs += 1;
            }
            assert!(diffs >= 3, "preset '{}' differs in only {diffs} params", p.name);
        }
    }
}
