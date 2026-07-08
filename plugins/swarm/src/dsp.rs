//! SWARM — pure-DSP core for the mass granulator (SPECS "SWARM", Glow clone).
//!
//! A **10-second stereo circular capture buffer** is continuously written from the input. A
//! **grain scheduler** (poisson random inter-onset intervals, or grid-sync to the host tempo)
//! spawns up to **128 concurrent grains**. Each grain, randomised at spawn, reads an interpolated
//! window of the buffer: a **position** sprayed around a read head, a **pitch** scattered ±24 st
//! (free or semitone-quantised), a **size** (10–500 ms) shaped by a **Tukey window**, an
//! **equal-power pan** within the stereo width, and a **reverse** probability. The grain sum is
//! optionally fed to a **+12 st shimmer** feedback send (in-loop `tanh` soft-limiter + DC blocker
//! per PRD §3) that re-enters the capture buffer to bloom. **Freeze** locks the write head (the
//! buffer holds; input is still monitored into the dry path per `mix`).
//!
//! ```text
//!  in ──┬─────────────────────────────────────────────── dry ───────────────┐
//!       │(write, unless frozen)                                              (1-mix)
//!       ▼                                                                     ▼
//!   [10 s circular capture buffer]  ◄── + shimmer(+12 st, tanh, DC) ◄──┐   out = dry·(1-mix)
//!       ▲   ▲   ▲  (interpolated reads)                                │       + wet·mix
//!    ┌──┴─┬─┴─┬─┴───────────────┐                                      │
//!    │ grain pool (≤128 voices) │── sum → pan/width → wet ─────────────┴──► shimmer send
//!    └──────────────────────────┘        (steal oldest when full)
//! ```
//!
//! ## Read-head / position model
//! Grains read from a point trailing the write head by [`READ_HEAD_MS`], sprayed ±`spray`. Reads
//! are **interpolated and wrap circularly** over the whole 10 s buffer, so a grain whose read span
//! momentarily crosses the write head (at high pitch on a large grain) reads older buffer content
//! rather than garbage — a benign, on-brand granular artefact, never NaN/inf. Grain amplitude is
//! **density-normalised** (`1/√overlap`, `overlap = density·size`) so the wash stays near unity
//! regardless of how many voices overlap.
//!
//! ## Latency (suite convention, per OUROBOROS)
//! A granulator is a **time-smearing effect**, not a fixed-latency FIR stage, so — like OUROBOROS
//! — SWARM reports **zero latency** and asserts the **`mix = 0` null** against the dry input (the
//! lag-0 `assert_single_coherent_peak` coherence check does not apply: there is no lag-0 wet to
//! align).
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the offline
//! render / done-bar tests.

use std::f32::consts::TAU;
use suite_core::dsp::OnePole;
use suite_core::testsig::Rng;

/// Capture-buffer length in seconds (SPECS: 10 s circular buffer).
pub const BUFFER_SECONDS: f32 = 10.0;
/// Voice cap (SPECS: 128, steal oldest).
pub const MAX_GRAINS: usize = 128;
/// Fixed base offset of the read head behind the write head (ms). Grains spray around this
/// point; large enough that ordinary grains read already-written history.
pub const READ_HEAD_MS: f32 = 300.0;
/// Maximum position-spray range (ms) at `spray = 1`.
pub const MAX_SPRAY_MS: f32 = 500.0;
/// Grain size bounds (ms).
pub const MIN_SIZE_MS: f32 = 10.0;
pub const MAX_SIZE_MS: f32 = 500.0;
/// Density bounds (grains / second).
pub const MIN_DENSITY: f32 = 1.0;
pub const MAX_DENSITY: f32 = 500.0;
/// Max pitch scatter (semitones, ±).
pub const MAX_SCATTER_ST: f32 = 24.0;
/// Tukey window taper fraction (α): fraction of the grain that is cosine-tapered (half at each
/// edge). A flat centre with raised-cosine edges = click-free grain boundaries.
const TUKEY_ALPHA: f32 = 0.5;

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Grid-sync divisions (BPM), shared design with OUROBOROS/DRIFT.
// ---------------------------------------------------------------------------

