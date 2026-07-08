//! CARVE done-bar + universal DSP tests (PRD §4 + build brief).
//!
//! Done bar (build brief): universal assertions, plus CARVE-specific —
//! (1) main = full-spectrum pink, SC = band-limited (500 Hz–2 kHz) → reduction ONLY in the
//!     SC-active bands (out-of-band within ±1 dB of dry, in-band reduced ≥ depth−3 dB at strong SC);
//! (2) SC silent → exact null vs latency-matched dry (wet path < −60 dB; mix=0 < −80 dB);
//! (3) Δ-listen + normal output sum ≈ dry (energy bookkeeping within 1 dB);
//! (4) pulsed SC → reduction envelope rise/fall times consistent with settings ±50%.

use crate::dsp::*;
use suite_core::dsp::Svf;
use suite_core::stft::{Complex, Stft};
use suite_core::testsig;

// ---------------------------------------------------------------------------
// Signal helpers
// ---------------------------------------------------------------------------

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

/// Band-limited noise: white noise through a cascade of state-variable band-passes centred on
/// `sqrt(f_lo*f_hi)` with Q = centre/bandwidth. Four sections give steep skirts so out-of-band
/// energy is tens of dB down (a clean spectral separation for the "only SC-active bands" bar).
pub fn band_limited_noise(f_lo: f32, f_hi: f32, amp: f32, n: usize, sr: f32, seed: u32) -> Vec<f32> {
    let white = testsig::white_noise(1.0, n, seed);
    let centre = (f_lo * f_hi).sqrt();
    let q = centre / (f_hi - f_lo).max(1.0);
    let mut secs: Vec<Svf> = (0..4)
        .map(|_| {
            let mut s = Svf::new();
            s.set(centre, q, sr);
            s
        })
        .collect();
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        let mut v = white[i];
        for s in secs.iter_mut() {
            v = s.process(v).bp;
        }
        out[i] = v;
    }
    // Normalise to the requested peak amplitude.
    let pk = peak(&out).max(1.0e-9);
    let g = amp / pk;
    for v in out.iter_mut() {
        *v *= g;
    }
    out
}

/// Band-limited noise gated into `pulse_hz` on/off pulses (50% duty) — used to open/close the
/// duck for the render artifacts.
pub fn band_limited_pulses(
    f_lo: f32,
    f_hi: f32,
    amp: f32,
    pulse_hz: f32,
    n: usize,
    sr: f32,
    seed: u32,
) -> Vec<f32> {
    let mut sig = band_limited_noise(f_lo, f_hi, amp, n, sr, seed);
    let period = (sr / pulse_hz.max(0.1)) as usize;
    for i in 0..n {
        let on = period == 0 || (i % period) < period / 2;
        if !on {
            sig[i] = 0.0;
        }
    }
    sig
}

/// Per-1/3-octave-group energy (dB) of a signal, measured with an independent analysis STFT
/// using the same bin→group mapping as the core.
fn group_energy_db(sig: &[f32], sr: f32) -> [f32; N_BANDS] {
    let ln_lo = F_LO.ln();
    let ln_span = F_HI.ln() - ln_lo;
    let nb = FFT_SIZE / 2 + 1;
    let bin_band: Vec<usize> = (0..nb)
        .map(|k| {
            let f = (k as f32 * sr / FFT_SIZE as f32).clamp(F_LO, F_HI);
            let pos = (f.ln() - ln_lo) / ln_span;
            ((pos * N_BANDS as f32) as usize).min(N_BANDS - 1)
        })
        .collect();

    let mut stft = Stft::new(FFT_SIZE, HOP);
    let mut acc = [0.0f64; N_BANDS];
    let mut frames = 0u64;
    for &x in sig {
        stft.process(x, &mut |spec: &mut [Complex<f32>]| {
            frames += 1;
            for (k, c) in spec.iter().enumerate() {
                acc[bin_band[k]] += c.norm_sqr() as f64;
            }
        });
    }
    let mut out = [0.0f32; N_BANDS];
    let f = frames.max(1) as f64;
    for g in 0..N_BANDS {
        out[g] = 10.0 * ((acc[g] / f).max(1.0e-20)).log10() as f32;
    }
    out
}

/// Centre frequency (Hz) of group `g`.
fn group_center_hz(g: usize) -> f32 {
    let ln_lo = F_LO.ln();
    let ln_span = F_HI.ln() - ln_lo;
    let pos = (g as f32 + 0.5) / N_BANDS as f32;
    (ln_lo + pos * ln_span).exp()
}

