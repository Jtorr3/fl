//! PLUCK offline harness — done-bar assertions + render artifacts.
//!
//! Done bar (PRD §4 + PLUCK-specific):
//!  (1) C-minor trigger → spectral peaks at the chord fundamentals ±10 cents (per string).
//!  (2) tail decays > 20 dB over the decay setting's expected window (2 settings).
//!  (3) strum: per-string onset times staggered by strum-time/5 ±20% in the configured dir.
//!  (4) MIDI mode: held E2+G2+B2 retunes the strings (peaks move accordingly).
//!  + universal (finite, ≤0 dBFS, non-silent) on all 6 preset renders (both channels);
//!  + mix = 0 nulls against dry < −80 dB.

use crate::dsp::{midi_to_freq, Chord, PluckCore, Settings, StrumDir, TuningSource, MAX_STRINGS};
use crate::presets;
use suite_core::harness::{render_path, write_wav};
use suite_core::presets::load_all;
use suite_core::testsig;

const SR: f32 = 48_000.0;

#[test]
fn manual_covers_all_params_and_has_recipes() {
    suite_core::manual::assert_manual_covers_params(
        crate::MANUAL_DOC,
        &crate::PluckParams::default(),
    );
}

/// A percussive exciter: two short white-noise bursts (strum onsets), then silence.
fn exciter(len: usize) -> Vec<f32> {
    let noise = testsig::white_noise(0.5, len, 0x51EED);
    let mut x = vec![0.0f32; len];
    let burst = (0.020 * SR) as usize; // 20 ms
    for n in 0..len {
        let in_first = n < burst;
        let in_second = n >= (SR as usize) && n < (SR as usize) + burst;
        if in_first || in_second {
            x[n] = noise[n];
        }
    }
    x
}

/// Windowed single-frequency DFT magnitude (Hann) over `sig` at `freq`.
fn mag_at(sig: &[f32], freq: f32, sr: f32) -> f64 {
    let n = sig.len();
    let w = 2.0 * std::f64::consts::PI * freq as f64 / sr as f64;
    let mut re = 0.0f64;
    let mut im = 0.0f64;
    for (i, &x) in sig.iter().enumerate() {
        let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n as f64 - 1.0)).cos();
        let v = x as f64 * win;
        let ph = w * i as f64;
        re += v * ph.cos();
        im += v * ph.sin();
    }
    (re * re + im * im).sqrt()
}

/// Locate the spectral peak within ±`span` cents of `target` and return its cents error.
fn peak_cents_error(sig: &[f32], target: f32, sr: f32, span: f32) -> f32 {
    let mut best_f = target;
    let mut best_m = -1.0f64;
    let steps = (span * 2.0 / 0.5) as i32; // 0.5-cent grid
    for k in 0..=steps {
        let cents = -span + 0.5 * k as f32;
        let f = target * 2.0f32.powf(cents / 1200.0);
        let m = mag_at(sig, f, sr);
        if m > best_m {
            best_m = m;
            best_f = f;
        }
    }
    1200.0 * (best_f / target).log2()
}

fn frame_rms(sig: &[f32], start: usize, win: usize) -> f32 {
    let end = (start + win).min(sig.len());
    if start >= end {
        return 0.0;
    }
    let mut acc = 0.0f64;
    for &x in &sig[start..end] {
        acc += (x as f64) * (x as f64);
    }
    (acc / (end - start) as f64).sqrt() as f32
}

// --------------------------------------------------------------------------
// (1) C-minor trigger → per-string peaks within ±10 cents.
// --------------------------------------------------------------------------

