//! SWARM done-bar + universal DSP tests (PRD §4 + build brief).
//!
//! Done bar (build brief): universal assertions, plus SWARM-specific —
//! (1) **onset / energy-burst count scales monotonically with density** across 3 settings
//!     (5 / 50 / 200 grains/s) on an **impulse-seeded frozen buffer** — each grain that reads
//!     across the single seeded impulse emits one sharp click, so onsets are countable even as
//!     grains overlap (counting method documented in `onset_count`);
//! (2) **freeze with silent input sustains output** (RMS > −50 dBFS over 5 s);
//! (3) **110 %-style shimmer feedback stays bounded** (peak ≤ 0 dBFS, no NaN over 30 s).
//!
//! Note on partial-mix coherence: SWARM is a time-smearing granular effect with **no lag-0 wet**,
//! so the suite's `assert_single_coherent_peak` (a lag-0 dry/wet alignment check) does not apply.
//! We assert `mix = 0` nulls against the dry input instead (see `mix_zero_nulls_against_dry`),
//! exactly as OUROBOROS does for its delay loop.

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

/// Count energy-burst onsets: threshold crossings of `|x|` above `frac`·(global peak), with a
/// refractory gap so one sharp click counts once. Relative threshold ⇒ robust to the
/// density-normalisation gain (which shrinks per-grain amplitude as density rises).
fn onset_count(x: &[f32], frac: f32, refractory: usize) -> usize {
    let pk = peak(x);
    if pk <= 0.0 {
        return 0;
    }
    let thr = frac * pk;
    let mut count = 0usize;
    let mut last: isize = -(refractory as isize) - 1;
    for (i, &v) in x.iter().enumerate() {
        if v.abs() >= thr && (i as isize - last) >= refractory as isize {
            count += 1;
            last = i as isize;
        }
    }
    count
}

/// DONE-BAR (1): onset count is monotonic in density. Impulse-seeded, frozen buffer; only the
/// density changes between the three renders (size/spray/pitch fixed).
#[test]
fn onset_count_scales_with_density() {
    let sr = 48_000.0f32;
    let dur = (sr * 4.0) as usize;

    let run = |density: f32| -> usize {
        let mut s = Settings::default();
        s.density = density;
        s.size_ms = 100.0;
        s.spray_ms = 100.0;
        s.scatter_st = 0.0; // unity pitch ⇒ each grain reads the impulse cleanly, once
        s.quantize = false;
        s.reverse_prob = 0.0;
        s.shimmer = 0.0;
        s.freeze = true; // lock the write head so the seeded impulse stays put
        s.width = 0.0;
        s.mix = 1.0;
        s.out_db = 0.0;

        let mut core = SwarmCore::new(sr);
        core.seed_impulse_at_readhead();
        let mut out = vec![0.0f32; dur]; // silent input; grains read the frozen buffer
        core.process_mono(&mut out, &s);
        assert!(out.iter().all(|v| v.is_finite()), "density {density}: NaN/inf");
        // 3 ms refractory: sharp impulse clicks are well separated in time.
        onset_count(&out, 0.15, (sr * 0.003) as usize)
    };

    let c_low = run(5.0);
    let c_mid = run(50.0);
    let c_high = run(200.0);

    assert!(c_low > 0, "no onsets at low density (counting failed)");
    assert!(
        c_low < c_mid && c_mid < c_high,
        "onset count not monotonic in density: 5→{c_low}, 50→{c_mid}, 200→{c_high}"
    );
}

/// DONE-BAR (2): freeze with a silent input sustains output. Fill the buffer with noise (write
/// head running), then freeze and feed silence for 5 s — grains keep reading the frozen buffer.
#[test]
fn freeze_sustains_without_input() {
    let sr = 48_000.0f32;
    let fill = (sr * 1.0) as usize;
    let hold = (sr * 5.0) as usize;

    let mut s = Settings::default();
    s.density = 60.0;
    s.size_ms = 150.0;
    s.spray_ms = 150.0;
    s.scatter_st = 3.0;
    s.width = 0.8;
    s.mix = 1.0;

    let mut core = SwarmCore::new(sr);

    // Phase 1: capture noise (not frozen).
    let mut ph1 = testsig::white_noise(0.4, fill, 99);
    core.process_mono(&mut ph1, &s);

    // Phase 2: freeze + silent input for 5 s.
    s.freeze = true;
    let mut ph2 = vec![0.0f32; hold];
    core.process_mono(&mut ph2, &s);

    assert!(ph2.iter().all(|v| v.is_finite()));
    let pk = peak(&ph2);
    assert!(pk <= 1.0, "frozen peak {pk} exceeded 0 dBFS");
    let tail_rms = rms_db(&ph2);
    assert!(
        tail_rms > -50.0,
        "freeze did not sustain: 5 s RMS {tail_rms:.2} dBFS <= -50 dBFS"
    );
}

