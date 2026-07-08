//! X-RAY done-bar (PRD §4): "reads ≥2 live slots' spectra from the bus in a two-instance
//! test", plus the bit-exact passthrough guarantee.

use super::out_gain;
use suite_core::bus::{new_instance_id, Bus, PluginKind};
use suite_core::dsp::Svf;
use suite_core::spectrum::{band_center_hz, dominant_band, SpectrumTap, NUM_BANDS};
use std::path::PathBuf;

fn temp_bus_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "qeynos-bus-xray-{}-{}-{}",
        tag,
        std::process::id(),
        new_instance_id()
    ))
}

#[test]
fn manual_covers_all_params_and_has_recipes() {
    suite_core::manual::assert_manual_covers_params(crate::MANUAL_DOC, &crate::XrayParams::default());
}

/// Deterministic xorshift noise (no `rand` dep).
struct Rng(u32);
impl Rng {
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// **The done-bar.** Two bus handles (two "DLLs") each publish a distinct spectrum through a
/// [`SpectrumTap`] — one low-band-limited noise, one high-band-limited noise. X-RAY's reader
/// path (`snapshot_live`) sees BOTH slots, and each carries a plausibly-shaped spectrum with
/// its energy concentrated in the expected part of the band.
#[test]
fn two_instances_publish_distinct_spectra_and_reader_sees_both() {
    let sr = 48_000.0;
    let path = temp_bus_path("done");

    let writer_low = Bus::open_or_create(&path).unwrap();
    let writer_high = Bus::open_or_create(&path).unwrap();
    let xray_reader = Bus::open_or_create(&path).unwrap();

    let id_low = new_instance_id();
    let id_high = new_instance_id();
    let slot_low = writer_low
        .claim(id_low, PluginKind::Generic, "LOW RUMBLE")
        .unwrap();
    let slot_high = writer_high
        .claim(id_high, PluginKind::Generic, "HIGH AIR")
        .unwrap();

    // Two band-limited noise sources.
    let mut rng = Rng(0xC0FF_EE01);
    let mut lp = Svf::new();
    lp.set(180.0, 0.707, sr);
    let mut hp = Svf::new();
    hp.set(7_500.0, 0.707, sr);

    let mut tap_low = SpectrumTap::new(sr);
    let mut tap_high = SpectrumTap::new(sr);
    let mut sp_low = [0.0f32; NUM_BANDS];
    let mut sp_high = [0.0f32; NUM_BANDS];

    // Publish over ~0.4 s of blocks so the smoothed spectra settle.
    for _blk in 0..40 {
        for _ in 0..512 {
            let w = rng.next_f32();
            tap_low.feed(lp.process(w).lp);
            tap_high.feed(hp.process(w).hp);
        }
        let (p_lo, r_lo) = tap_low.finish(&mut sp_low);
        let (p_hi, r_hi) = tap_high.finish(&mut sp_high);
        writer_low.publish_spectrum(slot_low, &sp_low, p_lo, r_lo);
        writer_low.beat(slot_low);
        writer_high.publish_spectrum(slot_high, &sp_high, p_hi, r_hi);
        writer_high.beat(slot_high);
    }

    // ---- X-RAY reader sees both, correctly shaped -------------------------
    let live = xray_reader.snapshot_live();
    assert_eq!(live.len(), 2, "X-RAY must see BOTH live slots, saw {}", live.len());

    let low = live.iter().find(|s| s.label == "LOW RUMBLE").expect("low slot");
    let high = live.iter().find(|s| s.label == "HIGH AIR").expect("high slot");

    // Each slot's energy sits in the expected half of the band.
    let split = (0..NUM_BANDS)
        .find(|&i| band_center_hz(i) >= 1_000.0)
        .unwrap();
    let energy = |s: &[f32; NUM_BANDS], r: std::ops::Range<usize>| -> f32 {
        r.map(|i| s[i] * s[i]).sum::<f32>()
    };
    assert!(
        energy(&low.spectrum, 0..split) > energy(&low.spectrum, split..NUM_BANDS) * 4.0,
        "LOW slot energy should sit in low bands"
    );
    assert!(
        energy(&high.spectrum, split..NUM_BANDS) > energy(&high.spectrum, 0..split) * 4.0,
        "HIGH slot energy should sit in high bands"
    );
    // The two spectra are distinctly shaped.
    assert!(
        dominant_band(&low.spectrum) < dominant_band(&high.spectrum),
        "the two slots must be distinguishable by shape"
    );
    assert!(low.rms > 0.0 && high.rms > 0.0, "both report a live level");

    writer_low.release(slot_low, id_low);
    writer_high.release(slot_high, id_high);
    let _ = std::fs::remove_file(&path);
}

/// Passthrough is bit-exact at the default 0 dB out trim: `out_gain(0) == 1.0` and every
/// sample survives `x * 1.0` unchanged to the bit.
#[test]
fn passthrough_is_bit_exact_at_unity() {
    assert_eq!(out_gain(0.0).to_bits(), 1.0f32.to_bits());
    let samples: [f32; 9] = [
        0.0, 1.0, -1.0, 0.5, -0.333_333_34, 1.0e-9, -2.5, 0.999_999_9, f32::MIN_POSITIVE,
    ];
    let g = out_gain(0.0);
    for &x in &samples {
        assert_eq!((x * g).to_bits(), x.to_bits(), "sample {x} changed under unity trim");
    }
}

/// A non-zero trim actually scales (and −inf/…: stays finite for finite input).
#[test]
fn out_trim_scales() {
    let g6 = out_gain(6.0206); // ~2x
    assert!((g6 - 2.0).abs() < 1e-3, "6 dB should ~double, got {g6}");
    let gm6 = out_gain(-6.0206);
    assert!((gm6 - 0.5).abs() < 1e-3, "-6 dB should ~halve, got {gm6}");
}