// ---------------------------------------------------------------------------
// Done-bar tests
// ---------------------------------------------------------------------------

/// DONE-BAR (1): band-limited (500 Hz–2 kHz) sidechain reduces ONLY the main's matching bands.
#[test]
fn reduction_only_in_sidechain_bands() {
    let sr = 48_000.0f32;
    let n = (sr * 2.0) as usize;

    let main = testsig::pink_noise(0.5, n, 4242);
    // Strong, steady band-limited sidechain so the in-band frac saturates to full depth.
    let sc = band_limited_noise(500.0, 2000.0, 0.6, n, sr, 99);

    let max_depth = 18.0f32;
    let s = Settings {
        amount: 1.0,
        max_depth_db: max_depth,
        threshold_db: -60.0,
        tilt: 0.0,
        attack_ms: 5.0,
        release_ms: 80.0,
        sens: 0.85,
        listen: ListenMode::Off,
        mix: 1.0,
        out_gain: 1.0,
        ..Settings::default()
    };

    let mut core = CarveCore::new(sr);
    let mut wet = main.clone();
    core.process_mono(&mut wet, &sc, &s);
    assert!(wet.iter().all(|v| v.is_finite()), "produced NaN/inf");

    // Steady-state window (skip pipeline fill + settle).
    let guard = FFT_SIZE * 4;
    let dry_g = group_energy_db(&main[guard..n - guard], sr);
    let wet_g = group_energy_db(&wet[guard..n - guard], sr);

    let mut in_checked = 0;
    let mut out_checked = 0;
    for g in 0..N_BANDS {
        let fc = group_center_hz(g);
        let delta = wet_g[g] - dry_g[g];
        if (700.0..=1500.0).contains(&fc) {
            // Clearly in-band: must be cut by at least depth − 3 dB.
            assert!(
                delta <= -(max_depth - 3.0),
                "in-band group {g} (~{fc:.0} Hz) cut only {delta:.1} dB (need <= {:.1})",
                -(max_depth - 3.0)
            );
            in_checked += 1;
        } else if fc < 150.0 || fc > 6000.0 {
            // Clearly out-of-band (well clear of the 500 Hz–2 kHz SC + its skirts):
            // within ±1 dB of dry.
            assert!(
                delta.abs() <= 1.0,
                "out-of-band group {g} (~{fc:.0} Hz) moved {delta:.2} dB (need |Δ| <= 1)"
            );
            out_checked += 1;
        }
    }
    assert!(in_checked >= 2 && out_checked >= 3, "not enough bands checked ({in_checked}/{out_checked})");
}

/// DONE-BAR (2a): SC silent, mix=1 → the carved wet path collapses to the STFT identity and
/// nulls against the latency-delayed dry below −60 dB (honest STFT round-trip bound).
#[test]
fn silent_sidechain_wet_nulls_against_delayed_dry() {
    let sr = 48_000.0f32;
    let n = (sr * 1.0) as usize;
    let main = testsig::log_chirp(50.0, 12_000.0, 0.5, n, sr);
    let sc = vec![0.0f32; n];

    let s = Settings {
        amount: 1.0,
        max_depth_db: 24.0,
        threshold_db: -60.0,
        mix: 1.0,
        listen: ListenMode::Off,
        ..Settings::default()
    };

    let mut core = CarveCore::new(sr);
    let mut wet = main.clone();
    core.process_mono(&mut wet, &sc, &s);

    let lat = core.latency();
    let (mut acc, mut cnt) = (0.0f32, 0usize);
    for i in lat..n {
        let d = wet[i] - main[i - lat];
        acc += d * d;
        cnt += 1;
    }
    let resid = 20.0 * (acc / cnt as f32).sqrt().max(1.0e-12).log10();
    assert!(resid < -60.0, "silent-SC wet null was {resid:.1} dB (need < -60)");
}

/// DONE-BAR (2b): mix=0 returns the latency-delayed dry exactly (< −80 dB), even with a loud
/// sidechain driving deep ducking internally.
#[test]
fn mix_zero_nulls_against_delayed_dry() {
    let sr = 48_000.0f32;
    let n = (sr * 1.0) as usize;
    let main = testsig::sine(220.0, 0.5, n, sr);
    let sc = band_limited_noise(200.0, 400.0, 0.6, n, sr, 7);

    let s = Settings {
        amount: 1.0,
        max_depth_db: 24.0,
        threshold_db: -70.0,
        mix: 0.0,
        listen: ListenMode::Off,
        ..Settings::default()
    };

    let mut core = CarveCore::new(sr);
    let mut out = main.clone();
    core.process_mono(&mut out, &sc, &s);

    let lat = core.latency();
    let (mut acc, mut cnt) = (0.0f32, 0usize);
    for i in lat..n {
        let d = out[i] - main[i - lat];
        acc += d * d;
        cnt += 1;
    }
    let resid = 20.0 * (acc / cnt as f32).sqrt().max(1.0e-12).log10();
    assert!(resid < -80.0, "mix=0 null was {resid:.1} dB (need < -80)");
}

