//! CLEAVE offline done-bar tests (PRD §4 universal + CLEAVE-specific), driven by the shared
//! `suite_core::testsig::FakeTransport` against the pure stereo core. Renders write into
//! renders/CLEAVE/.
//!
//! Done-bars (SPECS "CLEAVE" / the build brief):
//!   1. 120 BPM fake transport, grid slicing, straight pattern → output onsets land on grid ±5 ms.
//!   2. Reverse step → that step's audio is the time-reverse of the source slice (xcorr > 0.9).
//!   3. Roll ×3 → three sub-onsets tiling the step ±2 ms.
//!   4. Probability 0 on a step → silence in that slot.
//!   5. mix = 0 nulls vs the dry input (the zero-latency passthrough null contract).

use crate::dsp::{CleaveCore, GridDiv, Settings, SliceMode, StepData, MAX_STEPS};
use crate::presets::{grid_from_preset, settings_from_preset, PRESET_JSON};
use suite_core::harness::{assert_universal, null_residual_db, render_path, write_wav};
use suite_core::presets::load_all;
use suite_core::testsig::{self, FakeTransport};

const SR: f32 = 48_000.0;
const BLOCK: usize = 512;

#[test]
fn manual_covers_all_params_and_has_recipes() {
    suite_core::manual::assert_manual_covers_params(
        crate::MANUAL_DOC,
        &crate::CleaveParams::default(),
    );
}

/// Samples in `bars` bars of 4/4 at `bpm`.
fn bars_to_samples(bars: f64, bpm: f64) -> usize {
    (bars * 4.0 * (60.0 / bpm) * SR as f64).round() as usize
}

/// Drive the stereo core block-by-block with a fake 4/4 transport at `bpm`.
fn render(s: &Settings, grid: &[StepData], input: &[f32], bpm: f64) -> (Vec<f32>, Vec<f32>) {
    let mut core = CleaveCore::new(SR);
    core.configure(s);
    core.set_grid(grid);
    let mut t = FakeTransport::new(SR as f64, bpm);
    let total = input.len();
    let mut l = vec![0.0f32; total];
    let mut r = vec![0.0f32; total];
    let mut i = 0usize;
    while i < total {
        core.configure(s);
        core.set_grid(grid);
        core.set_transport(&t.frame());
        let end = (i + BLOCK).min(total);
        for j in i..end {
            let (a, b) = core.process_sample(input[j], input[j]);
            l[j] = a;
            r[j] = b;
        }
        t.advance(end - i);
        i = end;
    }
    (l, r)
}

/// A sharp 1.5 kHz blip (~1 ms decay) starting at sample 0 — a clean onset marker.
fn blip(len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / SR;
            0.7 * (2.0 * std::f32::consts::PI * 1500.0 * t).sin() * (-t / 0.001).exp()
        })
        .collect()
}

/// A click train: one blip at the start of every `slice_len`-sample slot, `total` samples.
fn click_train(total: usize, slice_len: usize) -> Vec<f32> {
    let mut x = vec![0.0f32; total];
    let b = blip((0.02 * SR) as usize);
    let mut p = 0usize;
    while p < total {
        for (k, &v) in b.iter().enumerate() {
            if p + k < total {
                x[p + k] += v;
            }
        }
        p += slice_len;
    }
    x
}

