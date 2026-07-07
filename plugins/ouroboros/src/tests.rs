//! OUROBOROS done-bar + universal DSP tests (PRD §4).
//!
//! Done bar (PRD §4 + build brief): universal assertions, plus OUROBOROS-specific —
//! (1) 110 % feedback with a saturator + filter in the loop, 30 s render → peak ≤ 0 dBFS,
//!     zero NaN, last-5 s RMS stable (not growing > 1 dB nor collapsing to silence —
//!     self-oscillation is the feature);
//! (2) a delay-time change while running produces no hard click (max sample-to-sample delta
//!     stays bounded vs a steady-state render).
//!
//! Note on partial-mix coherence: OUROBOROS is a time-delay effect with **no lag-0 wet**, so
//! the suite's `assert_single_coherent_peak` (a lag-0 dry/wet alignment check) does not apply.
//! We assert `mix = 0` nulls against the dry input instead (see `mix_zero_nulls_against_dry`).

use super::*;
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

/// A settings snapshot with a saturator (slot A) and a low-pass filter (slot B) active in the
/// loop, feedback at the 110 % maximum — the done-bar's self-oscillation configuration.
fn self_osc_settings() -> Settings {
    let mut s = Settings::default();
    s.delay_ms = 220.0;
    s.sync = false;
    s.feedback = 1.1; // 110 %
    s.decay_scale = 1.0;
    s.freeze = false;
    s.order = SlotOrder::Abc;
    s.slots[0] = SlotSettings {
        kind: SlotType::Saturate,
        amount: 0.45,
        param: 0.2,
    };
    s.slots[1] = SlotSettings {
        kind: SlotType::FilterLp,
        amount: 0.55, // cutoff ~mid
        param: 0.4,   // moderate resonance
    };
    s.slots[2] = SlotSettings {
        kind: SlotType::Off,
        amount: 0.5,
        param: 0.5,
    };
    s.mix = 1.0;
    s.out_db = 0.0;
    s
}

/// DONE-BAR (1): 110 % feedback, saturator + filter in the loop, 30 s → bounded, finite, and a
/// stable (self-oscillating, non-silent, non-growing) tail.
#[test]
fn self_oscillation_is_bounded_and_stable() {
    let sr = 48_000.0f32;
    let s = self_osc_settings();
    let total = (sr * 30.0) as usize;

    // Excite the loop with a 0.2 s noise burst, then let it run on its own for the rest.
    let mut input = vec![0.0f32; total];
    let burst = testsig::white_noise(0.5, (sr * 0.2) as usize, 4242);
    input[..burst.len()].copy_from_slice(&burst);

    let mut core = OuroCore::new(sr);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);

    // Finite + peak ≤ 0 dBFS.
    assert!(out.iter().all(|v| v.is_finite()), "output has NaN/inf");
    let pk = peak(&out);
    assert!(pk <= 1.0, "peak {pk} exceeded 0 dBFS");

    // Last 5 s split into 1 s windows.
    let sec = sr as usize;
    let last5_start = total - 5 * sec;
    let win_rms: Vec<f32> = (0..5)
        .map(|w| {
            let a = last5_start + w * sec;
            rms_db(&out[a..a + sec])
        })
        .collect();

    // Not silent: the tail is still self-oscillating.
    for (w, &r) in win_rms.iter().enumerate() {
        assert!(
            r > -50.0,
            "tail collapsed to silence in last-5s window {w}: RMS {r:.2} dBFS"
        );
    }
    // Not growing > 1 dB across the last 5 s, and not collapsing (drop bounded).
    let growth = win_rms[4] - win_rms[0];
    assert!(
        growth <= 1.0,
        "tail is growing: last-5s RMS rose {growth:.2} dB ( {:?} )",
        win_rms
    );
    assert!(
        growth >= -6.0,
        "tail is collapsing: last-5s RMS fell {growth:.2} dB ( {:?} )",
        win_rms
    );
}

/// DONE-BAR (2): changing the delay time while running produces no hard click — the largest
/// sample-to-sample step around the change stays bounded relative to a steady-state render.
#[test]
fn delay_change_is_click_free() {
    let sr = 48_000.0f32;
    let len = (sr * 4.0) as usize;
    // A steady tone makes a discontinuity obvious (a click = a large sample jump).
    let tone = testsig::sine(220.0, 0.5, len, sr);

    let mut base = Settings::default();
    base.delay_ms = 300.0;
    base.feedback = 0.6;
    base.decay_scale = 1.0;
    base.mix = 0.5;
    base.slots[0] = SlotSettings {
        kind: SlotType::FilterLp,
        amount: 0.6,
        param: 0.3,
    };

    // Steady-state render at a fixed delay.
    let mut core = OuroCore::new(sr);
    let mut steady = tone.clone();
    core.process_mono(&mut steady, &base);
    let max_step_steady = steady
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max);

    // Render with the delay stepped from 300 ms → 150 ms at the halfway point, block-by-block
    // (mirrors the host calling process() per block with a changed param).
    let mut core = OuroCore::new(sr);
    let block = 128usize;
    let mut changed = Vec::with_capacity(len);
    let half = len / 2;
    let mut n = 0usize;
    while n < len {
        let bs = block.min(len - n);
        let mut s = base;
        s.delay_ms = if n < half { 300.0 } else { 150.0 };
        core.configure(&s);
        for i in 0..bs {
            let (l, _r) = core.process_sample(tone[n + i], tone[n + i], &s);
            changed.push(l);
        }
        n += bs;
    }
    assert!(changed.iter().all(|v| v.is_finite()));

    // Max step in a ±2048-sample window around the change.
    let lo = half.saturating_sub(2048);
    let hi = (half + 2048).min(len);
    let max_step_change = changed[lo..hi]
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max);

    // A hard tap-switch click would be many times the steady-state slope; the smoothed
    // fractional read keeps it within a small factor.
    let bound = (max_step_steady * 3.0).max(0.05);
    assert!(
        max_step_change <= bound,
        "delay change clicked: max step {max_step_change:.4} > bound {bound:.4} \
         (steady {max_step_steady:.4})"
    );
}

