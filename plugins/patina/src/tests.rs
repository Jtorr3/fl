//! PATINA offline done-bar + universal tests (PRD §4). The pure `dsp::PatinaCore` is the
//! shipped path; these drive its mono harness path through `render_offline`.
//!
//! Done-bars (PATINA-specific, PRD §4):
//!   1. wow: 1 kHz sine → f0 modulation at ~0.4 Hz measurable (demod phase track peaks near
//!      0.4 Hz), amplitude scales with wow depth.
//!   2. noise keying: output RMS in a band with no input energy rises with the input envelope
//!      at key=1 and stays constant at key=0.
//!   3. dropouts: high rate/depth → windowed RMS dips well below baseline, edges click-free.
//!   4. age 0 + all sections 0 → null vs latency-matched dry; age monotonically raises a
//!      composite degradation metric (THD + noise floor + f0-mod depth).

use crate::dsp::{PatinaCore, Settings, LATENCY};
use suite_core::dsp::Svf;
use suite_core::harness::{
    assert_universal, has_nan_or_inf, null_residual_db, render_and_write, render_offline,
};
use suite_core::testsig;
use std::f32::consts::{PI, TAU};

const SR: f32 = 48_000.0;

#[test]
fn manual_covers_all_params_and_has_recipes() {
    suite_core::manual::assert_manual_covers_params(crate::MANUAL_DOC, &crate::PatinaParams::default());
}

/// A fully-neutral base setting (every section identity).
fn base() -> Settings {
    Settings {
        key_amount: 1.0,
        mix: 1.0,
        ..Settings::default()
    }
}

fn render_with(s: Settings, input: &[f32]) -> Vec<f32> {
    let mut core = PatinaCore::new(SR);
    core.configure(&s);
    render_offline(core, input, 512)
}

fn secs(t: f32) -> usize {
    (t * SR) as usize
}

// ---------------------------------------------------------------------------
// Measurement helpers
// ---------------------------------------------------------------------------

/// One-pole lowpass (measurement only).
fn lp1(x: &[f32], cutoff: f32) -> Vec<f32> {
    let a = (-2.0 * PI * cutoff / SR).exp();
    let mut z = 0.0f32;
    x.iter()
        .map(|&v| {
            z = v * (1.0 - a) + a * z;
            z
        })
        .collect()
}

/// Quadrature-demodulate a carrier and return the baseband phase track (radians), decimated to
/// `track_sr` Hz. Captures the wow/flutter pitch modulation as phase(t).
fn phase_track(sig: &[f32], carrier: f32, track_sr: f32) -> (Vec<f32>, f32) {
    let n = sig.len();
    let mut i = vec![0.0f32; n];
    let mut q = vec![0.0f32; n];
    for k in 0..n {
        let ph = TAU * carrier * k as f32 / SR;
        i[k] = 2.0 * sig[k] * ph.cos();
        q[k] = -2.0 * sig[k] * ph.sin();
    }
    let il = lp1(&i, 40.0);
    let ql = lp1(&q, 40.0);
    // Baseband phase, unwrapped.
    let mut phase = vec![0.0f32; n];
    let mut prev = 0.0f32;
    let mut acc = 0.0f32;
    for k in 0..n {
        let raw = ql[k].atan2(il[k]);
        let mut d = raw - prev;
        while d > PI {
            d -= TAU;
        }
        while d < -PI {
            d += TAU;
        }
        acc += d;
        phase[k] = acc;
        prev = raw;
    }
    // Decimate to track_sr.
    let step = (SR / track_sr).round().max(1.0) as usize;
    let dec: Vec<f32> = phase.iter().step_by(step).copied().collect();
    (dec, SR / step as f32)
}

/// Goertzel power at frequency `f` over `x` sampled at `fs`.
fn goertzel(x: &[f32], f: f32, fs: f32) -> f32 {
    let w = TAU * f / fs;
    let cw = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + cw * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - cw * s1 * s2
}

/// Remove the mean from a slice (detrend DC before spectral analysis).
fn demean(x: &[f32]) -> Vec<f32> {
    let m = x.iter().sum::<f32>() / x.len().max(1) as f32;
    x.iter().map(|&v| v - m).collect()
}