/// One grid tick = this many beats (quarter notes), assuming 4/4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncDivision {
    Sixteenth,
    Eighth,
    DottedEighth,
    Quarter,
    DottedQuarter,
    Half,
    Bar,
}

impl SyncDivision {
    pub fn from_index(i: usize) -> SyncDivision {
        match i {
            0 => SyncDivision::Sixteenth,
            1 => SyncDivision::Eighth,
            2 => SyncDivision::DottedEighth,
            3 => SyncDivision::Quarter,
            4 => SyncDivision::DottedQuarter,
            5 => SyncDivision::Half,
            _ => SyncDivision::Bar,
        }
    }
    #[inline]
    pub fn beats(self) -> f32 {
        match self {
            SyncDivision::Sixteenth => 0.25,
            SyncDivision::Eighth => 0.5,
            SyncDivision::DottedEighth => 0.75,
            SyncDivision::Quarter => 1.0,
            SyncDivision::DottedQuarter => 1.5,
            SyncDivision::Half => 2.0,
            SyncDivision::Bar => 4.0,
        }
    }
}

/// A full snapshot of SWARM's controls (plain, un-normalized values).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Grain density (grains / second), 1..500.
    pub density: f32,
    /// Grain size (ms), 10..500.
    pub size_ms: f32,
    /// Position spray (ms), 0..500 — random offset around the read head.
    pub spray_ms: f32,
    /// Pitch scatter (semitones, ±), 0..24.
    pub scatter_st: f32,
    /// Quantize pitch scatter to whole semitones.
    pub quantize: bool,
    /// Reverse probability, 0..1.
    pub reverse_prob: f32,
    /// Shimmer feedback amount (+12 st), 0..1.1 (110 %).
    pub shimmer: f32,
    /// Freeze — lock the write head (input still monitored per `mix`).
    pub freeze: bool,
    /// 0..1 — while Freeze is engaged, blend the output between the live input (0) and the
    /// fully-frozen grain cloud (1). 1.0 = classic hard freeze; lower keeps the live source in.
    pub freeze_mix: f32,
    /// Grid-sync the scheduler to the host tempo (else free-running poisson).
    pub sync: bool,
    pub division: SyncDivision,
    /// Host tempo (BPM) for sync; falls back to 120 if the host reports none.
    pub tempo_bpm: f32,
    /// Stereo width (pan spread), 0..1.
    pub width: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            density: 40.0,
            size_ms: 120.0,
            spray_ms: 80.0,
            scatter_st: 4.0,
            quantize: false,
            reverse_prob: 0.0,
            shimmer: 0.0,
            freeze: false,
            freeze_mix: 1.0,
            sync: false,
            division: SyncDivision::Sixteenth,
            tempo_bpm: 120.0,
            width: 0.7,
            mix: 0.5,
            out_db: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// One grain.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    /// Current fractional read index into the capture buffer.
    read_pos: f32,
    /// Read increment per output sample (± pitch ratio; negative = reversed).
    inc: f32,
    /// Elapsed output samples within the grain.
    pos: f32,
    /// Grain length (output samples).
    size: f32,
    /// Tukey taper length at each edge (output samples).
    taper: f32,
    gain_l: f32,
    gain_r: f32,
    /// Spawn serial (monotonic) — smallest = oldest, for voice stealing.
    serial: u64,
}

impl Grain {
    const fn silent() -> Self {
        Grain {
            active: false,
            read_pos: 0.0,
            inc: 1.0,
            pos: 0.0,
            size: 1.0,
            taper: 1.0,
            gain_l: 0.0,
            gain_r: 0.0,
            serial: 0,
        }
    }

    /// Tukey window value at the current `pos`.
    #[inline]
    fn window(&self) -> f32 {
        let n = self.pos;
        let edge = self.taper;
        if edge <= 0.0 {
            return 1.0;
        }
        if n < edge {
            0.5 * (1.0 + (std::f32::consts::PI * (n / edge - 1.0)).cos())
        } else if n > self.size - edge {
            let d = (n - (self.size - edge)) / edge;
            0.5 * (1.0 + (std::f32::consts::PI * d).cos())
        } else {
            1.0
        }
    }
}

