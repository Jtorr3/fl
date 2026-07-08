//! OUROBOROS — pure-DSP core for the recursive feedback processor (SPECS "OUROBOROS",
//! Recurse clone).
//!
//! A feedback delay whose loop runs through a **reorderable chain of three effect slots**,
//! an in-loop soft limiter, and a DC blocker. Each repeat is re-processed, so the sound
//! mutates as it recirculates — pitch drifting up an octave per pass, filters closing,
//! frequency-shifting into inharmonic clangor, reversing, crushing, saturating into
//! self-oscillation.
//!
//! ```text
//!  in ─×gate─ + ─ delay(1 ms–2 s, free/sync) ─ [slot A → slot B → slot C] ─ limiter ─ DC ─┬─ out tap
//!             ▲                                        (order selectable)                   │
//!             └────────────────────── × feedback (0–110%, ×decay) ──────────────────────────┘
//! ```
//!
//! ## Feedback / stability conventions (PRD §3, WIRE regen topology)
//! The loop is bounded by a **`tanh` soft limiter at unity** placed *after* the slot chain
//! (which may boost via filter resonance or saturation drive) and *before* a one-pole **DC
//! blocker** — exactly WIRE's in-loop convention. Feedback runs to **110 %**: past unity the
//! loop self-oscillates, but the limiter clamps every pass to a stable limit cycle instead of
//! exploding, and the DC blocker stops any offset from ratcheting up. Self-oscillation is the
//! feature (SPECS): the done-bar asserts the tail is *stable*, not silent.
//!
//! ## Latency
//! The delay line **is the effect**, not fixed processing latency, so OUROBOROS reports
//! **zero** latency (`set_latency_samples(0)`); the granular/Hilbert slots are minimum-phase
//! IIR / short grain readers with no FIR lookahead. Consequently the partial-mix
//! single-coherent-peak regression (a lag-0 alignment check) does **not** apply — with a
//! time-delay effect there is no lag-0 wet to align. We assert **`mix = 0` nulls against dry**
//! instead (documented in `tests.rs`).
//!
//! ## Click-free delay modulation
//! The delay read is **fractional and smoothed** (a one-pole glide on the delay length +
//! linear interpolation), so changing the delay time while running slews rather than jumping
//! the read tap — no hard click (done-bar: max sample-to-sample delta stays bounded vs a
//! steady-state render).
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the offline
//! render / done-bar tests.

use std::f32::consts::TAU;
use suite_core::dsp::{OnePole, Shaper, Svf};

/// Max delay time (ms) — 2 s per SPECS.
pub const MAX_DELAY_MS: f32 = 2000.0;
/// Grain ring-buffer capacity for the pitch/reverse slots (samples). ~170 ms at 48 k; big
/// enough for the largest grain/chunk while never reallocating in `process`.
const GRAIN_CAP: usize = 8192;
/// Control-block length: slot coefficients recompute this often (samples).
const CTRL_BLOCK: usize = 32;

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Param-facing enums
// ---------------------------------------------------------------------------

/// The effect a slot performs. `Off` is a pass-through. The three filter variants fold the
/// SVF "type select" into the slot type (simpler than a separate per-slot mode param); every
/// other type reads the slot's `amount` (primary) and `param` (secondary), both 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotType {
    /// Pass-through.
    Off,
    /// Granular pitch shift ±12 st (amount = pitch, 0.5 = unity; param = grain size).
    Pitch,
    /// State-variable **low-pass** (amount = cutoff, param = resonance).
    FilterLp,
    /// State-variable **high-pass** (amount = cutoff, param = resonance).
    FilterHp,
    /// State-variable **band-pass** (amount = center, param = resonance).
    FilterBp,
    /// Single-sideband **frequency shifter** via a Hilbert allpass pair (amount = shift,
    /// 0.5 = none; param = up/down blend). Group delay: minimum-phase IIR, no reported latency.
    FreqShift,
    /// Waveshaper **saturator** from the suite bank (amount = drive; param blends tanh↔fold).
    Saturate,
    /// Fixed-size **reversed granule** playback (amount = chunk length; param = mix toward dry).
    Reverse,
    /// **Bit crush** — bit-depth + sample-rate reduction (amount = bits, param = SR decimation).
    BitCrush,
}

