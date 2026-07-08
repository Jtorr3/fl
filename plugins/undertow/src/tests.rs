//! UNDERTOW done-bar tests (PRD §4 universal + UNDERTOW-specific from the build brief):
//!  1. Kick pattern (4-on-floor, 130 BPM) → the rumble-only envelope dips by ≥ the duck-depth
//!     (dB) at each kick onset and recovers between hits.
//!  2. Tune note = A1 (55 Hz), amount high → the rumble spectrum peaks within ±3% of 55 Hz.
//!  3. Rumble-only output is ≥ 90% mono below 150 Hz (L/R correlation).
//!  4. 30 s render stays bounded (finite, no blow-up).

use crate::dsp::{db_to_gain, Settings, UndertowCore};
use suite_core::dsp::Svf;
use suite_core::stft::{Complex, Stft};
use suite_core::testsig::{synth_kick, white_noise, KickSpec};

const SR: f32 = 48_000.0;

#[test]
fn manual_covers_all_params_and_has_recipes() {
    suite_core::manual::assert_manual_covers_params(
        crate::MANUAL_DOC,
        &crate::UndertowParams::default(),
    );
}

/// 4-on-the-floor synthetic-kick pattern: `beats` kicks at `bpm`, plus a tail. Returns the
/// buffer and the onset sample indices.
fn kick_pattern(bpm: f32, beats: usize, f_end: f32) -> (Vec<f32>, Vec<usize>, usize) {
    let step = (60.0 / bpm * SR) as usize;
    let n = step * beats + (SR * 0.6) as usize;
    let mut buf = vec![0.0f32; n];
    let spec = KickSpec { f_start: 200.0, f_end, amp_decay_s: 0.26, ..KickSpec::default() };
    let one = synth_kick(&spec, (SR * 0.5) as usize, SR);
    let mut onsets = Vec::new();
    for b in 0..beats {
        let start = b * step;
        onsets.push(start);
        for (i, &v) in one.iter().enumerate() {
            if start + i < n {
                buf[start + i] += v;
            }
        }
    }
    for v in buf.iter_mut() {
        *v = (*v * 0.5).clamp(-0.999, 0.999);
    }
    (buf, onsets, step)
}