#[test]
fn c_minor_peaks_within_10_cents() {
    let len = (2.0 * SR) as usize;
    let input = exciter(len);
    let s = Settings {
        source: TuningSource::Chord,
        root_pc: 0, // C
        chord: Chord::Minor,
        decay: 0.9,
        damp: 0.25,
        strum_ms: 12.0,
        exciter_gain: 1.4,
        body: 0.0,       // isolate the strings for clean peaks
        spread_cents: 0.0,
        stereo_alt: 0.0, // both channels carry the full string sum
        mix: 1.0,
        ..Settings::default()
    };
    let mut core = PluckCore::new(SR);
    let (l, _r) = core.process_stereo(&input, &s);
    let expected = core.tuned_freqs();

    // Analyze the sustained tail (0.7 s .. 1.7 s), after the pick transient has decayed.
    let a = (0.7 * SR) as usize;
    let b = (1.7 * SR) as usize;
    let tail = &l[a..b.min(l.len())];

    for (i, &f) in expected.iter().enumerate() {
        let err = peak_cents_error(tail, f, SR, 40.0);
        assert!(
            err.abs() <= 10.0,
            "string {i} (f={f:.2} Hz): peak off by {err:.2} cents (want ≤10)"
        );
    }
    write_wav(&render_path("PLUCK", "c_minor"), &l, SR as u32).unwrap();
}

// --------------------------------------------------------------------------
// (2) Tail decays > 20 dB over the decay setting's expected window (2 settings).
// --------------------------------------------------------------------------

fn assert_decay_over_window(decay: f32, window_s: f32) {
    let len = ((window_s + 0.5) * SR) as usize;
    let mut input = vec![0.0f32; len];
    let noise = testsig::white_noise(0.5, len, 0xBEEF);
    for n in 0..(0.02 * SR) as usize {
        input[n] = noise[n];
    }
    let s = Settings {
        source: TuningSource::Chord,
        chord: Chord::Minor,
        decay,
        damp: 0.35,
        exciter_gain: 1.4,
        body: 0.0,
        stereo_alt: 0.0,
        mix: 1.0,
        ..Settings::default()
    };
    let mut core = PluckCore::new(SR);
    let (l, _r) = core.process_stereo(&input, &s);

    let win = (0.05 * SR) as usize;
    // Reference just after the strum has settled.
    let ref_start = (0.08 * SR) as usize;
    let ref_rms = frame_rms(&l, ref_start, win);
    let end_start = ((0.08 + window_s) * SR) as usize;
    let end_rms = frame_rms(&l, end_start, win);
    let drop_db = 20.0 * (ref_rms / end_rms.max(1e-9)).log10();
    assert!(
        drop_db > 20.0,
        "decay={decay}: tail dropped {drop_db:.1} dB over {window_s}s (want >20)"
    );
}

#[test]
fn tail_decays_short_setting() {
    // Short decay: big drop within ~0.8 s.
    assert_decay_over_window(0.3, 0.8);
}

#[test]
fn tail_decays_long_setting() {
    // Long decay: still >20 dB over a longer 3 s window.
    assert_decay_over_window(0.85, 3.0);
}

// --------------------------------------------------------------------------
// (3) Strum stagger = strum-time/5 ±20% in the configured direction.
// --------------------------------------------------------------------------

fn strum_onsets(dir: StrumDir, strum_ms: f32) -> [usize; MAX_STRINGS] {
    let len = (0.5 * SR) as usize;
    let mut input = vec![0.0f32; len];
    let noise = testsig::white_noise(0.5, len, 0x1234);
    for n in 0..(0.02 * SR) as usize {
        input[n] = noise[n];
    }
    let s = Settings {
        source: TuningSource::Chord,
        chord: Chord::Minor,
        strum_ms,
        dir,
        exciter_gain: 1.2,
        ..Settings::default()
    };
    let mut core = PluckCore::new(SR);
    let _ = core.process_stereo(&input, &s);
    core.last_onsets()
}

#[test]
fn strum_stagger_up_and_down() {
    let strum_ms = 50.0;
    let expected_stride = (strum_ms * 0.001 * SR) / (MAX_STRINGS as f32 - 1.0); // strum_time/5
    let tol = expected_stride * 0.2;

    // Up: onsets increase with string index.
    let up = strum_onsets(StrumDir::Up, strum_ms);
    for i in 1..MAX_STRINGS {
        let d = up[i] as f32 - up[i - 1] as f32;
        assert!(d > 0.0, "up: string {i} should start after {}", i - 1);
        assert!(
            (d - expected_stride).abs() <= tol,
            "up stride {d} vs expected {expected_stride} (±{tol})"
        );
    }

    // Down: onsets decrease with string index.
    let down = strum_onsets(StrumDir::Down, strum_ms);
    for i in 1..MAX_STRINGS {
        let d = down[i - 1] as f32 - down[i] as f32;
        assert!(d > 0.0, "down: string {i} should start before {}", i - 1);
        assert!(
            (d - expected_stride).abs() <= tol,
            "down stride {d} vs expected {expected_stride} (±{tol})"
        );
    }
}

