//! VOXFIT done-bar + render tests (PRD §4 universal + VOXFIT-specific).
//!
//! Done bars (SPECS / build brief):
//! 1. Formant +3 st → avg-log-envelope peak positions move by ~2^(3/12) (±10%) while the measured
//!    f0 stays within ±10 cents (pitch-independent formant shift). Uses the VOXKEY cepstral
//!    helpers with a LOW f0 (≤150 Hz) so the n/16 lifter cutoff sits below the pitch quefrency.
//! 2. De-esser: synthetic sibilant bursts (HP-filtered noise riding a 150 Hz vowel tone) → 5–9 kHz
//!    band energy reduced (more with stronger amount) while the vowel band (<2 kHz) stays ±1 dB.
//! 3. Tilt at max dark → the spectral slope measurably tilts (chirp render, band energies).
//! Plus the universal `mix=0` null (in dsp.rs) and per-preset universal render assertions.

use crate::dsp::{Controls, Settings, VoxFitCore, MAIN_FFT};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::dsp::Svf;
use suite_core::harness::{assert_universal, render_path, write_wav};
use suite_core::pitch::{cents, Mpm};
use suite_core::presets::load_all;
use suite_core::testsig::{log_chirp, synth_vocal, white_noise};

const SR: f32 = 48_000.0;

/// Run a mono buffer through a fresh core with fixed settings, returning the wet output.
fn render(input: &[f32], s: &Settings) -> Vec<f32> {
    let mut core = VoxFitCore::new(SR);
    let mut buf = input.to_vec();
    core.process_mono(&mut buf, s);
    buf
}

/// RMS energy of `sig` inside the band [`lo`,`hi`] Hz (SVF high-pass → low-pass, 12 dB/oct skirts).
fn band_rms(sig: &[f32], lo: f32, hi: f32) -> f32 {
    let mut hp = Svf::new();
    let mut lp = Svf::new();
    hp.set(lo.clamp(1.0, SR * 0.49), 0.707, SR);
    lp.set(hi.clamp(1.0, SR * 0.49), 0.707, SR);
    let mut acc = 0.0f64;
    let mut n = 0usize;
    // Skip the 2048-sample latency + a settle margin.
    let start = (MAIN_FFT + (0.05 * SR) as usize).min(sig.len());
    for &x in &sig[start..] {
        let b = lp.process(hp.process(x).hp).lp;
        acc += (b * b) as f64;
        n += 1;
    }
    if n == 0 {
        return 0.0;
    }
    (acc / n as f64).sqrt() as f32
}

/// RMS of the loudest `frac` fraction of band samples — isolates the sibilant *bursts* (the
/// de-esser only acts when the band envelope is over threshold, so the quiet inter-burst spans
/// would otherwise dilute the measured reduction). SPECS asks for the band energy "during bursts".
fn band_rms_loud(sig: &[f32], lo: f32, hi: f32, frac: f32) -> f32 {
    let mut hp = Svf::new();
    let mut lp = Svf::new();
    hp.set(lo.clamp(1.0, SR * 0.49), 0.707, SR);
    lp.set(hi.clamp(1.0, SR * 0.49), 0.707, SR);
    let start = (MAIN_FFT + (0.05 * SR) as usize).min(sig.len());
    let mut sq: Vec<f32> = sig[start..].iter().map(|&x| {
        let b = lp.process(hp.process(x).hp).lp;
        b * b
    }).collect();
    if sq.is_empty() {
        return 0.0;
    }
    sq.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let n = ((sq.len() as f32 * frac) as usize).max(1);
    let acc: f64 = sq[..n].iter().map(|&v| v as f64).sum();
    (acc / n as f64).sqrt() as f32
}

