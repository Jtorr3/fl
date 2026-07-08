//! HALT offline done-bar tests (PRD §4 universal + HALT-specific), driven by the shared
//! `suite_core::testsig::FakeTransport` against the pure stereo core. Renders write into
//! renders/HALT/.
//!
//! Done-bars (SPECS "HALT" / the build brief):
//!   1. Tape-stop: a 300 Hz sine glides monotonically to < 50 Hz within the configured
//!      duration (±10%).
//!   2. Stutter at 1/8 @120 BPM: the loop period == 250 ms ±1 ms across ≥4 repeats.
//!   3. Reverse: the output segment cross-correlates > 0.9 with the time-reversed buffer.
//!   4. Every engage/disengage transition: the max sample-delta is bounded (≤3× steady-state).
//!   5. Inactive → bit-exact passthrough (and mix = 0 while active → passthrough).

use crate::dsp::{HaltCore, QuantDiv, Settings, StutterDiv, TapeRelease, TapeSync, NUM_MODES};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::harness::{assert_universal, null_residual_db, render_path, write_wav};
use suite_core::presets::load_all;
use suite_core::testsig::{self, FakeTransport};

const SR: f32 = 48_000.0;
const BLOCK: usize = 512;

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Held-mode state active at a given absolute sample, from a schedule of `(sample, held)` events
/// (the most-recent event with `sample <= at` wins; all-false before the first).
fn held_at(at: usize, events: &[(usize, [bool; NUM_MODES])]) -> [bool; NUM_MODES] {
    let mut held = [false; NUM_MODES];
    for &(s, h) in events {
        if s <= at {
            held = h;
        } else {
            break;
        }
    }
    held
}

/// Drive the stereo core block-by-block with a fake 4/4 transport, applying a mode schedule and
/// the plugin's own blend (`out = (1-mix)·dry + mix·wet`, idle → bit-exact passthrough).
fn render(
    s: &Settings,
    input: &[f32],
    bpm: f64,
    playing: bool,
    events: &[(usize, [bool; NUM_MODES])],
) -> (Vec<f32>, Vec<f32>) {
    let mut core = HaltCore::new(SR);
    core.configure(s);
    let mut t = FakeTransport::new(SR as f64, bpm);
    if !playing {
        t = t.playing(false);
    }
    let gain = db_to_gain(s.out_db);
    let total = input.len();
    let mut l = vec![0.0f32; total];
    let mut r = vec![0.0f32; total];
    let mut i = 0usize;
    while i < total {
        core.configure(s);
        core.set_transport(&t.frame());
        core.set_held(&held_at(i, events));
        let end = (i + BLOCK).min(total);
        for j in i..end {
            let x = input[j];
            let (wl, wr) = core.process_sample(x, x);
            if core.is_idle() {
                l[j] = x;
                r[j] = x;
            } else {
                l[j] = ((1.0 - s.mix) * x + s.mix * wl) * gain;
                r[j] = ((1.0 - s.mix) * x + s.mix * wr) * gain;
            }
        }
        t.advance(end - i);
        i = end;
    }
    (l, r)
}

/// One-hot held state for a single mode index.
fn only(mode: usize) -> [bool; NUM_MODES] {
    let mut h = [false; NUM_MODES];
    h[mode] = true;
    h
}

// ---------------------------------------------------------------------------
// small measurement helpers
// ---------------------------------------------------------------------------

/// Positive-going zero-crossing frequency estimate over `seg` (Hz).
fn zero_cross_freq(seg: &[f32]) -> f32 {
    if seg.len() < 2 {
        return 0.0;
    }
    let mut crossings = 0usize;
    for w in seg.windows(2) {
        if w[0] <= 0.0 && w[1] > 0.0 {
            crossings += 1;
        }
    }
    crossings as f32 / (seg.len() as f32 / SR)
}

/// Fast-attack / slow-release envelope (for onset detection).
fn envelope(x: &[f32]) -> Vec<f32> {
    let atk = (-1.0 / (0.0005 * SR)).exp();
    let rel = (-1.0 / (0.020 * SR)).exp();
    let mut env = 0.0f32;
    let mut e = vec![0.0f32; x.len()];
    for (i, &v) in x.iter().enumerate() {
        let a = v.abs();
        let c = if a > env { atk } else { rel };
        env = c * env + (1.0 - c) * a;
        e[i] = env;
    }
    e
}

