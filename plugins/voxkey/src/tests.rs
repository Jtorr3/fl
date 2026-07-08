//! VOXKEY done-bar + render tests (PRD §4 universal + VOXKEY-specific).
//!
//! Done bars (SPECS / build brief):
//! 1. Vocal gliding across a fifth, root A / natural minor, retune 0, amount 100% → measured
//!    output f0 sits on scale tones within ±15 cents for ≥ 80% of pitched frames.
//! 2. Formant preservation: +5 st correction forced (MIDI mode, held note a fifth up) on a
//!    fixed note → spectral-envelope peaks stay within ±8% while f0 moves +5 st.
//! 3. Confidence gate: white-noise input → output pitch-shift ratio stays ≈ 1.0 (no pumping).
//! Plus the universal `mix=0` null (in dsp.rs) and per-preset universal render assertions.

use crate::dsp::{nearest_scale_hz, Settings, VoxCore, MAIN_FFT};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::dsp::Svf;
use suite_core::harness::{assert_universal, render_path, write_wav};
use suite_core::pitch::{cents, Mpm};
use suite_core::presets::load_all;
use suite_core::testsig::{synth_vocal, white_noise};

const SR: f32 = 48_000.0;

/// A vocal-like source WITHOUT vibrato: a sawtooth glottal pulse through three static formant
/// band-passes (`/a/`-like: F1≈700, F2≈1220, F3≈2600). This is the "saw + formant ramp" the
/// build brief sanctions — vibrato-free so the pitch tracker settles cleanly per note and the
/// hard-snap retune can quantize to within a few cents (synth_vocal's ±17-cent vibrato would
/// otherwise pass through the correction as residual wobble).
fn saw_formant(f0: f32, len: usize) -> Vec<f32> {
    let formants = [(700.0f32, 6.0f32, 1.0f32), (1220.0, 8.0, 0.55), (2600.0, 10.0, 0.28)];
    let mut bp = [Svf::new(), Svf::new(), Svf::new()];
    for (i, &(fc, q, _)) in formants.iter().enumerate() {
        bp[i].set(fc.min(SR * 0.45), q, SR);
    }
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(len);
    let mut peak = 1.0e-6f32;
    for _ in 0..len {
        phase += f0 / SR;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
        let saw = 2.0 * phase - 1.0;
        let mut y = 0.0f32;
        for (i, &(_, _, g)) in formants.iter().enumerate() {
            y += bp[i].process(saw).bp * g;
        }
        peak = peak.max(y.abs());
        out.push(y);
    }
    let norm = 0.7 / peak;
    for v in out.iter_mut() {
        *v *= norm;
    }
    out
}

/// Concatenate vibrato-free vocal-like segments at geometrically-stepped f0s → a pitch glide
/// with steady dwells the tracker can lock onto.
fn glide_vocal(f_start: f32, f_end: f32, steps: usize, seg_len_s: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for i in 0..steps {
        let t = i as f32 / (steps as f32 - 1.0);
        let f = f_start * (f_end / f_start).powf(t);
        out.extend_from_slice(&saw_formant(f, (SR * seg_len_s) as usize));
    }
    out
}

/// Run a mono buffer through a fresh core with fixed settings, returning the wet output.
fn render(input: &[f32], s: &Settings) -> Vec<f32> {
    let mut core = VoxCore::new(SR);
    let mut buf = input.to_vec();
    core.process_mono(&mut buf, s);
    buf
}

// ---------------------------------------------------------------------------
// (1) Retune-to-scale accuracy across a fifth
// ---------------------------------------------------------------------------

#[test]
fn retune_snaps_to_scale_within_15_cents() {
    // A3 (220) up a perfect fifth to E4 (329.63), 10 stepped vocal-like notes (0.5 s dwell).
    let steps = 10usize;
    let dwell_s = 0.5f32;
    let dwell = (SR * dwell_s) as usize;
    let input = glide_vocal(220.0, 329.63, steps, dwell_s);
    let s = Settings {
        root: 9,          // A
        scale: 2,         // Natural Minor
        retune_ms: 0.0,   // hard snap
        amount: 1.0,      // full correction
        mix: 1.0,         // pure wet so we measure the corrected pitch
        formant_ratio: 1.0,
        conf_gate: 0.6,
        ..Settings::default()
    };
    let wet = render(&input, &s);
    assert_universal(&wet);

    // Measure each note's SETTLED output region (offset by the 2048-sample latency, past a
    // 0.15 s detector-settle margin), in 2048/1024 windows; count frames on-scale.
    let win = 2048usize;
    let hop = 1024usize;
    let settle = (0.15 * SR) as usize;
    let margin = (0.03 * SR) as usize;
    let mut mpm = Mpm::new(win, SR, 80.0, 700.0);
    let mut total = 0usize;
    let mut on_scale = 0usize;
    for i in 0..steps {
        let a = i * dwell + MAIN_FFT + settle;
        let b = ((i + 1) * dwell + MAIN_FFT).saturating_sub(margin).min(wet.len());
        let mut pos = a;
        while pos + win <= b {
            let r = mpm.analyze(&wet[pos..pos + win]);
            if r.confidence > 0.8 && r.f0_hz > 0.0 {
                total += 1;
                let tgt = nearest_scale_hz(r.f0_hz, 9, 2);
                if cents(r.f0_hz, tgt).abs() <= 15.0 {
                    on_scale += 1;
                }
            }
            pos += hop;
        }
    }
    assert!(total >= 20, "too few pitched frames analysed ({total})");
    let frac = on_scale as f32 / total as f32;
    assert!(
        frac >= 0.80,
        "only {:.0}% of pitched frames within ±15 cents of an A-minor scale tone ({on_scale}/{total})",
        frac * 100.0
    );
}