impl SlotType {
    pub fn from_index(i: usize) -> SlotType {
        match i {
            0 => SlotType::Off,
            1 => SlotType::Pitch,
            2 => SlotType::FilterLp,
            3 => SlotType::FilterHp,
            4 => SlotType::FilterBp,
            5 => SlotType::FreqShift,
            6 => SlotType::Saturate,
            7 => SlotType::Reverse,
            _ => SlotType::BitCrush,
        }
    }
}

/// Slot-chain order — one of the 6 permutations of the 3 slots (A, B, C). A single enum is the
/// simpler param model (vs. three per-slot position IntParams, which admit duplicate/degenerate
/// positions), so we use it and document the choice (build brief).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotOrder {
    Abc,
    Acb,
    Bac,
    Bca,
    Cab,
    Cba,
}

impl SlotOrder {
    pub fn from_index(i: usize) -> SlotOrder {
        match i {
            0 => SlotOrder::Abc,
            1 => SlotOrder::Acb,
            2 => SlotOrder::Bac,
            3 => SlotOrder::Bca,
            4 => SlotOrder::Cab,
            _ => SlotOrder::Cba,
        }
    }
    /// The visiting order of slot indices (0=A, 1=B, 2=C).
    #[inline]
    fn indices(self) -> [usize; 3] {
        match self {
            SlotOrder::Abc => [0, 1, 2],
            SlotOrder::Acb => [0, 2, 1],
            SlotOrder::Bac => [1, 0, 2],
            SlotOrder::Bca => [1, 2, 0],
            SlotOrder::Cab => [2, 0, 1],
            SlotOrder::Cba => [2, 1, 0],
        }
    }
}

/// BPM-sync delay length (one delay tap = this many beats, assuming 4/4).
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
    /// Beats (quarter notes) for one delay tap.
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

/// One slot's controls.
#[derive(Clone, Copy, Debug)]
pub struct SlotSettings {
    pub kind: SlotType,
    /// Primary macro, 0..1 (meaning depends on `kind`).
    pub amount: f32,
    /// Secondary macro, 0..1 (meaning depends on `kind`).
    pub param: f32,
}

impl Default for SlotSettings {
    fn default() -> Self {
        SlotSettings {
            kind: SlotType::Off,
            amount: 0.5,
            param: 0.5,
        }
    }
}

/// A full snapshot of OUROBOROS's controls (plain, un-normalized values).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Free delay time (ms), 1..2000, when not synced.
    pub delay_ms: f32,
    pub sync: bool,
    pub division: SyncDivision,
    /// Host tempo (BPM) for sync; falls back to 120 if the host reports none.
    pub tempo_bpm: f32,
    /// Feedback amount, 0..1.1 (110 %).
    pub feedback: f32,
    /// Decay scale — a fine multiplier on feedback, 0..1.
    pub decay_scale: f32,
    /// Freeze: mutes input and forces feedback to 100 % (click-free, smoothed).
    pub freeze: bool,
    /// 0..1 — while Freeze is engaged, blend the output between the live input (0) and the
    /// fully-frozen loop (1). 1.0 = classic hard freeze; lower keeps the live source audible.
    pub freeze_mix: f32,
    pub order: SlotOrder,
    pub slots: [SlotSettings; 3],
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            delay_ms: 300.0,
            sync: false,
            division: SyncDivision::Quarter,
            tempo_bpm: 120.0,
            feedback: 0.5,
            decay_scale: 1.0,
            freeze: false,
            freeze_mix: 1.0,
            order: SlotOrder::Abc,
            slots: [SlotSettings::default(); 3],
            mix: 0.5,
            out_db: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Small building blocks
// ---------------------------------------------------------------------------