/// Highpass a signal steeply (2× cascaded SVF HP) for band-limited energy measurement.
fn highpass(x: &[f32], fc: f32) -> Vec<f32> {
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let mut a = Svf::new();
    let mut b = Svf::new();
    a.set(fc, q, SR);
    b.set(fc, q, SR);
    x.iter().map(|&v| b.process(a.process(v).hp).hp).collect()
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| (v * v) as f64).sum::<f64>() / x.len() as f64).sqrt() as f32
}

// ---------------------------------------------------------------------------
// Latency alignment (guards the FracDelay write-then-read off-by-one)
// ---------------------------------------------------------------------------

#[test]
fn neutral_is_a_pure_latency_delay() {
    // An impulse through the all-neutral core reappears exactly LATENCY samples later.
    let mut input = vec![0.0f32; 4096];
    input[0] = 0.5; // below the 0.999 ceiling so the delayed impulse is unscaled
    let out = render_with(base(), &input);
    let (peak_idx, peak) = out
        .iter()
        .enumerate()
        .fold((0usize, 0.0f32), |(bi, bv), (i, &v)| {
            if v.abs() > bv {
                (i, v.abs())
            } else {
                (bi, bv)
            }
        });
    assert_eq!(peak_idx, LATENCY, "neutral latency peak at {peak_idx}, expected {LATENCY}");
    assert!((peak - 0.5).abs() < 1e-6, "neutral impulse not a pure delay: {peak}");
}

// ---------------------------------------------------------------------------
// Done-bar 1 — wow produces f0 modulation at ~0.4 Hz, scaling with depth
// ---------------------------------------------------------------------------

fn wow_mod_amplitude(wow_depth: f32) -> (f32, f32) {
    // 12 s of 1 kHz sine, wow only (flutter/sat/noise off).
    let dur = secs(12.0);
    let sig = testsig::sine(1000.0, 0.5, dur, SR);
    let mut s = base();
    s.wow_depth = wow_depth;
    let out = render_with(s, &sig);
    // Skip the first second (LFO/demod settling).
    let start = secs(1.0);
    let (track, tsr) = phase_track(&out[start..], 1000.0, 200.0);
    let track = demean(&track);
    // Scan wow candidate frequencies; find the dominant.
    let mut best_f = 0.0f32;
    let mut best_p = 0.0f32;
    let mut f = 0.1f32;
    while f <= 1.2 {
        let p = goertzel(&track, f, tsr);
        if p > best_p {
            best_p = p;
            best_f = f;
        }
        f += 0.05;
    }
    // Amplitude proxy = Goertzel power at the exact wow rate.
    (best_f, goertzel(&track, 0.4, tsr))
}

#[test]
fn wow_modulates_f0_at_point_four_hz() {
    let (peak_f, _) = wow_mod_amplitude(0.6);
    assert!(
        (peak_f - 0.4).abs() <= 0.1,
        "wow f0-modulation should peak near 0.4 Hz; peak at {peak_f:.2} Hz"
    );
}

#[test]
fn wow_depth_scales_modulation() {
    let (_, p_small) = wow_mod_amplitude(0.25);
    let (_, p_large) = wow_mod_amplitude(0.9);
    assert!(
        p_large > p_small * 3.0,
        "deeper wow must modulate f0 more: small {p_small:.3e} vs large {p_large:.3e}"
    );
}

// ---------------------------------------------------------------------------
// Done-bar 2 — keyed noise floor rises with input env at key=1, constant at key=0
// ---------------------------------------------------------------------------

/// 8 s of 200 Hz sine bursts (0.5 s on / 0.5 s off). Returns the signal and the burst/silence
/// window centers.
fn burst_train() -> (Vec<f32>, Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let total = secs(8.0);
    let mut sig = vec![0.0f32; total];
    let mut bursts = Vec::new();
    let mut silences = Vec::new();
    let seg = secs(0.5);
    let mut t = 0usize;
    let mut on = true;
    while t + seg <= total {
        if on {
            for i in 0..seg {
                sig[t + i] = 0.3 * (TAU * 200.0 * (t + i) as f32 / SR).sin();
            }
            // Measure the settled middle 200 ms of the burst.
            bursts.push((t + secs(0.25), t + secs(0.45)));
        } else {
            // Measure late in the silence so the keyed envelope has released.
            silences.push((t + secs(0.35), t + secs(0.49)));
        }
        t += seg;
        on = !on;
    }
    (sig, bursts, silences)
}

