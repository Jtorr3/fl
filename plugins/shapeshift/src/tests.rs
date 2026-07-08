//! SHAPESHIFT done-bar + regression tests (PRD §4, SHAPESHIFT-specific bars).
//!
//! Included from `dsp.rs` via `#[path = "tests.rs"] mod tests;`, so it sees the private core.

use super::*;
use std::f32::consts::TAU;
use suite_core::harness::{assert_single_coherent_peak, null_residual_db, rms_dbfs};

const SR: f32 = 48_000.0;

fn sine(freq: f32, amp: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (TAU * freq * i as f32 / SR).sin())
        .collect()
}

fn log_chirp(f0: f32, f1: f32, amp: f32, n: usize) -> Vec<f32> {
    // Same shape as suite testsig::log_chirp; local copy so the DSP-core tests don't reach for it.
    let t = n as f32 / SR;
    let k = (f1 / f0).ln();
    (0..n)
        .map(|i| {
            let tt = i as f32 / SR;
            let phase = TAU * f0 * t / k * ((k * tt / t).exp() - 1.0);
            amp * phase.sin()
        })
        .collect()
}

/// Pearson correlation of two equal-length signals (scale/offset invariant).
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let ma = a[..n].iter().sum::<f32>() / n as f32;
    let mb = b[..n].iter().sum::<f32>() / n as f32;
    let mut num = 0.0f32;
    let mut da = 0.0f32;
    let mut db = 0.0f32;
    for i in 0..n {
        let xa = a[i] - ma;
        let xb = b[i] - mb;
        num += xa * xb;
        da += xa * xa;
        db += xb * xb;
    }
    num / (da.sqrt() * db.sqrt()).max(1.0e-20)
}

/// Goertzel power at frequency `f` (Hz) over `x`.
fn power(x: &[f32], f: f32) -> f32 {
    let w = TAU * f / SR;
    let cw = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + cw * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - cw * s1 * s2
}

/// Total-harmonic-distortion ratio (harmonics 2..8 vs fundamental) — GRIT's measure.
fn thd_ratio(x: &[f32], fund_hz: f32) -> f32 {
    let fund = power(x, fund_hz).max(1.0e-20);
    let mut harm = 0.0f32;
    for h in 2..=8 {
        let f = fund_hz * h as f32;
        if f < SR * 0.5 {
            harm += power(x, f);
        }
    }
    (harm / fund).sqrt()
}

fn base_settings() -> Settings {
    Settings {
        pre_db: 6.0,
        auto_gain: false,
        orbit_on: false,
        post_lp_hz: 16_000.0,
        mix: 1.0,
        out_db: 0.0,
        ..Settings::default()
    }
}

// ---------------------------------------------------------------------------
// Done-bar 1: XY hard at a corner nulls against that shaper alone (< −60 dB), for A and D.
// ---------------------------------------------------------------------------

fn corner_nulls_single_shaper(corner_idx: usize, xy: (f32, f32)) {
    let n = 24_000usize;
    let input = sine(300.0, 0.6, n);

    // A morph with four DISTINCT corners + per-corner gain trims.
    let mut s = base_settings();
    s.corner = [
        Corner::TubeTanh,
        Corner::HardClip,
        Corner::Cheby3,
        Corner::SineFold,
    ];
    s.gain_db = [3.0, -2.0, 1.5, 4.0];
    s.x = xy.0;
    s.y = xy.1;

    // The reference: the SAME chain but every corner forced to the selected shaper with that
    // corner's gain — i.e. "shaper <corner> alone, same pre + corner gain". Any XY gives the
    // single shaper (all four corners identical), so we deliberately place it at centre to prove
    // the isolation is a property of the weights, not a matched config.
    let mut r = s;
    r.corner = [s.corner[corner_idx]; NUM_CORNERS];
    r.gain_db = [s.gain_db[corner_idx]; NUM_CORNERS];
    r.x = 0.5;
    r.y = 0.5;

    let mut out = input.clone();
    ShapeshiftCore::new(SR).process_mono(&mut out, &s);
    let mut refr = input.clone();
    ShapeshiftCore::new(SR).process_mono(&mut refr, &r);

    let resid = null_residual_db(&out, &refr);
    assert!(
        resid < -60.0,
        "corner {corner_idx} at {xy:?} did not null against its shaper alone: residual {resid:.1} dB"
    );
}

