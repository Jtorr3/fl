//! MURMUR done-bar tests (PRD §4 + build brief). Universal assertions live in the render test
//! in `lib.rs`; these are the MURMUR-specific mechanical assertions:
//!  1. two identical impulses 2 s apart (randomness up) → tail cross-correlation < 0.9;
//!  2. measured RT60 within ±25 % of the decay setting (randomness at 0);
//!  3. freeze sustains (RMS > −50 dBFS over 5 s);
//!  4. crossfade click bound (max sample delta during a re-roll bounded vs the steady tail);
//!  5. mix = 0 nulls against the dry signal.

use crate::dsp::{MurmurCore, Settings};
use suite_core::fdn::measure_rt60;
use suite_core::harness::{null_residual_db, rms_dbfs};
use suite_core::testsig;

/// Zero-lag Pearson correlation of two equal-length signals.
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
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
    if da <= 1.0e-20 || db <= 1.0e-20 {
        return 0.0;
    }
    num / (da.sqrt() * db.sqrt())
}

fn base_settings() -> Settings {
    Settings {
        size: 0.5,
        decay: 2.0,
        color: 0.0,
        randomness: 0.0,
        sensitivity: 0.6,
        freeze: false,
        freeze_mix: 1.0,
        width: 1.0,
        mix: 1.0,
    }
}

/// Freeze Mix: with Freeze engaged and Freeze Mix = 0, the output collapses to the live input
/// (the frozen tail is fully mixed out); at Freeze Mix = 1 the frozen tail is audible. Proves
/// the fader blends live↔frozen instead of a sudden all-or-nothing freeze.
#[test]
fn freeze_mix_blends_live_vs_frozen() {
    let sr = 48_000.0f32;
    let tone = suite_core::testsig::sine(220.0, 0.4, (sr * 1.0) as usize, sr);

    // Prime a tail, then freeze with mix = 0 → output should track the live input closely.
    let mut core = MurmurCore::new(sr);
    let warm = Settings { freeze: false, mix: 1.0, ..base_settings() };
    core.configure(&warm);
    for &x in tone.iter().take((sr * 0.5) as usize) {
        core.process_sample(x, x, warm.mix);
    }
    let frozen0 = Settings { freeze: true, freeze_mix: 0.0, mix: 1.0, ..base_settings() };
    core.configure(&frozen0);
    // Let the freeze-mix smoother settle, then measure the residual vs the live input.
    let mut resid = 0.0f64;
    let mut energy = 0.0f64;
    for (i, &x) in tone.iter().enumerate().skip((sr * 0.5) as usize) {
        let (l, _r) = core.process_sample(x, x, frozen0.mix);
        if i > (sr * 0.7) as usize {
            resid += ((l - x) as f64).powi(2);
            energy += (x as f64).powi(2);
        }
    }
    let residual_db = 10.0 * (resid / energy.max(1e-20)).log10();
    assert!(residual_db < -40.0, "freeze_mix=0 not live-passthrough: {residual_db:.1} dB");

    // freeze_mix = 1 → the frozen tail sustains (non-trivial output during silence).
    let mut core2 = MurmurCore::new(sr);
    core2.configure(&warm);
    for &x in tone.iter() {
        core2.process_sample(x, x, warm.mix);
    }
    let frozen1 = Settings { freeze: true, freeze_mix: 1.0, mix: 1.0, ..base_settings() };
    core2.configure(&frozen1);
    let mut tail = 0.0f64;
    let silence = (sr * 1.5) as usize;
    for _ in 0..silence {
        let (l, _r) = core2.process_sample(0.0, 0.0, frozen1.mix);
        tail += (l as f64).powi(2);
    }
    let tail_rms = (tail / silence as f64).sqrt();
    assert!(tail_rms > 1e-4, "freeze_mix=1 tail should sustain, rms {tail_rms:.6}");
}

/// Done-bar (1): two identical impulses 2 s apart with randomness up land in different rooms —
/// their tail waveforms must decorrelate (cross-correlation < 0.9).
#[test]
fn two_impulses_land_in_different_rooms() {
    let sr = 48_000.0f32;
    let n = (sr * 4.0) as usize;
    let mut input = vec![0.0f32; n];
    input[0] = 0.9;
    input[(sr * 2.0) as usize] = 0.9;

    let s = Settings {
        randomness: 1.0,
        decay: 2.0,
        mix: 1.0,
        ..base_settings()
    };
    let mut core = MurmurCore::new(sr);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);

    // Tail windows, avoiding the 50 ms crossfade transient right after each impulse.
    let t1a = (sr * 0.15) as usize;
    let t1b = (sr * 1.8) as usize;
    let t2a = (sr * 2.15) as usize;
    let t2b = (sr * 3.8) as usize;
    let tail1 = &out[t1a..t1b];
    let tail2 = &out[t2a..t2b];
    // Both tails must be non-trivial.
    assert!(rms_dbfs(tail1) > -60.0, "tail1 silent");
    assert!(rms_dbfs(tail2) > -60.0, "tail2 silent");

    let c = correlation(tail1, tail2);
    assert!(
        c < 0.9,
        "two impulses produced correlated tails ({c:.3}) — rooms not different enough"
    );
    // Sanity: the core actually re-rolled (drew new rooms) on the onsets.
    assert!(core.draws() >= 4, "expected ≥4 room draws, got {}", core.draws());
}

