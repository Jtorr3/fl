//! CLEAVE — pure-DSP core for the multi-slicer / transport-locked step sequencer (SPECS
//! "CLEAVE", Slice clone). Shared verbatim between the nih-plug `process` path and the
//! offline done-bar tests.
//!
//! ```text
//!   in ─┬─────────────────────────────────────────────────────── dry ───────────┐
//!       │                                                                          ├─(1-mix)/mix─► out
//!       └─► 2-bar rolling capture ring ──(latch at each pattern wrap)──► playback │
//!               buffer ──► slice (grid 1/8..1/32  OR  transient: spectral-flux +   │
//!               zero-cross backtrack) ──► step sequencer (transport-locked) ──►    │
//!               grain voices (windowed reads: slice idx/as-played, gate, reverse,  │
//!               pitch ±12 resample, roll ×2/3/4, probability, level) ─── wet ──────┘
//! ```
//!
//! **Capture / latency.** The dry path is a zero-latency copy of the input, so CLEAVE reports
//! **zero latency** and `mix = 0` nulls the wet away exactly (done-bar 5, the null contract:
//! the dry crossfade path is never delayed; the wet is a re-timed creative signal and is not
//! expected to null while the pattern is active). The 2-bar source the slicer plays is a
//! **snapshot latched at each pattern boundary** (you hear the previous 2 bars re-chopped), so
//! slice boundaries always line up with the musical grid.
//!
//! **Transport lock.** The core is handed a [`suite_core::testsig::TransportFrame`] each block
//! (the plugin fills it from the host, tests from a `FakeTransport`) and advances an internal
//! pattern position sample-accurately, re-syncing on a seek. When the host is **stopped** the
//! core free-runs an internal clock at the host tempo so the slicer still plays standalone
//! (documented; `mix = 0` still nulls via the dry path).

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use suite_core::dsp::OnePole;
use suite_core::stft::{Complex, Stft};
use suite_core::testsig::{Rng, TransportFrame};

/// Pattern length, in bars, and the length of the rolling capture snapshot. Fixed at 2 bars
/// to match SPECS ("2-bar rolling buffer"); the step count divides these 2 bars.
pub const PATTERN_BARS: f64 = 2.0;
/// Maximum step count (SPECS: 16–64).
pub const MAX_STEPS: usize = 64;
/// Minimum step count.
pub const MIN_STEPS: usize = 16;
/// Maximum number of slices (grid 1/32 over 2 bars = 64; transient capped here too).
pub const MAX_SLICES: usize = 128;
/// Grain voice pool (rolls + fade-tail overlap).
const MAX_VOICES: usize = 12;
/// Longest capture we allocate for: 2 bars down to 30 BPM (16 s). Slower tempos clamp.
const MAX_CAPTURE_S: f32 = 16.0;
/// Grain edge fade (SPECS 3–8 ms — no clicks).
const FADE_MS: f32 = 5.0;
/// STFT geometry for transient (spectral-flux) slicing.
const FLUX_FFT: usize = 1024;
const FLUX_HOP: usize = 256;

/// How the capture buffer is cut into slices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliceMode {
    /// Onset detection: spectral flux + backtrack to the nearest zero crossing.
    Transient,
    /// Fixed musical grid (1/8, 1/16, 1/32).
    Grid,
}

impl SliceMode {
    pub fn from_index(i: usize) -> SliceMode {
        match i {
            0 => SliceMode::Transient,
            _ => SliceMode::Grid,
        }
    }
}

/// Grid slice division (slices per bar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridDiv {
    Eighth,       // 8 per bar
    Sixteenth,    // 16 per bar
    ThirtySecond, // 32 per bar
}

impl GridDiv {
    pub fn from_index(i: usize) -> GridDiv {
        match i {
            0 => GridDiv::Eighth,
            1 => GridDiv::Sixteenth,
            _ => GridDiv::ThirtySecond,
        }
    }
    /// Slices per bar.
    pub fn per_bar(self) -> usize {
        match self {
            GridDiv::Eighth => 8,
            GridDiv::Sixteenth => 16,
            GridDiv::ThirtySecond => 32,
        }
    }
}

/// One step's lanes (SPECS: slice index/as-played, gate, reverse, pitch ±12, roll ×2/3/4,
/// probability, level). Persisted host state (not automatable params — see the module docs).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct StepData {
    /// Whether this step fires at all.
    pub active: bool,
    /// Slice index to play, or `-1` for "as played" (the slice at this step's time position).
    pub slice: i32,
    /// Gate length as a fraction of the step duration (0..1).
    pub gate: f32,
    /// Play the slice time-reversed.
    pub reverse: bool,
    /// Pitch offset in semitones (−12..12), applied as a resample read.
    pub pitch: i32,
    /// Retrigger count tiling the step: 1 (off), 2, 3, or 4.
    pub roll: u8,
    /// Trigger probability (0..1); 0 = always silent, 1 = always plays.
    pub probability: f32,
    /// Output level (0..1).
    pub level: f32,
}