#[test]
fn corner_a_nulls_against_shaper_a() {
    // Corner A = (0,0).
    corner_nulls_single_shaper(0, (0.0, 0.0));
}

#[test]
fn corner_d_nulls_against_shaper_d() {
    // Corner D = (1,1).
    corner_nulls_single_shaper(3, (1.0, 1.0));
}

// ---------------------------------------------------------------------------
// Done-bar 2: centre XY differs from every single corner (corr < 0.99), bounded.
// ---------------------------------------------------------------------------

#[test]
fn center_morph_differs_from_every_corner() {
    let n = (SR * 1.0) as usize;
    // Drive hard so the shaper curves genuinely differ in waveform shape (not just scale — at low
    // level every shaper is ~linear and would correlate ~1).
    let input = sine(400.0, 0.6, n);

    let mut base = base_settings();
    base.pre_db = 15.0;
    base.corner = [
        Corner::TubeTanh,
        Corner::HardClip,
        Corner::Cheby3,
        Corner::SineFold,
    ];
    base.gain_db = [0.0; NUM_CORNERS];

    let render = |x: f32, y: f32| {
        let mut s = base;
        s.x = x;
        s.y = y;
        let mut out = input.clone();
        ShapeshiftCore::new(SR).process_mono(&mut out, &s);
        out
    };

    let center = render(0.5, 0.5);
    // Clamp policy (TRIAGE 2026-07-08): this hard-driven render may legitimately exceed
    // 0 dBFS now that the final clamp is a ±8.0 guard; assert finite/non-silent/≤ guard.
    assert!(!suite_core::harness::has_nan_or_inf(&center), "center render NaN/inf");
    assert!(suite_core::harness::peak_dbfs(&center) <= 18.1, "center render exceeds +18 dBFS guard");
    assert!(suite_core::harness::rms_dbfs(&center) > -60.0, "center render silent");

    // Skip the first 10 ms (smoother settle) when correlating.
    let skip = (SR * 0.01) as usize;
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
    for (k, &(cx, cy)) in corners.iter().enumerate() {
        let c = render(cx, cy);
        let corr = correlation(&center[skip..], &c[skip..]);
        assert!(
            corr < 0.99,
            "centre morph correlates {corr:.4} with corner {k} ({cx},{cy}) — not distinct"
        );
    }
}

// ---------------------------------------------------------------------------
// Done-bar 3: orbit at 1 Hz → spectral character (THD) varies periodically at the orbit rate.
// ---------------------------------------------------------------------------

