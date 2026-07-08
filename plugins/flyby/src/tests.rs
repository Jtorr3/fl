//! FLYBY done-bar + universal DSP tests (PRD §4).
//!
//! Done bar (PRD §4 + build brief), FLYBY-specific:
//! (1) sine input, circular path → measured f0 of the output shows periodic deviation at the
//!     traversal rate (doppler present; f0 tracked over ≥ 2 cycles);
//! (2) L/R RMS ratio crosses 1.0 at least once per traversal (panning sweeps);
//! (3) air/distance: spectral centroid + level are lower at the far point than the near point;
//! (4) rate clamp: no output sample step exceeds a bound at the sharpest corner of the Figure-8
//!     at max speed.
//!
//! Latency note: the fractional delay IS the effect (distance = delay), so FLYBY reports zero
//! latency and the suite's lag-0 `assert_single_coherent_peak` does not apply. We assert
//! `mix = 0` nulls against the dry input instead (`mix_zero_nulls_against_dry`).

use super::*;
use suite_core::testsig;

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Dominant frequency of a (near-single-tone) window via negative→positive zero-crossings.
fn f0_zero_cross(win: &[f32], sr: f32) -> f32 {
    let mut crossings = 0usize;
    for w in win.windows(2) {
        if w[0] <= 0.0 && w[1] > 0.0 {
            crossings += 1;
        }
    }
    crossings as f32 * sr / win.len() as f32
}

/// Naive spectral centroid (Hz) of a window via a direct real DFT over a Hann window. Fine for a
/// test at these sizes; measures the air low-pass's effect on broadband noise unambiguously.
fn spectral_centroid(win: &[f32], sr: f32) -> f32 {
    let n = win.len();
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    // Only the lower half is unique; step a handful of bins for speed.
    for k in 1..n / 2 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        let w = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
        for (i, &x) in win.iter().enumerate() {
            // Hann window to reduce leakage.
            let hann = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
            let s = x as f64 * hann;
            re += s * (w * i as f64).cos();
            im -= s * (w * i as f64).sin();
        }
        let mag = (re * re + im * im).sqrt();
        let f = k as f64 * sr as f64 / n as f64;
        num += f * mag;
        den += mag;
    }
    if den > 0.0 {
        (num / den) as f32
    } else {
        0.0
    }
}

/// Normalized autocorrelation of a series at a given lag.
fn autocorr(x: &[f32], lag: usize) -> f32 {
    if lag >= x.len() {
        return 0.0;
    }
    let mean = x.iter().sum::<f32>() / x.len() as f32;
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for i in 0..x.len() {
        let a = x[i] - mean;
        den += a * a;
        if i + lag < x.len() {
            num += a * (x[i + lag] - mean);
        }
    }
    if den > 0.0 {
        num / den
    } else {
        0.0
    }
}

fn circle_settings() -> Settings {
    let mut s = Settings::default();
    let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
    s.node_count = PathShape::Circle.layout(&mut nodes, 6);
    s.nodes = nodes;
    s
}

/// DONE-BAR (1): a circular fly-by bends the pitch of a steady sine periodically at the traversal
/// rate. We track f0 (on L+R, so pan-induced amplitude nulls don't disturb it) across 3 cycles
/// and require both a measurable bend and periodicity at exactly one cycle.
#[test]
fn circular_path_produces_periodic_doppler() {
    let sr = 48_000.0f32;
    let mut s = circle_settings();
    s.speed_hz = 1.0;
    s.sync = false;
    s.size = 8.0;
    s.doppler = 1.0;
    s.air = 0.0; // isolate pitch (no spectral tilt on the sine)
    s.itd = false;
    s.mix = 1.0;

    let cycles = 3.0f32;
    let len = (sr * cycles) as usize;
    let sig = testsig::sine(300.0, 0.5, len, sr);
    let mut core = FlybyCore::new(sr);
    let (l, r) = core.process_stereo(&sig, &s);
    assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()));

    // Sum to a pan-independent mid signal, window it, track f0.
    let mid: Vec<f32> = (0..len).map(|i| l[i] + r[i]).collect();
    let win = 1024usize;
    let mut f0s = Vec::new();
    let mut i = 0;
    while i + win <= len {
        let f = f0_zero_cross(&mid[i..i + win], sr);
        if f > 50.0 {
            f0s.push(f);
        }
        i += win;
    }
    assert!(f0s.len() > 60, "not enough f0 windows: {}", f0s.len());

    let fmin = f0s.iter().cloned().fold(f32::INFINITY, f32::min);
    let fmax = f0s.iter().cloned().fold(0.0f32, f32::max);
    let bend = fmax / fmin;
    assert!(
        bend > 1.02,
        "doppler bend too small: fmin {fmin:.1} fmax {fmax:.1} (ratio {bend:.4})"
    );

    // Periodicity: one full up/down of f0 per loop → autocorr peaks near one-cycle lag and is
    // clearly weaker at the half-cycle lag.
    let windows_per_cycle = (f0s.len() as f32 / cycles).round() as usize;
    let ac_full = autocorr(&f0s, windows_per_cycle);
    let ac_half = autocorr(&f0s, windows_per_cycle / 2);
    assert!(
        ac_full > 0.3 && ac_full > ac_half,
        "f0 not periodic at traversal rate: ac(cycle)={ac_full:.3} ac(half)={ac_half:.3}"
    );
}