/// Fast-attack / slow-release amplitude envelope.
fn envelope(x: &[f32]) -> Vec<f32> {
    let atk = (-1.0 / (0.0005 * SR)).exp();
    let rel = (-1.0 / (0.015 * SR)).exp();
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

/// Rising-edge onset sample indices in `x[lo..hi]` (absolute), threshold = `thr_frac` of the
/// window peak, de-duplicated by `min_gap_ms`.
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
        let mut num = 0.0f64;
        let mut na = 0.0f64;
        let mut nb = 0.0f64;
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

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// A straight, all-active, as-played grid.
fn straight_grid(steps: usize) -> Vec<StepData> {
    let mut g = vec![StepData::default(); MAX_STEPS];
    for s in g.iter_mut().take(steps) {
        s.active = true;
        s.slice = -1;
        s.gate = 0.9;
        s.level = 1.0;
        s.reverse = false;
        s.roll = 1;
        s.probability = 1.0;
        s.pitch = 0;
    }
    for s in g.iter_mut().skip(steps) {
        s.active = false;
    }
    g
}

// ---------------------------------------------------------------------------
// Done-bar 1: grid slicing, straight pattern → onsets land on the grid ±5 ms.
// ---------------------------------------------------------------------------
#[test]
fn onsets_land_on_grid() {
    let bpm = 120.0;
    let steps = 32usize;
    let bar = bars_to_samples(1.0, bpm);
    let cycle = 2 * bar; // pattern = 2 bars
    let slice_len = cycle / steps; // 16th at 120 BPM
    let total = 4 * cycle; // 4 pattern cycles (buffer warms after the first)
    let input = click_train(total, slice_len);

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Sixteenth, // 32 slices over 2 bars
        steps,
        swing: 0.0,
        mix: 1.0,
        out_db: 0.0,
        ..Settings::default()
    };
    let (l, _r) = render(&s, &straight_grid(steps), &input, bpm);

    // Analyse the last full cycle.
    let cyc0 = 3 * cycle;
    let det = onsets(&l, cyc0, cyc0 + cycle, 0.35, 40.0);
    assert!(det.len() >= 28, "expected ~{steps} onsets, got {}", det.len());

    let step_samps = slice_len as f32;
    let tol = 0.005 * SR; // ±5 ms
    for &o in &det {
        let rel = (o - cyc0) as f32;
        let nearest = (rel / step_samps).round() * step_samps;
        let err = (rel - nearest).abs();
        assert!(
            err <= tol,
            "onset at {rel:.0} is {:.1} ms off the grid",
            err / SR * 1000.0
        );
    }
}

// ---------------------------------------------------------------------------
// Done-bar 2: a reversed step plays the time-reverse of the source slice (xcorr > 0.9).
// ---------------------------------------------------------------------------
#[test]
fn reverse_step_is_time_reversed_slice() {
    let bpm = 120.0;
    let steps = 16usize;
    let bar = bars_to_samples(1.0, bpm);
    let cycle = 2 * bar;
    let slice_len = cycle / steps; // 1/8 slices (16 over 2 bars)
    let total = 4 * cycle;

    // Asymmetric per-slice content: a chirp in the first 40% of each slice, silence after —
    // so forward vs reversed are clearly distinguishable.
    let mut input = vec![0.0f32; total];
    let chirp = testsig::log_chirp(1000.0, 3000.0, 0.6, (slice_len as f32 * 0.4) as usize, SR);
    let mut p = 0usize;
    while p < total {
        for (k, &v) in chirp.iter().enumerate() {
            if p + k < total {
                input[p + k] = v;
            }
        }
        p += slice_len;
    }

    let rev_step = 4usize;
    let mut grid = straight_grid(steps);
    grid[rev_step].reverse = true;
    grid[rev_step].gate = 1.0;

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Eighth, // 16 slices == steps → as-played slice i = grid slice i
        steps,
        swing: 0.0,
        mix: 1.0,
        out_db: 0.0,
        ..Settings::default()
    };
    let (l, _r) = render(&s, &grid, &input, bpm);

    // Source slice `rev_step` (input is periodic over one cycle).
    let src: Vec<f32> = input[rev_step * slice_len..(rev_step + 1) * slice_len].to_vec();
    let mut rev_ref = src.clone();
    rev_ref.reverse();

    // Output of that step in the last cycle.
    let cyc0 = 3 * cycle;
    let start = cyc0 + rev_step * slice_len;
    let out_step: Vec<f32> = l[start..start + slice_len].to_vec();

    let c_rev = best_xcorr(&out_step, &rev_ref, 64);
    let c_fwd = best_xcorr(&out_step, &src, 64);
    assert!(c_rev > 0.9, "reverse xcorr {c_rev:.3} not > 0.9");
    assert!(
        c_rev > c_fwd + 0.2,
        "output correlates with forward ({c_fwd:.3}) as much as reversed ({c_rev:.3})"
    );
}