impl Default for StepData {
    fn default() -> Self {
        Self {
            active: true,
            slice: -1,
            gate: 0.9,
            reverse: false,
            pitch: 0,
            roll: 1,
            probability: 1.0,
            level: 1.0,
        }
    }
}

/// The persisted per-step grid (edited by the step-grid widget; snapshotted into the core each
/// block). `steps` may hold up to [`MAX_STEPS`] entries; the active count comes from the params.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepGrid {
    pub steps: Vec<StepData>,
}

impl Default for StepGrid {
    fn default() -> Self {
        Self {
            steps: vec![StepData::default(); MAX_STEPS],
        }
    }
}

impl StepGrid {
    /// Ensure the grid holds at least `n` steps (grows with defaults; never shrinks storage).
    pub fn ensure_len(&mut self, n: usize) {
        if self.steps.len() < n {
            self.steps.resize(n, StepData::default());
        }
    }
}

/// Global (automatable) parameters, snapshotted once per block.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub slice_mode: SliceMode,
    pub sensitivity: f32, // transient sensitivity 0..1 (higher = more onsets)
    pub grid_div: GridDiv,
    pub steps: usize, // 16..64
    pub swing: f32,   // 0..1 (fraction of a half-step delay on the off-steps)
    pub mix: f32,     // 0..1 dry/wet
    pub out_db: f32,  // output trim
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            slice_mode: SliceMode::Grid,
            sensitivity: 0.5,
            grid_div: GridDiv::Sixteenth,
            steps: 32,
            swing: 0.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

/// A single windowed slice read.
#[derive(Clone, Copy, Default)]
struct Grain {
    active: bool,
    read_pos: f32, // fractional index into the playback buffer
    step: f32,     // read increment per sample (± for reverse, scaled by pitch)
    lo: f32,       // slice lower bound (samples)
    hi: f32,       // slice upper bound (samples)
    age: u32,
    dur: u32,  // amp-envelope length (samples)
    fade: u32, // fade in/out length (samples)
    level: f32,
}

/// A pending roll schedule (sub-onsets after the first, tiling the current step).
#[derive(Clone, Copy, Default)]
struct RollSched {
    remaining: u32,
    interval: u32, // samples between sub-onsets
    next_at: u64,  // global sample index of the next sub-onset
    // grain template
    lo: f32,
    hi: f32,
    step: f32,
    reverse: bool,
    dur: u32,
    fade: u32,
    level: f32,
}

/// The CLEAVE processor (stereo).
pub struct CleaveCore {
    sr: f32,

    // --- capture ring (continuous) ---
    ring_l: Vec<f32>,
    ring_r: Vec<f32>,
    ring_cap: usize,
    wpos: usize,

    // --- latched playback snapshot (frozen at each pattern wrap) ---
    play_l: Vec<f32>,
    play_r: Vec<f32>,
    pb_len: usize,

    // --- slices over the playback snapshot ---
    slice_starts: [usize; MAX_SLICES + 1], // +1 sentinel = pb_len
    slice_count: usize,

    // --- transient-detection scratch (preallocated; STFT reused) ---
    stft: Stft,
    prev_mag: Vec<f32>,
    flux: Vec<f32>, // per-frame spectral flux
    flux_len: usize,

    // --- settings (latched each block) ---
    s: Settings,

    // --- transport / pattern position ---
    playing: bool,
    bars_per_sample: f64,
    pattern_pos: f64, // bars within [0, PATTERN_BARS)
    synced: bool,
    host_bar_pos: f64, // previous frame's raw (unwrapped) host bar position — direction of a seek/loop
    next_step: usize,  // next step index to trigger this cycle
    prev_step_idx: i64,

    // --- voices ---
    voices: [Grain; MAX_VOICES],
    voice_rr: usize,
    roll: RollSched,
    now: u64,
    trig_count: u64, // total step triggers that actually spawned a grain (test observability)

    // --- per-step grid snapshot (copied from the persisted grid each block) ---
    grid_snapshot: [StepData; MAX_STEPS],

    // --- probability RNG ---
    rng: Rng,

    // --- smoothing ---
    mix_sm: OnePole,
    out_sm: OnePole,
    primed: bool,

    // --- GUI publish ---
    cur_step: usize,
}