fn keyed_hf_ratio(key: f32) -> (f32, f32) {
    let (sig, bursts, silences) = burst_train();
    let mut s = base();
    s.hiss = 0.5;
    s.key_amount = key;
    let out = render_with(s, &sig);
    let hf = highpass(&out, 3000.0);
    let mean_win = |wins: &[(usize, usize)]| -> f32 {
        let v: Vec<f32> = wins.iter().map(|&(a, b)| rms(&hf[a..b.min(hf.len())])).collect();
        v.iter().sum::<f32>() / v.len().max(1) as f32
    };
    (mean_win(&bursts), mean_win(&silences))
}

#[test]
fn keyed_noise_tracks_input_at_key_one() {
    let (burst, silence) = keyed_hf_ratio(1.0);
    assert!(
        burst > silence * 3.0,
        "at key=1 hiss should follow the input: burst HF {burst:.4} vs silence {silence:.4}"
    );
}

#[test]
fn keyed_noise_constant_at_key_zero() {
    let (burst, silence) = keyed_hf_ratio(0.0);
    let ratio = burst / silence.max(1e-9);
    assert!(
        (0.5..2.0).contains(&ratio),
        "at key=0 the hiss floor must be constant: burst/silence ratio {ratio:.2}"
    );
}

// ---------------------------------------------------------------------------
// Done-bar 3 — dropouts dip the windowed RMS and stay click-free
// ---------------------------------------------------------------------------

#[test]
fn dropouts_dip_rms_and_are_click_free() {
    let sig = testsig::pink_noise(0.4, secs(5.0), 0x7A5E);
    let mut s = base();
    s.dropout_rate = 1.0;
    s.dropout_depth = 0.9;
    let out = render_with(s, &sig);

    // Windowed RMS (30 ms windows) — the deepest window must be well below the baseline.
    let win = secs(0.030);
    let mut rmss: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + win <= out.len() {
        rmss.push(rms(&out[i..i + win]));
        i += win;
    }
    let mut sorted = rmss.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let min = sorted[0];
    assert!(
        min < 0.45 * median,
        "high rate/depth dropouts should dip RMS: min window {min:.4} vs median {median:.4}"
    );

    // Click-free: the max output sample-delta must not exceed the input's own by much (the
    // 8 ms-smoothed gain edge adds negligible slope).
    let in_dmax = sig.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    let out_dmax = out.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    assert!(
        out_dmax <= in_dmax * 1.25 + 1e-4,
        "dropout edges click: out Δ {out_dmax:.4} vs in Δ {in_dmax:.4}"
    );
}

// ---------------------------------------------------------------------------
// Done-bar 4a — neutral (age 0 + all sections 0) nulls vs latency-matched dry
// ---------------------------------------------------------------------------

fn null_vs_delayed_dry(out: &[f32], input: &[f32], latency: usize) -> f32 {
    // Compare out[latency..] against input[..] (aligned), over the overlapping region.
    let n = (out.len() - latency).min(input.len());
    let a = &out[latency..latency + n];
    null_residual_db(a, &input[..n])
}

#[test]
fn neutral_nulls_against_latency_matched_dry() {
    let mut sig = testsig::pink_noise(0.4, secs(1.0), 0x1234);
    let chirp = testsig::log_chirp(20.0, 20_000.0, 0.4, secs(1.0), SR);
    for (s, c) in sig.iter_mut().zip(chirp.iter()) {
        *s = (*s + *c) * 0.5;
    }
    let out = render_with(base(), &sig);
    let residual = null_vs_delayed_dry(&out, &sig, LATENCY);
    assert!(
        residual < -120.0,
        "neutral must null vs latency-matched dry; residual {residual:.2} dB"
    );
}

#[test]
fn mix_zero_nulls_against_latency_matched_dry() {
    let sig = testsig::pink_noise(0.4, secs(1.0), 0x9999);
    // Extreme character everywhere, but mix=0 returns the latency-matched dry.
    let mut s = Settings {
        wow_depth: 0.9,
        flutter: 0.9,
        sat_drive: 0.8,
        bump_amount: 0.6,
        azimuth: 0.7,
        dropout_rate: 0.8,
        dropout_depth: 0.8,
        hiss: 0.6,
        hum: 0.5,
        crackle: 0.7,
        age: 0.9,
        ..base()
    };
    s.mix = 0.0;
    let out = render_with(s, &sig);
    let residual = null_vs_delayed_dry(&out, &sig, LATENCY);
    assert!(residual < -120.0, "mix=0 must null vs latency-matched dry; residual {residual:.2} dB");
}