// ---------------------------------------------------------------------------
// Done-bar 3: roll ×3 → three sub-onsets tiling the step ±2 ms.
// ---------------------------------------------------------------------------
#[test]
fn roll_x3_tiles_the_step() {
    let bpm = 120.0;
    let steps = 16usize;
    let bar = bars_to_samples(1.0, bpm);
    let cycle = 2 * bar;
    let slice_len = cycle / steps;
    let total = 4 * cycle;
    let input = click_train(total, slice_len);

    // Only step 4 fires, with roll ×3.
    let mut grid = vec![StepData::default(); MAX_STEPS];
    for s in grid.iter_mut() {
        s.active = false;
    }
    let step = 4usize;
    grid[step] = StepData {
        active: true,
        slice: -1,
        gate: 0.9,
        reverse: false,
        pitch: 0,
        roll: 3,
        probability: 1.0,
        level: 1.0,
    };

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Eighth,
        steps,
        swing: 0.0,
        mix: 1.0,
        out_db: 0.0,
        ..Settings::default()
    };
    let (l, _r) = render(&s, &grid, &input, bpm);

    let cyc0 = 3 * cycle;
    let step_start = cyc0 + step * slice_len;
    let det = onsets(&l, step_start, step_start + slice_len, 0.35, 10.0);
    assert_eq!(det.len(), 3, "roll ×3 produced {} onsets", det.len());

    let sub = slice_len as f32 / 3.0;
    let tol = 0.002 * SR; // ±2 ms
    for (k, &o) in det.iter().enumerate() {
        let expect = step_start as f32 + k as f32 * sub;
        let err = (o as f32 - expect).abs();
        assert!(
            err <= tol,
            "roll sub-onset {k} is {:.2} ms off ({} vs {:.0})",
            err / SR * 1000.0,
            o,
            expect
        );
    }
}

// ---------------------------------------------------------------------------
// Done-bar 4: probability 0 on a step → silence in that slot.
// ---------------------------------------------------------------------------
#[test]
fn probability_zero_is_silent_slot() {
    let bpm = 120.0;
    let steps = 16usize;
    let bar = bars_to_samples(1.0, bpm);
    let cycle = 2 * bar;
    let slice_len = cycle / steps;
    let total = 4 * cycle;
    let input = click_train(total, slice_len);

    let mut grid = straight_grid(steps);
    let dead = 4usize;
    grid[dead].probability = 0.0;

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Eighth,
        steps,
        swing: 0.0,
        mix: 1.0,
        out_db: 0.0,
        ..Settings::default()
    };
    let (l, _r) = render(&s, &grid, &input, bpm);

    let cyc0 = 3 * cycle;
    // The dead step's slot (guard a few ms in from each edge to avoid neighbour fade tails).
    let guard = (0.006 * SR) as usize;
    let ds = cyc0 + dead * slice_len + guard;
    let de = cyc0 + (dead + 1) * slice_len - guard;
    let dead_rms = rms(&l[ds..de]);
    // A live neighbouring step for reference.
    let ls = cyc0 + (dead + 1) * slice_len + guard;
    let le = cyc0 + (dead + 2) * slice_len - guard;
    let live_rms = rms(&l[ls..le]);

    assert!(
        dead_rms < 1e-3,
        "prob-0 slot not silent: RMS {dead_rms:.2e} (neighbour {live_rms:.2e})"
    );
    assert!(live_rms > 10.0 * dead_rms.max(1e-9), "neighbour slot should be much louder");
}

// ---------------------------------------------------------------------------
// Done-bar 5: mix = 0 nulls exactly against the dry input (both channels).
// ---------------------------------------------------------------------------
#[test]
fn mix_zero_nulls_against_dry() {
    let bpm = 120.0;
    let steps = 32usize;
    let cycle = bars_to_samples(2.0, bpm);
    let total = 3 * cycle;
    let input = testsig::pink_noise(0.5, total, 909);

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Sixteenth,
        steps,
        swing: 0.0,
        mix: 0.0, // fully dry
        out_db: 0.0,
        ..Settings::default()
    };
    let (l, r) = render(&s, &straight_grid(steps), &input, bpm);
    assert!(null_residual_db(&l, &input) < -120.0, "L null too high");
    assert!(null_residual_db(&r, &input) < -120.0, "R null too high");
}

// ---------------------------------------------------------------------------
// Universal: every preset renders and passes finite / ≤0 dBFS / non-silent on both channels.
// ---------------------------------------------------------------------------
#[test]
fn every_preset_renders_and_passes_universal() {
    let bpm = 120.0;
    let steps = 32usize;
    let cycle = bars_to_samples(2.0, bpm);
    let total = 4 * cycle;
    // A drum-ish source: a click train plus pink noise so slices carry real content.
    let slice_len = cycle / steps;
    let mut input = click_train(total, slice_len);
    let noise = testsig::pink_noise(0.25, total, 4242);
    for (i, v) in input.iter_mut().enumerate() {
        *v = (*v + noise[i]).clamp(-0.95, 0.95);
    }

    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6);
    for p in &presets {
        let s = settings_from_preset(p);
        let grid = grid_from_preset(p);
        let (l, r) = render(&s, &grid, &input, bpm);
        // Analyse from the 2nd cycle on (buffer warm), so the render is non-silent.
        let warm_l = &l[cycle..];
        let warm_r = &r[cycle..];
        assert_universal(warm_l);
        assert_universal(warm_r);
        let fname = p.name.to_lowercase().replace([' ', '&', '-', '/'], "_");
        let path = render_path("CLEAVE", &fname);
        write_wav(&path, warm_l, SR as u32).expect("write render");
    }
}