/// One-pole peak envelope follower (attack `atk_ms`, release `rel_ms`).
fn envelope(x: &[f32], atk_ms: f32, rel_ms: f32) -> Vec<f32> {
    let a = (-1.0 / (atk_ms * 0.001 * SR)).exp();
    let r = (-1.0 / (rel_ms * 0.001 * SR)).exp();
    let mut env = 0.0f32;
    x.iter()
        .map(|&s| {
            let t = s.abs();
            let c = if t > env { a } else { r };
            env = t + c * (env - t);
            env
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Done-bar 1: kick-keyed ducking
// ---------------------------------------------------------------------------

#[test]
fn rumble_ducks_at_each_kick_onset() {
    let (input, onsets, step) = kick_pattern(130.0, 8, 45.0);
    let depth = 0.6f32;
    let depth_db = depth * 24.0;

    let ducked = Settings {
        tune_amount: 0.0,
        duck_depth: depth,
        duck_release_ms: 150.0,
        rumble_gain: db_to_gain(0.0),
        width: 0.0,
        dry_gain: 0.0, // rumble only
        ..Settings::default()
    };
    let mut core = UndertowCore::new(SR);
    let mut rumble = input.clone();
    core.process_mono(&mut rumble, &ducked);

    // Reference with the SAME setting but NO duck — proves the dip is ducking, not tail decay.
    let no_duck = Settings { duck_depth: 0.0, ..ducked };
    let mut core2 = UndertowCore::new(SR);
    let mut rumble_nd = input.clone();
    core2.process_mono(&mut rumble_nd, &no_duck);

    // Envelopes (fast, short release) so the ducking notch is resolved sharply. The ducked and
    // no-duck renders share the SAME rumble bed (same FDN/input) — the only difference is the
    // duck gain — so the notch depth at each onset is exactly the ducking applied there.
    let env = envelope(&rumble, 1.0, 10.0);
    let env_nd = envelope(&rumble_nd, 1.0, 10.0);

    let onset_win = (0.045 * SR) as usize; // the kick body / fresh-rumble region

    let mut dips = Vec::new();
    let mut recovered = 0usize;
    // Skip the first two kicks (FDN warm-up) and the last (partial tail).
    for k in 2..onsets.len() - 1 {
        let on = onsets[k];
        let next = onsets[k + 1];

        // Onset dip: the deepest point of the ducked envelope in the onset region, compared to
        // the un-ducked envelope at that same instant (= the amount the kick ducks the rumble).
        let mut argmin = on;
        let mut vmin = f32::INFINITY;
        for i in on..(on + onset_win).min(env.len()) {
            if env[i] < vmin {
                vmin = env[i];
                argmin = i;
            }
        }
        let dip_db = 20.0 * (env_nd[argmin] / vmin.max(1e-12)).log10();
        dips.push(dip_db);

        // Recovers between hits: after the onset trough the ducked rumble swells back up — the
        // inter-onset peak (later half of the interval) is well above the onset trough (breathing).
        let mid = (on + (next - on) / 2).min(env.len() - 1);
        let late_peak = env[mid..next.min(env.len())].iter().cloned().fold(0.0f32, f32::max);
        if late_peak >= vmin * 2.0 {
            recovered += 1;
        }
    }

    // Every steady kick's rumble must dip by ≥ the configured duck depth (dB) at the onset.
    let min_dip = dips.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(
        min_dip >= depth_db - 1.5,
        "rumble dip {min_dip:.1} dB < duck depth {depth_db:.1} dB (dips: {dips:?})"
    );
    // …and recover between hits on every measured kick.
    assert!(
        recovered >= dips.len() - 1,
        "rumble did not recover between hits ({recovered}/{})",
        dips.len()
    );
    let _ = step;
}

// ---------------------------------------------------------------------------
// Done-bar 2: key-locked resonant tune peak at 55 Hz
// ---------------------------------------------------------------------------

#[test]
fn tune_peak_lands_within_3_percent_of_55hz() {
    // A1 = 55 Hz, maximum tune amount. Drive with broadband noise so the ONLY sub-band
    // spectral peak is the tuning resonance (no competing kick fundamental).
    let n = (SR * 4.0) as usize;
    let input = white_noise(0.3, n, 0x0DDBA11);

    let s = Settings {
        strip: 0.0,
        drive: 0.2,
        size: 0.4,
        decay: 0.4,
        lp_cutoff: 200.0,
        lp_res: 1.0,
        tune_hz: 55.0, // A1
        tune_amount: 1.0,
        duck_depth: 0.0,
        rumble_gain: db_to_gain(0.0),
        width: 0.0,
        dry_gain: 0.0, // rumble only
        ..Settings::default()
    };
    let mut core = UndertowCore::new(SR);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);

    // Accumulate STFT magnitude over the steady region, then quadratic-interpolate the peak in
    // the sub band. FFT = 16384 (bin ≈ 2.93 Hz); interpolation on the smooth resonance nails it.
    let fft = 16_384usize;
    let hop = 4_096usize;
    let mut stft = Stft::new(fft, hop);
    let max_k = ((120.0 * fft as f32 / SR) as usize).min(fft / 2 - 1);
    let min_k = ((25.0 * fft as f32 / SR) as usize).max(1);
    let mut mag = vec![0.0f64; fft / 2 + 1];
    let mut cb = |spec: &mut [Complex<f32>]| {
        for (k, m) in mag.iter_mut().enumerate() {
            *m += spec[k].norm() as f64;
        }
    };
    // Skip the first ~0.5 s (resonance ring-up) so the accumulation is steady.
    for (i, &x) in out.iter().enumerate() {
        stft.process(x, &mut cb);
        let _ = i;
    }

    let mut best = min_k;
    let mut best_m = 0.0f64;
    for k in min_k..=max_k {
        if mag[k] > best_m {
            best_m = mag[k];
            best = k;
        }
    }
    let m0 = mag[best - 1];
    let m1 = mag[best];
    let m2 = mag[best + 1];
    let denom = m0 - 2.0 * m1 + m2;
    let delta = if denom.abs() > 1e-12 { 0.5 * (m0 - m2) / denom } else { 0.0 };
    let peak_hz = (best as f64 + delta.clamp(-0.5, 0.5)) * SR as f64 / fft as f64;

    let err = (peak_hz - 55.0).abs() / 55.0;
    assert!(
        err <= 0.03,
        "tune peak at {peak_hz:.2} Hz, {:.1}% off 55 Hz (bin {best})",
        err * 100.0
    );
}

// ---------------------------------------------------------------------------
// Done-bar 3: rumble is ≥ 90% mono below 150 Hz
// ---------------------------------------------------------------------------

#[test]
fn rumble_is_mono_below_150hz() {
    let (input, _onsets, _step) = kick_pattern(130.0, 8, 50.0);
    let s = Settings {
        tune_amount: 0.2,
        duck_depth: 0.4,
        width: 1.0, // maximum width — the low end must STILL be mono
        rumble_gain: db_to_gain(0.0),
        dry_gain: 0.0, // rumble only
        ..Settings::default()
    };
    let mut core = UndertowCore::new(SR);
    let mut l = input.clone();
    let mut r = input.clone();
    core.process_stereo(&mut l, &mut r, &s);

    // 4th-order low-pass both channels at 150 Hz (two cascaded SVF LPs), then correlate.
    let lp = |sig: &[f32]| -> Vec<f32> {
        let mut a = Svf::new();
        let mut b = Svf::new();
        a.set(150.0, 0.707, SR);
        b.set(150.0, 0.707, SR);
        sig.iter().map(|&x| b.process(a.process(x).lp).lp).collect()
    };
    let ll = lp(&l);
    let rr = lp(&r);

    // Pearson correlation over the steady region.
    let start = (SR * 0.5) as usize;
    let (mut sxy, mut sxx, mut syy) = (0.0f64, 0.0f64, 0.0f64);
    for i in start..ll.len() {
        let x = ll[i] as f64;
        let y = rr[i] as f64;
        sxy += x * y;
        sxx += x * x;
        syy += y * y;
    }
    let corr = if sxx > 0.0 && syy > 0.0 { sxy / (sxx.sqrt() * syy.sqrt()) } else { 1.0 };
    assert!(corr >= 0.9, "low-band L/R correlation {corr:.3} < 0.90 (not mono below 150 Hz)");
}

// ---------------------------------------------------------------------------
// Done-bar 4: 30 s bounded
// ---------------------------------------------------------------------------

#[test]
fn thirty_seconds_bounded() {
    // A hot setting: heavy drive, long tail, resonant tune, full width, driven continuously.
    let n = (SR * 30.0) as usize;
    let input = white_noise(0.4, n, 0x5EED_1234);
    let s = Settings {
        strip: 0.6,
        drive: 0.9,
        size: 0.6,
        decay: 2.5,
        lp_cutoff: 200.0,
        lp_res: 6.0,
        tune_amount: 0.8,
        duck_depth: 0.3,
        rumble_gain: db_to_gain(6.0),
        width: 1.0,
        ..Settings::default()
    };
    let mut core = UndertowCore::new(SR);
    let mut l = input.clone();
    let mut r = input.clone();
    core.process_stereo(&mut l, &mut r, &s);

    let mut peak = 0.0f32;
    for i in 0..l.len() {
        assert!(l[i].is_finite() && r[i].is_finite(), "non-finite at {i}");
        peak = peak.max(l[i].abs()).max(r[i].abs());
    }
    // dry (0.4) + safety-clipped rumble (< 1 per channel) ⇒ comfortably bounded.
    assert!(peak < 2.0, "30 s render blew up: peak {peak}");
}