impl CleaveCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let cap = (MAX_CAPTURE_S * sr) as usize + 8;
        let num_bins = FLUX_FFT / 2 + 1;
        let max_frames = cap / FLUX_HOP + 4;
        let mut mix_sm = OnePole::new();
        mix_sm.set_time(8.0, sr);
        mix_sm.reset(1.0);
        let mut out_sm = OnePole::new();
        out_sm.set_time(8.0, sr);
        out_sm.reset(1.0);
        Self {
            sr,
            ring_l: vec![0.0; cap],
            ring_r: vec![0.0; cap],
            ring_cap: cap,
            wpos: 0,
            play_l: vec![0.0; cap],
            play_r: vec![0.0; cap],
            pb_len: 0,
            slice_starts: [0; MAX_SLICES + 1],
            slice_count: 0,
            stft: Stft::new(FLUX_FFT, FLUX_HOP),
            prev_mag: vec![0.0; num_bins],
            flux: vec![0.0; max_frames],
            flux_len: 0,
            s: Settings::default(),
            playing: false,
            bars_per_sample: 0.0,
            pattern_pos: 0.0,
            synced: false,
            host_bar_pos: 0.0,
            next_step: 0,
            prev_step_idx: -1,
            voices: [Grain::default(); MAX_VOICES],
            voice_rr: 0,
            roll: RollSched::default(),
            now: 0,
            trig_count: 0,
            grid_snapshot: [StepData::default(); MAX_STEPS],
            rng: Rng::new(0xC1EA_5EED),
            mix_sm,
            out_sm,
            primed: false,
            cur_step: 0,
        }
    }

    pub fn reset(&mut self) {
        for v in self.ring_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.ring_r.iter_mut() {
            *v = 0.0;
        }
        for v in self.play_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.play_r.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
        self.pb_len = 0;
        self.slice_count = 0;
        self.pattern_pos = 0.0;
        self.synced = false;
        self.host_bar_pos = 0.0;
        self.next_step = 0;
        self.prev_step_idx = -1;
        self.voices = [Grain::default(); MAX_VOICES];
        self.roll = RollSched::default();
        self.now = 0;
        self.trig_count = 0;
        self.rng = Rng::new(0xC1EA_5EED);
        self.stft.reset();
        for m in self.prev_mag.iter_mut() {
            *m = 0.0;
        }
        self.mix_sm.reset(self.s.mix);
        self.out_sm.reset(suite_core::db_to_lin(self.s.out_db));
        self.primed = false;
    }

    /// Latch the global settings for this block.
    pub fn configure(&mut self, s: &Settings) {
        self.s = *s;
        // Snap the dry/wet + output smoothers to their first target so a fresh instance at
        // mix = 0 is an exact passthrough from sample 0 (no startup glide breaking the null).
        if !self.primed {
            self.mix_sm.reset(s.mix.clamp(0.0, 1.0));
            self.out_sm.reset(suite_core::db_to_lin(s.out_db));
            self.primed = true;
        }
    }

    /// Hand the core the per-block transport snapshot (from the host or a `FakeTransport`).
    pub fn set_transport(&mut self, t: &TransportFrame) {
        let was_playing = self.playing;
        self.playing = t.playing;
        self.bars_per_sample = t.bars_per_sample.max(0.0);
        let expected = t.bar_pos.rem_euclid(PATTERN_BARS);
        // Direction of any host discontinuity: negative == the host jumped BACKWARD (a loop wrap
        // or a seek-back). Tracked on the raw (unwrapped) host bar so a 1-bar loop wrap — which
        // is antipodal on the 2-bar pattern circle and so ambiguous in `expected` alone — reads
        // unambiguously as backward motion.
        let raw_delta = t.bar_pos - self.host_bar_pos;
        self.host_bar_pos = t.bar_pos;

        if !self.synced {
            self.pattern_pos = expected;
            self.synced = true;
            self.prime_next_step();
            return;
        }

        // FIX 2 — stopped free-run: while the host is stopped, freeze transport tracking and let
        // the internal clock free-run in `process_sample`. Re-syncing to the host's frozen
        // `bar_pos` here would snap `pattern_pos` back every block (drift > 0.05 bars) and
        // machine-gun the sequencer instead of free-running (module docs, done-bar free-run).
        if !self.playing {
            return;
        }

        // Resuming from a stop: honor any genuine reposition made while stopped, no latch.
        if !was_playing {
            self.pattern_pos = expected;
            self.prime_next_step();
            return;
        }

        // Re-sync only on a real discontinuity (a seek/loop), so per-sample advance drives
        // steady playback without block-boundary jitter.
        let mut d = (expected - self.pattern_pos).abs();
        if d > PATTERN_BARS * 0.5 {
            d = PATTERN_BARS - d; // wrap-around distance
        }
        if d > 0.05 {
            // FIX 1 — a BACKWARD host jump is a latch point. A host loop shorter than the 2-bar
            // internal boundary (the standard 1-bar FL pattern loop) wraps the host position every
            // bar, hitting this seek branch but never the internal `on_wrap`, so the snapshot would
            // never latch and the wet stays silent at mix=1 forever. Latch the freshly-captured
            // audio on the wrap so the slicer has something to play. A 2-bar (or longer even-bar)
            // host loop wraps `pattern_pos` internally first, so `d` stays ~0 here and this branch
            // never double-latches on the same wrap; forward seeks re-sync without latching.
            if raw_delta < -1.0e-9 {
                self.latch_snapshot();
            }
            self.pattern_pos = expected;
            self.prime_next_step();
        }
    }

    /// Copy a fresh per-step grid snapshot into the core.
    pub fn set_grid(&mut self, grid: &[StepData]) {
        for (i, s) in grid.iter().take(MAX_STEPS).enumerate() {
            self.grid_snapshot[i] = *s;
        }
    }

    /// Zero latency — the dry path is a direct copy of the input.
    pub fn latency_samples(&self) -> u32 {
        0
    }

    /// The core's sample rate (Hz).
    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Current step index (for the GUI playhead).
    pub fn current_step(&self) -> usize {
        self.cur_step
    }

    /// After a seek/first-sync, set `next_step` to the first step at or after `pattern_pos`.
    fn prime_next_step(&mut self) {
        let steps = self.s.steps.clamp(MIN_STEPS, MAX_STEPS);
        let frac = (self.pattern_pos / PATTERN_BARS).clamp(0.0, 1.0);
        let mut ns = (frac * steps as f64).floor() as usize;
        // Skip steps whose swung onset already passed.
        while ns < steps && self.swung_onset_bars(ns, steps) <= self.pattern_pos {
            ns += 1;
        }
        self.next_step = ns;
        self.roll.remaining = 0;
    }

    /// Swung onset of step `si` in bars within the pattern (odd steps pushed later by swing).
    #[inline]
    fn swung_onset_bars(&self, si: usize, steps: usize) -> f64 {
        let step_bars = PATTERN_BARS / steps as f64;
        let mut on = si as f64 * step_bars;
        if si % 2 == 1 {
            on += self.s.swing.clamp(0.0, 0.9) as f64 * 0.5 * step_bars;
        }
        on
    }

    /// Process one stereo sample. Returns the mixed (dry/wet) stereo output.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32) -> (f32, f32) {
        // 1. Capture into the ring.
        self.ring_l[self.wpos] = l_in;
        self.ring_r[self.wpos] = r_in;
        self.wpos += 1;
        if self.wpos >= self.ring_cap {
            self.wpos = 0;
        }

        // 2. Advance the pattern position (internal clock; free-runs when stopped).
        let prev_pos = self.pattern_pos;
        self.pattern_pos += self.bars_per_sample;
        if self.pattern_pos >= PATTERN_BARS {
            self.pattern_pos -= PATTERN_BARS;
            self.on_wrap();
        } else if self.pattern_pos < prev_pos {
            // Shouldn't happen without a seek, but guard.
            self.on_wrap();
        }

        // 3. Step-onset detection (trigger the next step when its swung onset is reached).
        let steps = self.s.steps.clamp(MIN_STEPS, MAX_STEPS);
        while self.next_step < steps
            && self.pattern_pos >= self.swung_onset_bars(self.next_step, steps)
        {
            let si = self.next_step;
            self.trigger_step(si, steps);
            self.next_step += 1;
        }
        // Publish the current step for the GUI playhead.
        let frac = (self.pattern_pos / PATTERN_BARS).clamp(0.0, 1.0);
        self.cur_step = ((frac * steps as f64).floor() as usize).min(steps - 1);

        // 4. Fire any scheduled roll sub-onsets.
        if self.roll.remaining > 0 && self.now >= self.roll.next_at {
            self.spawn_from_roll();
        }

        // 5. Render voices → wet.
        let (mut wl, mut wr) = (0.0f32, 0.0f32);
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            let env = grain_env(v.age, v.dur, v.fade) * v.level;
            wl += interp(&self.play_l, self.pb_len, v.read_pos) * env;
            wr += interp(&self.play_r, self.pb_len, v.read_pos) * env;
            v.read_pos += v.step;
            if v.read_pos < v.lo {
                v.read_pos = v.lo;
            } else if v.read_pos > v.hi {
                v.read_pos = v.hi;
            }
            v.age += 1;
            if v.age >= v.dur {
                v.active = false;
            }
        }

        self.now += 1;

        // 6. Dry/wet mix (dry is zero-latency) + output trim.
        let mix = self.mix_sm.process(self.s.mix.clamp(0.0, 1.0));
        let out_g = self.out_sm.process(suite_core::db_to_lin(self.s.out_db));
        let ol = ((1.0 - mix) * l_in + mix * wl) * out_g;
        let or = ((1.0 - mix) * r_in + mix * wr) * out_g;
        // Hard ceiling at ±1.0 (== 0 dBFS). This is the unique continuous, null-preserving
        // bound (identity in-range, so `mix = 0` passes the dry through exactly).
        (ol.clamp(-1.0, 1.0), or.clamp(-1.0, 1.0))
    }

    /// At an internal 2-bar pattern boundary: latch the snapshot and restart the step cursor.
    fn on_wrap(&mut self) {
        self.latch_snapshot();
        self.next_step = 0;
        self.roll.remaining = 0;
    }

    /// Latch the last 2 bars of the capture ring into the playback buffer and (re)slice it.
    /// Called from the internal 2-bar wrap ([`on_wrap`]) and from a host loop-wrap / seek-back
    /// detected in [`set_transport`] (FIX 1). Does not touch the step cursor — callers reprime.
    fn latch_snapshot(&mut self) {
        // Snapshot length = 2 bars in samples (clamped to the ring).
        let pb_len = if self.bars_per_sample > 1.0e-12 {
            (PATTERN_BARS / self.bars_per_sample).round() as usize
        } else {
            0
        };
        self.pb_len = pb_len.min(self.ring_cap);
        // Copy the most recent pb_len samples ending at the write head into play_*.
        let n = self.pb_len;
        for i in 0..n {
            // sample i counts forward from (wpos - n) modulo cap.
            let idx = (self.wpos + self.ring_cap - n + i) % self.ring_cap;
            self.play_l[i] = self.ring_l[idx];
            self.play_r[i] = self.ring_r[idx];
        }
        self.slice_buffer();
    }

    /// (Re)compute slice boundaries over the current playback snapshot.
    fn slice_buffer(&mut self) {
        let n = self.pb_len;
        if n == 0 {
            self.slice_count = 0;
            return;
        }
        match self.s.slice_mode {
            SliceMode::Grid => {
                let per_bar = self.s.grid_div.per_bar();
                let num = (per_bar as f64 * PATTERN_BARS) as usize;
                let num = num.clamp(1, MAX_SLICES);
                for k in 0..num {
                    self.slice_starts[k] = (k as f64 / num as f64 * n as f64).round() as usize;
                }
                self.slice_starts[num] = n;
                self.slice_count = num;
            }
            SliceMode::Transient => self.slice_transients(),
        }
    }

    /// Transient slicing: spectral flux onset detection with backtrack to the nearest zero
    /// crossing (SPECS). Falls back to a coarse grid if too few onsets are found.
    fn slice_transients(&mut self) {
        let n = self.pb_len;
        // --- spectral flux over the mono sum of the snapshot ---
        self.stft.reset();
        for m in self.prev_mag.iter_mut() {
            *m = 0.0;
        }
        let num_bins = self.prev_mag.len();
        let max_frames = self.flux.len();
        let mut frame = 0usize;
        // We drive the STFT with the mono signal; the callback fires once per hop with the
        // spectrum of the frame that *ended* ~fft_size samples earlier.
        let prev_mag = &mut self.prev_mag;
        let flux = &mut self.flux;
        let mut cb = |spec: &mut [Complex<f32>]| {
            if frame < max_frames {
                let mut f = 0.0f32;
                for k in 0..num_bins {
                    let mag = spec[k].norm();
                    let d = mag - prev_mag[k];
                    if d > 0.0 {
                        f += d;
                    }
                    prev_mag[k] = mag;
                }
                flux[frame] = f;
            }
            frame += 1;
        };
        for i in 0..n {
            let mono = 0.5 * (self.play_l[i] + self.play_r[i]);
            self.stft.process(mono, &mut cb);
        }
        self.flux_len = frame.min(max_frames);

        // --- adaptive peak pick ---
        let fl = self.flux_len;
        if fl < 3 {
            self.grid_fallback();
            return;
        }
        let mut mean = 0.0f32;
        for &f in self.flux[..fl].iter() {
            mean += f;
        }
        mean /= fl as f32;
        let mut var = 0.0f32;
        for &f in self.flux[..fl].iter() {
            var += (f - mean) * (f - mean);
        }
        let std = (var / fl as f32).sqrt();
        // Higher sensitivity → lower threshold → more onsets. k in ~[0.2, 2.2].
        let k = 2.2 - 2.0 * self.s.sensitivity.clamp(0.0, 1.0);
        let thresh = mean + k * std;

        let lat = self.stft.latency(); // fft_size
        // Always start slice 0 at the buffer origin.
        self.slice_starts[0] = 0;
        let mut count = 1usize;
        let mut last_sample = 0i64;
        let min_gap = (0.03 * self.sr) as i64; // ≥30 ms between onsets
        for f in 1..fl.saturating_sub(1) {
            let fv = self.flux[f];
            if fv > thresh && fv >= self.flux[f - 1] && fv > self.flux[f + 1] {
                // Frame f's onset sample ≈ f*hop − latency (input time of the analysed frame).
                let mut s = f as i64 * FLUX_HOP as i64 - lat as i64;
                if s < 0 {
                    s = 0;
                }
                if s - last_sample < min_gap {
                    continue;
                }
                let s = self.backtrack_zero_cross(s as usize);
                if s == 0 {
                    continue;
                }
                // Write-guard `< MAX_SLICES`: index `count` must stay ≤ MAX_SLICES-1 here so
                // that after `count += 1` it is ≤ MAX_SLICES, leaving `slice_starts[count]` (the
                // sentinel below) a valid index into the [usize; MAX_SLICES + 1] array.
                if count < MAX_SLICES {
                    self.slice_starts[count] = s;
                    count += 1;
                    last_sample = s as i64;
                }
                if count >= MAX_SLICES {
                    break;
                }
            }
        }
        if count < 2 {
            self.grid_fallback();
            return;
        }
        // Clamp before the sentinel write: `slice_starts` has MAX_SLICES + 1 slots (indices
        // 0..=MAX_SLICES), so the sentinel index must never exceed MAX_SLICES. The write-guard
        // above already bounds `count`, but clamp defensively so a busy snapshot with ≥128 onsets
        // can never index slice_starts[MAX_SLICES + 1] and panic on the audio thread.
        let count = count.min(MAX_SLICES);
        self.slice_starts[count] = n;
        self.slice_count = count;
    }

    /// Backtrack from `s` to the nearest earlier zero crossing of the mono snapshot (≤ ~10 ms).
    fn backtrack_zero_cross(&self, s: usize) -> usize {
        let win = (0.010 * self.sr) as usize;
        let lo = s.saturating_sub(win);
        let mono = |i: usize| 0.5 * (self.play_l[i] + self.play_r[i]);
        let mut i = s.min(self.pb_len.saturating_sub(1));
        while i > lo && i > 0 {
            let a = mono(i - 1);
            let b = mono(i);
            if (a <= 0.0 && b > 0.0) || (a >= 0.0 && b < 0.0) {
                return i;
            }
            i -= 1;
        }
        s
    }

    /// Coarse 1/8 grid fallback when transient detection finds too little.
    fn grid_fallback(&mut self) {
        let n = self.pb_len;
        let num = (8.0 * PATTERN_BARS) as usize; // 16 over 2 bars
        for k in 0..num {
            self.slice_starts[k] = (k as f64 / num as f64 * n as f64).round() as usize;
        }
        self.slice_starts[num] = n;
        self.slice_count = num;
    }

    /// Trigger step `si`: apply probability, resolve the slice, spawn the first grain, and
    /// schedule the roll sub-onsets.
    fn trigger_step(&mut self, si: usize, steps: usize) {
        if self.slice_count == 0 || self.pb_len == 0 {
            return;
        }
        let sd = self.grid_snapshot[si.min(MAX_STEPS - 1)];
        if !sd.active {
            return;
        }
        // Probability (deterministic RNG): draw in [0,1); play if draw < probability.
        let p = sd.probability.clamp(0.0, 1.0);
        if p < 1.0 {
            let draw = (self.rng.next_u32() as f32 / u32::MAX as f32).min(0.999999);
            if draw >= p {
                return;
            }
        }

        // Resolve the slice.
        let slice_idx = if sd.slice < 0 {
            // "As played": the slice at this step's temporal position.
            ((si as f64 / steps as f64) * self.slice_count as f64).floor() as usize
        } else {
            sd.slice as usize
        }
        .min(self.slice_count - 1);
        let lo = self.slice_starts[slice_idx] as f32;
        let hi = self.slice_starts[slice_idx + 1] as f32;

        // Step + roll timing.
        let step_bars = PATTERN_BARS / steps as f64;
        let step_samps = (step_bars / self.bars_per_sample.max(1.0e-12)).round().max(1.0) as u32;
        let roll = sd.roll.clamp(1, 4) as u32;
        let sub_samps = (step_samps / roll).max(1);
        let gate = sd.gate.clamp(0.02, 1.0);
        let dur = ((sub_samps as f32) * gate).round().max(1.0) as u32;
        let fade = ((FADE_MS * 0.001 * self.sr) as u32).clamp(1, dur / 2 + 1);
        let ratio = 2.0f32.powf(sd.pitch.clamp(-12, 12) as f32 / 12.0);
        let step_inc = if sd.reverse { -ratio } else { ratio };
        let level = sd.level.clamp(0.0, 1.0);

        // First sub-onset now.
        self.trig_count = self.trig_count.wrapping_add(1);
        self.spawn_grain(lo, hi, step_inc, sd.reverse, dur, fade, level);

        // Schedule the remaining roll sub-onsets.
        if roll > 1 {
            self.roll = RollSched {
                remaining: roll - 1,
                interval: sub_samps,
                next_at: self.now + sub_samps as u64,
                lo,
                hi,
                step: step_inc,
                reverse: sd.reverse,
                dur,
                fade,
                level,
            };
        } else {
            self.roll.remaining = 0;
        }
    }

    fn spawn_from_roll(&mut self) {
        let r = self.roll;
        self.spawn_grain(r.lo, r.hi, r.step, r.reverse, r.dur, r.fade, r.level);
        self.roll.remaining -= 1;
        self.roll.next_at += r.interval as u64;
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_grain(&mut self, lo: f32, hi: f32, step: f32, reverse: bool, dur: u32, fade: u32, level: f32) {
        // Find a free voice, else steal the oldest (largest age fraction).
        let mut idx = usize::MAX;
        for _ in 0..MAX_VOICES {
            let i = self.voice_rr;
            self.voice_rr = (self.voice_rr + 1) % MAX_VOICES;
            if !self.voices[i].active {
                idx = i;
                break;
            }
        }
        if idx == usize::MAX {
            // steal the voice nearest the end of its life
            let mut best = 0usize;
            let mut best_frac = -1.0f32;
            for (i, v) in self.voices.iter().enumerate() {
                let frac = v.age as f32 / v.dur.max(1) as f32;
                if frac > best_frac {
                    best_frac = frac;
                    best = i;
                }
            }
            idx = best;
        }
        let read_pos = if reverse { (hi - 1.0).max(lo) } else { lo };
        self.voices[idx] = Grain {
            active: true,
            read_pos,
            step,
            lo,
            hi: (hi - 1.0).max(lo),
            age: 0,
            dur,
            fade,
            level,
        };
    }
}