/// DONE-BAR (3): Δ-listen output + normal output sum back to the dry (energy within 1 dB).
#[test]
fn delta_plus_normal_sums_to_dry() {
    let sr = 48_000.0f32;
    let n = (sr * 2.0) as usize;
    let main = testsig::pink_noise(0.5, n, 313);
    let sc = band_limited_noise(500.0, 2000.0, 0.6, n, sr, 555);

    let base = Settings {
        amount: 1.0,
        max_depth_db: 18.0,
        threshold_db: -60.0,
        sens: 0.8,
        mix: 1.0,
        out_gain: 1.0,
        ..Settings::default()
    };

    // Normal (carved) output.
    let mut normal = main.clone();
    CarveCore::new(sr).process_mono(&mut normal, &sc, &Settings { listen: ListenMode::Off, ..base });
    // Δ (residual) output.
    let mut delta = main.clone();
    CarveCore::new(sr).process_mono(&mut delta, &sc, &Settings { listen: ListenMode::Delta, ..base });

    // Sum reconstructs the STFT identity of the dry.
    let sum: Vec<f32> = normal.iter().zip(delta.iter()).map(|(a, b)| a + b).collect();

    let lat = FFT_SIZE;
    let guard = FFT_SIZE * 3;
    let dry_win = &main[guard - lat..n - guard - lat];
    let sum_win = &sum[guard..n - guard];
    let de = rms_db(dry_win);
    let se = rms_db(sum_win);
    assert!(
        (se - de).abs() <= 1.0,
        "Δ+normal energy off by {:.2} dB (dry {de:.2} → sum {se:.2}), need within 1 dB",
        se - de
    );
}

/// DONE-BAR (4): pulsed SC → the reduction envelope's rise (attack) and fall (release) times
/// track the settings within ±50%. Measured as the 10%→63% (attack) and 90%→37% (release)
/// intervals of a one-pole step, which isolate the smoothing slope from transport dead-time.
#[test]
fn attack_release_track_settings() {
    let sr = 48_000.0f32;
    let attack_ms = 40.0f32;
    let release_ms = 400.0f32;

    let on = (sr * 0.8) as usize;
    let off = (sr * 1.2) as usize;
    let n = on + off;

    // Main: broadband so it's carve-able; sidechain: band-limited, gated ON for [0,on).
    let main = testsig::pink_noise(0.5, n, 8);
    let sc_full = band_limited_noise(500.0, 2000.0, 0.6, n, sr, 21);
    let mut sc = sc_full.clone();
    for v in sc.iter_mut().skip(on) {
        *v = 0.0;
    }

    let s = Settings {
        amount: 1.0,
        max_depth_db: 18.0,
        threshold_db: -70.0,
        sens: 0.9,
        attack_ms,
        release_ms,
        listen: ListenMode::Off,
        mix: 1.0,
        out_gain: 1.0,
        ..Settings::default()
    };

    let mut core = CarveCore::new(sr);
    core.configure(&s);
    let mut red = vec![0.0f32; n];
    for i in 0..n {
        let _ = core.process_sample(main[i], main[i], sc[i], s.mix, s.out_gain);
        red[i] = core.max_reduction_db();
    }

    // Peak reduction reached during the ON phase.
    let peak_red = red[..on].iter().fold(0.0f32, |m, &v| m.max(v));
    assert!(peak_red > 6.0, "duck never engaged (peak {peak_red:.1} dB)");

    // --- Attack: 10% -> 63% of peak within the ON phase. ---
    let t10 = red[..on].iter().position(|&v| v >= 0.10 * peak_red).unwrap();
    let t63 = red[..on].iter().position(|&v| v >= 0.632 * peak_red).unwrap();
    let attack_meas = (t63 - t10) as f32 / sr * 1000.0;
    let attack_expect = 0.9 * attack_ms; // one-pole 10%->63% interval
    assert!(
        attack_meas >= 0.5 * attack_expect && attack_meas <= 1.5 * attack_expect,
        "attack {attack_meas:.1} ms vs expected {attack_expect:.1} ms (need within ±50%)"
    );

    // --- Release: 90% -> 37% of peak after SC-off. ---
    let t90 = on + red[on..].iter().position(|&v| v <= 0.90 * peak_red).unwrap();
    let t37 = on + red[on..].iter().position(|&v| v <= 0.37 * peak_red).unwrap();
    let release_meas = (t37 - t90) as f32 / sr * 1000.0;
    let release_expect = 0.9 * release_ms;
    assert!(
        release_meas >= 0.5 * release_expect && release_meas <= 1.5 * release_expect,
        "release {release_meas:.1} ms vs expected {release_expect:.1} ms (need within ±50%)"
    );
}