// ---------------------------------------------------------------------------
// (2) Formant preservation while pitch moves +5 st (MIDI mode)
// ---------------------------------------------------------------------------

/// A frame-averaged, cepstrally-smoothed log-magnitude spectral envelope (mirrors the
/// suite_core::shift test technique — Welch averaging blurs the harmonic comb so the cepstral
/// lift recovers the formants). Own FFT, independent of the engine internals.
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
    // Complex spectrum vector (length nbins) via the inverse plan's input constructor —
    // avoids naming num_complex::Complex directly.
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

#[test]
fn formant_preserved_while_pitch_shifts_five_semitones() {
    // Fixed 150 Hz vocal (low enough that the cepstral lifter cutoff n/16=256 sits below the
    // pitch-period quefrency sr/f0≈320, so the envelope measure isn't polluted by harmonics);
    // MIDI mode holds +5 st → forced +5 st correction.
    let f0 = 150.0f32;
    let dry = synth_vocal(f0, (SR * 1.6) as usize, SR);
    let held = f0 * 2.0f32.powf(5.0 / 12.0); // +5 st
    let s = Settings {
        midi_mode: true,
        held_midi_hz: Some(held),
        retune_ms: 0.0,
        amount: 1.0,
        formant_ratio: 1.0, // formant offset 0 → formants must stay put
        mix: 1.0,
        ..Settings::default()
    };
    let wet = render(&dry, &s);
    assert_universal(&wet);

    // f0 moved up ~+5 st.
    let f0_dry = measure_mid_f0(&dry);
    let f0_wet = measure_mid_f0(&wet);
    let expected = f0_dry * 2.0f32.powf(5.0 / 12.0);
    let err = cents(f0_wet, expected).abs();
    assert!(
        err < 30.0,
        "output f0 {f0_wet:.1} Hz not ~+5 st from {f0_dry:.1} (expected {expected:.1}, err {err:.1} cents)"
    );

    // Formant envelope barely moves — global shift ratio ≈ 1 within ±8%.
    let ratio = formant_shift_ratio(&dry, &wet, 4096);
    assert!(
        (ratio - 1.0).abs() < 0.08,
        "formant envelope shifted {ratio:.3}× while pitch moved +5 st (want ≈1.0 ±8%)"
    );
}

// ---------------------------------------------------------------------------
// (3) Confidence gate: white noise → no retune (ratio stays 1.0)
// ---------------------------------------------------------------------------

#[test]
fn confidence_gate_holds_ratio_at_unity_on_noise() {
    let input = white_noise(0.5, (SR * 1.5) as usize, 0xF00D);
    let s = Settings {
        root: 9,
        scale: 2,
        retune_ms: 40.0,
        amount: 1.0,
        mix: 1.0,
        conf_gate: 0.7, // unpitched noise sits below this → no correction
        ..Settings::default()
    };
    let mut core = VoxCore::new(SR);
    core.configure(&s);
    core.reset();

    let mut max_dev = 0.0f32;
    let mut out = Vec::with_capacity(input.len());
    for &x in &input {
        let (l, _r) = core.process_sample(x, x);
        out.push(l);
        max_dev = max_dev.max((core.ratio() - 1.0).abs());
    }
    assert_universal(&out);
    assert!(
        max_dev < 0.06,
        "pitch-shift ratio drifted {:.3} from 1.0 on white noise (confidence gate should hold it)",
        max_dev
    );
}

// ---------------------------------------------------------------------------
// Preset renders
// ---------------------------------------------------------------------------

#[test]
fn every_preset_renders_and_passes_universal() {
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6, "need ≥ 6 presets, got {}", presets.len());
    // A sung phrase (four notes, some off-scale) then a breath of silence.
    let mut input = Vec::new();
    for &f in &[233.0f32, 262.0, 294.0, 349.0] {
        input.extend_from_slice(&synth_vocal(f, (SR * 0.6) as usize, SR));
    }
    input.extend_from_slice(&vec![0.0f32; (SR * 0.5) as usize]);

    for p in &presets {
        let s = settings_from_preset(p);
        let buf = render(&input, &s);
        assert_universal(&buf);
        let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
        write_wav(&render_path("VOXKEY", &fname), &buf, SR as u32).expect("write render");
    }
}

/// A showcase render: hard-snap autotune glide across a fifth (the classic effect).
#[test]
fn showcase_render() {
    let input = glide_vocal(220.0, 329.63, 16, 0.18);
    let s = Settings {
        root: 9,
        scale: 2,
        retune_ms: 0.0,
        amount: 1.0,
        mix: 1.0,
        ..Settings::default()
    };
    let buf = render(&input, &s);
    assert_universal(&buf);
    write_wav(&render_path("VOXKEY", "showcase_hard_snap"), &buf, SR as u32).unwrap();
}