// ---------------------------------------------------------------------------
// Done-bar 4b — AGE monotonically increases a composite degradation metric
// ---------------------------------------------------------------------------

/// Composite degradation of a 1 kHz sine render: THD + broadband noise floor + wow f0-mod.
fn degradation(age: f32) -> f32 {
    let dur = secs(12.0);
    let sig = testsig::sine(1000.0, 0.5, dur, SR);
    let mut s = base();
    s.age = age; // base sections all 0 → age drives everything
    let out = render_with(s, &sig);
    let start = secs(1.0);
    let body = &out[start..];

    // THD: harmonics 2..6 vs the 1 kHz fundamental.
    let fund = goertzel(body, 1000.0, SR).max(1e-20);
    let mut harm = 0.0f32;
    for h in 2..=6 {
        harm += goertzel(body, 1000.0 * h as f32, SR);
    }
    let thd = (harm / fund).sqrt();

    // Noise floor: RMS of the >6 kHz band (hiss/crackle live here; the sine does not).
    let hf = highpass(body, 6000.0);
    let floor = rms(&hf);

    // Wow f0-mod depth: phase-track power at 0.4 Hz.
    let (track, tsr) = phase_track(body, 1000.0, 200.0);
    let track = demean(&track);
    let wow = goertzel(&track, 0.4, tsr).sqrt();

    // Normalize the three to comparable scales and sum.
    thd * 4.0 + floor * 20.0 + wow * 0.2
}

#[test]
fn age_monotonically_increases_degradation() {
    let d0 = degradation(0.0);
    let d1 = degradation(0.33);
    let d2 = degradation(0.66);
    let d3 = degradation(1.0);
    assert!(
        d0 < d1 && d1 < d2 && d2 < d3,
        "AGE must monotonically degrade: {d0:.4} < {d1:.4} < {d2:.4} < {d3:.4}"
    );
    // age=1 must add substantial degradation over the age=0 measurement floor (the tiny d0 is
    // dominated by single-bin Goertzel spectral leakage, not real distortion — the null test
    // proves age 0 + sections 0 is bit-exact clean).
    assert!(d3 > d0 * 4.0, "AGE range too small: age0 {d0:.4} vs age1 {d3:.4}");
}

// ---------------------------------------------------------------------------
// Universal assertions on every preset render (+ write to renders/PATINA/)
// ---------------------------------------------------------------------------

#[test]
fn presets_pass_universal_assertions() {
    // A musical-ish stereo-summed source: pink + a mid chirp so every section has something
    // to act on.
    let mut sig = testsig::pink_noise(0.3, secs(3.0), 0x5151);
    let chirp = testsig::log_chirp(80.0, 8000.0, 0.3, secs(3.0), SR);
    for (s, c) in sig.iter_mut().zip(chirp.iter()) {
        *s = (*s + *c) * 0.5;
    }
    let presets = suite_core::presets::load_all(crate::presets::PRESET_JSON);
    assert!(presets.len() >= 6);
    for p in &presets {
        let s = crate::presets::settings_from_preset(p);
        let mut core = PatinaCore::new(SR);
        core.configure(&s);
        let safe = p.name.replace(' ', "_").replace('/', "-");
        let out = render_and_write("PATINA", &safe, core, &sig, 512, SR as u32);
        assert_universal(&out);
    }
}

// ---------------------------------------------------------------------------
// SOUND-PASS audition renders (ignored; driven by tools/audition.py)
// ---------------------------------------------------------------------------