#[test]
fn orbit_modulates_thd_periodically() {
    let secs = 3.0f32;
    let n = (SR * secs) as usize;
    let f0 = 300.0f32;
    let input = sine(f0, 0.6, n);

    // THD depends monotonically on X: clean tape (low gain → low THD) at X=0 corners, hard clip
    // (high gain → near-square, high THD) at X=1 corners. A circular orbit sweeps X once per cycle
    // → THD oscillates at 1 Hz. The per-corner gains exaggerate the contrast so the character
    // change is unambiguous.
    let mut s = base_settings();
    s.corner = [
        Corner::TapeSoft, // A (0,0) — clean
        Corner::HardClip, // B (1,0) — harsh
        Corner::TapeSoft, // C (0,1) — clean
        Corner::HardClip, // D (1,1) — harsh
    ];
    s.gain_db = [-6.0, 15.0, -6.0, 15.0];
    s.x = 0.5;
    s.y = 0.5;
    s.orbit_on = true;
    s.orbit_shape = OrbitShape::Circle;
    s.orbit_rate_hz = 1.0;
    s.orbit_radius = 0.45;
    s.orbit_sync = false;

    let mut out = input.clone();
    ShapeshiftCore::new(SR).process_mono(&mut out, &s);
    // Clamp policy (TRIAGE 2026-07-08): hard-driven render; finite/non-silent/≤ +18 dBFS guard.
    assert!(!suite_core::harness::has_nan_or_inf(&out), "orbit render NaN/inf");
    assert!(suite_core::harness::peak_dbfs(&out) <= 18.1, "orbit render exceeds +18 dBFS guard");
    assert!(suite_core::harness::rms_dbfs(&out) > -60.0, "orbit render silent");

    // THD-vs-time: a 40 ms window every 40 ms.
    let win = (SR * 0.04) as usize;
    let hop = win;
    let mut thd: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + win <= out.len() {
        thd.push(thd_ratio(&out[i..i + win], f0));
        i += hop;
    }

    // (a) The character genuinely varies over the orbit.
    let tmax = thd.iter().cloned().fold(0.0f32, f32::max);
    let tmin = thd.iter().cloned().fold(f32::INFINITY, f32::min);
    let tmean = thd.iter().sum::<f32>() / thd.len() as f32;
    assert!(
        (tmax - tmin) / tmean.max(1e-6) > 0.4,
        "THD barely varies over the orbit: min {tmin:.4} max {tmax:.4} mean {tmean:.4}"
    );

    // (b) Periodicity at the 1 Hz orbit rate: autocorrelation of the THD sequence is stronger at a
    // one-period lag than at a half-period lag (and than a good fraction of zero-lag).
    let windows_per_sec = (SR / hop as f32) as usize; // ~25
    let autocorr = |lag: usize| -> f32 {
        let m = thd.len() - lag;
        let mut acc = 0.0f32;
        for j in 0..m {
            acc += (thd[j] - tmean) * (thd[j + lag] - tmean);
        }
        acc / m as f32
    };
    let a0 = autocorr(0);
    let a_full = autocorr(windows_per_sec); // 1 s = one orbit period
    let a_half = autocorr(windows_per_sec / 2); // 0.5 s = half period (anti-phase)
    assert!(
        a_full > a_half && a_full > 0.3 * a0,
        "THD not periodic at orbit rate: a0 {a0:.4} a_full {a_full:.4} a_half {a_half:.4}"
    );
}

// ---------------------------------------------------------------------------
// Regression: ORBIT PHASE applies live (read every block, not only at prime).
// Pre-fix, `orbit_phase0` was read solely inside the `!primed` gate, so turning the
// PHASE knob during playback did nothing. A phase change *after* the orbit is primed
// must rotate the orbit on the following samples.
// ---------------------------------------------------------------------------

#[test]
fn orbit_phase_applies_live_after_prime() {
    let block = 2048usize;
    let f0 = 300.0f32;
    // Contrasting corners (clean A/C vs harsh B/D) so the orbit position strongly colours
    // the output — a rotated orbit lands on audibly different shaper blends.
    let mut s = base_settings();
    s.corner = [
        Corner::TapeSoft, // A (0,0) — clean
        Corner::HardClip, // B (1,0) — harsh
        Corner::TapeSoft, // C (0,1) — clean
        Corner::HardClip, // D (1,1) — harsh
    ];
    s.gain_db = [-6.0, 15.0, -6.0, 15.0];
    s.x = 0.5;
    s.y = 0.5;
    s.orbit_on = true;
    s.orbit_shape = OrbitShape::Circle;
    s.orbit_rate_hz = 1.0;
    s.orbit_radius = 0.45;
    s.orbit_sync = false;
    s.orbit_phase0 = 0.0;

    let input = sine(f0, 0.6, block);

    // Control core: prime with a block, then a SECOND block with the phase unchanged.
    let mut core_a = ShapeshiftCore::new(SR);
    let mut a1 = input.clone();
    core_a.process_mono(&mut a1, &s); // primes at phase0 = 0.0
    let mut a2 = input.clone();
    core_a.process_mono(&mut a2, &s); // still phase0 = 0.0

    // Test core: identical priming block, then a second block with the PHASE knob moved.
    let mut core_b = ShapeshiftCore::new(SR);
    let mut b1 = input.clone();
    core_b.process_mono(&mut b1, &s); // identical prime
    let mut s2 = s;
    s2.orbit_phase0 = 0.5; // half-cycle rotation, applied AFTER prime
    let mut b2 = input.clone();
    core_b.process_mono(&mut b2, &s2);

    // The priming blocks are identical (same init, input, params) → exact null.
    assert!(
        null_residual_db(&a1, &b1) < -120.0,
        "priming blocks diverged unexpectedly"
    );

    // After the mid-stream PHASE change the second blocks must differ: the orbit is rotated
    // half a cycle, so the blend (and thus the output) changes. Pre-fix this residual was
    // effectively -inf (identical), so a loud residual proves the phase now applies live.
    let resid = null_residual_db(&a2, &b2);
    assert!(
        resid > -30.0,
        "ORBIT PHASE change after prime had no effect (residual {resid:.1} dB) — phase still gated by !primed"
    );
}