/// Low-band RMS (below `hi` Hz) via a single SVF low-pass.
fn low_rms(sig: &[f32], hi: f32) -> f32 {
    let mut lp = Svf::new();
    lp.set(hi.clamp(1.0, SR * 0.49), 0.707, SR);
    let mut acc = 0.0f64;
    let mut n = 0usize;
    let start = (MAIN_FFT + (0.05 * SR) as usize).min(sig.len());
    for &x in &sig[start..] {
        let y = lp.process(x).lp;
        acc += (y * y) as f64;
        n += 1;
    }
    if n == 0 {
        return 0.0;
    }
    (acc / n as f64).sqrt() as f32
}

#[inline]
fn to_db(x: f32) -> f32 {
    20.0 * x.max(1.0e-12).log10()
}

// ---------------------------------------------------------------------------
// Cepstral spectral-envelope helpers (mirrors the VOXKEY / suite_core::shift technique)
// ---------------------------------------------------------------------------

/// A frame-averaged, cepstrally-smoothed log-magnitude spectral envelope. Welch averaging blurs
/// the harmonic comb so the cepstral lift recovers the formants. Own FFT (independent of engine).
fn avg_log_env(sig: &[f32], n: usize) -> Vec<f32> {
    use realfft::RealFftPlanner;
    let nbins = n / 2 + 1;
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);
    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
        .collect();
    let hop = n / 4;
    let lo = sig.len() / 3;
    let hi = 2 * sig.len() / 3;
    let mut acc = vec![0.0f64; nbins];
    let mut frames = 0usize;
    let mut buf = vec![0.0f32; n];
    let mut spec = fwd.make_output_vec();
    let mut start = lo;
    while start + n <= hi {
        for i in 0..n {
            buf[i] = sig[start + i] * window[i];
        }
        fwd.process(&mut buf, &mut spec).unwrap();
        for k in 0..nbins {
            acc[k] += (spec[k].norm() as f64).max(1e-9);
        }
        frames += 1;
        start += hop;
    }
    assert!(frames > 0, "signal too short for averaged envelope");
    let mut logspec = inv.make_input_vec();
    for k in 0..nbins {
        logspec[k].re = ((acc[k] / frames as f64) as f32).max(1e-7).ln();
        logspec[k].im = 0.0;
    }
    let mut ceps = vec![0.0f32; n];
    inv.process(&mut logspec, &mut ceps).unwrap();
    let l = (n / 16).max(4);
    for q in (l + 1)..(n - l) {
        ceps[q] = 0.0;
    }
    let mut envspec = fwd.make_output_vec();
    fwd.process(&mut ceps, &mut envspec).unwrap();
    (0..nbins).map(|k| envspec[k].re / n as f32).collect()
}

/// Best global formant-shift ratio mapping `dry`'s envelope onto `wet`'s (log-freq
/// cross-correlation of the two log-envelopes — robust to the harmonic comb).
fn formant_shift_ratio(dry: &[f32], wet: &[f32], n: usize) -> f32 {
    let ed = avg_log_env(dry, n);
    let ew = avg_log_env(wet, n);
    let nbins = ed.len();
    let bin_hz = SR / n as f32;
    let (f_lo, f_hi, m) = (250.0f32, 4000.0f32, 400usize);
    let dlog = (f_hi / f_lo).ln() / (m as f32 - 1.0);
    let sample = |env: &[f32], f: f32| -> f32 {
        let pos = f / bin_hz;
        let i = pos.floor() as usize;
        if i >= nbins - 1 {
            env[nbins - 1]
        } else {
            let fr = pos - i as f32;
            env[i] * (1.0 - fr) + env[i + 1] * fr
        }
    };
    let grid = |env: &[f32]| -> Vec<f32> {
        let raw: Vec<f32> = (0..m).map(|j| sample(env, f_lo * (j as f32 * dlog).exp())).collect();
        let mean = raw.iter().sum::<f32>() / m as f32;
        raw.into_iter().map(|v| v - mean).collect()
    };
    let (gd, gw) = (grid(&ed), grid(&ew));
    let max_shift = (10.0f32 / 12.0 * 2.0f32.ln() / dlog).ceil() as isize;
    let mut best = 0isize;
    let mut best_corr = f32::NEG_INFINITY;
    for s in -max_shift..=max_shift {
        let mut c = 0.0f32;
        for j in 0..m {
            let jj = j as isize + s;
            if jj >= 0 && (jj as usize) < m {
                c += gd[j] * gw[jj as usize];
            }
        }
        if c > best_corr {
            best_corr = c;
            best = s;
        }
    }
    (best as f32 * dlog).exp()
}