// ---------------------------------------------------------------------------
// +12 st shimmer pitch shifter (two-tap half-grain crossfading ring reader,
// fixed octave-up ratio). Shared design with OUROBOROS's granular slot.
// ---------------------------------------------------------------------------

const SHIMMER_GRAIN: f32 = 2048.0;
/// Pre-limiter drive on the shimmer send. Grains read decorrelated buffer positions and are
/// density-normalised (`1/√overlap`), so the round-trip loss through the grain cloud is high
/// (~−10 dB); this makeup pushes the loop gain to ~unity at `shimmer = 1.0` so the +12 st
/// feedback actually blooms/sustains, while the in-loop `tanh` keeps it bounded past unity.
const SHIMMER_DRIVE: f32 = 6.0;

#[derive(Clone)]
struct ShimmerShifter {
    buf: Vec<f32>,
    wpos: usize,
    phase: f32,
    dc_x1: f32,
    dc_y1: f32,
}

impl ShimmerShifter {
    fn new() -> Self {
        Self {
            buf: vec![0.0; 8192],
            wpos: 0,
            phase: 0.0,
            dc_x1: 0.0,
            dc_y1: 0.0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
        self.phase = 0.0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }
    #[inline]
    fn read_at(&self, back: f32) -> f32 {
        let len = self.buf.len();
        let rpos = self.wpos as f32 - back;
        let base = rpos.floor();
        let frac = rpos - base;
        let mut i0 = base as isize;
        i0 = ((i0 % len as isize) + len as isize) % len as isize;
        let i1 = ((i0 + 1) % len as isize) as usize;
        let a = self.buf[i0 as usize];
        let b = self.buf[i1];
        a + (b - a) * frac
    }
    /// Push one input sample, return the +12 st (octave-up), tanh-limited, DC-blocked output.
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
        // +12 st ⇒ ratio 2.0 ⇒ phase drifts at (1 - 2) = -1 sample/sample.
        self.phase += 1.0 - 2.0;
        while self.phase < 0.0 {
            self.phase += SHIMMER_GRAIN;
        }
        while self.phase >= SHIMMER_GRAIN {
            self.phase -= SHIMMER_GRAIN;
        }
        let d1 = self.phase;
        let mut d2 = self.phase + SHIMMER_GRAIN * 0.5;
        if d2 >= SHIMMER_GRAIN {
            d2 -= SHIMMER_GRAIN;
        }
        let base = 2.0;
        let t1 = self.read_at(base + d1);
        let t2 = self.read_at(base + d2);
        let w1 = 0.5 - 0.5 * (TAU * d1 / SHIMMER_GRAIN).cos();
        let w2 = 0.5 - 0.5 * (TAU * d2 / SHIMMER_GRAIN).cos();
        let shifted = t1 * w1 + t2 * w2;
        // In-loop soft limiter (PRD §3) then a one-pole DC blocker (~20 Hz @ 48 k).
        let limited = shifted.tanh();
        let y = limited - self.dc_x1 + 0.9975 * self.dc_y1;
        self.dc_x1 = limited;
        self.dc_y1 = y;
        y
    }
}

// ---------------------------------------------------------------------------
// SWARM core (stereo).
// ---------------------------------------------------------------------------

pub struct SwarmCore {
    sr: f32,
    cap_l: Vec<f32>,
    cap_r: Vec<f32>,
    cap_len: usize,
    wpos: usize,
    grains: [Grain; MAX_GRAINS],
    serial: u64,
    /// Fractional countdown (output samples) until the next spawn event.
    spawn_countdown: f32,
    rng: Rng,
    shimmer: ShimmerShifter,

    // Smoothed audible controls.
    width_s: OnePole,
    mix_s: OnePole,
    out_s: OnePole,
    shimmer_s: OnePole,
    fm_s: OnePole, // Freeze-Mix (live↔frozen blend, applied only while frozen)
    primed: bool,
}