// ---------------------------------------------------------------------------
// Done-bar 4: partial-mix single coherent peak (PDC) + mix=0 nulls against latency-matched dry.
// ---------------------------------------------------------------------------

#[test]
fn partial_mix_impulse_is_single_coherent_peak() {
    let n = 256usize;
    let mut s = base_settings();
    s.mix = 0.5;
    s.pre_db = 0.0; // ~unity drive
    s.gain_db = [0.0; NUM_CORNERS];
    s.corner = [Corner::TubeTanh; NUM_CORNERS];
    s.post_lp_hz = 20_000.0;
    s.x = 0.0;
    s.y = 0.0;

    let mut main = vec![0.0f32; n];
    main[0] = 1.0;
    ShapeshiftCore::new(SR).process_mono(&mut main, &s);
    // Dry (0.5) and wet (~0.5) coincide at the compensated group-delay lag → one cluster.
    assert_single_coherent_peak(&main, 2, 0.5);
}

#[test]
fn mix_zero_nulls_against_latency_matched_dry() {
    let n = 24_000usize;
    let input = sine(440.0, 0.5, n);
    let mut s = base_settings();
    s.mix = 0.0;
    s.out_db = 0.0;

    let core = ShapeshiftCore::new(SR);
    let lat = core.latency_samples() as usize;
    let mut out = input.clone();
    let mut core = core;
    core.process_mono(&mut out, &s);

    // At mix=0 the output is the dry path delayed by the reported latency.
    let m = n - lat;
    let mse = (0..m)
        .map(|i| {
            let d = input[i] - out[i + lat];
            d * d
        })
        .sum::<f32>()
        / m as f32;
    let resid = 20.0 * mse.sqrt().max(1.0e-12).log10();
    assert!(resid < -80.0, "mix=0 did not null: residual {resid:.1} dB");
}

// ---------------------------------------------------------------------------
// Bounds: every corner shaper stays finite and bounded under hard drive.
// ---------------------------------------------------------------------------

#[test]
fn all_corners_bounded_under_hard_drive() {
    for &c in &Corner::ALL {
        for i in -200..=200 {
            let x = i as f32 * 0.05; // −10 .. +10
            let y = c.apply(x);
            assert!(y.is_finite(), "{c:?} produced non-finite at x={x}");
            assert!(y.abs() <= 1.05, "{c:?} exceeded bound at x={x}: {y}");
        }
    }
}

// ---------------------------------------------------------------------------
// Regression (SOUND-PASS): the Cheby-3 corner must not be a level hole. `T₃(x)=4x³−3x`
// inverts small-signal polarity (slope −3 near 0), so at the corner AND — worse — in the
// XY blend around it, its output cancels against the other (positive-polarity) corners,
// producing a level dip when morphing through corner C. The shaper is sign-corrected to a
// polarity-preserving odd cubic, so the Cheby-3 corner now sits within a few dB of the
// other three corners on the same reese input.
// ---------------------------------------------------------------------------