/// Done-bar (2): with randomness at 0 the room is the deterministic nominal one, so the
/// measured RT60 must be within ±25 % of the `decay` setting.
#[test]
fn rt60_matches_decay_setting_at_zero_randomness() {
    let sr = 48_000.0f32;
    for &target in &[1.5f32, 3.0f32] {
        let n = (sr * target * 2.5) as usize;
        let mut input = vec![0.0f32; n];
        input[0] = 0.9;
        let s = Settings {
            randomness: 0.0,
            decay: target,
            color: -0.3, // light damping so broadband RT60 tracks the line gains
            mix: 1.0,
            ..base_settings()
        };
        let mut core = MurmurCore::new(sr);
        let mut out = input.clone();
        core.process_mono(&mut out, &s);

        let measured = measure_rt60(&out, sr).expect("RT60 measurable");
        let err = (measured - target).abs() / target;
        assert!(
            err <= 0.25,
            "RT60 target {target}s measured {measured:.3}s (err {:.1}%)",
            err * 100.0
        );
    }
}

/// Done-bar (3): freeze sustains the tail — after charging the reverb and enabling freeze, the
/// output RMS over 5 s of silence stays above −50 dBFS.
#[test]
fn freeze_sustains_the_tail() {
    let sr = 48_000.0f32;
    let mut core = MurmurCore::new(sr);

    // Charge with 1 s of pink noise (freeze off).
    let charge_settings = Settings {
        randomness: 0.0,
        decay: 3.0,
        freeze: false,
        mix: 1.0,
        ..base_settings()
    };
    let mut charge = testsig::pink_noise(0.5, sr as usize, 24601);
    core.process_mono(&mut charge, &charge_settings);

    // Freeze, then 5 s of silence — the tail must sustain.
    let freeze_settings = Settings {
        freeze: true,
        ..charge_settings
    };
    let mut tail = vec![0.0f32; (sr * 5.0) as usize];
    core.process_mono(&mut tail, &freeze_settings);

    let rms = rms_dbfs(&tail);
    assert!(rms > -50.0, "frozen tail decayed to {rms:.1} dBFS (< −50)");
    assert!(tail.iter().all(|v| v.is_finite()));
    let peak = tail.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    assert!(peak <= 1.0, "frozen tail exceeded 0 dBFS: {peak}");
}

/// Done-bar (4): a re-roll crossfade is click-free — the maximum sample-to-sample delta around
/// a manual re-roll stays bounded relative to the steady-state tail (a hard tap-switch is
/// 10–100×; the 50 ms equal-power crossfade must be only a few ×).
#[test]
fn reroll_crossfade_is_click_free() {
    let sr = 48_000.0f32;
    // Steady 220 Hz tone: no transients, so the onset detector never fires on its own.
    let tone = testsig::sine(220.0, 0.5, (sr * 3.0) as usize, sr);
    let s = Settings {
        randomness: 0.8,
        decay: 2.0,
        sensitivity: 0.0, // high trigger threshold → the steady tone won't self-trigger
        mix: 1.0,
        ..base_settings()
    };

    let max_delta = |sig: &[f32], a: usize, b: usize| -> f32 {
        let mut m = 0.0f32;
        for i in a + 1..b.min(sig.len()) {
            m = m.max((sig[i] - sig[i - 1]).abs());
        }
        m
    };

    // Baseline: no re-roll.
    let mut core = MurmurCore::new(sr);
    core.configure(&s);
    let mut baseline = vec![0.0f32; tone.len()];
    for (i, &x) in tone.iter().enumerate() {
        let (y, _) = core.process_sample(x, x, s.mix);
        baseline[i] = y;
    }
    let steady_a = (sr * 1.0) as usize;
    let steady_b = (sr * 1.4) as usize;
    let max_steady = max_delta(&baseline, steady_a, steady_b);

    // With a re-roll injected at t = 1.5 s.
    let mut core = MurmurCore::new(sr);
    core.configure(&s);
    let reroll_at = (sr * 1.5) as usize;
    let mut rolled = vec![0.0f32; tone.len()];
    for (i, &x) in tone.iter().enumerate() {
        if i == reroll_at {
            core.request_reroll();
        }
        let (y, _) = core.process_sample(x, x, s.mix);
        rolled[i] = y;
    }
    // Window spanning the whole 50 ms crossfade.
    let max_roll = max_delta(&rolled, reroll_at, reroll_at + (sr * 0.1) as usize);

    assert!(rolled.iter().all(|v| v.is_finite()));
    assert!(
        max_roll <= 4.0 * max_steady + 1.0e-4,
        "re-roll delta {max_roll:.5} exceeds 4× steady {max_steady:.5} — crossfade clicks"
    );
}

/// Universal: mix = 0 leaves the signal untouched (nulls against the dry input).
#[test]
fn mix_zero_nulls_against_dry() {
    let sr = 48_000.0f32;
    let input = testsig::pink_noise(0.5, (sr * 2.0) as usize, 999);
    let s = Settings {
        randomness: 0.7,
        mix: 0.0,
        ..base_settings()
    };
    let mut core = MurmurCore::new(sr);
    let mut out = input.clone();
    core.process_mono(&mut out, &s);
    let null = null_residual_db(&input, &out);
    assert!(null < -80.0, "mix=0 did not null against dry: {null:.1} dB");
}
