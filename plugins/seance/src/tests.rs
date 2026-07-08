//! SEANCE done-bar + render tests (PRD §4 universal + SEANCE-specific).
//!
//! Done bars:
//! 1. +12 st (pitch, preserve on) doubles the measured f0 of a synthetic vocal ±20 cents.
//! 2. Chop gate periods match the selected BPM division ±1 ms (rendered at 120 BPM).
//! 3. Ducker (drowned-vocal swell): wet level during dry-active segments is ≥ 6 dB below the
//!    wet level in the silence after, on a burst-then-silence vocal.

use crate::dsp::{db_to_gain, Chopper, RawControls, SeanceCore, Settings, CHOP_DIVISIONS};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::harness::{assert_universal, render_path, write_wav};
use suite_core::pitch::{cents, Mpm};
use suite_core::presets::load_all;
use suite_core::testsig::{sine, synth_vocal};

const SR: f32 = 48_000.0;

fn measure_f0(sig: &[f32], win: usize) -> f32 {
    let win = win.min(sig.len());
    let start = (sig.len().saturating_sub(win)) / 2;
    let mut mpm = Mpm::new(win, SR, 60.0, 900.0);
    mpm.analyze(&sig[start..start + win]).f0_hz
}

/// Isolate the shift stage: everything else neutral (no chop/verb/wash/duck, full wet).
fn shift_only(pitch_st: f32, formant_st: f32, preserve: bool) -> Settings {
    RawControls {
        pitch_st,
        formant_st,
        preserve,
        chop_depth: 0.0,
        verb_wet: 0.0,
        verb_shimmer: 0.0,
        wash: 0.0,
        duck_depth: 0.0,
        mix: 1.0,
        ..RawControls::default()
    }
    .resolve()
}

/// (1) +12 st doubles the measured f0 (±20 cents).
#[test]
fn plus_twelve_st_doubles_f0() {
    let f0 = 150.0f32;
    let dry = synth_vocal(f0, (SR * 1.6) as usize, SR);
    let s = shift_only(12.0, 0.0, true);
    let mut core = SeanceCore::new(SR);
    let mut buf = dry.clone();
    core.process_mono(&mut buf, &s);

    let f0_dry = measure_f0(&dry, 4096);
    let f0_wet = measure_f0(&buf, 4096);
    let expected = f0_dry * 2.0;
    let err = cents(f0_wet, expected).abs();
    assert!(
        err < 20.0,
        "+12 st f0 {f0_wet:.1} Hz not double of {f0_dry:.1} (expected {expected:.1}, err {err:.1} cents)"
    );
    assert_universal(&buf);
}

/// (2) Chop gate periods match the selected division ±1 ms at 120 BPM. Measured directly on
/// the [`Chopper`] (a steady square gate), and confirmed through the full core.
#[test]
fn chop_gate_period_matches_division() {
    let bpm = 120.0f32;
    for &rate in &[1usize, 2, 4] {
        // 1/4, 1/8, 1/16
        let beats = CHOP_DIVISIONS[rate].1;
        let period_s = beats * 60.0 / bpm;

        let mut chop = Chopper::new(SR);
        chop.configure(bpm, rate, 0, 1.0); // Square, full depth
        // Record the gate over several periods; find rising-edge crossings of 0.5.
        let n = (period_s * SR * 8.0) as usize;
        let mut g = vec![0.0f32; n];
        for x in g.iter_mut() {
            *x = chop.process();
        }
        // Rising-edge times where the gate crosses 0.5 upward (skip the first period to let
        // the edge smoother settle).
        let mut edges = Vec::new();
        let warm = (period_s * SR) as usize;
        for i in (warm + 1)..n {
            if g[i - 1] < 0.5 && g[i] >= 0.5 {
                edges.push(i as f32 / SR);
            }
        }
        assert!(edges.len() >= 3, "rate {rate}: too few gate edges ({})", edges.len());
        // Mean inter-edge spacing == period_s ± 1 ms.
        let mut diffs = 0.0f32;
        for w in edges.windows(2) {
            diffs += w[1] - w[0];
        }
        let measured = diffs / (edges.len() - 1) as f32;
        let err_ms = (measured - period_s).abs() * 1000.0;
        assert!(
            err_ms < 1.0,
            "rate {rate} ({} div): gate period {measured:.5}s vs {period_s:.5}s (err {err_ms:.3} ms)",
            CHOP_DIVISIONS[rate].0
        );
    }
}

/// The full core's chopper also gates at the division period (envelope periodicity check).
#[test]
fn chop_through_full_core_is_periodic() {
    let bpm = 120.0f32;
    let rate = 2usize; // 1/8
    let period_s = CHOP_DIVISIONS[rate].1 * 60.0 / bpm;
    // Steady tone in, chopper on (square, full depth), everything else neutral, full wet.
    let s = Settings {
        chop_pattern: 0,
        chop_rate: rate,
        chop_depth: 1.0,
        tempo_bpm: bpm,
        verb_wet: 0.0,
        verb_shimmer: 0.0,
        wash: 0.0,
        duck_depth: 0.0,
        mix: 1.0,
        ..Settings::default()
    };
    let tone = sine(300.0, 0.5, (SR * 2.0) as usize, SR);
    let mut core = SeanceCore::new(SR);
    let mut buf = tone.clone();
    core.process_mono(&mut buf, &s);
    assert_universal(&buf);

    // Envelope (abs, smoothed) autocorrelation should peak near the division period.
    let env: Vec<f32> = buf.iter().map(|v| v.abs()).collect();
    let lag = (period_s * SR) as usize;
    let start = 4096usize; // past the shifter fill-in
    let corr_at = |l: usize| -> f32 {
        let mut num = 0.0f64;
        let mut d0 = 0.0f64;
        let mut dl = 0.0f64;
        for i in start..(env.len() - l) {
            num += env[i] as f64 * env[i + l] as f64;
            d0 += (env[i] as f64).powi(2);
            dl += (env[i + l] as f64).powi(2);
        }
        (num / (d0.sqrt() * dl.sqrt()).max(1e-12)) as f32
    };
    // Correlation at the period lag should exceed the correlation at half the period.
    let c_period = corr_at(lag);
    let c_half = corr_at(lag / 2);
    assert!(
        c_period > c_half,
        "chop envelope not periodic at the division (period corr {c_period:.3} !> half corr {c_half:.3})"
    );
}