#[test]
fn cheby3_corner_no_level_hole() {
    // A low fundamental like the reese sub, driven into the shaper bank. The raw `T₃(x)=4x³−3x`
    // maps a cosine to `cos3θ` — a PURE 3rd harmonic with ZERO fundamental — and inverts the
    // small-signal polarity (slope −3 near 0), so the Cheby-3 corner nulls the sub/fundamental
    // (a "volume hole") and cancels the other corners in the XY blend. The sign-corrected cubic
    // preserves the fundamental, so corner C carries comparable low-end to the other corners.
    let n = (SR * 1.0) as usize;
    let f0 = 55.0f32;
    let input = sine(f0, 0.6, n);

    // Four DISTINCT corners with Cheby-3 at C=(0,1); flat gains so the only difference between
    // corners is the shaper. Isolate each corner by parking the XY point on it.
    let mut base = base_settings();
    base.corner = [
        Corner::TubeTanh, // A (0,0)
        Corner::HardClip, // B (1,0)
        Corner::Cheby3,   // C (0,1)  <- under scrutiny
        Corner::SineFold, // D (1,1)
    ];
    base.gain_db = [0.0; NUM_CORNERS];
    base.pre_db = 6.0;

    let skip = (SR * 0.05) as usize;
    let render = |x: f32, y: f32| -> Vec<f32> {
        let mut s = base;
        s.x = x;
        s.y = y;
        let mut out = input.clone();
        ShapeshiftCore::new(SR).process_mono(&mut out, &s);
        out
    };
    // Fundamental level (dB) at each isolated corner (Goertzel — delay/phase invariant).
    let fund_db = |out: &[f32]| -> f32 { 10.0 * power(&out[skip..], f0).max(1e-20).log10() };
    let a = fund_db(&render(0.0, 0.0));
    let b = fund_db(&render(1.0, 0.0));
    let c = fund_db(&render(0.0, 1.0)); // Cheby-3
    let d = fund_db(&render(1.0, 1.0));
    let others_mean = (a + b + d) / 3.0;
    eprintln!("cheby3 fundamentals dB: A={a:.1} B={b:.1} C(cheby3)={c:.1} D={d:.1} others_mean={others_mean:.1}");
    // Comparable low-end: the Cheby-3 corner fundamental is within a few dB of the others' mean.
    assert!(
        c >= others_mean - 6.0,
        "Cheby-3 corner is a fundamental/level hole: C={c:.1} dB vs others mean {others_mean:.1} dB \
         (A={a:.1} B={b:.1} D={d:.1})"
    );

    // Broadband level parity too (the corner should not be quiet overall).
    let c_rms = rms_dbfs(&render(0.0, 1.0)[skip..]);
    let others_rms =
        (rms_dbfs(&render(0.0, 0.0)[skip..]) + rms_dbfs(&render(1.0, 0.0)[skip..]) + rms_dbfs(&render(1.0, 1.0)[skip..])) / 3.0;
    assert!(
        (c_rms - others_rms).abs() <= 5.0,
        "Cheby-3 corner RMS off the others: C={c_rms:.1} dBFS vs others {others_rms:.1} dBFS"
    );
}

#[test]
fn extreme_settings_stay_bounded() {
    let n = (SR * 0.5) as usize;
    // A full-band chirp exercises every part of the shaper curves as it sweeps.
    let input = log_chirp(40.0, 12_000.0, 0.9, n);
    let mut s = Settings::default();
    s.pre_db = 24.0;
    s.gain_db = [24.0, 24.0, 24.0, 24.0];
    s.corner = [
        Corner::HardClip,
        Corner::WavefoldTri,
        Corner::Cheby3,
        Corner::BitcrushSoft,
    ];
    s.orbit_on = true;
    s.orbit_rate_hz = 8.0;
    s.orbit_radius = 0.5;
    s.auto_gain = true;
    let mut out = input.clone();
    ShapeshiftCore::new(SR).process_mono(&mut out, &s);
    // Clamp policy (TRIAGE 2026-07-08): final clamp is a ±8.0 runaway/NaN guard
    // (≈ +18 dBFS), not a 0 dBFS ceiling — extreme fuzz asserts finite && ≤ the guard.
    for &v in &out {
        assert!(v.is_finite() && v.abs() <= 8.001, "extreme render out of range: {v}");
    }
}