// --------------------------------------------------------------------------
// (4) MIDI mode: held E2+G2+B2 retunes the strings.
// --------------------------------------------------------------------------

#[test]
fn midi_held_retunes_strings() {
    let len = (2.0 * SR) as usize;
    let input = exciter(len);
    // E2, G2, B2.
    let e2 = midi_to_freq(40.0);
    let g2 = midi_to_freq(43.0);
    let b2 = midi_to_freq(47.0);
    let mut held = [f32::NAN; MAX_STRINGS];
    held[0] = e2;
    held[1] = g2;
    held[2] = b2;

    let s = Settings {
        source: TuningSource::Midi,
        held,
        held_count: 3,
        decay: 0.9,
        damp: 0.25,
        strum_ms: 12.0,
        exciter_gain: 1.4,
        body: 0.0,
        spread_cents: 0.0,
        stereo_alt: 0.0,
        mix: 1.0,
        ..Settings::default()
    };
    let mut core = PluckCore::new(SR);
    let (l, _r) = core.process_stereo(&input, &s);
    let a = (0.7 * SR) as usize;
    let b = (1.7 * SR) as usize;
    let tail = &l[a..b.min(l.len())];

    for (name, f) in [("E2", e2), ("G2", g2), ("B2", b2)] {
        let err = peak_cents_error(tail, f, SR, 45.0);
        assert!(
            err.abs() <= 15.0,
            "{name} ({f:.2} Hz): peak off by {err:.2} cents (want ≤15)"
        );
    }
    // And these differ from the C-chord tuning: no strong peak where C2 (65.4) would be.
    let c2 = midi_to_freq(36.0);
    let e2_mag = mag_at(tail, e2, SR);
    let c2_mag = mag_at(tail, c2, SR);
    assert!(
        e2_mag > c2_mag * 2.0,
        "MIDI tuning should favor E2 over C2 (e2={e2_mag:.3}, c2={c2_mag:.3})"
    );
    write_wav(&render_path("PLUCK", "midi_egb"), &l, SR as u32).unwrap();
}

// --------------------------------------------------------------------------
// Universal assertions across all 6 factory presets (both channels).
// --------------------------------------------------------------------------

#[test]
fn presets_pass_universal() {
    let factory = load_all(presets::PRESET_JSON);
    assert!(factory.len() >= 6, "need ≥6 presets, got {}", factory.len());
    let len = (2.0 * SR) as usize;
    let input = exciter(len);

    for p in &factory {
        let s = presets::settings_from_preset(p);
        let mut core = PluckCore::new(SR);
        let (l, r) = core.process_stereo(&input, &s);
        for (ch, sig) in [("L", &l), ("R", &r)] {
            assert!(
                !suite_core::harness::has_nan_or_inf(sig),
                "{}/{ch}: NaN/inf",
                p.name
            );
            let peak = suite_core::harness::peak_dbfs(sig);
            assert!(peak <= 0.01, "{}/{ch}: peak {peak:.2} dBFS > 0", p.name);
            let rms = suite_core::harness::rms_dbfs(sig);
            assert!(rms > -60.0, "{}/{ch}: RMS {rms:.1} dBFS (silent)", p.name);
        }
        let slug = p.name.to_lowercase().replace(' ', "_");
        write_wav(&render_path("PLUCK", &slug), &l, SR as u32).unwrap();
    }
}

// --------------------------------------------------------------------------
// mix = 0 nulls against dry.
// --------------------------------------------------------------------------