/// DONE-BAR (2): the pan sweeps side to side, so the L/R RMS ratio crosses 1.0 at least once per
/// traversal (in fact twice on a circle). We use noise for a stable per-window RMS.
#[test]
fn lr_rms_ratio_crosses_unity_each_traversal() {
    let sr = 48_000.0f32;
    let mut s = circle_settings();
    s.speed_hz = 1.0;
    s.size = 8.0;
    s.doppler = 0.5;
    s.air = 0.3;
    s.itd = true;
    s.width = 1.0;
    s.mix = 1.0;

    let cycles = 2usize;
    let len = (sr * cycles as f32) as usize;
    let sig = testsig::white_noise(0.5, len, 1234);
    let mut core = FlybyCore::new(sr);
    let (l, r) = core.process_stereo(&sig, &s);

    // Windowed log-ratio; count sign changes = unity crossings.
    let win = 2048usize;
    let mut sign: i32 = 0;
    let mut crossings = 0usize;
    let mut i = 0;
    while i + win <= len {
        let rl = rms(&l[i..i + win]).max(1e-9);
        let rr = rms(&r[i..i + win]).max(1e-9);
        let d = (rl / rr).ln();
        let s_now = if d > 0.02 {
            1
        } else if d < -0.02 {
            -1
        } else {
            0
        };
        if s_now != 0 {
            if sign != 0 && s_now != sign {
                crossings += 1;
            }
            sign = s_now;
        }
        i += win;
    }
    assert!(
        crossings >= cycles,
        "L/R ratio crossed unity {crossings} times over {cycles} traversals (want >= {cycles})"
    );
}

/// DONE-BAR (3): at the far point of the path the source is both quieter (inverse-distance) and
/// darker (air absorption) than at the near point. We locate the near/far windows from the known
/// path geometry, then compare level + spectral centroid.
#[test]
fn far_point_is_quieter_and_darker_than_near() {
    let sr = 48_000.0f32;
    let mut s = circle_settings();
    s.speed_hz = 0.5; // slow so each window sits at a well-defined distance
    s.size = 8.0;
    s.doppler = 0.0; // isolate distance/air from pitch shift
    s.air = 1.0;
    s.itd = false;
    s.width = 1.0;
    s.mix = 1.0;

    let len = (sr * 2.0) as usize; // one full cycle
    let sig = testsig::white_noise(0.5, len, 777);
    let mut core = FlybyCore::new(sr);
    let (l, r) = core.process_stereo(&sig, &s);
    let mid: Vec<f32> = (0..len).map(|i| 0.5 * (l[i] + r[i])).collect();

    // Radius at a given sample from the path geometry.
    let r_at = |n: usize| -> f32 {
        let phase = (n as f32 / sr) * 0.5; // speed_hz = 0.5
        let (px, py) = path_position(&s.nodes, s.node_count, phase);
        let (x, y) = (px * s.size, py * s.size);
        (x * x + y * y).sqrt()
    };

    let win = 4096usize;
    let mut near = (f32::INFINITY, 0usize);
    let mut far = (0.0f32, 0usize);
    let mut i = win; // skip the first window (filter warm-up)
    while i + win <= len {
        let rr = r_at(i + win / 2);
        if rr < near.0 {
            near = (rr, i);
        }
        if rr > far.0 {
            far = (rr, i);
        }
        i += win;
    }

    let near_win = &mid[near.1..near.1 + win];
    let far_win = &mid[far.1..far.1 + win];
    let near_rms = rms(near_win);
    let far_rms = rms(far_win);
    let near_cen = spectral_centroid(near_win, sr);
    let far_cen = spectral_centroid(far_win, sr);

    assert!(
        far_rms < near_rms,
        "far not quieter: near_rms {near_rms:.5} far_rms {far_rms:.5} (r near {:.2} far {:.2})",
        near.0,
        far.0
    );
    assert!(
        far_cen < near_cen,
        "far not darker: near_centroid {near_cen:.0} Hz far_centroid {far_cen:.0} Hz"
    );
}

