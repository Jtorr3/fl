//! WIRE factory presets (SPECS "PRESET-EXPANSION" deep bank). Each is an embedded
//! flat-JSON blob parsed by `suite_core::presets`. The same list drives the GUI
//! selector (grouped by the `"category"` tag into preset-bar sections) and the
//! offline render tests. Values are plain (un-normalized): kbps for bitrate, % for
//! loss, ms for regen delay, dB for out, 0..1 for crunch/mix, 0..2 for width,
//! 0/1 for fec, and integer indices for the enums.
//!
//! Enum encodings: `mode` 0 = Voice, 1 = Music. `bandwidth` 0 = Narrow, 1 = Medium,
//! 2 = Wide, 3 = Superwide, 4 = Full (see `dsp::BandwidthSel::cutoff_hz`).
//!
//! Categories (preset-bar sections): Subtle-Lo-Fi / Codec-Crunch / Bitcrush /
//! Telephone-Radio / Destroyed. Names are purpose-driven and genre-aware (dark
//! techno / atmospheric dnb / digital-decay) — never settings descriptions.

use crate::dsp::{BandwidthSel, Mode, Settings};
use suite_core::presets::Preset;

/// The factory presets, in menu order, tagged by category (PRESET-EXPANSION deep bank).
pub const PRESET_JSON: &[&str] = &[
    // ---- Subtle-Lo-Fi -----------------------------------------------------
    // A barely-there digital sheen for glue on a full mix — parallel, high bitrate. The
    // faint codec-width artifact (width 1.1) is the only tell.
    r#"{ "name": "Subtle Digital", "category": "Subtle-Lo-Fi", "bitrate": 96.0, "mode": 1, "bandwidth": 4, "fec": 0,
         "loss": 0.0, "crunch": 0.05, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 1.1, "mix": 0.4, "out": 0.0 }"#,
    // Gentle high-bitrate haze with a touch of top-end rolloff — a well-loved cassette dub.
    r#"{ "name": "Faded Cassette Sheen", "category": "Subtle-Lo-Fi", "bitrate": 80.0, "mode": 1, "bandwidth": 3, "fec": 0,
         "loss": 0.0, "crunch": 0.08, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 1.1, "mix": 0.6, "out": 0.0 }"#,
    // Warm parallel MP3 glue for a bus: light crunch, narrowed image, a hair of make-up.
    r#"{ "name": "Warm MP3 Glue", "category": "Subtle-Lo-Fi", "bitrate": 64.0, "mode": 1, "bandwidth": 4, "fec": 0,
         "loss": 0.0, "crunch": 0.05, "regen_delay": 150.0, "regen_amount": 0.0,
         "width": 0.9, "mix": 0.5, "out": 0.5 }"#,
    // A quiet Bandcamp rip: mid-high bitrate, softened air, sits behind the mix.
    r#"{ "name": "Dusty Bandcamp Rip", "category": "Subtle-Lo-Fi", "bitrate": 48.0, "mode": 1, "bandwidth": 3, "fec": 0,
         "loss": 0.0, "crunch": 0.12, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 1.0, "mix": 0.7, "out": -0.5 }"#,
    // ---- Codec-Crunch -----------------------------------------------------
    // Crunchy, glitchy low-bitrate voice with intermittent dropouts — a VoIP ghost.
    r#"{ "name": "Discord Ghost", "category": "Codec-Crunch", "bitrate": 16.0, "mode": 0, "bandwidth": 2, "fec": 1,
         "loss": 12.0, "crunch": 0.15, "regen_delay": 60.0, "regen_amount": 0.0,
         "width": 1.0, "mix": 1.0, "out": -0.5 }"#,
    // Tape-style generation loss: re-encoding feedback compounds the artifacts each pass.
    r#"{ "name": "Generation Loss", "category": "Codec-Crunch", "bitrate": 24.0, "mode": 1, "bandwidth": 3, "fec": 0,
         "loss": 3.0, "crunch": 0.2, "regen_delay": 180.0, "regen_amount": 0.7,
         "width": 1.1, "mix": 1.0, "out": -1.0 }"#,
    // Full-wet mid-bitrate mush with a fine crust of packet dropouts — the sound of a leak.
    r#"{ "name": "Sewer Codec", "category": "Codec-Crunch", "bitrate": 14.0, "mode": 1, "bandwidth": 2, "fec": 0,
         "loss": 8.0, "crunch": 0.3, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 1.2, "mix": 1.0, "out": -0.5 }"#,
    // Pairing dropped mid-word: narrowed VoIP profile, FEC clawing at the gaps.
    r#"{ "name": "Broken Bluetooth", "category": "Codec-Crunch", "bitrate": 18.0, "mode": 0, "bandwidth": 1, "fec": 1,
         "loss": 22.0, "crunch": 0.2, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 0.7, "mix": 1.0, "out": -0.5 }"#,
    // ---- Bitcrush ---------------------------------------------------------
    // Sample-rate reduction front and centre, mourning in 8-bit — parallel-leaning.
    r#"{ "name": "8-Bit Grief", "category": "Bitcrush", "bitrate": 40.0, "mode": 1, "bandwidth": 3, "fec": 0,
         "loss": 0.0, "crunch": 0.6, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 1.0, "mix": 0.9, "out": -1.0 }"#,
    // Chiptune laid to rest: aggressive crunch, band-limited, tightened image.
    r#"{ "name": "Nintendo Funeral", "category": "Bitcrush", "bitrate": 36.0, "mode": 1, "bandwidth": 2, "fec": 0,
         "loss": 0.0, "crunch": 0.7, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 0.8, "mix": 1.0, "out": -1.0 }"#,
    // Ground to dust — near-max crunch with a widening regen haze, hard trim to hold level.
    r#"{ "name": "Crushed to Powder", "category": "Bitcrush", "bitrate": 28.0, "mode": 1, "bandwidth": 2, "fec": 0,
         "loss": 0.0, "crunch": 0.85, "regen_delay": 130.0, "regen_amount": 0.2,
         "width": 1.3, "mix": 1.0, "out": -2.0 }"#,
    // ---- Telephone-Radio --------------------------------------------------
    // Muffled telephone hold-music: narrowband, low bitrate, no loss, collapsed width.
    r#"{ "name": "Hold Music", "category": "Telephone-Radio", "bitrate": 12.0, "mode": 0, "bandwidth": 0, "fec": 0,
         "loss": 0.0, "crunch": 0.1, "regen_delay": 40.0, "regen_amount": 0.0,
         "width": 0.4, "mix": 1.0, "out": -0.5 }"#,
    // The last announcement before the doors close — narrowband voice, FEC, faint dropouts.
    r#"{ "name": "Last Train Transmission", "category": "Telephone-Radio", "bitrate": 12.0, "mode": 0, "bandwidth": 0, "fec": 1,
         "loss": 5.0, "crunch": 0.15, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 0.5, "mix": 1.0, "out": -0.5 }"#,
    // A modem handshake haunting the line: heavy loss, a short howling regen, mono-ish.
    r#"{ "name": "Dial-Up Ghost", "category": "Telephone-Radio", "bitrate": 10.0, "mode": 0, "bandwidth": 0, "fec": 0,
         "loss": 18.0, "crunch": 0.4, "regen_delay": 70.0, "regen_amount": 0.25,
         "width": 0.6, "mix": 1.0, "out": -1.5 }"#,
    // Squeezed through a handset: medium band, FEC, choppy loss, pushed hot.
    r#"{ "name": "Walkie-Talkie Prayer", "category": "Telephone-Radio", "bitrate": 14.0, "mode": 0, "bandwidth": 1, "fec": 1,
         "loss": 10.0, "crunch": 0.2, "regen_delay": 120.0, "regen_amount": 0.0,
         "width": 0.4, "mix": 1.0, "out": -0.7 }"#,
    // Voices bleeding through static bands — long regen ghosting, parallel-blended.
    r#"{ "name": "AM Radio Séance", "category": "Telephone-Radio", "bitrate": 16.0, "mode": 0, "bandwidth": 0, "fec": 0,
         "loss": 6.0, "crunch": 0.25, "regen_delay": 200.0, "regen_amount": 0.4,
         "width": 0.5, "mix": 0.9, "out": 0.0 }"#,
    // ---- Destroyed --------------------------------------------------------
    // A buffering, falling-apart stream: very low bitrate, heavy packet loss, FEC fighting it.
    r#"{ "name": "Dying Stream", "category": "Destroyed", "bitrate": 8.0, "mode": 0, "bandwidth": 1, "fec": 1,
         "loss": 35.0, "crunch": 0.25, "regen_delay": 100.0, "regen_amount": 0.0,
         "width": 0.8, "mix": 1.0, "out": -0.7 }"#,
    // Everything at once — bitcrushed, starved, feeding back into the void.
    r#"{ "name": "Bitcrushed Void", "category": "Destroyed", "bitrate": 6.0, "mode": 1, "bandwidth": 1, "fec": 0,
         "loss": 20.0, "crunch": 0.75, "regen_delay": 90.0, "regen_amount": 0.6,
         "width": 1.4, "mix": 1.0, "out": -2.0 }"#,
    // Wall-of-static breakcore texture: 6 kbps voice, drowned in loss, spraying wide.
    r#"{ "name": "Sewerslvt Static", "category": "Destroyed", "bitrate": 6.0, "mode": 0, "bandwidth": 1, "fec": 0,
         "loss": 30.0, "crunch": 0.8, "regen_delay": 80.0, "regen_amount": 0.5,
         "width": 1.5, "mix": 1.0, "out": -2.0 }"#,
    // The transmission finally gives out: max crunch, near-total loss, howling regen, trimmed.
    r#"{ "name": "Total Signal Collapse", "category": "Destroyed", "bitrate": 6.0, "mode": 1, "bandwidth": 0, "fec": 0,
         "loss": 45.0, "crunch": 0.9, "regen_delay": 60.0, "regen_amount": 0.7,
         "width": 1.8, "mix": 1.0, "out": -3.0 }"#,
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

    /// Count how many `Settings` fields differ between two presets (enums/bools by equality,
    /// floats by a loose epsilon). Drives both the differ-from-default and pairwise-distinctness
    /// quality gates. All 11 controls participate (including regen_delay_ms and out_db).
    fn count_diffs(a: &Settings, b: &Settings) -> usize {
        let mut n = 0;
        if a.mode != b.mode {
            n += 1;
        }
        if a.bandwidth != b.bandwidth {
            n += 1;
        }
        if a.fec != b.fec {
            n += 1;
        }
        let fs = [
            (a.bitrate_kbps, b.bitrate_kbps),
            (a.loss_pct, b.loss_pct),
            (a.crunch, b.crunch),
            (a.regen_delay_ms, b.regen_delay_ms),
            (a.regen_amount, b.regen_amount),
            (a.width, b.width),
            (a.mix, b.mix),
            (a.out_db, b.out_db),
        ];
        for (x, y) in fs {
            if (x - y).abs() > 1e-3 {
                n += 1;
            }
        }
        n
    }

    /// PRESET-EXPANSION quality gate (mechanical), all four rules across the full bank.
    #[test]
    fn bank_meets_expansion_quality_gate() {
        let presets = load_all(PRESET_JSON);
        // Deep bank: SPECS target 15-24 for this codec-degradation FX.
        assert!(presets.len() >= 15, "WIRE bank too small: {}", presets.len());

        let d = Settings::default();
        let settings: Vec<Settings> = presets.iter().map(settings_from_preset).collect();

        // Rule 1 (loads) is implicit in load_all. Rule 2: every preset differs from the
        // default in >= 4 params, and every preset is categorised.
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
        // `render_tests::every_preset_renders_and_passes_universal` test in lib.rs.
    }
}
