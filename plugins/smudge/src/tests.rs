//! SMUDGE done-bar + universal DSP tests (PRD §4 + build brief).
//!
//! Done bar (build brief): universal assertions, plus SMUDGE-specific —
//! (1) **ALL op amounts 0 → wet nulls against the latency-delayed dry < −60 dB** (stricter
//!     than mix=0: proves each op's amount=0 bypass is EXACT — the wet path collapses to the
//!     STFT's own identity reconstruction);
//! (2) **scramble > 0 → per-frame spectral correlation with the dry drops below 0.9** while
//!     total energy stays within ±3 dB (a bin permutation is energy-preserving but
//!     decorrelating).

use crate::dsp::*;
use suite_core::stft::{Complex, Stft};
use suite_core::testsig;

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}
fn rms_db(x: &[f32]) -> f32 {
    20.0 * rms(x).max(1.0e-12).log10()
}
fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, &v| m.max(v.abs()))
}

/// Per-frame magnitude spectra of a signal via an independent analysis STFT (2048/512).
fn mag_frames(sig: &[f32]) -> Vec<Vec<f32>> {
    let mut stft = Stft::new(FFT_SIZE, HOP);
    let mut frames: Vec<Vec<f32>> = Vec::new();
    for &x in sig {
        stft.process(x, &mut |spec: &mut [Complex<f32>]| {
            frames.push(spec.iter().map(|c| c.norm()).collect());
        });
    }
    frames
}

/// Pearson correlation of two equal-length vectors.
fn corr(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let ma = a[..n].iter().sum::<f32>() / n as f32;
    let mb = b[..n].iter().sum::<f32>() / n as f32;
    let (mut num, mut da, mut db) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..n {
        let x = a[i] - ma;
        let y = b[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 0.0 || db <= 0.0 {
        return 0.0;
    }
    num / (da.sqrt() * db.sqrt())
}

/// DONE-BAR (1): with every op amount at 0, the wet output equals the latency-delayed dry
/// (residual < −60 dB). Proves per-op exact bypass, not just the mix=0 path.
#[test]
fn all_amounts_zero_nulls_against_delayed_dry() {
    let sr = 48_000.0f32;
    let n = (sr * 1.0) as usize;
    let input = testsig::log_chirp(50.0, 12_000.0, 0.5, n, sr);

    let s = Settings {
        // Every op amount exactly 0; mix fully wet so we test the WET path, not the dry return.
        scramble_amt: 0.0,
        delay_amt: 0.0,
        blur_amt: 0.0,
        stretch_amt: 0.0,
        mix: 1.0,
        ..Settings::default()
    };

    let mut core = SmudgeCore::new(sr);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);

    let lat = core.latency();
    let (mut acc, mut cnt) = (0.0f32, 0usize);
    for i in lat..n {
        let d = out[i] - input[i - lat];
        acc += d * d;
        cnt += 1;
    }
    let resid_db = 20.0 * (acc / cnt as f32).sqrt().max(1.0e-12).log10();
    assert!(
        resid_db < -60.0,
        "all-amounts-0 null was {resid_db:.1} dB (need < -60): per-op bypass not exact"
    );
}

/// Exact bypass survives an aggressive chaos macro: with base amounts 0 but chaos at full
/// depth/rate, the multiplicative amount modulation can never lift a zero, so the null holds.
#[test]
fn bypass_exact_under_full_chaos() {
    let sr = 48_000.0f32;
    let n = (sr * 1.0) as usize;
    let input = testsig::log_chirp(50.0, 12_000.0, 0.5, n, sr);

    let s = Settings {
        scramble_amt: 0.0,
        delay_amt: 0.0,
        blur_amt: 0.0,
        stretch_amt: 0.0,
        chaos_depth: 1.0,
        chaos_rate: 1,
        mix: 1.0,
        ..Settings::default()
    };

    let mut core = SmudgeCore::new(sr);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);

    let lat = core.latency();
    let (mut acc, mut cnt) = (0.0f32, 0usize);
    for i in lat..n {
        let d = out[i] - input[i - lat];
        acc += d * d;
        cnt += 1;
    }
    let resid_db = 20.0 * (acc / cnt as f32).sqrt().max(1.0e-12).log10();
    assert!(
        resid_db < -60.0,
        "chaos lifted a zero amount: null was {resid_db:.1} dB (need < -60)"
    );
}

/// DONE-BAR (2): scramble > 0 decorrelates the spectrum (per-frame magnitude correlation with
/// the dry drops below 0.9) while total energy stays within ±3 dB.
#[test]
fn scramble_decorrelates_but_preserves_energy() {
    let sr = 48_000.0f32;
    let n = (sr * 2.0) as usize;
    // Broadband noise: high per-bin variance so a permutation clearly decorrelates.
    let input = testsig::white_noise(0.4, n, 20_260);

    let s = Settings {
        scramble_amt: 1.0,
        scramble_range: 1.0, // N = MAX_NEIGH
        // Musical redraw rate (held ≥ the 4-frame WOLA overlap) so overlapping frames stay
        // coherent → energy preserved; per-frame (rate=1) is valid "chaos" but drops level
        // ~6 dB via incoherent overlap-add (documented in docs/SMUDGE.md).
        scramble_rate: 6,
        delay_amt: 0.0,
        blur_amt: 0.0,
        stretch_amt: 0.0,
        mix: 1.0,
        ..Settings::default()
    };

    let mut core = SmudgeCore::new(sr);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);
    assert!(out.iter().all(|v| v.is_finite()), "scramble produced NaN/inf");

    let dry_f = mag_frames(&input);
    let wet_f = mag_frames(&out);
    // The wet path is delayed by latency = 4 hops, so wet frame f aligns with dry frame f-4.
    let off = FFT_SIZE / HOP;
    let lo = off + 4;
    let hi = wet_f.len().min(dry_f.len() + off) - 4;
    assert!(hi > lo + 8, "not enough frames to correlate");
    let mut sum = 0.0f32;
    let mut cnt = 0usize;
    for f in lo..hi {
        let c = corr(&wet_f[f], &dry_f[f - off]);
        sum += c;
        cnt += 1;
    }
    let avg = sum / cnt as f32;
    assert!(
        avg < 0.9,
        "scramble did not decorrelate: mean per-frame correlation {avg:.3} (need < 0.9)"
    );

    // Total energy within ±3 dB (a permutation is energy-preserving; WOLA + windowing add
    // only a small, bounded deviation).
    let guard = FFT_SIZE * 2;
    let de = rms_db(&input[guard..n - guard]);
    let we = rms_db(&out[guard..n - guard]);
    assert!(
        (we - de).abs() <= 3.0,
        "scramble energy drifted {:.2} dB (dry {de:.2} → wet {we:.2}), need ±3 dB",
        we - de
    );
}