fn measure_mid_f0(sig: &[f32]) -> f32 {
    let win = 4096.min(sig.len());
    let start = (sig.len().saturating_sub(win)) / 2;
    let mut mpm = Mpm::new(win, SR, 60.0, 800.0);
    mpm.analyze(&sig[start..start + win]).f0_hz
}

// ---------------------------------------------------------------------------
// (1) Formant shift +3 st moves the envelope, keeps f0
// ---------------------------------------------------------------------------

#[test]
fn formant_shift_moves_envelope_keeps_pitch() {
    // 145 Hz vocal — low enough that the cepstral lifter cutoff n/16=256 sits below the
    // pitch-period quefrency sr/f0≈331, so the envelope measure isn't polluted by harmonics.
    let f0 = 145.0f32;
    let dry = synth_vocal(f0, (SR * 1.6) as usize, SR);
    let mut c = Controls::default();
    c.formant_st = 3.0; // +3 st formant, everything else neutral, pure wet
    let s = c.resolve();
    let wet = render(&dry, &s);
    assert_universal(&wet);

    // f0 barely moves (pitch-independent shift).
    let f0_dry = measure_mid_f0(&dry);
    let f0_wet = measure_mid_f0(&wet);
    let err = cents(f0_wet, f0_dry).abs();
    assert!(err < 10.0, "f0 moved {err:.1} cents on a formant-only shift (want ≤10)");

    // Envelope peaks move up by ~2^(3/12) = 1.189× (±10%).
    let expected = 2.0f32.powf(3.0 / 12.0);
    let ratio = formant_shift_ratio(&dry, &wet, 4096);
    let rel = (ratio - expected).abs() / expected;
    assert!(
        rel < 0.10,
        "formant envelope shifted {ratio:.3}× (expected {expected:.3}, {:.0}% off)",
        rel * 100.0
    );
}

// ---------------------------------------------------------------------------
// (2) De-esser reduces the 5–9 kHz band, leaves the vowel band alone
// ---------------------------------------------------------------------------

/// A 150 Hz vowel tone with HP-filtered noise sibilant bursts riding on top (esses).
fn sibilant_vocal() -> Vec<f32> {
    let len = (SR * 1.6) as usize;
    let vowel = synth_vocal(150.0, len, SR);
    let noise = white_noise(1.0, len, 0x51B1);
    // High-pass the noise > 5 kHz so it lands in the sibilant band.
    let mut hp = Svf::new();
    hp.set(6000.0, 0.707, SR);
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / SR;
        // Four 80 ms sibilant bursts.
        let phase = (t * 2.5).fract();
        let gate = if phase < 0.13 { 1.0 } else { 0.0 };
        let sib = hp.process(noise[i]).hp * 0.5 * gate;
        out.push((vowel[i] * 0.7 + sib).clamp(-0.98, 0.98));
    }
    out
}