/// Tilt biases depth toward one end: with tilt < 0 the low bands are cut more than the highs
/// for the same broadband sidechain, and vice-versa.
#[test]
fn tilt_biases_depth() {
    let sr = 48_000.0f32;
    let n = (sr * 1.5) as usize;
    let main = testsig::pink_noise(0.5, n, 12);
    // Broadband sidechain so every band is a candidate; tilt decides where the cut lands.
    let sc = testsig::pink_noise(0.6, n, 34);

    let base = Settings {
        amount: 1.0,
        max_depth_db: 18.0,
        threshold_db: -70.0,
        sens: 0.6,
        attack_ms: 5.0,
        release_ms: 80.0,
        mix: 1.0,
        ..Settings::default()
    };

    let measure = |tilt: f32| -> (f32, f32) {
        let mut core = CarveCore::new(sr);
        let mut wet = main.clone();
        core.process_mono(&mut wet, &sc, &Settings { tilt, ..base });
        let guard = FFT_SIZE * 4;
        let dry_g = group_energy_db(&main[guard..n - guard], sr);
        let wet_g = group_energy_db(&wet[guard..n - guard], sr);
        // Average reduction over a low region (~80-200 Hz) and a high region (~4-10 kHz).
        let mut lo = (0.0f32, 0);
        let mut hi = (0.0f32, 0);
        for g in 0..N_BANDS {
            let fc = group_center_hz(g);
            let d = dry_g[g] - wet_g[g]; // reduction (positive)
            if (80.0..=200.0).contains(&fc) {
                lo.0 += d;
                lo.1 += 1;
            } else if (4000.0..=10000.0).contains(&fc) {
                hi.0 += d;
                hi.1 += 1;
            }
        }
        (lo.0 / lo.1.max(1) as f32, hi.0 / hi.1.max(1) as f32)
    };

    let (lo_neg, hi_neg) = measure(-0.8);
    let (lo_pos, hi_pos) = measure(0.8);
    assert!(
        lo_neg > hi_neg + 2.0,
        "tilt<0 should cut lows more: lo {lo_neg:.1} vs hi {hi_neg:.1} dB"
    );
    assert!(
        hi_pos > lo_pos + 2.0,
        "tilt>0 should cut highs more: hi {hi_pos:.1} vs lo {lo_pos:.1} dB"
    );
}

/// Fuzz: extreme settings over broadband noise stay finite and ≤ 0 dBFS.
#[test]
fn extremes_finite_and_bounded() {
    let sr = 48_000.0f32;
    let n = (sr * 5.0) as usize;
    let main = testsig::white_noise(0.8, n, 1);
    let sc = testsig::white_noise(0.9, n, 2);

    let configs = [
        (1.0f32, 24.0f32, -90.0f32, 1.0f32, 1.0f32, 20.0f32, 1.0f32),
        (1.0, 24.0, 0.0, -1.0, 50.0, 500.0, 0.0),
        (0.5, 12.0, -45.0, 0.5, 10.0, 200.0, 0.5),
    ];
    for (amount, md, th, tilt, atk, rel, sens) in configs {
        let s = Settings {
            amount,
            max_depth_db: md,
            threshold_db: th,
            tilt,
            attack_ms: atk,
            release_ms: rel,
            sens,
            listen: ListenMode::Off,
            mix: 1.0,
            out_gain: 1.0,
        };
        let mut core = CarveCore::new(sr);
        let mut out = main.clone();
        core.process_mono(&mut out, &sc, &s);
        assert!(out.iter().all(|v| v.is_finite()), "config amount={amount} md={md} NaN/inf");
        let pk = peak(&out);
        assert!(pk <= 1.0, "config amount={amount} md={md} peak {pk} > 0 dBFS");
    }
}