/// Freeze Mix = 0 with Freeze engaged collapses the output to the live input — the fader
/// blends live↔frozen instead of an all-or-nothing freeze.
#[test]
fn freeze_mix_zero_passes_live() {
    let sr = 48_000.0f32;
    let mut s = Settings::default();
    s.density = 40.0;
    s.mix = 1.0;
    let mut core = SwarmCore::new(sr);
    let tone = testsig::sine(280.0, 0.4, (sr * 1.0) as usize, sr);
    // Warm the buffer (not frozen).
    core.configure(&s);
    for &x in tone.iter().take((sr * 0.4) as usize) {
        core.process_sample(x, x, &s);
    }
    s.freeze = true;
    s.freeze_mix = 0.0;
    core.configure(&s);
    let (mut resid, mut en) = (0.0f64, 0.0f64);
    for (i, &x) in tone.iter().enumerate().skip((sr * 0.4) as usize) {
        let (y, _) = core.process_sample(x, x, &s);
        if i > (sr * 0.6) as usize {
            resid += ((y - x) as f64).powi(2);
            en += (x as f64).powi(2);
        }
    }
    let residual_db = 10.0 * (resid / en.max(1e-20)).log10();
    assert!(residual_db < -40.0, "swarm freeze_mix=0 not live-passthrough: {residual_db:.1} dB");
}

/// DONE-BAR (3): 110 % shimmer feedback stays bounded (peak ≤ 0 dBFS, finite) over 30 s.
#[test]
fn shimmer_feedback_bounded() {
    let sr = 48_000.0f32;
    let total = (sr * 30.0) as usize;

    let mut s = Settings::default();
    s.density = 80.0;
    s.size_ms = 160.0;
    s.spray_ms = 150.0;
    s.scatter_st = 2.0;
    s.shimmer = 1.1; // 110 %
    s.width = 0.9;
    s.mix = 1.0;

    // Excite with a 0.25 s noise burst, then let the shimmer feedback run on its own.
    let mut input = vec![0.0f32; total];
    let burst = testsig::white_noise(0.6, (sr * 0.25) as usize, 4242);
    input[..burst.len()].copy_from_slice(&burst);

    let mut core = SwarmCore::new(sr);
    let mut out = input;
    core.process_mono(&mut out, &s);

    assert!(out.iter().all(|v| v.is_finite()), "shimmer produced NaN/inf");
    let pk = peak(&out);
    assert!(pk <= 1.0, "shimmer peak {pk} exceeded 0 dBFS");

    // The feedback must not decay to nothing across the 30 s (it should sustain/bloom).
    let sec = sr as usize;
    let late = &out[total - 5 * sec..];
    assert!(
        rms_db(late) > -60.0,
        "shimmer collapsed to silence: last-5 s RMS {:.2} dBFS",
        rms_db(late)
    );
}

/// Universal + `mix = 0` null: with `mix = 0` the output is exactly the dry input (grains and
/// the capture/shimmer path still run internally, but are not tapped to the output). Replaces the
/// lag-0 single-coherent-peak check, which does not apply to a time-smearing effect.
#[test]
fn mix_zero_nulls_against_dry() {
    let sr = 48_000.0f32;
    let n = 48_000usize;
    let dry = testsig::sine(220.0, 0.5, n, sr);

    let mut s = Settings::default();
    s.density = 120.0;
    s.shimmer = 0.5;
    s.mix = 0.0;
    s.out_db = 0.0;

    let mut core = SwarmCore::new(sr);
    let mut out = dry.clone();
    core.process_mono(&mut out, &s);

    let resid = suite_core::harness::null_residual_db(&dry, &out);
    assert!(resid < -80.0, "mix=0 did not null: residual {resid:.1} dB");
}

/// The voice cap holds: even under extreme density the active-grain count never exceeds 128.
#[test]
fn voice_cap_never_exceeded() {
    let sr = 48_000.0f32;
    let mut s = Settings::default();
    s.density = MAX_DENSITY; // 500 grains/s
    s.size_ms = MAX_SIZE_MS; // long grains ⇒ many overlap
    s.spray_ms = 300.0;
    s.mix = 1.0;

    let mut core = SwarmCore::new(sr);
    let input = testsig::pink_noise(0.5, (sr * 3.0) as usize, 7);
    core.configure(&s);
    let mut max_active = 0usize;
    for &x in &input {
        core.process_sample(x, x, &s);
        max_active = max_active.max(core.active_grains());
    }
    assert!(
        max_active <= MAX_GRAINS,
        "active grains {max_active} exceeded the {MAX_GRAINS} voice cap"
    );
    assert!(max_active > 64, "voice pool barely used ({max_active}); density not saturating");
}

/// Every render must stay finite and bounded under extreme macros (fuzz-style).
#[test]
fn extreme_settings_finite_and_bounded() {
    let sr = 48_000.0f32;
    let input = testsig::white_noise(0.9, (sr * 3.0) as usize, 13);
    let configs = [
        (MAX_DENSITY, MIN_SIZE_MS, MAX_SPRAY_MS, MAX_SCATTER_ST, 1.1f32, 1.0f32),
        (MIN_DENSITY, MAX_SIZE_MS, 0.0, 0.0, 0.0, 1.0),
        (250.0, 300.0, 400.0, 12.0, 0.8, 0.9),
    ];
    for (den, size, spray, scat, shimmer, mix) in configs {
        let mut s = Settings::default();
        s.density = den;
        s.size_ms = size;
        s.spray_ms = spray;
        s.scatter_st = scat;
        s.reverse_prob = 0.5;
        s.shimmer = shimmer;
        s.width = 1.0;
        s.mix = mix;
        let mut core = SwarmCore::new(sr);
        let mut out = input.clone();
        core.process_mono(&mut out, &s);
        assert!(
            out.iter().all(|v| v.is_finite()),
            "config den={den} size={size} produced NaN/inf"
        );
        let pk = peak(&out);
        assert!(pk <= 1.0, "config den={den} peak {pk} exceeded 0 dBFS");
    }
}