#[test]
fn deesser_reduces_sibilant_band_only() {
    let input = sibilant_vocal();

    let off = render(&input, &{
        let mut c = Controls::default();
        c.deess_amount = 0.0;
        c.resolve()
    });
    let mild = render(&input, &{
        let mut c = Controls::default();
        c.deess_thresh_db = -34.0;
        c.deess_amount = 0.4;
        c.resolve()
    });
    let strong = render(&input, &{
        let mut c = Controls::default();
        c.deess_thresh_db = -34.0;
        c.deess_amount = 0.9;
        c.resolve()
    });
    assert_universal(&off);
    assert_universal(&strong);

    // 5–9 kHz band energy DURING bursts (loudest 15% of band samples): reduced by the de-esser,
    // more with stronger amount.
    let sib_off = to_db(band_rms_loud(&off, 5000.0, 9000.0, 0.15));
    let sib_mild = to_db(band_rms_loud(&mild, 5000.0, 9000.0, 0.15));
    let sib_strong = to_db(band_rms_loud(&strong, 5000.0, 9000.0, 0.15));
    assert!(
        sib_strong < sib_off - 3.0,
        "de-esser did not reduce the 5–9 kHz band (off {sib_off:.1} dB → strong {sib_strong:.1} dB)"
    );
    assert!(
        sib_strong < sib_mild - 1.0,
        "stronger de-ess amount did not reduce more (mild {sib_mild:.1} → strong {sib_strong:.1} dB)"
    );

    // Vowel band (<2 kHz) is essentially untouched (±1 dB): only the shared formant-shift stage
    // acts on it in both renders, so this isolates the de-ess side-effect on the vowel.
    let vow_off = to_db(low_rms(&off, 2000.0));
    let vow_strong = to_db(low_rms(&strong, 2000.0));
    assert!(
        (vow_strong - vow_off).abs() < 1.0,
        "de-esser disturbed the <2 kHz vowel band by {:.2} dB (want ≤1)",
        (vow_strong - vow_off).abs()
    );
}

// ---------------------------------------------------------------------------
// (3) Tilt at max dark tilts the spectrum
// ---------------------------------------------------------------------------

#[test]
fn tilt_dark_tilts_spectrum() {
    let chirp = log_chirp(20.0, 20_000.0, 0.5, (SR * 2.0) as usize, SR);

    let flat = render(&chirp, &Controls::default().resolve());
    let dark = render(&chirp, &{
        let mut c = Controls::default();
        c.tilt_db = -6.0; // max dark: boost lows, cut highs
        c.resolve()
    });
    assert_universal(&flat);
    assert_universal(&dark);

    let ratio = |sig: &[f32]| to_db(low_rms(sig, 500.0)) - to_db(band_rms(sig, 4000.0, 12000.0));
    let flat_bal = ratio(&flat);
    let dark_bal = ratio(&dark);
    // Dark tilt should raise the low/high balance by clearly more than the shelf pair (~+8 dB).
    assert!(
        dark_bal > flat_bal + 6.0,
        "dark tilt did not tilt the spectrum (flat low−high {flat_bal:.1} dB, dark {dark_bal:.1} dB)"
    );
}

// ---------------------------------------------------------------------------
// Preset renders
// ---------------------------------------------------------------------------

#[test]
fn every_preset_renders_and_passes_universal() {
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6, "need ≥ 6 presets, got {}", presets.len());
    // A sung phrase (four notes) with a sibilant tail, then a breath of silence.
    let mut input = Vec::new();
    for &f in &[220.0f32, 262.0, 175.0, 330.0] {
        input.extend_from_slice(&synth_vocal(f, (SR * 0.5) as usize, SR));
    }
    input.extend_from_slice(&vec![0.0f32; (SR * 0.4) as usize]);

    for p in &presets {
        let s = settings_from_preset(p);
        let buf = render(&input, &s);
        assert_universal(&buf);
        let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
        write_wav(&render_path("VOXFIT", &fname), &buf, SR as u32).expect("write render");
    }
}

/// A showcase render: the SIT macro dropping a bright vocal into a dark mix.
#[test]
fn showcase_render() {
    let mut input = Vec::new();
    for &f in &[233.0f32, 277.0, 208.0, 311.0] {
        input.extend_from_slice(&synth_vocal(f, (SR * 0.4) as usize, SR));
    }
    let s = {
        let mut c = Controls::default();
        c.sit = 0.85;
        c.resolve()
    };
    let buf = render(&input, &s);
    assert_universal(&buf);
    write_wav(&render_path("VOXFIT", "showcase_sit"), &buf, SR as u32).unwrap();
}
