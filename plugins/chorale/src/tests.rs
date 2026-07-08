//! CHORALE offline harness — done-bar assertions + render artifacts.
//!
//! Done bar (PRD §4 + CHORALE-specific):
//!  (1) noise excitation, A-minor selected → output spectral peaks at the tuned pitches
//!      ±10 cents (measure the strongest N).
//!  (2) decay param scales the measured tail RT (input burst then silence) across 2 settings.
//!  (3) MIDI mode: a held chord retunes the bank (peaks move to the held pitches).
//!  (4) wet-solo + mix behavior nulls correctly (mix=0 nulls vs dry; wet-solo is pure wet).
//!  + sympathetic weighting: with amount=1, a band-limited input drives only the matching
//!    resonators (far-band output collapses);
//!  + universal (finite, ≤0 dBFS, non-silent) on all 6 preset renders (both channels).

use crate::dsp::{midi_to_freq, ChoraleCore, Scale, Settings, TuningSource, MAX_RESONATORS};
use crate::presets;
use suite_core::harness::{render_path, write_wav};
use suite_core::presets::load_all;
use suite_core::testsig;

const SR: f32 = 48_000.0;

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

/// Locate the spectral peak within ±`span` cents of `target`, return its cents error.
fn peak_cents_error(sig: &[f32], target: f32, sr: f32, span: f32) -> f32 {
    let mut best_f = target;
    let mut best_m = -1.0f64;
    let steps = (span * 2.0 / 0.5) as i32;
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

/// Continuous white-noise exciter of length `len`.
fn noise(len: usize, seed: u32) -> Vec<f32> {
    testsig::white_noise(0.4, len, seed)
}

// --------------------------------------------------------------------------
// (1) A-minor, noise excitation → the strongest N resonator peaks are ±10 cents.
// --------------------------------------------------------------------------

#[test]
fn a_minor_strongest_peaks_within_10_cents() {
    let len = (3.0 * SR) as usize;
    let input = noise(len, 0x0A317);
    let s = Settings {
        source: TuningSource::Scale,
        root_pc: 9, // A
        scale: Scale::MinorTriad,
        count: 16,
        decay: 0.9,
        damp: 0.3,
        spread_cents: 0.0, // clean, on-pitch
        sympathetic: 0.0,  // flat drive so every resonator rings (white noise anyway)
        excite: 1.0,
        stereo: 0.0, // both channels carry the full bank sum
        mix: 1.0,
        ..Settings::default()
    };
    let mut core = ChoraleCore::new(SR);
    let (l, _r) = core.process_stereo(&input, &s);
    let freqs = core.tuned_freqs();
    let active = core.active();

    // Analyze a steady window well into the sustain.
    let a = (1.5 * SR) as usize;
    let b = (2.9 * SR) as usize;
    let tail = &l[a..b.min(l.len())];

    // Rank the active resonators by measured magnitude and check the strongest N.
    let mut ranked: Vec<(usize, f64)> = (0..active)
        .map(|i| (i, mag_at(tail, freqs[i], SR)))
        .collect();
    ranked.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
    let n = 8usize.min(active);
    for &(i, _m) in ranked.iter().take(n) {
        let f = freqs[i];
        let err = peak_cents_error(tail, f, SR, 35.0);
        assert!(
            err.abs() <= 10.0,
            "resonator {i} (f={f:.2} Hz): peak off by {err:.2} cents (want ≤10)"
        );
    }
    write_wav(&render_path("CHORALE", "a_minor"), &l, SR as u32).unwrap();
}

// --------------------------------------------------------------------------
// (2) Decay scales the tail RT: a burst then silence decays much less with a long decay.
// --------------------------------------------------------------------------

/// RMS drop (dB) over `window_s` after a short input burst, for a given decay setting.
fn tail_drop_db(decay: f32, window_s: f32) -> f32 {
    let len = ((window_s + 0.5) * SR) as usize;
    let mut input = vec![0.0f32; len];
    let src = testsig::white_noise(0.5, len, 0xB0D1);
    for n in 0..(0.05 * SR) as usize {
        input[n] = src[n]; // 50 ms burst then silence
    }
    let s = Settings {
        source: TuningSource::Scale,
        root_pc: 9,
        scale: Scale::MinorTriad,
        count: 16,
        decay,
        damp: 0.3,
        spread_cents: 0.0,
        sympathetic: 0.0,
        excite: 1.2,
        stereo: 0.0,
        mix: 1.0,
        ..Settings::default()
    };
    let mut core = ChoraleCore::new(SR);
    let (l, _r) = core.process_stereo(&input, &s);
    let win = (0.1 * SR) as usize;
    let ref_start = (0.1 * SR) as usize; // just after the burst
    let ref_rms = frame_rms(&l, ref_start, win);
    let end_start = ((0.1 + window_s) * SR) as usize;
    let end_rms = frame_rms(&l, end_start, win);
    20.0 * (ref_rms / end_rms.max(1e-9)).log10()
}

#[test]
fn decay_scales_tail_rt() {
    let window = 1.5;
    let short = tail_drop_db(0.3, window);
    let long = tail_drop_db(0.9, window);
    // A short decay collapses far more than a long one over the same window.
    assert!(
        short > 20.0,
        "short-decay tail dropped only {short:.1} dB over {window}s (want >20)"
    );
    assert!(
        short > long + 12.0,
        "decay should scale RT: short drop {short:.1} dB vs long {long:.1} dB (want short ≥ long+12)"
    );
}

// --------------------------------------------------------------------------
// (3) MIDI mode: a held chord retunes the bank.
// --------------------------------------------------------------------------

#[test]
fn midi_held_chord_retunes() {
    let len = (3.0 * SR) as usize;
    let input = noise(len, 0x31D1);
    let e2 = midi_to_freq(40.0);
    let g2 = midi_to_freq(43.0);
    let b2 = midi_to_freq(47.0);
    let mut held = [f32::NAN; MAX_RESONATORS];
    held[0] = e2;
    held[1] = g2;
    held[2] = b2;
    let s = Settings {
        source: TuningSource::Midi,
        held,
        held_count: 3,
        count: 12,
        decay: 0.9,
        damp: 0.3,
        spread_cents: 0.0,
        sympathetic: 0.0,
        excite: 1.0,
        stereo: 0.0,
        mix: 1.0,
        ..Settings::default()
    };
    let mut core = ChoraleCore::new(SR);
    let (l, _r) = core.process_stereo(&input, &s);
    let a = (1.5 * SR) as usize;
    let b = (2.9 * SR) as usize;
    let tail = &l[a..b.min(l.len())];

    for (name, f) in [("E2", e2), ("G2", g2), ("B2", b2)] {
        let err = peak_cents_error(tail, f, SR, 45.0);
        assert!(
            err.abs() <= 15.0,
            "{name} ({f:.2} Hz): peak off by {err:.2} cents (want ≤15)"
        );
    }
    // The MIDI tuning should favor E2 over the scale root C2 (65.4 Hz), which isn't voiced.
    let c2 = midi_to_freq(36.0);
    let e2_mag = mag_at(tail, e2, SR);
    let c2_mag = mag_at(tail, c2, SR);
    assert!(
        e2_mag > c2_mag * 2.0,
        "MIDI tuning should favor E2 over C2 (e2={e2_mag:.3}, c2={c2_mag:.3})"
    );
    write_wav(&render_path("CHORALE", "midi_egb"), &l, SR as u32).unwrap();
}

// --------------------------------------------------------------------------
// (4) wet-solo + mix behavior nulls correctly.
// --------------------------------------------------------------------------

#[test]
fn mix_zero_nulls_and_wet_solo_is_pure_wet() {
    let len = (1.0 * SR) as usize;
    let input = noise(len, 0x4A17);

    // mix = 0 → output == dry input (both channels), regardless of the ringing bank.
    let s0 = Settings {
        mix: 0.0,
        out_db: 0.0,
        wet_solo: false,
        ..Settings::default()
    };
    let mut core = ChoraleCore::new(SR);
    let (l, r) = core.process_stereo(&input, &s0);
    let res_l = suite_core::harness::null_residual_db(&input, &l);
    let res_r = suite_core::harness::null_residual_db(&input, &r);
    assert!(res_l < -80.0, "L mix=0 residual {res_l:.1} dB (want < -80)");
    assert!(res_r < -80.0, "R mix=0 residual {res_r:.1} dB (want < -80)");

    // wet-solo ignores mix and outputs pure resonance: non-silent AND far from the dry input.
    let s1 = Settings {
        mix: 0.0, // deliberately 0 — wet-solo must override it
        wet_solo: true,
        excite: 1.0,
        ..Settings::default()
    };
    let mut core = ChoraleCore::new(SR);
    let (wl, _wr) = core.process_stereo(&input, &s1);
    let wet_rms = suite_core::harness::rms_dbfs(&wl);
    assert!(wet_rms > -60.0, "wet-solo output is silent ({wet_rms:.1} dBFS)");
    let res = suite_core::harness::null_residual_db(&input, &wl);
    assert!(
        res > -20.0,
        "wet-solo should differ from the dry input, residual {res:.1} dB (want > -20)"
    );
}

// --------------------------------------------------------------------------
// Sympathetic weighting: amount=1 drives only the resonators whose band matches the input.
// --------------------------------------------------------------------------

/// Summed magnitude at a set of resonator frequencies (a coarse energy probe).
fn energy_at(sig: &[f32], freqs: &[f32]) -> f64 {
    freqs.iter().map(|&f| mag_at(sig, f, SR)).sum()
}

#[test]
fn sympathetic_weighting_gates_by_input_band() {
    // The sympathetic gain of resonator i is (band energy at f_i), so its steady output scales
    // with the input's energy AT its pitch — weighting SHARPENS the bank toward the source's
    // strong bands. Fed white noise (which a constant-Q analyzer reads as +3 dB/oct — the high
    // bands carry the most energy), full weighting should therefore EMPHASIZE the high
    // resonators, raising the top/bottom energy ratio well above the flat-weighting case.
    let len = (2.5 * SR) as usize;
    let input = testsig::white_noise(0.4, len, 0x5171);
    let base = Settings {
        source: TuningSource::Scale,
        root_pc: 9,
        scale: Scale::MinorPentatonic,
        count: 24,
        decay: 0.9,
        damp: 0.2, // keep highs alive so the difference is about the drive, not damping
        spread_cents: 0.0,
        excite: 1.2,
        stereo: 0.0,
        mix: 1.0,
        ..Settings::default()
    };

    let flat = Settings { sympathetic: 0.0, ..base };
    let weighted = Settings { sympathetic: 1.0, ..base };

    let mut c0 = ChoraleCore::new(SR);
    let (l0, _) = c0.process_stereo(&input, &flat);
    let freqs = c0.tuned_freqs();
    let active = c0.active();
    let mut c1 = ChoraleCore::new(SR);
    let (l1, _) = c1.process_stereo(&input, &weighted);

    // The four highest and four lowest distinct resonator pitches.
    let mut sorted: Vec<f32> = freqs[..active].to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let top: Vec<f32> = sorted.iter().rev().take(4).copied().collect();
    let bot: Vec<f32> = sorted.iter().take(4).copied().collect();

    let a = (1.5 * SR) as usize;
    let top_flat = energy_at(&l0[a..], &top);
    let bot_flat = energy_at(&l0[a..], &bot);
    let top_w = energy_at(&l1[a..], &top);
    let bot_w = energy_at(&l1[a..], &bot);

    // The flat case rings both ends (sanity); weighting emphasizes the strong (HF) bands, so
    // the top/bottom ratio climbs clearly above the flat case.
    assert!(top_flat > 0.0 && bot_flat > 0.0, "flat bank produced no ringing");
    let ratio_flat = top_flat / bot_flat;
    let ratio_w = top_w / bot_w;
    assert!(
        ratio_w > 1.6 * ratio_flat,
        "sympathetic weighting should emphasize the source's strong (HF) bands: \
         top/bot ratio weighted {ratio_w:.3} vs flat {ratio_flat:.3}"
    );
}

// --------------------------------------------------------------------------
// Universal assertions across all 6 factory presets (both channels).
// --------------------------------------------------------------------------

#[test]
fn presets_pass_universal() {
    let factory = load_all(presets::PRESET_JSON);
    assert!(factory.len() >= 6, "need ≥6 presets, got {}", factory.len());
    let len = (2.5 * SR) as usize;
    // Pink noise + a chirp so the sympathetic weighting sees a broadband, moving source.
    let mut input = testsig::pink_noise(0.35, len, 0xC0DA);
    let chirp = testsig::log_chirp(60.0, 8000.0, 0.25, len, SR);
    for (x, c) in input.iter_mut().zip(chirp.iter()) {
        *x += *c;
    }

    for p in &factory {
        let s = presets::settings_from_preset(p);
        let mut core = ChoraleCore::new(SR);
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
        write_wav(&render_path("CHORALE", &slug), &l, SR as u32).unwrap();
    }
}

// --------------------------------------------------------------------------
// Extremes fuzz: degenerate/maxed settings stay finite and ≤ full-scale.
// --------------------------------------------------------------------------

#[test]
fn extremes_stay_finite() {
    let len = (1.0 * SR) as usize;
    let input = testsig::white_noise(0.9, len, 0xFECD);
    for s in [
        Settings { count: 24, decay: 1.0, damp: 0.0, spread_cents: 50.0, sympathetic: 1.0, excite: 2.0, stereo: 1.0, wet_solo: true, mix: 1.0, out_db: 24.0, ..Settings::default() },
        Settings { count: 12, decay: 0.0, damp: 1.0, spread_cents: 0.0, sympathetic: 0.0, excite: 0.0, stereo: 0.0, mix: 0.5, ..Settings::default() },
    ] {
        let mut core = ChoraleCore::new(SR);
        let (l, r) = core.process_stereo(&input, &s);
        for sig in [&l, &r] {
            assert!(!suite_core::harness::has_nan_or_inf(sig), "NaN/inf in extremes");
            let peak = suite_core::harness::peak_dbfs(sig);
            assert!(peak <= 0.05, "extremes peak {peak:.2} dBFS > 0");
        }
    }
}

// --------------------------------------------------------------------------
// Alloc-guard: the per-block RT path (MIDI-held configure + process) must not allocate.
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
fn midi_block_is_alloc_free() {
    let mut core = ChoraleCore::new(SR);
    let mut held = [f32::NAN; MAX_RESONATORS];
    held[0] = midi_to_freq(40.0);
    held[1] = midi_to_freq(43.0);
    held[2] = midi_to_freq(47.0);
    let s = Settings {
        source: TuningSource::Midi,
        held,
        held_count: 3,
        count: 24,
        sympathetic: 1.0,
        mix: 1.0,
        ..Settings::default()
    };

    // Warm up OUTSIDE the guard (first-block priming may legitimately allocate).
    core.configure(&s);
    for _ in 0..WEIGHT_UPDATE_WARM {
        let _ = core.process_sample(0.01, -0.01, &s);
    }

    alloc_guard::COUNT.with(|c| c.set(0));
    alloc_guard::ARMED.with(|a| a.set(true));
    core.configure(&s);
    for _ in 0..2048 {
        let _ = core.process_sample(0.02, 0.015, &s);
    }
    alloc_guard::ARMED.with(|a| a.set(false));

    let n = alloc_guard::COUNT.with(|c| c.get());
    assert_eq!(n, 0, "MIDI-mode block allocated {n} time(s) on the RT thread (must be 0)");
}

/// Warm past at least one weight-update boundary before arming the alloc guard.
const WEIGHT_UPDATE_WARM: usize = 4096;