/// mix = 0 returns the latency-delayed dry input exactly (all internal ops still run, but are
/// not tapped to the output). The dry path is delayed by the reported latency so it aligns
/// with the wet path under host PDC.
#[test]
fn mix_zero_nulls_against_delayed_dry() {
    let sr = 48_000.0f32;
    let n = 48_000usize;
    let dry = testsig::sine(220.0, 0.5, n, sr);

    let s = Settings {
        scramble_amt: 0.8,
        delay_amt: 0.6,
        blur_amt: 0.7,
        stretch_amt: 0.5,
        stretch_factor: 1.5,
        chaos_depth: 0.5,
        mix: 0.0,
        ..Settings::default()
    };

    let mut core = SmudgeCore::new(sr);
    let mut out = dry.clone();
    core.process_mono(&mut out, &s);

    let lat = core.latency();
    let (mut acc, mut cnt) = (0.0f32, 0usize);
    for i in lat..n {
        let d = out[i] - dry[i - lat];
        acc += d * d;
        cnt += 1;
    }
    let resid = 20.0 * (acc / cnt as f32).sqrt().max(1.0e-12).log10();
    assert!(resid < -80.0, "mix=0 did not null vs delayed dry: residual {resid:.1} dB");
}

/// Every op fully engaged, plus full chaos, stays finite and bounded (fuzz-style) over 30 s of
/// noise — exercises delay feedback + stretch + scramble + blur together.
#[test]
fn everything_on_finite_and_bounded() {
    let sr = 48_000.0f32;
    let n = (sr * 30.0) as usize;
    let mut input = vec![0.0f32; n];
    let burst = testsig::white_noise(0.6, (sr * 1.0) as usize, 4242);
    input[..burst.len()].copy_from_slice(&burst);

    let configs = [
        // (scr, range, drate, del, tilt, fb, blur, tau, btilt, str, factor, crate_, cdepth)
        (1.0f32, 1.0f32, 1u32, 1.0f32, 1.0f32, MAX_DELAY_FEEDBACK, 1.0f32, 5.0f32, 1.0f32, 1.0f32, MIN_STRETCH, 1u32, 1.0f32),
        (0.7, 0.5, 8, 0.8, -1.0, 0.9, 0.9, 2000.0, -1.0, 1.0, MAX_STRETCH, 64, 0.8),
        (0.4, 0.2, 4, 1.0, 0.0, 0.95, 0.5, 200.0, 0.0, 0.6, 1.7, 16, 0.5),
    ];

    for (scr, range, drate, del, tilt, fb, blur, tau, btilt, strc, factor, crate_, cdepth) in configs
    {
        let s = Settings {
            scramble_amt: scr,
            scramble_range: range,
            scramble_rate: drate,
            delay_amt: del,
            delay_tilt: tilt,
            delay_feedback: fb,
            blur_amt: blur,
            blur_tau_ms: tau,
            blur_tilt: btilt,
            stretch_amt: strc,
            stretch_factor: factor,
            chaos_rate: crate_,
            chaos_depth: cdepth,
            mix: 1.0,
        };
        let mut core = SmudgeCore::new(sr);
        let mut out = input.clone();
        core.process_mono(&mut out, &s);
        assert!(
            out.iter().all(|v| v.is_finite()),
            "config scr={scr} del={del} produced NaN/inf"
        );
        let pk = peak(&out);
        assert!(pk <= 1.0, "config scr={scr} del={del} peak {pk} exceeded 0 dBFS");
    }
}

/// Delay feedback stays bounded (peak ≤ 0 dBFS, finite) over 30 s at max feedback — the
/// in-loop soft-limiter + fb < 1 guarantee decay to a bounded state.
#[test]
fn delay_feedback_bounded() {
    let sr = 48_000.0f32;
    let n = (sr * 30.0) as usize;
    let mut input = vec![0.0f32; n];
    let burst = testsig::white_noise(0.7, (sr * 0.5) as usize, 777);
    input[..burst.len()].copy_from_slice(&burst);

    let s = Settings {
        delay_amt: 1.0,
        delay_tilt: 0.5,
        delay_feedback: MAX_DELAY_FEEDBACK,
        mix: 1.0,
        ..Settings::default()
    };

    let mut core = SmudgeCore::new(sr);
    let mut out = input;
    core.process_mono(&mut out, &s);
    assert!(out.iter().all(|v| v.is_finite()), "delay produced NaN/inf");
    let pk = peak(&out);
    assert!(pk <= 1.0, "delay feedback peak {pk} exceeded 0 dBFS");
}