// ---------------------------------------------------------------------------
// Extra: the fake transport actually locks the pattern (a stopped transport still
// free-runs the internal clock, and mix=0 remains a passthrough).
// ---------------------------------------------------------------------------
#[test]
fn stopped_transport_still_nulls_at_mix_zero() {
    let steps = 16usize;
    let cycle = bars_to_samples(2.0, 120.0);
    let total = 2 * cycle;
    let input = testsig::white_noise(0.4, total, 77);
    let mut core = CleaveCore::new(SR);
    let s = Settings {
        mix: 0.0,
        ..Settings::default()
    };
    core.configure(&s);
    core.set_grid(&straight_grid(steps));
    let t = FakeTransport::new(SR as f64, 120.0).playing(false);
    let mut out = vec![0.0f32; total];
    let mut i = 0;
    while i < total {
        core.set_transport(&t.frame());
        let end = (i + BLOCK).min(total);
        for j in i..end {
            let (a, _b) = core.process_sample(input[j], input[j]);
            out[j] = a;
        }
        i = end;
    }
    assert!(null_residual_db(&out, &input) < -120.0);
}

// ---------------------------------------------------------------------------
// P0 regression 1: a 1-bar host loop (bar_pos wraps every bar — the standard FL pattern-loop
// workflow) must latch the capture and make sound at mix = 1. Before the fix the internal 2-bar
// `on_wrap` never fired, `pb_len` stayed 0, and the wet output was permanently silent. Also
// asserts the latch is *rolling*: it re-latches every wrap, so new (silent) input eventually
// replaces the old (loud) snapshot.
// ---------------------------------------------------------------------------
#[test]
fn one_bar_host_loop_latches_and_refreshes() {
    let bpm = 120.0;
    let steps = 16usize;
    let bar = bars_to_samples(1.0, bpm); // 1-bar host loop length, in samples
    let passes = 6usize;
    let total = passes * bar;

    // Loud noise for passes 0..3, silence for passes 3..6. A rolling latch must pick up the new
    // silence within a couple of passes; a one-shot latch would hold the loud snapshot forever.
    let noise = testsig::white_noise(0.5, total, 4242);
    let mut input = vec![0.0f32; total];
    for p in 0..3 {
        input[p * bar..(p + 1) * bar].copy_from_slice(&noise[p * bar..(p + 1) * bar]);
    }

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Eighth,
        steps,
        swing: 0.0,
        mix: 1.0, // fully wet — exposes the "silent forever" bug directly
        out_db: 0.0,
        ..Settings::default()
    };
    let mut core = CleaveCore::new(SR);
    core.configure(&s);
    let grid = straight_grid(steps);
    core.set_grid(&grid);

    let mut t = FakeTransport::new(SR as f64, bpm);
    let mut out = vec![0.0f32; total];
    for p in 0..passes {
        let pass_start = p * bar;
        let mut k = 0usize; // samples into this pass
        while k < bar {
            // Host position within the 1-bar loop; at k == 0 it jumps back from ~1 bar (the loop
            // wrap the host reports) — the backward jump the fix latches on.
            t.seek_samples(k as f64);
            core.configure(&s);
            core.set_grid(&grid);
            core.set_transport(&t.frame());
            let blk = BLOCK.min(bar - k);
            for j in 0..blk {
                let idx = pass_start + k + j;
                let (a, _b) = core.process_sample(input[idx], input[idx]);
                out[idx] = a;
            }
            k += blk;
        }
    }

    let pass_rms = |p: usize| rms(&out[p * bar..(p + 1) * bar]);
    let r2 = pass_rms(2);
    let r5 = pass_rms(5);
    assert!(
        r2 > 1e-3,
        "1-bar host loop is silent after the 2nd pass (RMS {r2:.2e}) — the capture never latched"
    );
    assert!(
        r5 < 0.1 * r2,
        "rolling latch did not refresh: pass-5 RMS {r5:.2e} is not << pass-2 RMS {r2:.2e} \
         (silent input never replaced the old loud snapshot)"
    );
}