#[cfg(test)]
impl CleaveCore {
    /// Test hook: load `mono` as the playback snapshot, run the slicer in `mode`, and return
    /// `(slice_count, sentinel)` where `sentinel == slice_starts[slice_count]`. Drives
    /// `slice_transients` directly with a controlled onset density (the RT panic regression).
    pub fn test_slice_snapshot(
        &mut self,
        mono: &[f32],
        mode: SliceMode,
        sensitivity: f32,
    ) -> (usize, usize) {
        let n = mono.len().min(self.ring_cap);
        self.pb_len = n;
        for i in 0..n {
            self.play_l[i] = mono[i];
            self.play_r[i] = mono[i];
        }
        self.s.slice_mode = mode;
        self.s.sensitivity = sensitivity;
        self.slice_buffer();
        (self.slice_count, self.slice_starts[self.slice_count])
    }

    /// The playback-snapshot length the slicer operated on (== clamped `mono.len()`).
    pub fn test_pb_len(&self) -> usize {
        self.pb_len
    }

    /// Total step triggers that actually spawned a grain since construction/reset — used by the
    /// stopped free-run regression to count onsets at the free-run rate (vs the machine-gun bug).
    pub fn test_trig_count(&self) -> u64 {
        self.trig_count
    }
}

// ===========================================================================
// Pattern builders (pure, deterministic — shared by presets, the randomizer button,
// and the offline tests).
// ===========================================================================