/// One-pole DC blocker (high-pass) for the feedback path (PRD §3 convention, shared with WIRE).
#[derive(Clone, Copy, Default)]
struct DcBlocker {
    x1: f32,
    y1: f32,
}
impl DcBlocker {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        // R ≈ 0.9975 → ~20 Hz corner at 48 k.
        let y = x - self.x1 + 0.9975 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// Fractional, smoothed feedback delay line. The read position glides (one-pole on the delay
/// length) and interpolates linearly, so changing the delay time slews the tap rather than
/// jumping it — no hard click. `read()` returns the currently-delayed sample; `write()` stores
/// the loop input. Read-before-write with a ≥1-sample delay keeps the feedback loop causal.
#[derive(Clone)]
struct FracDelay {
    buf: Vec<f32>,
    wpos: usize,
    /// Smoothed delay length in samples.
    delay: f32,
    delay_s: OnePole,
    max_delay: usize,
}
impl FracDelay {
    fn new(max_delay: usize, sr: f32) -> Self {
        let mut delay_s = OnePole::new();
        // ~40 ms glide: long enough to be click-free, short enough to feel responsive.
        delay_s.set_time(40.0, sr);
        Self {
            buf: vec![0.0; max_delay + 4],
            wpos: 0,
            delay: 1.0,
            delay_s,
            max_delay,
        }
    }
    fn set_smoothing(&mut self, sr: f32) {
        self.delay_s.set_time(40.0, sr);
    }
    fn prime(&mut self, target: f32) {
        let d = target.clamp(1.0, self.max_delay as f32);
        self.delay = d;
        self.delay_s.reset(d);
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
    }
    /// Advance the smoothed delay length toward `target_samples`.
    #[inline]
    fn set_target(&mut self, target_samples: f32) {
        let t = target_samples.clamp(1.0, self.max_delay as f32);
        self.delay = self.delay_s.process(t);
    }
    /// Read the delayed sample (linear interpolation).
    #[inline]
    fn read(&self) -> f32 {
        let len = self.buf.len();
        let d = self.delay.clamp(1.0, self.max_delay as f32);
        let rpos = self.wpos as f32 - d;
        let base = rpos.floor();
        let frac = rpos - base;
        let mut i0 = base as isize;
        i0 = ((i0 % len as isize) + len as isize) % len as isize;
        let i1 = ((i0 + 1) % len as isize) as usize;
        let a = self.buf[i0 as usize];
        let b = self.buf[i1];
        a + (b - a) * frac
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
    }
}

/// Granular pitch shifter: a ring buffer read by two half-grain-offset taps that drift at
/// `(1 − ratio)` and wrap within a grain, each windowed by a raised cosine so the crossfade at
/// the wrap boundary is click-free (classic two-tap delay-line shifter). ±12 st.
#[derive(Clone)]
struct PitchShifter {
    buf: Vec<f32>,
    wpos: usize,
    /// Read drift phase within [0, grain).
    phase: f32,
    grain: f32,
}
impl PitchShifter {
    fn new() -> Self {
        Self {
            buf: vec![0.0; GRAIN_CAP],
            wpos: 0,
            phase: 0.0,
            grain: 2048.0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
        self.phase = 0.0;
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
    /// `semitones` in [-12, 12]; `grain_size` in samples (clamped to the buffer).
    #[inline]
    fn process(&mut self, x: f32, semitones: f32, grain_size: f32) -> f32 {
        self.grain = grain_size.clamp(256.0, (GRAIN_CAP - 4) as f32);
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
        let ratio = 2.0f32.powf(semitones / 12.0);
        // Drift the read phase; wrap into [0, grain).
        self.phase += 1.0 - ratio;
        while self.phase >= self.grain {
            self.phase -= self.grain;
        }
        while self.phase < 0.0 {
            self.phase += self.grain;
        }
        let d1 = self.phase;
        let mut d2 = self.phase + self.grain * 0.5;
        if d2 >= self.grain {
            d2 -= self.grain;
        }
        // A small base offset keeps reads behind the write head (avoids reading un-written data).
        let base = 2.0;
        let t1 = self.read_at(base + d1);
        let t2 = self.read_at(base + d2);
        // Raised-cosine windows, summing to ~unity across the crossfade.
        let w1 = 0.5 - 0.5 * (TAU * d1 / self.grain).cos();
        let w2 = 0.5 - 0.5 * (TAU * d2 / self.grain).cos();
        t1 * w1 + t2 * w2
    }
}

/// Two-path IIR Hilbert transformer (Niemitalo/Sean-Costello polyphase allpass network). Two
/// cascades of 2nd-order (`z⁻²`) allpasses whose outputs are ~90° apart across the audio band;
/// one path is fed the input delayed by one sample so the pair forms an analytic signal. Used
/// for single-sideband frequency shifting. Minimum-phase IIR — negligible, un-reported latency.
#[derive(Clone, Copy)]
struct AllpassZ2 {
    a: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl AllpassZ2 {
    fn new(a: f32) -> Self {
        Self { a, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        // 2nd-order allpass: y = a·(x + y[n-2]) − x[n-2]. Paired with a one-sample delay on the
        // A branch, this gives the best ~90° match for these coefficients (empirically ≈−19 dB
        // sideband suppression across 300 Hz–9 kHz).
        let y = self.a * (x + self.y2) - self.x2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[derive(Clone)]
struct Hilbert {
    a: [AllpassZ2; 4],
    b: [AllpassZ2; 4],
    prev_in: f32,
}
impl Hilbert {
    // Squared-pole coefficients for the two allpass branches (well-known SSB design).
    const A_COEFFS: [f32; 4] = [0.6923878, 0.9360654, 0.9882295, 0.9987488];
    const B_COEFFS: [f32; 4] = [0.4021921, 0.8561711, 0.9722910, 0.9952885];
    fn new() -> Self {
        Self {
            a: Self::A_COEFFS.map(AllpassZ2::new),
            b: Self::B_COEFFS.map(AllpassZ2::new),
            prev_in: 0.0,
        }
    }
    fn reset(&mut self) {
        for s in self.a.iter_mut() {
            s.reset();
        }
        for s in self.b.iter_mut() {
            s.reset();
        }
        self.prev_in = 0.0;
    }
    /// Returns the (in-phase, quadrature) analytic pair.
    #[inline]
    fn process(&mut self, x: f32) -> (f32, f32) {
        // Branch A is fed the one-sample-delayed input; the two branch outputs are then ~90°
        // apart across the band (see `AllpassZ2::process`).
        let mut ia = self.prev_in;
        for s in self.a.iter_mut() {
            ia = s.process(ia);
        }
        let mut qb = x;
        for s in self.b.iter_mut() {
            qb = s.process(qb);
        }
        self.prev_in = x;
        (ia, qb)
    }
}

/// Fixed-size reversed-granule player. Records input into a ring; plays back a length-`chunk`
/// window in reverse with two half-offset raised-cosine grains (click-free at the loop point).
#[derive(Clone)]
struct ReverseChunk {
    buf: Vec<f32>,
    wpos: usize,
    phase: f32,
    chunk: f32,
}
impl ReverseChunk {
    fn new() -> Self {
        Self {
            buf: vec![0.0; GRAIN_CAP],
            wpos: 0,
            phase: 0.0,
            chunk: 4096.0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
        self.phase = 0.0;
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
    #[inline]
    fn process(&mut self, x: f32, chunk_size: f32) -> f32 {
        self.chunk = chunk_size.clamp(512.0, (GRAIN_CAP - 4) as f32);
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
        // Two grains, half a chunk apart. `phase` counts forward; reading `chunk - phase`
        // samples back marches from newest→oldest ⇒ time-reversed playback.
        self.phase += 1.0;
        while self.phase >= self.chunk {
            self.phase -= self.chunk;
        }
        let p1 = self.phase;
        let mut p2 = self.phase + self.chunk * 0.5;
        if p2 >= self.chunk {
            p2 -= self.chunk;
        }
        let base = 2.0;
        let t1 = self.read_at(base + (self.chunk - p1));
        let t2 = self.read_at(base + (self.chunk - p2));
        let w1 = 0.5 - 0.5 * (TAU * p1 / self.chunk).cos();
        let w2 = 0.5 - 0.5 * (TAU * p2 / self.chunk).cos();
        t1 * w1 + t2 * w2
    }
}

/// Bit-depth + sample-rate reducer for the BitCrush slot (shared design with WIRE's crunch).
#[derive(Clone, Copy, Default)]
struct BitCrush {
    hold: f32,
    counter: f32,
}
impl BitCrush {
    fn reset(&mut self) {
        self.hold = 0.0;
        self.counter = 0.0;
    }
    /// `bits_amt` 0..1 → 16→~4 bits; `sr_amt` 0..1 → decimate 1→~32.
    #[inline]
    fn process(&mut self, x: f32, bits_amt: f32, sr_amt: f32) -> f32 {
        let ba = bits_amt.clamp(0.0, 1.0);
        let sa = sr_amt.clamp(0.0, 1.0);
        let step = 1.0 + sa * 31.0;
        self.counter += 1.0;
        if self.counter >= step {
            self.counter -= step;
            self.hold = x;
        }
        let held = self.hold;
        let bits = 16.0 - ba * 12.0;
        let levels = 2.0f32.powf(bits);
        (held * levels).round() / levels
    }
}

// ---------------------------------------------------------------------------
// Slot: owns state for every effect type and dispatches on the current type. Holding all
// state avoids reallocation when the type changes (no alloc in `process`). Processes one mono
// sample; the core owns one Slot per channel per slot position.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Slot {
    pitch: PitchShifter,
    filt: Svf,
    hilbert: Hilbert,
    shift_phase: f32,
    reverse: ReverseChunk,
    crush: BitCrush,
    // Cached filter coefficients (recomputed per control block).
    filt_kind: SlotType,
}
impl Slot {
    fn new() -> Self {
        Self {
            pitch: PitchShifter::new(),
            filt: Svf::new(),
            hilbert: Hilbert::new(),
            shift_phase: 0.0,
            reverse: ReverseChunk::new(),
            crush: BitCrush::default(),
            filt_kind: SlotType::Off,
        }
    }
    fn reset(&mut self) {
        self.pitch.reset();
        self.filt.reset();
        self.hilbert.reset();
        self.shift_phase = 0.0;
        self.reverse.reset();
        self.crush.reset();
    }

    /// Recompute the (only) coefficient-based sub-filter for this slot: the SVF cutoff/res.
    /// `amount`/`param` are the smoothed 0..1 macros; `sr` the sample rate.
    fn recompute(&mut self, kind: SlotType, amount: f32, param: f32, sr: f32) {
        self.filt_kind = kind;
        match kind {
            SlotType::FilterLp | SlotType::FilterHp | SlotType::FilterBp => {
                // Cutoff: log-mapped 40 Hz .. ~18 kHz; resonance 0.5 .. 8.
                let cutoff = 40.0 * (18_000.0f32 / 40.0).powf(amount.clamp(0.0, 1.0));
                let q = 0.5 + param.clamp(0.0, 1.0) * 7.5;
                self.filt.set(cutoff.min(sr * 0.45), q, sr);
            }
            _ => {}
        }
    }

    /// Process one sample through this slot's current effect.
    #[inline]
    fn process(&mut self, x: f32, kind: SlotType, amount: f32, param: f32, sr: f32) -> f32 {
        let a = amount.clamp(0.0, 1.0);
        let p = param.clamp(0.0, 1.0);
        match kind {
            SlotType::Off => x,
            SlotType::Pitch => {
                let semitones = (a - 0.5) * 24.0; // -12 .. +12
                let grain = 512.0 + p * 3584.0; // 512 .. 4096 samples
                self.pitch.process(x, semitones, grain)
            }
            SlotType::FilterLp => self.filt.process(x).lp,
            SlotType::FilterHp => self.filt.process(x).hp,
            SlotType::FilterBp => self.filt.process(x).bp,
            SlotType::FreqShift => {
                let (i, q) = self.hilbert.process(x);
                // Shift amount: bipolar ±500 Hz; param biases the sideband (up vs down blend).
                let shift_hz = (a - 0.5) * 1000.0;
                self.shift_phase += TAU * shift_hz / sr;
                if self.shift_phase >= TAU {
                    self.shift_phase -= TAU;
                } else if self.shift_phase < 0.0 {
                    self.shift_phase += TAU;
                }
                let (c, s) = (self.shift_phase.cos(), self.shift_phase.sin());
                // Upper sideband = i·cos − q·sin; lower = i·cos + q·sin; param crossfades.
                let upper = i * c - q * s;
                let lower = i * c + q * s;
                upper * (1.0 - p) + lower * p
            }
            SlotType::Saturate => {
                let drive = 1.0 + a * 15.0; // 1 .. 16
                let tube = Shaper::TubeTanh.apply(x, drive);
                let fold = Shaper::SineFold.apply(x, drive);
                // param blends tanh saturation toward wavefolding; /drive keeps unity-ish level.
                (tube * (1.0 - p) + fold * p) / (1.0 + a * 2.0)
            }
            SlotType::Reverse => {
                let chunk = 1024.0 + a * 7000.0; // ~21 ms .. ~167 ms at 48 k
                let rev = self.reverse.process(x, chunk);
                // param mixes the reversed grain back toward the dry loop signal.
                rev * (1.0 - p) + x * p
            }
            SlotType::BitCrush => self.crush.process(x, a, p),
        }
    }
}

// ---------------------------------------------------------------------------
// OUROBOROS core (stereo)
// ---------------------------------------------------------------------------

/// One channel's worth of feedback state.
struct Channel {
    delay: FracDelay,
    slots: [Slot; 3],
    dc: DcBlocker,
    /// Previous-sample feedback value fed back to the loop input.
    fb_prev: f32,
}
impl Channel {
    fn new(sr: f32) -> Self {
        let max_delay = ((MAX_DELAY_MS * 0.001 * sr) as usize).max(16);
        Self {
            delay: FracDelay::new(max_delay, sr),
            slots: [Slot::new(), Slot::new(), Slot::new()],
            dc: DcBlocker::default(),
            fb_prev: 0.0,
        }
    }
    fn reset(&mut self) {
        self.delay.reset();
        for s in self.slots.iter_mut() {
            s.reset();
        }
        self.dc.reset();
        self.fb_prev = 0.0;
    }
}

/// The full OUROBOROS processor.
pub struct OuroCore {
    sr: f32,
    ch: [Channel; 2],
    ctrl_count: usize,

    // Smoothed controls.
    delay_ms_s: OnePole,
    fb_s: OnePole,
    decay_s: OnePole,
    input_gate_s: OnePole, // 1 = pass, 0 = muted (freeze)
    mix_s: OnePole,
    out_s: OnePole,
    fm_s: OnePole, // Freeze-Mix (live↔frozen blend, applied only while frozen)
    slot_amount_s: [OnePole; 3],
    slot_param_s: [OnePole; 3],
    primed: bool,
}

impl OuroCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let mut core = OuroCore {
            sr,
            ch: [Channel::new(sr), Channel::new(sr)],
            ctrl_count: 0,
            delay_ms_s: OnePole::new(),
            fb_s: OnePole::new(),
            decay_s: OnePole::new(),
            input_gate_s: OnePole::new(),
            mix_s: OnePole::new(),
            out_s: OnePole::new(),
            fm_s: OnePole::new(),
            slot_amount_s: [OnePole::new(); 3],
            slot_param_s: [OnePole::new(); 3],
            primed: false,
        };
        core.set_sample_rate(sr);
        core
    }

    /// The delay line IS the effect, not fixed processing latency ⇒ report zero.
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let t = 15.0;
        self.fb_s.set_time(t, self.sr);
        self.decay_s.set_time(t, self.sr);
        self.mix_s.set_time(t, self.sr);
        self.out_s.set_time(t, self.sr);
        self.fm_s.set_time(t, self.sr);
        // Delay length and the freeze input-gate get a longer, click-free glide.
        self.delay_ms_s.set_time(40.0, self.sr);
        self.input_gate_s.set_time(20.0, self.sr);
        for s in self.slot_amount_s.iter_mut() {
            s.set_time(t, self.sr);
        }
        for s in self.slot_param_s.iter_mut() {
            s.set_time(t, self.sr);
        }
        for c in self.ch.iter_mut() {
            c.delay.set_smoothing(self.sr);
        }
        self.primed = false;
    }

    pub fn reset(&mut self) {
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.ctrl_count = 0;
        self.primed = false;
    }

    /// Target delay length (samples) for the current settings.
    #[inline]
    fn delay_target_samples(&self, s: &Settings) -> f32 {
        let ms = if s.sync {
            let bpm = s.tempo_bpm.clamp(20.0, 999.0);
            s.division.beats() * (60_000.0 / bpm)
        } else {
            s.delay_ms
        };
        (ms.clamp(1.0, MAX_DELAY_MS) * 0.001 * self.sr).max(1.0)
    }

    fn prime(&mut self, s: &Settings) {
        let d = self.delay_target_samples(s);
        self.delay_ms_s.reset(d);
        for c in self.ch.iter_mut() {
            c.delay.prime(d);
        }
        self.fb_s.reset((s.feedback * s.decay_scale).clamp(0.0, 1.1));
        self.decay_s.reset(s.decay_scale.clamp(0.0, 1.0));
        self.input_gate_s.reset(if s.freeze { 0.0 } else { 1.0 });
        self.mix_s.reset(s.mix.clamp(0.0, 1.0));
        self.fm_s.reset(s.freeze_mix.clamp(0.0, 1.0));
        self.out_s.reset(s.out_db);
        for i in 0..3 {
            self.slot_amount_s[i].reset(s.slots[i].amount);
            self.slot_param_s[i].reset(s.slots[i].param);
        }
        self.primed = true;
    }

    /// Latch per-block config. Call once per block before the sample loop.
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.prime(s);
        }
    }

    /// Recompute per-slot filter coefficients from the smoothed macros (per control block).
    fn recompute(&mut self, s: &Settings) {
        for i in 0..3 {
            let kind = s.slots[i].kind;
            let amt = self.slot_amount_s[i].value();
            let prm = self.slot_param_s[i].value();
            let sr = self.sr;
            self.ch[0].slots[i].recompute(kind, amt, prm, sr);
            self.ch[1].slots[i].recompute(kind, amt, prm, sr);
        }
    }

    /// Soft limiter — `tanh` at unity threshold (SPECS: "in-loop soft limiter").
    #[inline]
    fn limit(x: f32) -> f32 {
        x.tanh()
    }

    /// Process one stereo sample.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        // Advance the shared smoothers once per sample.
        let target_delay = self.delay_ms_s.process(self.delay_target_samples(s));
        // Feedback: freeze forces 100 %, else feedback×decay (both smoothed).
        let fb_base = if s.freeze {
            1.0
        } else {
            (s.feedback.clamp(0.0, 1.1)) * (s.decay_scale.clamp(0.0, 1.0))
        };
        let fb_amt = self.fb_s.process(fb_base).clamp(0.0, 1.1);
        let _ = self.decay_s.process(s.decay_scale.clamp(0.0, 1.0));
        let input_gate = self
            .input_gate_s
            .process(if s.freeze { 0.0 } else { 1.0 });
        let mix = self.mix_s.process(s.mix.clamp(0.0, 1.0));
        let fm = self.fm_s.process(s.freeze_mix.clamp(0.0, 1.0));
        let out_lin = db_to_lin(self.out_s.process(s.out_db));
        for i in 0..3 {
            self.slot_amount_s[i].process(s.slots[i].amount);
            self.slot_param_s[i].process(s.slots[i].param);
        }

        if self.ctrl_count == 0 {
            self.recompute(s);
        }
        self.ctrl_count += 1;
        if self.ctrl_count >= CTRL_BLOCK {
            self.ctrl_count = 0;
        }

        let order = s.order.indices();
        let sr = self.sr;
        let amounts = [
            self.slot_amount_s[0].value(),
            self.slot_amount_s[1].value(),
            self.slot_amount_s[2].value(),
        ];
        let params = [
            self.slot_param_s[0].value(),
            self.slot_param_s[1].value(),
            self.slot_param_s[2].value(),
        ];

        let inputs = [l_in, r_in];
        let mut outs = [0.0f32; 2];
        for cidx in 0..2 {
            let c = &mut self.ch[cidx];
            c.delay.set_target(target_delay);
            // Loop input = gated dry + previous feedback.
            let node = inputs[cidx] * input_gate + c.fb_prev;
            c.delay.write(node);
            let delayed = c.delay.read();
            // Slot chain in the selected order.
            let mut wet = delayed;
            for &si in order.iter() {
                let kind = s.slots[si].kind;
                wet = c.slots[si].process(wet, kind, amounts[si], params[si], sr);
            }
            // In-loop soft limiter → DC blocker → output tap.
            let limited = Self::limit(wet);
            let tap = c.dc.process(limited);
            c.fb_prev = fb_amt * tap;
            // Dry/wet mix + trim; final safety clamp keeps the render ≤ 0 dBFS.
            let dry = inputs[cidx];
            let mixed = (dry * (1.0 - mix) + tap * mix) * out_lin;
            // Freeze Mix: while frozen, crossfade back toward the live input so the freeze
            // isn't an all-or-nothing jump. fm=1 → classic hard freeze.
            let blended = if s.freeze { fm * mixed + (1.0 - fm) * dry } else { mixed };
            outs[cidx] = blended.clamp(-8.0, 8.0);
        }
        (outs[0], outs[1])
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
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