// ---------------------------------------------------------------------------
// P0 regression 2: a stopped transport (playing = false, bar_pos frozen) must free-run the
// internal clock at the host tempo. Before the fix the seek detector re-snapped `pattern_pos` to
// the frozen host bar every ~0.05 bars and re-primed, stalling/machine-gunning the sequencer. We
// count actual step triggers over several cycles and require the free-run rate (`steps`/cycle).
// ---------------------------------------------------------------------------
#[test]
fn stopped_transport_free_runs_at_tempo() {
    let bpm = 120.0;
    let steps = 16usize;
    let cycle = bars_to_samples(2.0, bpm); // internal 2-bar pattern length, in samples
    let cycles = 4usize;
    let total = cycles * cycle;
    let input = testsig::white_noise(0.5, total, 909);

    let s = Settings {
        slice_mode: SliceMode::Grid,
        grid_div: GridDiv::Eighth,
        steps,
        swing: 0.0,
        mix: 1.0,
        out_db: 0.0,
        ..Settings::default()
    };
    let mut core = CleaveCore::new(SR);
    core.configure(&s);
    core.set_grid(&straight_grid(steps));

    // Stopped, position frozen at bar 0 the whole render (never advanced). The internal clock
    // must free-run at the tempo regardless.
    let t = FakeTransport::new(SR as f64, bpm).playing(false);
    let mut i = 0usize;
    while i < total {
        core.set_transport(&t.frame());
        let end = (i + BLOCK).min(total);
        for j in i..end {
            let _ = core.process_sample(input[j], input[j]);
        }
        i = end;
    }

    // Free-run count: `steps` triggers per 2-bar cycle. The first cycle captures nothing (pb_len
    // is 0 until the first internal wrap) so it fires no grains; cycles 2..=cycles each fire all
    // `steps`. A stalled sequencer fires ~0; a machine-gunning one fires far more than this.
    let expected = steps * (cycles - 1); // 16 * 3 = 48
    let got = core.test_trig_count() as usize;
    assert!(
        got >= expected - steps && got <= expected + 2,
        "stopped free-run fired {got} step triggers; expected ~{expected} at the free-run rate \
         (a stalled or machine-gunning sequencer fails this band)"
    );
}

// --------------------------------------------------------------------------
// Regression (HARD CHECKPOINT 3, BLOCKER): the Transient slicer must never index past its
// slice_starts array on a busy snapshot. A dense onset train (≥128 onsets at the 30 ms
// min-gap resolution — reachable at slow tempo with busy percussion) used to drive
// `count` to MAX_SLICES+1 and panic on the audio thread at the post-loop sentinel write.
// --------------------------------------------------------------------------

#[test]
fn transient_slicer_clamps_dense_onset_train_without_panic() {
    let gap = (0.030 * SR) as usize; // 30 ms == the detector's minimum onset gap
    let n_onsets = 360usize; // ~11 s of 2-bar snapshot at a slow tempo — well past MAX_SLICES
    let len = gap * (n_onsets + 2);
    let mut mono = vec![0.0f32; len];
    // Each onset: a sharp two-sample bipolar transient (broadband → strong spectral flux).
    for k in 0..n_onsets {
        let p = k * gap + gap / 2;
        if p + 1 < len {
            mono[p] = 0.9;
            mono[p + 1] = -0.7;
        }
    }

    let mut core = CleaveCore::new(SR);
    // Max sensitivity → lowest threshold → the most onsets picked (worst case for the bound).
    // The assertion is that this does NOT panic (it did before the off-by-one fix).
    let (count, sentinel) = core.test_slice_snapshot(&mono, SliceMode::Transient, 1.0);

    assert!(
        count <= crate::dsp::MAX_SLICES,
        "slice_count {count} exceeds MAX_SLICES {}",
        crate::dsp::MAX_SLICES
    );
    assert_eq!(
        count,
        crate::dsp::MAX_SLICES,
        "a ≥128-onset train must saturate the slice pool (clamped to MAX_SLICES)"
    );
    // The sentinel `slice_starts[count]` must be a valid, in-bounds end marker == snapshot length.
    assert_eq!(
        sentinel,
        core.test_pb_len(),
        "sentinel slice_starts[count] must equal pb_len (valid end marker)"
    );
}