/// A `[f32; 0..1)` draw from a deterministic xorshift stream.
#[inline]
fn draw(rng: &mut Rng) -> f32 {
    (rng.next_u32() as f32 / u32::MAX as f32).min(0.999_999)
}

/// Build one of the named pattern archetypes into a fresh grid. `steps` is the active count;
/// entries beyond `steps` are left at default (inactive-safe). Deterministic in `seed`.
pub fn build_pattern(archetype: usize, steps: usize, seed: u32) -> [StepData; MAX_STEPS] {
    let n = steps.clamp(MIN_STEPS, MAX_STEPS);
    let mut g = [StepData::default(); MAX_STEPS];
    let mut rng = Rng::new(seed.max(1));
    // Start from a clean slate: everything off.
    for s in g.iter_mut() {
        s.active = false;
        s.slice = -1;
        s.gate = 0.85;
        s.reverse = false;
        s.pitch = 0;
        s.roll = 1;
        s.probability = 1.0;
        s.level = 1.0;
    }
    match archetype {
        // Straight Rechop — every step plays its own slice, full gate.
        0 => {
            for s in g.iter_mut().take(n) {
                s.active = true;
                s.slice = -1;
                s.gate = 0.95;
                s.level = 1.0;
            }
        }
        // Rolls & Ghosts — straight bed with rolled fills + quieter ghost steps + some chance.
        1 => {
            for (i, s) in g.iter_mut().take(n).enumerate() {
                s.active = true;
                s.slice = -1;
                let beat = i % 4;
                if beat == 0 {
                    s.level = 1.0;
                    s.gate = 0.9;
                } else {
                    s.level = 0.55; // ghost
                    s.gate = 0.6;
                    s.probability = 0.85;
                }
                if beat == 3 && draw(&mut rng) < 0.6 {
                    s.roll = if draw(&mut rng) < 0.5 { 2 } else { 3 };
                }
            }
        }
        // Reverse Accents — straight, but every 4th step is a loud reversed accent.
        2 => {
            for (i, s) in g.iter_mut().take(n).enumerate() {
                s.active = true;
                s.slice = -1;
                if i % 4 == 2 {
                    s.reverse = true;
                    s.level = 1.0;
                    s.gate = 1.0;
                } else {
                    s.level = 0.8;
                    s.gate = 0.85;
                }
            }
        }
        // Half-Time Flip — only the first half of each pair fires, long gates, occasional flip.
        3 => {
            for (i, s) in g.iter_mut().take(n).enumerate() {
                if i % 2 == 0 {
                    s.active = true;
                    s.slice = -1;
                    s.gate = 1.0;
                    s.level = 1.0;
                    if i % 8 == 4 {
                        s.reverse = true;
                    }
                }
            }
        }
        // Jungle Scatter — busy shuffled slices, rolls, reverses, pitch jumps (amen energy).
        4 => {
            let slices = n as i32; // grid slices ≈ steps for the default div
            for (i, s) in g.iter_mut().take(n).enumerate() {
                s.active = draw(&mut rng) < 0.9;
                // shuffle which slice plays
                s.slice = (draw(&mut rng) * slices as f32) as i32;
                s.gate = 0.4 + draw(&mut rng) * 0.6;
                s.level = 0.6 + draw(&mut rng) * 0.4;
                if draw(&mut rng) < 0.25 {
                    s.reverse = true;
                }
                let r = draw(&mut rng);
                s.roll = if r < 0.12 {
                    3
                } else if r < 0.24 {
                    2
                } else {
                    1
                };
                if draw(&mut rng) < 0.15 {
                    s.pitch = if draw(&mut rng) < 0.5 { -12 } else { 12 };
                }
                if i % 4 == 0 {
                    // keep the downbeats solid
                    s.active = true;
                    s.slice = -1;
                    s.reverse = false;
                    s.roll = 1;
                    s.pitch = 0;
                    s.level = 1.0;
                }
            }
        }
        // Four Flat — only the four beats fire (four-on-the-floor), everything else silent.
        _ => {
            let per_beat = (n / 8).max(1); // 2 bars = 8 beats
            for (i, s) in g.iter_mut().take(n).enumerate() {
                if i % per_beat == 0 {
                    s.active = true;
                    s.slice = -1;
                    s.gate = 0.9;
                    s.level = 1.0;
                }
            }
        }
    }
    g
}