/// Universal + mix=0 null: with `mix = 0` the output is exactly the dry input (the feedback
/// loop still runs internally but is not tapped to the output). This replaces the lag-0
/// single-coherent-peak check, which does not apply to a time-delay effect (see module docs).
#[test]
fn mix_zero_nulls_against_dry() {
    let sr = 48_000.0f32;
    let n = 48_000usize;
    let dry = testsig::sine(220.0, 0.5, n, sr);

    let mut s = self_osc_settings();
    s.mix = 0.0;
    s.out_db = 0.0;
    s.feedback = 1.0;

    let mut core = OuroCore::new(sr);
    let mut out = dry.clone();
    core.process_mono(&mut out, &s);

    let mse = (0..n).map(|i| (dry[i] - out[i]).powi(2)).sum::<f32>() / n as f32;
    let resid = 20.0 * mse.sqrt().max(1.0e-12).log10();
    assert!(resid < -80.0, "mix=0 did not null: residual {resid:.1} dB");
}

/// Freeze holds a sustained tail with the input muted (fb → 100 %, click-free): after freezing
/// and cutting the input, the output stays audible and bounded.
#[test]
fn freeze_sustains_without_input() {
    let sr = 48_000.0f32;
    let pre = (sr * 1.0) as usize; // 1 s of excitation, unfrozen
    let hold = (sr * 4.0) as usize; // 4 s frozen with no input

    let mut s = Settings::default();
    s.delay_ms = 180.0;
    s.feedback = 0.5;
    s.mix = 1.0;
    s.slots[0] = SlotSettings {
        kind: SlotType::FilterLp,
        amount: 0.7,
        param: 0.2,
    };

    let mut core = OuroCore::new(sr);

    // Phase 1: feed noise, not frozen.
    let exc = testsig::white_noise(0.4, pre, 99);
    let mut ph1 = exc.clone();
    core.process_mono(&mut ph1, &s);

    // Phase 2: freeze + no input.
    s.freeze = true;
    let mut ph2 = vec![0.0f32; hold];
    core.process_mono(&mut ph2, &s);

    assert!(ph2.iter().all(|v| v.is_finite()));
    let pk = peak(&ph2);
    assert!(pk <= 1.0, "frozen peak {pk} exceeded 0 dBFS");
    // Still audible near the end of the 4 s hold (freeze forces ~100 % feedback).
    let tail = &ph2[hold - sr as usize..];
    assert!(
        rms_db(tail) > -40.0,
        "freeze did not sustain: tail RMS {:.2} dBFS",
        rms_db(tail)
    );
}

/// Every slot type must stay finite and bounded under extreme macros in the feedback loop.
#[test]
fn all_slot_types_finite_and_bounded() {
    let sr = 48_000.0f32;
    let input = testsig::white_noise(0.9, (sr * 2.0) as usize, 7);
    for k in 1..=8usize {
        let kind = SlotType::from_index(k);
        let mut s = Settings::default();
        s.delay_ms = 90.0;
        s.feedback = 1.1;
        s.decay_scale = 1.0;
        s.mix = 1.0;
        // Same effect in all three slots, extreme macros.
        for slot in s.slots.iter_mut() {
            *slot = SlotSettings {
                kind,
                amount: 0.95,
                param: 0.85,
            };
        }
        let mut core = OuroCore::new(sr);
        let mut out = input.clone();
        core.process_mono(&mut out, &s);
        assert!(
            out.iter().all(|v| v.is_finite()),
            "slot {kind:?} produced NaN/inf"
        );
        let pk = peak(&out);
        assert!(pk <= 1.0, "slot {kind:?} peak {pk} exceeded 0 dBFS");
    }
}

/// The Hilbert pair used by the freq-shifter is ~quadrature: the analytic magnitude of a pure
/// tone is roughly constant (a sanity check on the allpass network, not a done-bar gate).
#[test]
fn hilbert_pair_is_approximately_analytic() {
    let sr = 48_000.0f32;
    let n = 8_192usize;
    let mut hb = Hilbert::new();
    // Skip the filter warm-up, then measure envelope flatness over a steady window.
    let mut env_min = f32::INFINITY;
    let mut env_max = 0.0f32;
    for i in 0..n {
        let x = (TAU * 2_000.0 * i as f32 / sr).sin();
        let (ip, qp) = hb.process(x);
        if i > n / 2 {
            let env = (ip * ip + qp * qp).sqrt();
            env_min = env_min.min(env);
            env_max = env_max.max(env);
        }
    }
    // A perfect Hilbert pair gives a flat envelope; allow generous ripple for a 4-section design.
    assert!(env_max.is_finite() && env_min > 0.0);
    let ripple = env_max / env_min;
    assert!(
        ripple < 1.6,
        "Hilbert envelope ripple {ripple:.3} too high (pair not ~quadrature)"
    );
}