/// DONE-BAR (4): at the sharpest Figure-8 corner and maximum speed, the rate clamp keeps the read
/// pointer smooth — the largest sample-to-sample step stays bounded relative to a stationary
/// reference render of the same tone (a hard tap jump would be many times larger).
#[test]
fn rate_clamp_bounds_step_at_figure8_max_speed() {
    let sr = 48_000.0f32;
    let len = (sr * 4.0) as usize;
    let tone = testsig::sine(220.0, 0.5, len, sr);

    // Reference: motion frozen (speed 0) — a clean baseline slope for a stationary tone.
    let mut still = Settings::default();
    let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
    still.node_count = PathShape::Figure8.layout(&mut nodes, 8);
    still.nodes = nodes;
    still.speed_hz = 0.0;
    still.doppler = 1.0;
    still.air = 0.0;
    still.itd = false;
    still.width = 1.0;
    still.mix = 1.0;
    let mut core = FlybyCore::new(sr);
    let (bl, _br) = core.process_stereo(&tone, &still);
    let max_step_still = bl
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max);

    // Moving: Figure-8 at max speed.
    let mut moving = still;
    moving.speed_hz = 20.0;
    let mut core = FlybyCore::new(sr);
    let (ml, mr) = core.process_stereo(&tone, &moving);
    assert!(ml.iter().chain(mr.iter()).all(|v| v.is_finite()));
    let max_step_move = ml
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max);

    // The rate clamp caps the read speed at 1.5×, so the moving step can't exceed ~1.5× the
    // stationary slope; allow a safety factor for interpolation + pan/gain motion.
    let bound = (max_step_still * 2.5).max(0.02);
    assert!(
        max_step_move <= bound,
        "figure-8 max-speed step {max_step_move:.4} exceeded bound {bound:.4} \
         (stationary {max_step_still:.4}) — rate clamp failed"
    );
}

/// Universal + mix=0 null: with `mix = 0` the output is exactly the dry input (the spatialiser
/// still runs internally but is not tapped). Replaces the lag-0 single-coherent-peak check, which
/// does not apply to a delay-based effect (see module docs).
#[test]
fn mix_zero_nulls_against_dry() {
    let sr = 48_000.0f32;
    let n = 48_000usize;
    let dry = testsig::sine(220.0, 0.5, n, sr);

    let mut s = circle_settings();
    s.mix = 0.0;
    s.out_db = 0.0;

    let mut core = FlybyCore::new(sr);
    let (l, r) = core.process_stereo(&dry, &s);
    // Both output channels equal the dry mono input at mix = 0.
    let resid_l = {
        let mse = (0..n).map(|i| (dry[i] - l[i]).powi(2)).sum::<f32>() / n as f32;
        20.0 * mse.sqrt().max(1e-12).log10()
    };
    let resid_r = {
        let mse = (0..n).map(|i| (dry[i] - r[i]).powi(2)).sum::<f32>() / n as f32;
        20.0 * mse.sqrt().max(1e-12).log10()
    };
    assert!(resid_l < -80.0, "mix=0 L did not null: {resid_l:.1} dB");
    assert!(resid_r < -80.0, "mix=0 R did not null: {resid_r:.1} dB");
}

/// A normal wet render stays finite and within 0 dBFS on all three shapes.
#[test]
fn wet_render_is_bounded_on_every_shape() {
    let sr = 48_000.0f32;
    for shape_idx in 0..3 {
        let mut s = Settings::default();
        let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
        s.node_count = PathShape::from_index(shape_idx).layout(&mut nodes, 6);
        s.nodes = nodes;
        s.mix = 1.0;
        let sig = testsig::pink_noise(0.5, (sr * 2.0) as usize, 42 + shape_idx as u32);
        let mut core = FlybyCore::new(sr);
        let (l, r) = core.process_stereo(&sig, &s);
        assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()));
        let peak = l
            .iter()
            .chain(r.iter())
            .fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak <= 1.0, "shape {shape_idx} peak {peak} exceeded 0 dBFS");
    }
}

/// The three starting layouts differ, and the path evaluates to a smooth closed loop (successive
/// positions are close — no jumps between segments).
#[test]
fn path_shapes_are_distinct_and_smooth() {
    let mut a = [(0.0f32, 0.0f32); MAX_NODES];
    let mut b = [(0.0f32, 0.0f32); MAX_NODES];
    let na = PathShape::Circle.layout(&mut a, 6);
    let nb = PathShape::Figure8.layout(&mut b, 8);
    // Distinct layouts.
    assert!(a[..na] != b[..nb]);

    // Closed + smooth: the path at phase→1 wraps back to phase 0 with a small step.
    let steps = 400;
    let mut max_jump = 0.0f32;
    let mut prev = path_position(&a, na, 0.0);
    for k in 1..=steps {
        let p = k as f32 / steps as f32;
        let cur = path_position(&a, na, p % 1.0);
        let dx = cur.0 - prev.0;
        let dy = cur.1 - prev.1;
        max_jump = max_jump.max((dx * dx + dy * dy).sqrt());
        prev = cur;
    }
    // The whole loop spans O(1) units; a per-step jump must be a small fraction of that.
    assert!(max_jump < 0.2, "path not smooth: max step {max_jump:.3}");
}