#[test]
fn mix_zero_nulls_against_dry() {
    let len = (1.0 * SR) as usize;
    let input = exciter(len);
    let s = Settings {
        mix: 0.0,
        out_db: 0.0,
        ..Settings::default()
    };
    let mut core = PluckCore::new(SR);
    let (l, r) = core.process_stereo(&input, &s);
    // Dry path is a direct copy of the (mono→both) input; at mix=0 out == in.
    let res_l = suite_core::harness::null_residual_db(&input, &l);
    let res_r = suite_core::harness::null_residual_db(&input, &r);
    assert!(res_l < -80.0, "L mix=0 residual {res_l:.1} dB (want < -80)");
    assert!(res_r < -80.0, "R mix=0 residual {res_r:.1} dB (want < -80)");
}

// --------------------------------------------------------------------------
// (5) Alloc-guard (HARD CHECKPOINT 3, MAJOR): the per-block RT path must not allocate. The
// MIDI-with-held-notes branch of `compute_freqs` (reached from the block-rate `configure()`)
// used to `collect()` + stable-`sort_by` a `Vec<f32>` every block — a heap alloc on the audio
// thread. A thread-local counting global allocator counts allocations only while ARMED on THIS
// thread, so parallel test threads don't perturb the count; const-initialised thread-locals
// never allocate, so the allocator hook is re-entrancy-safe.
// --------------------------------------------------------------------------

mod alloc_guard {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        pub static ARMED: Cell<bool> = const { Cell::new(false) };
        pub static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    #[inline]
    fn bump() {
        let _ = ARMED.try_with(|a| {
            if a.get() {
                let _ = COUNT.try_with(|c| c.set(c.get() + 1));
            }
        });
    }

    pub struct Counting;
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            bump();
            System.alloc(l)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            System.dealloc(p, l)
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
            bump();
            System.realloc(p, l, new)
        }
    }
}

#[global_allocator]
static GLOBAL: alloc_guard::Counting = alloc_guard::Counting;

#[test]
fn midi_held_process_block_is_alloc_free() {
    let mut core = PluckCore::new(SR);
    let mut held = [f32::NAN; MAX_STRINGS];
    held[0] = midi_to_freq(40.0); // E2
    held[1] = midi_to_freq(43.0); // G2
    held[2] = midi_to_freq(47.0); // B2
    let s = Settings {
        source: TuningSource::Midi,
        held,
        held_count: 3,
        mix: 1.0,
        ..Settings::default()
    };

    // Warm up OUTSIDE the guard — first-block smoother priming may legitimately allocate.
    core.configure(&s);
    for _ in 0..256 {
        let _ = core.process_sample(0.01, -0.01, &s);
    }

    // Measure one full block on the RT path: block-rate configure() (the MIDI branch that used
    // to collect+sort a Vec of held notes) + a 512-sample process loop. Must not allocate.
    alloc_guard::COUNT.with(|c| c.set(0));
    alloc_guard::ARMED.with(|a| a.set(true));
    core.configure(&s);
    for _ in 0..512 {
        let _ = core.process_sample(0.02, 0.015, &s);
    }
    alloc_guard::ARMED.with(|a| a.set(false));

    let n = alloc_guard::COUNT.with(|c| c.get());
    assert_eq!(n, 0, "MIDI-mode block allocated {n} time(s) on the RT thread (must be 0)");
}

// --------------------------------------------------------------------------
// (6) BODY_LEN spec compliance (HARD CHECKPOINT 3, MINOR): SPECS "PLUCK" mandates a 2048-tap
// body IR. Confirm the direct-FIR body convolution at that length stays within the RT budget.
// The printed figure is the number recorded in the checkpoint decision.
// --------------------------------------------------------------------------

#[test]
fn body_conv_within_rt_budget() {
    assert_eq!(crate::dsp::BODY_LEN, 2048, "SPECS mandates a 2048-tap body IR");
    // Take the best of a few samples (least scheduler noise).
    let mut best = f32::INFINITY;
    for _ in 0..5 {
        let p = crate::dsp::bench_body_rt_percent(48_000.0, 2.0);
        if p < best {
            best = p;
        }
    }
    println!("PLUCK body IR ({} taps) direct-FIR conv = {best:.2}% RT @48k stereo", crate::dsp::BODY_LEN);
    // Expected < 5% RT for the body conv on the build machine; a generous 25% ceiling guards
    // against a gross regression without being flaky on a loaded CI box.
    assert!(best < 25.0, "body conv {best:.2}% RT exceeds the sanity ceiling");
}