/// (3) Drowned-vocal duck: wet during the dry-active burst is ≥ 6 dB below the wet in the
/// silence after. Measured on a burst-then-silence vocal, full wet, moderate verb tail.
#[test]
fn ducker_swells_in_silence() {
    // Vocal burst then silence.
    let burst = synth_vocal(160.0, (SR * 0.9) as usize, SR);
    let silence = vec![0.0f32; (SR * 1.8) as usize];
    let mut input = burst.clone();
    input.extend_from_slice(&silence);

    let s = Settings {
        chop_depth: 0.0,
        verb_wet: 0.6,
        verb_shimmer: 0.3,
        verb_size: 0.7,
        verb_decay: 3.5,
        wash: 0.0,
        duck_depth: 0.9,
        duck_release_ms: 300.0,
        mix: 1.0, // pure wet so we measure the wet level directly
        ..Settings::default()
    };
    let mut core = SeanceCore::new(SR);
    let mut buf = input.clone();
    core.process_mono(&mut buf, &s);
    assert_universal(&buf);

    let rms = |seg: &[f32]| -> f32 {
        let m = seg.iter().map(|v| (v * v) as f64).sum::<f64>() / seg.len().max(1) as f64;
        (m.sqrt() as f32).max(1e-12)
    };
    // "dry-active" window: middle of the burst (after onset, before it ends).
    let active_lo = (SR * 0.4) as usize;
    let active_hi = (SR * 0.8) as usize;
    // "silence after" window: well into the tail, after the duck release lets wet swell.
    let silent_lo = (SR * 1.3) as usize;
    let silent_hi = (SR * 1.9) as usize;

    let rms_active = rms(&buf[active_lo..active_hi]);
    let rms_silent = rms(&buf[silent_lo..silent_hi]);
    let swell_db = 20.0 * (rms_silent / rms_active).log10();
    assert!(
        swell_db >= 6.0,
        "drowned-vocal swell only {swell_db:.1} dB (wet in silence {rms_silent:.4} vs active {rms_active:.4}); need ≥ 6 dB"
    );
}

/// Every factory preset renders, passes universal assertions, and writes a WAV to
/// renders/SEANCE/. Rendered over a synthetic vocal phrase at 120 BPM.
#[test]
fn every_preset_renders_and_passes_universal() {
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6, "need ≥ 6 presets, got {}", presets.len());
    // A short vocal phrase: three notes.
    let mut input = Vec::new();
    for &f in &[147.0f32, 175.0, 196.0] {
        input.extend_from_slice(&synth_vocal(f, (SR * 0.7) as usize, SR));
    }
    input.extend_from_slice(&vec![0.0f32; (SR * 0.8) as usize]);

    for p in &presets {
        let s = settings_from_preset(p);
        let mut core = SeanceCore::new(SR);
        let mut buf = input.clone();
        core.process_mono(&mut buf, &s);
        assert_universal(&buf);
        let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
        write_wav(&render_path("SEANCE", &fname), &buf, SR as u32).expect("write render");
    }
}

/// A couple of showcase renders: a shimmer octave-up wash and a chopped-ether render.
#[test]
fn showcase_renders() {
    let mut input = Vec::new();
    for &f in &[131.0f32, 165.0, 196.0, 147.0] {
        input.extend_from_slice(&synth_vocal(f, (SR * 0.6) as usize, SR));
    }
    input.extend_from_slice(&vec![0.0f32; (SR * 1.0) as usize]);

    let shimmer = RawControls {
        pitch_st: 12.0,
        verb_shimmer: 0.7,
        verb_wet: 0.6,
        verb_decay: 4.5,
        wash: 0.5,
        mix: 0.8,
        ..RawControls::default()
    }
    .resolve();
    let mut core = SeanceCore::new(SR);
    let mut buf = input.clone();
    core.process_mono(&mut buf, &shimmer);
    assert_universal(&buf);
    write_wav(&render_path("SEANCE", "showcase_shimmer"), &buf, SR as u32).unwrap();

    let chopped = RawControls {
        pitch_st: 5.0,
        chop_pattern: 1,
        chop_rate: 4,
        chop_depth: 0.7,
        verb_wet: 0.4,
        wash: 0.3,
        mix: 0.75,
        ..RawControls::default()
    }
    .resolve();
    let mut core2 = SeanceCore::new(SR);
    let mut buf2 = input.clone();
    core2.process_mono(&mut buf2, &chopped);
    assert_universal(&buf2);
    write_wav(&render_path("SEANCE", "showcase_chopped"), &buf2, SR as u32).unwrap();
}

/// Sanity: `db_to_gain` monotonic (guards the preset `out` mapping).
#[test]
fn db_gain_sane() {
    assert!(db_to_gain(0.0) > 0.99 && db_to_gain(0.0) < 1.01);
    assert!(db_to_gain(-6.0) < db_to_gain(0.0));
}