impl SwarmCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let cap_len = ((BUFFER_SECONDS * sr) as usize).max(16) + 4;
        let mut core = SwarmCore {
            sr,
            cap_l: vec![0.0; cap_len],
            cap_r: vec![0.0; cap_len],
            cap_len,
            wpos: 0,
            grains: [Grain::silent(); MAX_GRAINS],
            serial: 0,
            spawn_countdown: 0.0,
            rng: Rng::new(0x5341524d), // "SARM"
            shimmer: ShimmerShifter::new(),
            width_s: OnePole::new(),
            mix_s: OnePole::new(),
            out_s: OnePole::new(),
            shimmer_s: OnePole::new(),
            fm_s: OnePole::new(),
            primed: false,
        };
        core.set_sample_rate(sr);
        core
    }

    /// A granulator is a time-smearing effect, not fixed processing latency ⇒ report zero
    /// (suite convention, per OUROBOROS).
    pub fn latency_samples(&self) -> u32 {
        0
    }

    /// (Re)allocate the capture buffer for a new sample rate and reset smoothers. Called from
    /// the plugin's `initialize()` (outside the audio thread) — allocation is fine here.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let cap_len = ((BUFFER_SECONDS * self.sr) as usize).max(16) + 4;
        if cap_len != self.cap_len {
            self.cap_l = vec![0.0; cap_len];
            self.cap_r = vec![0.0; cap_len];
            self.cap_len = cap_len;
            self.wpos = 0;
        }
        let t = 15.0;
        self.width_s.set_time(t, self.sr);
        self.mix_s.set_time(t, self.sr);
        self.out_s.set_time(t, self.sr);
        self.shimmer_s.set_time(t, self.sr);
        self.fm_s.set_time(t, self.sr);
        self.primed = false;
    }

    pub fn reset(&mut self) {
        for v in self.cap_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.cap_r.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
        self.grains = [Grain::silent(); MAX_GRAINS];
        self.serial = 0;
        self.spawn_countdown = 0.0;
        self.shimmer.reset();
        self.primed = false;
    }

    fn prime(&mut self, s: &Settings) {
        self.width_s.reset(s.width.clamp(0.0, 1.0));
        self.mix_s.reset(s.mix.clamp(0.0, 1.0));
        self.out_s.reset(s.out_db);
        self.shimmer_s.reset(s.shimmer.clamp(0.0, 1.1));
        self.fm_s.reset(s.freeze_mix.clamp(0.0, 1.0));
        self.primed = true;
    }

    /// Latch per-block config. Call once per block before the sample loop.
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.prime(s);
        }
    }

    #[inline]
    fn uniform(&mut self) -> f32 {
        // [0, 1)
        (self.rng.next_u32() as f64 / (u32::MAX as f64 + 1.0)) as f32
    }

    /// Interpolated circular read of a capture channel.
    #[inline]
    fn read_cap(buf: &[f32], pos: f32) -> f32 {
        let len = buf.len();
        let base = pos.floor();
        let frac = pos - base;
        let mut i0 = base as isize;
        i0 = ((i0 % len as isize) + len as isize) % len as isize;
        let i1 = ((i0 + 1) % len as isize) as usize;
        let a = buf[i0 as usize];
        let b = buf[i1];
        a + (b - a) * frac
    }

    /// Spawn one grain with parameters randomised from `s`. Reuses a free voice or steals the
    /// oldest (SPECS: 128-cap, steal oldest).
    fn spawn_grain(&mut self, s: &Settings) {
        // --- Randomise grain parameters ---------------------------------------------------
        let size = (s.size_ms.clamp(MIN_SIZE_MS, MAX_SIZE_MS) * 0.001 * self.sr).max(4.0);
        let taper = (size * TUKEY_ALPHA * 0.5).max(1.0);

        // Position: base read head (wpos - READ_HEAD_MS) sprayed ±spray.
        let base_off = READ_HEAD_MS * 0.001 * self.sr;
        let spray = s.spray_ms.clamp(0.0, MAX_SPRAY_MS) * 0.001 * self.sr;
        let spray_off = (self.uniform() * 2.0 - 1.0) * spray;
        let start = self.wpos as f32 - base_off + spray_off;

        // Pitch: scatter ±scatter st, optional semitone quantise.
        let scat = s.scatter_st.clamp(0.0, MAX_SCATTER_ST);
        let mut st = (self.uniform() * 2.0 - 1.0) * scat;
        if s.quantize {
            st = st.round();
        }
        let ratio = 2.0f32.powf(st / 12.0);

        // Reverse probability.
        let reverse = self.uniform() < s.reverse_prob.clamp(0.0, 1.0);
        let inc = if reverse { -ratio } else { ratio };
        // When reversed, start at the far end so the grain reads backward through the window.
        let read_pos = if reverse { start + ratio * size } else { start };

        // Equal-power pan within the stereo width.
        let width = s.width.clamp(0.0, 1.0);
        let pan = (self.uniform() * 2.0 - 1.0) * width; // -width..+width
        let theta = (pan * 0.5 + 0.5) * std::f32::consts::FRAC_PI_2; // 0..π/2
        let gain_l = theta.cos();
        let gain_r = theta.sin();

        // --- Find a voice: free slot, else steal the oldest --------------------------------
        let serial = self.serial;
        self.serial = self.serial.wrapping_add(1);
        let mut slot = usize::MAX;
        for (i, g) in self.grains.iter().enumerate() {
            if !g.active {
                slot = i;
                break;
            }
        }
        if slot == usize::MAX {
            // Steal the oldest (smallest serial).
            let mut oldest = 0usize;
            let mut oldest_serial = u64::MAX;
            for (i, g) in self.grains.iter().enumerate() {
                if g.serial < oldest_serial {
                    oldest_serial = g.serial;
                    oldest = i;
                }
            }
            slot = oldest;
        }
        self.grains[slot] = Grain {
            active: true,
            read_pos,
            inc,
            pos: 0.0,
            size,
            taper,
            gain_l,
            gain_r,
            serial,
        };
    }

    /// Number of grains to release on a scheduler event, and the samples until the next event.
    /// Poisson: exponential inter-arrival at rate `density`. Grid: fixed division period, with a
    /// cluster of `round(density·period)` grains so `density` stays meaningful when synced.
    fn schedule_next(&mut self, s: &Settings) -> (usize, f32) {
        let density = s.density.clamp(MIN_DENSITY, MAX_DENSITY);
        if s.sync {
            let bpm = s.tempo_bpm.clamp(20.0, 999.0);
            let period_s = s.division.beats() * (60.0 / bpm);
            let n = (density * period_s).round().max(1.0) as usize;
            (n.min(MAX_GRAINS), (period_s * self.sr).max(1.0))
        } else {
            // Exponential inter-arrival: interval = -ln(u)/density seconds.
            let u = self.uniform().max(1.0e-7);
            let interval = (-u.ln() / density) * self.sr;
            (1, interval.max(1.0))
        }
    }

    /// One stereo output sample. `in_l/in_r` are the dry inputs.
    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32, s: &Settings) -> (f32, f32) {
        // --- Scheduler: spawn due grains --------------------------------------------------
        self.spawn_countdown -= 1.0;
        // Guard against a runaway loop if a degenerate interval ever comes back tiny.
        let mut budget = MAX_GRAINS;
        while self.spawn_countdown <= 0.0 && budget > 0 {
            let (count, next) = self.schedule_next(s);
            for _ in 0..count {
                self.spawn_grain(s);
            }
            self.spawn_countdown += next;
            budget -= 1;
        }

        // --- Sum active grains ------------------------------------------------------------
        let mut wet_l = 0.0f32;
        let mut wet_r = 0.0f32;
        for g in self.grains.iter_mut() {
            if !g.active {
                continue;
            }
            let sample = 0.5 * (Self::read_cap(&self.cap_l, g.read_pos) + Self::read_cap(&self.cap_r, g.read_pos));
            let w = g.window();
            let v = sample * w;
            wet_l += v * g.gain_l;
            wet_r += v * g.gain_r;
            g.read_pos += g.inc;
            g.pos += 1.0;
            if g.pos >= g.size {
                g.active = false;
            }
        }

        // Density-normalise: overlap ≈ density·size_s grains stacked ⇒ 1/√overlap keeps the
        // wash near unity. Equal-power windows already sum coherently.
        let overlap = (s.density.clamp(MIN_DENSITY, MAX_DENSITY) * s.size_ms.clamp(MIN_SIZE_MS, MAX_SIZE_MS) * 0.001).max(1.0);
        let norm = 1.0 / overlap.sqrt();
        wet_l *= norm;
        wet_r *= norm;

        // --- Shimmer feedback send (+12 st, tanh, DC) into the capture buffer -------------
        let shimmer_amt = self.shimmer_s.process(s.shimmer.clamp(0.0, 1.1));
        let shimmer_in = 0.5 * (wet_l + wet_r) * SHIMMER_DRIVE;
        let shimmer_out = self.shimmer.process(shimmer_in) * shimmer_amt;

        // --- Write to the capture buffer (unless frozen) ----------------------------------
        if !s.freeze {
            self.cap_l[self.wpos] = in_l + shimmer_out;
            self.cap_r[self.wpos] = in_r + shimmer_out;
            self.wpos += 1;
            if self.wpos >= self.cap_len {
                self.wpos = 0;
            }
        }

        // --- Dry/wet mix + trim -----------------------------------------------------------
        let mix = self.mix_s.process(s.mix.clamp(0.0, 1.0));
        let _ = self.width_s.process(s.width.clamp(0.0, 1.0)); // smoothed for future use/parity
        let out_lin = db_to_lin(self.out_s.process(s.out_db));
        let fm = self.fm_s.process(s.freeze_mix.clamp(0.0, 1.0));
        let base_l = (in_l * (1.0 - mix) + wet_l * mix) * out_lin;
        let base_r = (in_r * (1.0 - mix) + wet_r * mix) * out_lin;
        // Freeze Mix: while frozen, crossfade back toward the live input so the freeze isn't an
        // all-or-nothing jump. fm=1 → classic hard freeze (frozen cloud per mix).
        let (base_l, base_r) = if s.freeze {
            (fm * base_l + (1.0 - fm) * in_l, fm * base_r + (1.0 - fm) * in_r)
        } else {
            (base_l, base_r)
        };
        let ol = base_l.clamp(-8.0, 8.0);
        let or = base_r.clamp(-8.0, 8.0);
        (ol, or)
    }

    /// Convenience mono renderer for the offline harness (R = L, returns L in place).
    pub fn process_mono(&mut self, main: &mut [f32], s: &Settings) {
        self.configure(s);
        for m in main.iter_mut() {
            let (l, _r) = self.process_sample(*m, *m, s);
            *m = l;
        }
    }

    /// Stereo renderer from a mono input.
    pub fn process_stereo(&mut self, input: &[f32], s: &Settings) -> (Vec<f32>, Vec<f32>) {
        self.configure(s);
        let mut l = Vec::with_capacity(input.len());
        let mut r = Vec::with_capacity(input.len());
        for &x in input {
            let (ol, or) = self.process_sample(x, x, s);
            l.push(ol);
            r.push(or);
        }
        (l, r)
    }

    // --- Test / done-bar helpers ----------------------------------------------------------

    /// Number of currently-active grains (for tests).
    pub fn active_grains(&self) -> usize {
        self.grains.iter().filter(|g| g.active).count()
    }

    /// Seed the capture buffer so the read head sits exactly on a unit impulse, then behave as
    /// if frozen. Used by the density done-bar: with a single impulse in the buffer, every grain
    /// that reads across it emits one sharp click, so onsets are countable and scale with
    /// density. Writes an impulse at `wpos - READ_HEAD_MS` (the spray centre).
    pub fn seed_impulse_at_readhead(&mut self) {
        for v in self.cap_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.cap_r.iter_mut() {
            *v = 0.0;
        }
        let base_off = (READ_HEAD_MS * 0.001 * self.sr) as isize;
        let idx = (((self.wpos as isize - base_off) % self.cap_len as isize) + self.cap_len as isize)
            % self.cap_len as isize;
        self.cap_l[idx as usize] = 1.0;
        self.cap_r[idx as usize] = 1.0;
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