/// The Randomize button: rebuild the grid from `density` (0 = sparse, 1 = busy) with per-step
/// probability/roll/reverse variation. Deterministic in `seed`. Writes into `grid` (first
/// `steps` entries active-eligible; the rest cleared).
pub fn randomize_grid(grid: &mut [StepData], steps: usize, density: f32, seed: u32) {
    let n = steps.clamp(MIN_STEPS, MAX_STEPS).min(grid.len());
    let d = density.clamp(0.0, 1.0);
    let mut rng = Rng::new(seed.max(1));
    for (i, s) in grid.iter_mut().enumerate() {
        if i >= n {
            s.active = false;
            continue;
        }
        // Downbeats are more likely to fire; density lifts the rest.
        let base = if i % 4 == 0 { 0.9 } else { 0.25 + 0.7 * d };
        s.active = draw(&mut rng) < base;
        s.slice = -1;
        s.gate = 0.5 + draw(&mut rng) * 0.5;
        s.level = 0.6 + draw(&mut rng) * 0.4;
        s.reverse = draw(&mut rng) < 0.15 * (0.5 + d);
        let r = draw(&mut rng);
        s.roll = if r < 0.1 * d {
            3
        } else if r < 0.25 * d {
            2
        } else {
            1
        };
        s.probability = if draw(&mut rng) < 0.2 { 0.6 + draw(&mut rng) * 0.4 } else { 1.0 };
        s.pitch = 0;
    }
}

/// Linear interpolation read into a buffer of valid length `len`.
#[inline]
fn interp(buf: &[f32], len: usize, pos: f32) -> f32 {
    if len == 0 {
        return 0.0;
    }
    let p = pos.clamp(0.0, (len - 1) as f32);
    let i = p.floor() as usize;
    let frac = p - i as f32;
    let a = buf[i];
    let b = if i + 1 < len { buf[i + 1] } else { a };
    a + (b - a) * frac
}

/// Grain amplitude envelope: linear fade in/out over `fade` samples, unity between.
#[inline]
fn grain_env(age: u32, dur: u32, fade: u32) -> f32 {
    if dur == 0 {
        return 0.0;
    }
    let f = fade.max(1);
    if age < f {
        // raised-cosine fade in (smoother than linear, still click-free)
        let x = age as f32 / f as f32;
        0.5 - 0.5 * (PI * x).cos()
    } else if age + f >= dur {
        let x = (dur - age) as f32 / f as f32;
        (0.5 - 0.5 * (PI * x.clamp(0.0, 1.0)).cos()).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