/// Render every factory preset + the default state over two genre-right musical sources
/// (a sustained minor pad + an amen-ish breakbeat) for offline audition. Writes to
/// `renders/_audition/PATINA/<QVS_AUDITION_DIR>/<preset>__{pad,break}.wav` (subdir defaults
/// to "before"). `#[ignore]` — not part of the normal gate; run explicitly with `--ignored`.
#[test]
#[ignore]
fn audition_render_presets() {
    let subdir = std::env::var("QVS_AUDITION_DIR").unwrap_or_else(|_| "before".to_string());
    let pad = testsig::synth_pad(110.0, 4.0, SR); // ~4 s sustained minor pad (texture)
    let brk = testsig::synth_break(140.0, 2, SR); // ~3.4 s dnb-ish break (transients + gaps)

    let mut render_pair = |label: &str, s: &Settings| {
        let safe = label.replace(' ', "_").replace('/', "-");
        for (src, tag) in [(&pad, "pad"), (&brk, "break")] {
            let mut core = PatinaCore::new(SR);
            core.configure(s);
            let name = format!("{subdir}/{safe}__{tag}");
            let out = render_and_write("_audition/PATINA", &name, core, src, 512, SR as u32);
            let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
            eprintln!("[audition] {name}: {} samples, peak {peak:.4}", out.len());
        }
    };

    render_pair("default", &Settings::default());
    let presets = suite_core::presets::load_all(crate::presets::PRESET_JSON);
    for p in &presets {
        let s = crate::presets::settings_from_preset(p);
        render_pair(&p.name, &s);
    }
}

// ---------------------------------------------------------------------------
// Fuzz / robustness: extreme + degenerate settings stay finite and bounded
// ---------------------------------------------------------------------------

#[test]
fn extremes_stay_finite_and_bounded() {
    let sig = testsig::pink_noise(0.9, secs(2.0), 0xBEEF);
    let s = Settings {
        wow_depth: 1.0,
        wow_rate: 4.0,
        flutter: 1.0,
        sat_drive: 1.0,
        bump_amount: 1.0,
        bump_freq: 120.0,
        azimuth: 1.0,
        dropout_rate: 1.0,
        dropout_depth: 1.0,
        hiss: 1.0,
        hum: 1.0,
        crackle: 1.0,
        hum_60: false,
        key_amount: 0.0,
        age: 1.0,
        mix: 1.0,
        out_db: 24.0,
    };
    // Stereo path too (azimuth exercised).
    let mut core = PatinaCore::new(SR);
    core.configure(&s);
    let mut peak = 0.0f32;
    for &x in &sig {
        let (l, r) = core.process_stereo(x, -x);
        assert!(l.is_finite() && r.is_finite(), "stereo output NaN/inf");
        peak = peak.max(l.abs()).max(r.abs());
    }
    // Clamp policy (TRIAGE 2026-07-08): final clamp is a ±8.0 runaway/NaN guard
    // (≈ +18 dBFS), not a 0 dBFS ceiling — extreme fuzz asserts finite && ≤ the guard.
    assert!(peak <= 8.001, "stereo peak exceeds the +18 dBFS safety guard: {peak}");

    let out = render_with(s, &sig);
    assert!(!has_nan_or_inf(&out), "mono output has NaN/inf");
    let mpeak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    assert!(mpeak <= 8.001, "mono peak exceeds the +18 dBFS safety guard: {mpeak}");
}

/// Regression — user SOUND-PASS report: "adds heavy phasing when mix is turned down".
/// Blending the wow/flutter-modulated wet against the static latency-matched dry comb-filters
/// at partial mix. We scale the wow/flutter delay offset by `mix`, so as the effect is dialled
/// back the wet/dry time-detune shrinks and a steady tone keeps a near-constant envelope
/// instead of deep periodic flange notches.
#[test]
fn partial_mix_does_not_heavily_phase() {
    let dur = secs(8.0);
    let sig = testsig::sine(1000.0, 0.5, dur, SR);
    let mut s = base();
    s.wow_depth = 0.7;
    s.flutter = 0.5;
    s.age = 0.4;
    s.mix = 0.3; // "turned down" — a subtle tape colour
    let out = render_with(s, &sig);
    // Short-time RMS envelope over ~10 ms windows across the steady body (skip 1 s settle).
    let win = (SR * 0.010) as usize;
    let body = &out[secs(1.0)..];
    let mut env: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + win <= body.len() {
        env.push(rms(&body[i..i + win]));
        i += win;
    }
    let emax = env.iter().cloned().fold(0.0f32, f32::max);
    let emin = env.iter().cloned().fold(f32::INFINITY, f32::min);
    let ratio_db = 20.0 * (emax / emin.max(1e-9)).log10();
    // A deep flange notch swings the envelope many dB; the mix-scaled offset keeps it shallow.
    assert!(
        ratio_db < 3.0,
        "partial-mix envelope swing {ratio_db:.2} dB — wow/dry comb is flanging at low mix"
    );
}