/// Absolute onset sample indices in `x[lo..hi]`, threshold = `thr_frac` of the window peak,
/// de-duplicated by `min_gap_ms`.
fn onsets(x: &[f32], lo: usize, hi: usize, thr_frac: f32, min_gap_ms: f32) -> Vec<usize> {
    let e = envelope(&x[lo..hi]);
    let peak = e.iter().cloned().fold(0.0f32, f32::max);
    if peak <= 1e-9 {
        return vec![];
    }
    let thr = thr_frac * peak;
    let gap = (min_gap_ms * 0.001 * SR) as usize;
    let mut out = vec![];
    let mut last: Option<usize> = None;
    let mut armed = true;
    for i in 1..e.len() {
        if armed && e[i - 1] < thr && e[i] >= thr {
            if last.map_or(true, |l| i - l >= gap) {
                out.push(lo + i);
                last = Some(i);
            }
            armed = false;
        }
        if e[i] < thr * 0.5 {
            armed = true;
        }
    }
    out
}

/// Normalized cross-correlation at the best lag in ±`max_lag`.
fn best_xcorr(a: &[f32], b: &[f32], max_lag: isize) -> f32 {
    let n = a.len().min(b.len()) as isize;
    let mut best = -1.0f32;
    for lag in -max_lag..=max_lag {
        let (mut num, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n {
            let j = i + lag;
            if j < 0 || j >= n {
                continue;
            }
            let av = a[i as usize] as f64;
            let bv = b[j as usize] as f64;
            num += av * bv;
            na += av * av;
            nb += bv * bv;
        }
        if na > 0.0 && nb > 0.0 {
            let c = (num / (na.sqrt() * nb.sqrt())) as f32;
            if c > best {
                best = c;
            }
        }
    }
    best
}

/// Max absolute sample-to-sample delta over `x[lo..hi]`.
fn max_delta(x: &[f32], lo: usize, hi: usize) -> f32 {
    let lo = lo.min(x.len());
    let hi = hi.min(x.len());
    let mut m = 0.0f32;
    for w in x[lo..hi].windows(2) {
        m = m.max((w[1] - w[0]).abs());
    }
    m
}

fn sine(freq: f32, amp: f32, len: usize) -> Vec<f32> {
    testsig::sine(freq, amp, len, SR)
}

// ---------------------------------------------------------------------------
// Done-bar 1: tape-stop glides a 300 Hz sine monotonically to < 50 Hz within the duration.
// ---------------------------------------------------------------------------
#[test]
fn tape_stop_glides_below_50hz() {
    let f0 = 300.0f32;
    let dur_s = 1.0f32;
    let engage = 24_064usize; // ~0.5 s warmup, block-aligned (47·512)
    let dur = (dur_s * SR) as usize;
    let total = engage + dur + 4_800;
    let input = sine(f0, 0.7, total);

    let s = Settings {
        tape_sync: TapeSync::Free,
        tape_free_s: dur_s,
        tape_curve: 0.5, // linear rate law
        tape_release: TapeRelease::Instant,
        mix: 1.0,
        ..Settings::default()
    };
    // Tape-stop = mode index 0, engaged from `engage` to the end.
    let (l, _r) = render(&s, &input, 120.0, true, &[(engage, only(0))]);

    // Measure frequency in 60 ms windows across the stop.
    let win = (0.06 * SR) as usize;
    let mut freqs = vec![];
    let mut c = engage + 256; // skip the 5 ms engage crossfade
    while c + win <= engage + dur {
        freqs.push((c - engage, zero_cross_freq(&l[c..c + win])));
        c += win;
    }
    assert!(freqs.len() >= 8, "not enough measurement windows");

    // Starts near f0.
    assert!(
        freqs[0].1 > 220.0 && freqs[0].1 < 340.0,
        "start freq {:.1} not near {f0}",
        freqs[0].1
    );
    // Monotone non-increasing within the zero-cross quantization tolerance (±1 crossing/window).
    let quant = 1.0 / (win as f32 / SR); // Hz per crossing
    for pair in freqs.windows(2) {
        assert!(
            pair[1].1 <= pair[0].1 + quant + 1.0,
            "freq rose beyond quantization: {:.1} -> {:.1}",
            pair[0].1,
            pair[1].1
        );
    }
    // Gradual: still well above 50 Hz at ~35% of the duration.
    let mid = freqs
        .iter()
        .find(|(t, _)| *t as f32 >= 0.35 * dur as f32)
        .expect("mid window");
    assert!(mid.1 > 60.0, "not gradual — {:.1} Hz at 35% already low", mid.1);
    // Reaches < 50 Hz by the end of the configured duration.
    let last = freqs.last().unwrap().1;
    assert!(last < 50.0, "final freq {:.1} not < 50 Hz within duration", last);
}

// ---------------------------------------------------------------------------
// Done-bar 2: stutter at 1/8 @120 BPM → loop period == 250 ms ±1 ms across ≥4 repeats.
// ---------------------------------------------------------------------------
#[test]
fn stutter_loop_period_is_250ms() {
    let bpm = 120.0;
    let samples_per_beat = (60.0 / bpm * SR as f64) as usize; // 24000
    let loop_len = samples_per_beat / 2; // 1/8 note = 12000 samples = 250 ms
    let engage = 48_128usize; // ~1 s warmup, block-aligned (94·512)
    let total = engage + 6 * loop_len;

    // A single sharp click sitting inside the last 1/8 before the engage point.
    let mut input = vec![0.0f32; total];
    let click_at = engage - loop_len / 2;
    let blip = sine(1500.0, 0.8, (0.003 * SR) as usize);
    for (k, &v) in blip.iter().enumerate() {
        let d = (-(k as f32) / (0.001 * SR)).exp();
        if click_at + k < total {
            input[click_at + k] = v * d;
        }
    }

    let s = Settings {
        stutter_div: StutterDiv::Eighth,
        stutter_decay: 0.0,
        stutter_pitch: 0,
        quantize: QuantDiv::Off,
        mix: 1.0,
        ..Settings::default()
    };
    let (l, _r) = render(&s, &input, bpm, true, &[(engage, only(1))]);

    // The looped click repeats every `loop_len`; collect its onsets after the engage.
    let det = onsets(&l, engage, total, 0.4, 100.0);
    assert!(det.len() >= 4, "expected ≥4 looped onsets, got {}", det.len());

    let expected = loop_len as f32;
    let tol = 0.001 * SR; // ±1 ms
    for pair in det.windows(2) {
        let period = (pair[1] - pair[0]) as f32;
        assert!(
            (period - expected).abs() <= tol,
            "loop period {:.3} ms != 250 ms (±1 ms)",
            period / SR * 1000.0
        );
    }
}

// ---------------------------------------------------------------------------
// Done-bar 3: reverse output cross-correlates > 0.9 with the time-reversed buffer content.
// ---------------------------------------------------------------------------
#[test]
fn reverse_is_time_reversed_buffer() {
    let engage = 48_128usize; // block-aligned (94·512)
    let n = 8_000usize;
    let total = engage + 16_000;
    // An asymmetric source (chirp) so forward vs reversed are clearly distinguishable.
    let input = testsig::log_chirp(400.0, 4000.0, 0.6, total, SR);

    let s = Settings {
        mix: 1.0,
        ..Settings::default()
    };
    let (l, _r) = render(&s, &input, 120.0, true, &[(engage, only(2))]);

    let guard = 512usize; // skip the 5 ms engage crossfade
    // Output reads backward from `engage`: out[guard+k] == input[engage-1-guard-k].
    let out_seg: Vec<f32> = l[engage + guard..engage + guard + n].to_vec();
    let mut rev_ref: Vec<f32> = input[engage - guard - n..engage - guard].to_vec();
    rev_ref.reverse();
    let fwd_ref: Vec<f32> = input[engage + guard..engage + guard + n].to_vec();

    let c_rev = best_xcorr(&out_seg, &rev_ref, 64);
    let c_fwd = best_xcorr(&out_seg, &fwd_ref, 64);
    assert!(c_rev > 0.9, "reverse xcorr {c_rev:.3} not > 0.9");
    assert!(
        c_rev > c_fwd + 0.2,
        "output correlates with forward ({c_fwd:.3}) as much as reversed ({c_rev:.3})"
    );
}

// ---------------------------------------------------------------------------
// Done-bar 4: engage + disengage transitions are click-free (bounded sample-delta).
// ---------------------------------------------------------------------------
#[test]
fn transitions_are_click_free() {
    let f0 = 200.0f32;
    let amp = 0.6f32;
    let engage = 24_000usize;
    let disengage = 48_000usize;
    let total = 72_000usize;
    let input = sine(f0, amp, total);

    // Reverse a sine (worst realistic case: reads backward from the trigger point).
    let s = Settings {
        mix: 1.0,
        ..Settings::default()
    };
    let events = [(engage, only(2)), (disengage, [false; NUM_MODES])];
    let (l, _r) = render(&s, &input, 120.0, true, &events);

    // Steady-state reference: the max slope of the clean sine (reverse has the same slope).
    let steady = max_delta(&input, 1_000, 20_000).max(1e-6);
    let guard = (0.004 * SR) as usize; // ±4 ms around each transition
    let eng_delta = max_delta(&l, engage - guard, engage + guard);
    let dis_delta = max_delta(&l, disengage - guard, disengage + guard);

    assert!(
        eng_delta <= 3.0 * steady,
        "engage click: max delta {eng_delta:.4} > 3× steady {steady:.4}"
    );
    assert!(
        dis_delta <= 3.0 * steady,
        "disengage click: max delta {dis_delta:.4} > 3× steady {steady:.4}"
    );
    assert_universal(&l[1_000..]);
}

// ---------------------------------------------------------------------------
// Done-bar 5a: inactive (no mode) → bit-exact passthrough regardless of mix.
// ---------------------------------------------------------------------------
#[test]
fn inactive_is_bit_exact_passthrough() {
    let total = 64_000usize;
    let input = testsig::pink_noise(0.5, total, 1234);
    let s = Settings {
        mix: 0.5, // even a partial mix must not touch an idle HALT
        out_db: 3.0,
        ..Settings::default()
    };
    let (l, r) = render(&s, &input, 120.0, true, &[]);
    // Exactly equal, sample for sample.
    assert!(l.iter().zip(&input).all(|(a, b)| a.to_bits() == b.to_bits()), "L not bit-exact");
    assert!(r.iter().zip(&input).all(|(a, b)| a.to_bits() == b.to_bits()), "R not bit-exact");
    assert!(null_residual_db(&l, &input) < -120.0);
}

// ---------------------------------------------------------------------------
// Done-bar 5b: mix = 0 while a mode is active → still an exact passthrough.
// ---------------------------------------------------------------------------
#[test]
fn mix_zero_nulls_while_active() {
    let total = 96_000usize;
    let input = testsig::pink_noise(0.5, total, 5678);
    let s = Settings {
        mix: 0.0,
        ..Settings::default()
    };
    // Engage every mode in turn — mix=0 must null through all of them.
    let events = [
        (12_000usize, only(0)),
        (36_000, only(1)),
        (60_000, only(2)),
        (84_000, only(3)),
    ];
    let (l, r) = render(&s, &input, 120.0, true, &events);
    assert!(null_residual_db(&l, &input) < -120.0, "L null too high");
    assert!(null_residual_db(&r, &input) < -120.0, "R null too high");
}

// ---------------------------------------------------------------------------
// Universal: every preset renders (with its signature modes exercised) and passes
// finite / ≤0 dBFS / non-silent on both channels. Renders → renders/HALT/.
// ---------------------------------------------------------------------------
#[test]
fn every_preset_renders_and_passes_universal() {
    let total = 120_000usize;
    // A drum-ish source: a click train under pink noise so the buffer carries real content.
    let mut input = vec![0.0f32; total];
    let blip = sine(1200.0, 0.7, (0.02 * SR) as usize);
    let mut p = 0usize;
    while p < total {
        for (k, &v) in blip.iter().enumerate() {
            let d = (-(k as f32) / (0.004 * SR)).exp();
            if p + k < total {
                input[p + k] += v * d;
            }
        }
        p += 12_000;
    }
    let noise = testsig::pink_noise(0.3, total, 4242);
    for (i, v) in input.iter_mut().enumerate() {
        *v = (*v + noise[i]).clamp(-0.95, 0.95);
    }

    // Exercise all four modes across the render so the output is never silent.
    let events = [
        (24_000usize, only(3)), // half-speed
        (48_000, only(2)),      // reverse
        (72_000, only(1)),      // stutter
        (96_000, only(0)),      // tape-stop
    ];

    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6);
    for pre in &presets {
        let s = settings_from_preset(pre);
        let (l, r) = render(&s, &input, 120.0, true, &events);
        let warm_l = &l[16_000..];
        let warm_r = &r[16_000..];
        assert_universal(warm_l);
        assert_universal(warm_r);
        let fname = pre.name.to_lowercase().replace([' ', '&', '-', '/'], "_");
        let path = render_path("HALT", &fname);
        write_wav(&path, warm_l, SR as u32).expect("write render");
    }
}

// ---------------------------------------------------------------------------
// Extra: last-pressed-wins priority when multiple modes are held.
// ---------------------------------------------------------------------------
#[test]
fn last_pressed_wins() {
    let mut core = HaltCore::new(SR);
    core.configure(&Settings::default());
    core.set_transport(&FakeTransport::new(SR as f64, 120.0).frame());
    // Warm the buffer a touch.
    for _ in 0..1000 {
        core.process_sample(0.1, 0.1);
    }
    // Hold reverse, then also hold stutter → stutter (last pressed) wins.
    core.set_held(&only(2));
    assert_eq!(core.active_mode(), crate::dsp::Mode::Reverse);
    let mut both = only(2);
    both[1] = true;
    core.set_held(&both);
    assert_eq!(core.active_mode(), crate::dsp::Mode::Stutter);
    // Release stutter → falls back to the still-held reverse.
    core.set_held(&only(2));
    assert_eq!(core.active_mode(), crate::dsp::Mode::Reverse);
    // Release all → dry.
    core.set_held(&[false; NUM_MODES]);
    assert_eq!(core.active_mode(), crate::dsp::Mode::Dry);
}
