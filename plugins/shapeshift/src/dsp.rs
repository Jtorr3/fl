//! SHAPESHIFT — pure-DSP core for the XY-morphing distortion (SPECS "SHAPESHIFT", Teuri clone).
//!
//! Signal flow (per SPECS):
//! ```text
//! in ─ pre-gain ─ 4x OS ─ [shaper A][B][C][D] ─ bilinear XY blend ─ post LP ─ mix ─ out
//! ```
//! Four **corners** (A/B/C/D) each select a waveshaper from a local 8-curve bank and carry a
//! per-corner input gain trim. An **XY position** (0..1 × 0..1) sets bilinear blend weights so
//! the output morphs continuously between the four shaper characters:
//!
//! ```text
//! y = Σ wᵢ(x,y) · shaperᵢ(gᵢ · pre · x)
//! ```
//!
//! with `wA=(1-X)(1-Y)`, `wB=X(1-Y)`, `wC=(1-X)Y`, `wD=XY` (a partition of unity, so the blend
//! is a convex combination and stays bounded). The whole morph runs **inside** a 4x oversampler
//! (GRIT's recipe); the blend weights are computed at the base rate (control rate relative to the
//! 4x block) from smoothed X/Y and applied to every oversampled sub-sample. A built-in **orbit
//! LFO** rotates the XY point around the user position (circle / figure-8, free or BPM-synced).
//!
//! Because the nonlinear path passes through the oversampler's linear-phase halfband FIRs it
//! imposes a fixed group delay; the dry/parallel path is delayed by the same integer amount
//! (`Oversampler4x::measure_group_delay()`, reported to the host as latency) so partial mix does
//! not comb-filter — the GRIT / HARD CHECKPOINT 1 discipline.
//!
//! This module is API-agnostic pure Rust and is shared verbatim between the nih-plug `process`
//! path and the offline harness tests, so the tested math is the shipped math.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use suite_core::dsp::{tape_soft, DelayLine, OnePole, Oversampler4x, Svf};

pub const NUM_CORNERS: usize = 4;

/// A local waveshaper bank (kept LOCAL to the crate per the suite DSP-local convention — nothing
/// added to suite-core). Each variant maps an already-gain-scaled input sample to a bounded
/// output (|y| ≲ 1.05). The suite's `tanh` tube, tape soft-knee, hard clip and sine fold are
/// reproduced here alongside four SHAPESHIFT additions (diode asymmetry, triangle wavefold,
/// 3rd-order Chebyshev, and a soft bit-crush) so the corner select is a single flat enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    /// Tube-style odd-harmonic saturation via `tanh`.
    TubeTanh,
    /// Tape-style cubic soft-knee saturation (suite `tape_soft`).
    TapeSoft,
    /// Asymmetric diode-style clipper (positive half saturates harder → even harmonics).
    DiodeAsym,
    /// Hard clipper at ±1.
    HardClip,
    /// Sine wavefolder (rounded fold-back on overdrive).
    SineFold,
    /// Triangle wavefolder (`asin(sin)` — sharper fold than the sine fold).
    WavefoldTri,
    /// 3rd-harmonic saturator — a polarity-preserving odd cubic `0.4x + 0.6x³` (normalised so
    /// `f(±1)=±1`). The textbook Chebyshev `T₃(x)=4x³−3x` maps a cosine to a *pure* 3rd harmonic
    /// (`cos3θ`) with ZERO fundamental and an INVERTED small-signal slope (−3 near 0); in this XY
    /// morph that made corner C a sub/fundamental "volume hole" that cancelled the other corners
    /// in the bilinear blend. The sign-corrected cubic keeps the strong 3rd-harmonic colour while
    /// preserving the fundamental's level and polarity, so the corner morphs smoothly.
    Cheby3,
    /// Soft digital bit-crush: quantise to a few levels, then blend back toward the linear
    /// value so the staircase is smoothed (bit-crush character without the raw alias floor).
    BitcrushSoft,
}

impl Corner {
    pub const ALL: [Corner; 8] = [
        Corner::TubeTanh,
        Corner::TapeSoft,
        Corner::DiodeAsym,
        Corner::HardClip,
        Corner::SineFold,
        Corner::WavefoldTri,
        Corner::Cheby3,
        Corner::BitcrushSoft,
    ];

    pub fn from_index(i: usize) -> Corner {
        Corner::ALL[i.min(Corner::ALL.len() - 1)]
    }

    /// Apply the shaper to an already-gain-scaled sample `x`. Output is bounded to ≈±1.
    #[inline]
    pub fn apply(self, x: f32) -> f32 {
        match self {
            Corner::TubeTanh => x.tanh(),
            Corner::TapeSoft => tape_soft(x),
            Corner::DiodeAsym => {
                // Positive half driven harder than the negative half → asymmetric transfer
                // (even + odd harmonics). Bounded by tanh; the DC blocker downstream removes
                // the small offset the asymmetry introduces.
                if x >= 0.0 {
                    (x * 1.20).tanh()
                } else {
                    0.85 * (x * 0.70).tanh()
                }
            }
            Corner::HardClip => x.clamp(-1.0, 1.0),
            Corner::SineFold => (x * FRAC_PI_2).sin(),
            Corner::WavefoldTri => {
                // Triangle-wave fold: identity for |x|≤1, folds back linearly beyond. `asin(sin)`
                // gives the ideal triangle, bounded ±1.
                (2.0 / PI) * (x * FRAC_PI_2).sin().clamp(-1.0, 1.0).asin()
            }
            Corner::Cheby3 => {
                // Polarity-preserving 3rd-harmonic cubic (see the `Cheby3` doc): `0.4c + 0.6c³`.
                // Raw `T₃ = 4c³ − 3c` nulls the fundamental and inverts small-signal polarity,
                // which cancels in the XY blend (a volume hole at corner C). This form keeps a
                // pronounced 3rd harmonic (~18% on a sinusoid) but preserves the fundamental so
                // the corner has comparable level and morphs smoothly. Monotonic, `f(±1)=±1`.
                let c = x.clamp(-1.0, 1.0);
                0.4 * c + 0.6 * c * c * c
            }
            Corner::BitcrushSoft => {
                const LEVELS: f32 = 6.0;
                let c = x.clamp(-1.0, 1.0);
                let q = (c * LEVELS).round() / LEVELS;
                (0.72 * q + 0.28 * c).clamp(-1.0, 1.0)
            }
        }
    }
}

/// Orbit LFO trajectory shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitShape {
    Circle,
    Figure8,
}

impl OrbitShape {
    pub fn from_index(i: usize) -> OrbitShape {
        match i {
            1 => OrbitShape::Figure8,
            _ => OrbitShape::Circle,
        }
    }
}

/// BPM-sync division for the orbit LFO (one full orbit cycle per division). Beats assume 4/4,
/// matching the rest of the suite's tempo-only sync helpers (FLYBY).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncDivision {
    Half,
    Bar,
    TwoBars,
    FourBars,
}

impl SyncDivision {
    pub fn from_index(i: usize) -> SyncDivision {
        match i {
            0 => SyncDivision::Half,
            1 => SyncDivision::Bar,
            2 => SyncDivision::TwoBars,
            _ => SyncDivision::FourBars,
        }
    }
    /// Beats (quarter notes) per full orbit cycle.
    pub fn beats_per_cycle(self) -> f32 {
        match self {
            SyncDivision::Half => 2.0,
            SyncDivision::Bar => 4.0,
            SyncDivision::TwoBars => 8.0,
            SyncDivision::FourBars => 16.0,
        }
    }
}

/// The XY offset the orbit LFO adds to the user point at a given normalised `phase` (0..1).
/// Public so the GUI can draw the live orbit path + dot from the same math the audio uses.
#[inline]
pub fn orbit_offset(shape: OrbitShape, phase: f32, radius: f32) -> (f32, f32) {
    let th = TAU * phase;
    match shape {
        OrbitShape::Circle => (radius * th.cos(), radius * th.sin()),
        // Lemniscate of Gerono: x = r·sin, y = r·sin·cos (a clean figure-8, bounded by r).
        OrbitShape::Figure8 => (radius * th.sin(), radius * th.sin() * th.cos()),
    }
}

/// Bilinear blend weights for a point (x,y) in the unit square. Corner A=(0,0), B=(1,0),
/// C=(0,1), D=(1,1). The four weights sum to 1 for all (x,y) in the square.
#[inline]
pub fn bilinear_weights(x: f32, y: f32) -> [f32; NUM_CORNERS] {
    let x = x.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);
    [
        (1.0 - x) * (1.0 - y), // A
        x * (1.0 - y),         // B
        (1.0 - x) * y,         // C
        x * y,                 // D
    ]
}

/// A full snapshot of SHAPESHIFT's controls (plain, un-normalized). Cheap to copy; the plugin
/// builds one per block from its params/smoothers, tests build them directly.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// User XY point, each 0..1.
    pub x: f32,
    pub y: f32,
    /// Corner shaper selects (A,B,C,D).
    pub corner: [Corner; NUM_CORNERS],
    /// Per-corner input gain trim, dB.
    pub gain_db: [f32; NUM_CORNERS],
    /// Pre-gain into the shaper bank, dB.
    pub pre_db: f32,
    /// Orbit LFO.
    pub orbit_on: bool,
    pub orbit_rate_hz: f32,
    pub orbit_sync: bool,
    pub orbit_div: SyncDivision,
    pub orbit_radius: f32,
    pub orbit_shape: OrbitShape,
    /// Orbit start-phase offset, 0..1.
    pub orbit_phase0: f32,
    /// Host tempo (BPM) for orbit sync.
    pub tempo_bpm: f32,
    /// Post low-pass cutoff, Hz (tames fold/crush harshness).
    pub post_lp_hz: f32,
    /// Auto-gain: match post-RMS to pre-RMS over 300 ms (±12 dB clamp).
    pub auto_gain: bool,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim, dB.
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            x: 0.5,
            y: 0.5,
            corner: [
                Corner::TubeTanh,
                Corner::TapeSoft,
                Corner::Cheby3,
                Corner::HardClip,
            ],
            gain_db: [0.0; NUM_CORNERS],
            pre_db: 6.0,
            orbit_on: false,
            orbit_rate_hz: 0.5,
            orbit_sync: false,
            orbit_div: SyncDivision::Bar,
            orbit_radius: 0.3,
            orbit_shape: OrbitShape::Circle,
            orbit_phase0: 0.0,
            tempo_bpm: 120.0,
            post_lp_hz: 16_000.0,
            auto_gain: false,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// First-order DC blocker (~5 Hz corner), keeps the diode/asymmetric offset out of the mix.
#[derive(Clone, Copy, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
    r: f32,
}

impl DcBlock {
    fn set(&mut self, sample_rate: f32) {
        self.r = 1.0 - (TAU * 5.0 / sample_rate);
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Per-channel nonlinear + post state.
struct Channel {
    os: Oversampler4x,
    dc: DcBlock,
    post_lp: Svf,
}

impl Channel {
    fn new() -> Self {
        Channel {
            os: Oversampler4x::new(),
            dc: DcBlock::default(),
            post_lp: Svf::new(),
        }
    }
    fn reset(&mut self, sr: f32) {
        self.os.reset();
        self.dc = DcBlock::default();
        self.dc.set(sr);
        self.post_lp.reset();
    }
}

/// Stereo SHAPESHIFT core (also usable mono by passing R = L). Holds per-channel oversampler /
/// filter state plus the shared XY-position smoothers, orbit phase, and auto-gain trackers.
pub struct ShapeshiftCore {
    sr: f32,
    ch: [Channel; 2],
    // Smoothed XY position (post-orbit). Weights are computed per base sample from these.
    x_s: OnePole,
    y_s: OnePole,
    primed: bool,
    // Orbit LFO. `orbit_phase` is a free-running accumulator (phase-offset independent); the
    // PHASE-knob offset is added at read time so turning PHASE rotates the orbit live.
    orbit_phase: f32,
    orbit_phase0: f32,
    phase_inc: f32,
    // Auto-gain: 300 ms one-pole running mean-square of pre / post (mono sum).
    ag_coef: f32,
    pre_ms: f32,
    post_ms: f32,
    // Dry-path delay compensation (matches the 4x oversampler group delay; reported as latency).
    dry_delay: [DelayLine; 2],
    latency: usize,
}

impl ShapeshiftCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let latency = Oversampler4x::measure_group_delay();
        let mut x_s = OnePole::new();
        let mut y_s = OnePole::new();
        x_s.set_time(5.0, sr);
        y_s.set_time(5.0, sr);
        let mut core = ShapeshiftCore {
            sr,
            ch: [Channel::new(), Channel::new()],
            x_s,
            y_s,
            primed: false,
            orbit_phase: 0.0,
            orbit_phase0: 0.0,
            phase_inc: 0.0,
            ag_coef: 0.0,
            pre_ms: 0.0,
            post_ms: 0.0,
            dry_delay: [DelayLine::new(latency), DelayLine::new(latency)],
            latency,
        };
        core.set_sample_rate(sr);
        core
    }

    /// Reported plugin latency (samples) = the oversampler group delay the dry path is
    /// compensated by. Constant across sample rates.
    pub fn latency_samples(&self) -> u32 {
        self.latency as u32
    }

    /// Current orbit phase (0..1), published to the GUI for the live orbit dot. Includes the
    /// live PHASE-knob offset so the drawn dot tracks the same angle the audio uses.
    pub fn orbit_phase(&self) -> f32 {
        (self.orbit_phase + self.orbit_phase0).rem_euclid(1.0)
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let n = 0.300 * self.sr;
        self.ag_coef = (-1.0 / n).exp();
        self.x_s.set_time(5.0, self.sr);
        self.y_s.set_time(5.0, self.sr);
        for c in self.ch.iter_mut() {
            c.dc.set(self.sr);
        }
    }

    pub fn reset(&mut self) {
        for c in self.ch.iter_mut() {
            c.reset(self.sr);
        }
        self.primed = false;
        self.orbit_phase = 0.0;
        self.orbit_phase0 = 0.0;
        self.pre_ms = 0.0;
        self.post_ms = 0.0;
        for d in self.dry_delay.iter_mut() {
            d.reset();
        }
    }

    /// Reconfigure block-rate state (post-LP cutoff, orbit phase increment, start phase) from a
    /// settings snapshot. Cheap enough for once-per-block use.
    pub fn configure(&mut self, s: &Settings) {
        for c in self.ch.iter_mut() {
            c.post_lp.set(s.post_lp_hz, 0.707, self.sr);
        }
        // Orbit phase increment (cycles/sample).
        let cyc_hz = if s.orbit_sync {
            (s.tempo_bpm.max(1.0) / 60.0) / s.orbit_div.beats_per_cycle()
        } else {
            s.orbit_rate_hz
        };
        self.phase_inc = (cyc_hz / self.sr).max(0.0);

        // Read the PHASE-knob offset every block (NOT only at prime) so turning ORBIT PHASE during
        // playback rotates the orbit live. It is applied as `angle = orbit_phase + orbit_phase0`
        // at read time, leaving the free-running accumulator undisturbed (no jump).
        self.orbit_phase0 = s.orbit_phase0.rem_euclid(1.0);

        // Prime the position smoothers on the first configure after a reset so a freshly-
        // instantiated core sits exactly at its target XY from sample 0 (needed for the
        // corner-null done-bar to be an exact-path null). The free-running orbit accumulator
        // starts at 0; the initial position already reflects the phase offset via `orbit_phase0`.
        if !self.primed {
            let (ox, oy) = if s.orbit_on {
                orbit_offset(s.orbit_shape, self.orbit_phase0, s.orbit_radius)
            } else {
                (0.0, 0.0)
            };
            self.x_s.reset((s.x + ox).clamp(0.0, 1.0));
            self.y_s.reset((s.y + oy).clamp(0.0, 1.0));
            self.orbit_phase = 0.0;
            self.primed = true;
        }
    }

    /// Process one stereo sample. Call [`configure`] once per block first. Returns `(l, r)`.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        // Advance the dry-delay lines every sample (before any early return) so the dry path
        // stays group-delay aligned with the oversampled wet path.
        let dry_l = self.dry_delay[0].process(l_in);
        let dry_r = self.dry_delay[1].process(r_in);

        // --- XY position (user point + orbit), smoothed → bilinear weights ---
        let (ox, oy) = if s.orbit_on {
            // Current angle = free-running accumulator + live PHASE offset (see `configure`).
            let angle = (self.orbit_phase + self.orbit_phase0).rem_euclid(1.0);
            orbit_offset(s.orbit_shape, angle, s.orbit_radius)
        } else {
            (0.0, 0.0)
        };
        // Advance the free-running orbit accumulator.
        self.orbit_phase = (self.orbit_phase + self.phase_inc).rem_euclid(1.0);

        let tx = (s.x + ox).clamp(0.0, 1.0);
        let ty = (s.y + oy).clamp(0.0, 1.0);
        let xs = self.x_s.process(tx);
        let ys = self.y_s.process(ty);
        let w = bilinear_weights(xs, ys);

        // Per-corner gains (copied into the OS closure).
        let pre = db_to_lin(s.pre_db);
        let g = [
            db_to_lin(s.gain_db[0]),
            db_to_lin(s.gain_db[1]),
            db_to_lin(s.gain_db[2]),
            db_to_lin(s.gain_db[3]),
        ];
        let c = s.corner;

        let inputs = [l_in, r_in];
        let mut wet = [0.0f32; 2];
        let mut pre_sum = 0.0f32;
        let mut post_sum = 0.0f32;

        for ci in 0..2 {
            let x = inputs[ci] * pre;
            pre_sum += inputs[ci] * inputs[ci];
            // XY-morph waveshaping, run at 4x oversampling. Weights are constant across the four
            // sub-samples (control rate); each corner shaper sees its own gain trim.
            let y = self.ch[ci].os.process(x, |v| {
                w[0] * c[0].apply(g[0] * v)
                    + w[1] * c[1].apply(g[1] * v)
                    + w[2] * c[2].apply(g[2] * v)
                    + w[3] * c[3].apply(g[3] * v)
            });
            let y = self.ch[ci].dc.process(y);
            let y = self.ch[ci].post_lp.process(y).lp;
            post_sum += y * y;
            wet[ci] = y;
        }

        // --- Auto-gain: match post-RMS to pre-RMS over 300 ms, ±12 dB clamp ---
        let mut ag = 1.0f32;
        if s.auto_gain {
            self.pre_ms = pre_sum + self.ag_coef * (self.pre_ms - pre_sum);
            self.post_ms = post_sum + self.ag_coef * (self.post_ms - post_sum);
            let ratio = (self.pre_ms.max(1.0e-12) / self.post_ms.max(1.0e-12)).sqrt();
            ag = ratio.clamp(db_to_lin(-12.0), db_to_lin(12.0));
        }

        // --- Mix (latency-compensated dry) + output trim, runaway/NaN safety clamp ±8.0 ---
        let out_lin = db_to_lin(s.out_db);
        let mix = s.mix.clamp(0.0, 1.0);
        let dry = [dry_l, dry_r];
        let mut out = [0.0f32; 2];
        for ci in 0..2 {
            let wv = wet[ci] * ag;
            let mixed = dry[ci] * (1.0 - mix) + wv * mix;
            out[ci] = (mixed * out_lin).clamp(-8.0, 8.0);
        }
        (out[0], out[1])
    }

    /// Stereo convenience: process interleaved-by-channel slices, returning `(l, r)` vectors.
    /// Feeds the same mono input to both channels (the harness signals are mono).
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

    /// Mono convenience for the offline harness: process `main` in place (L channel).
    pub fn process_mono(&mut self, main: &mut [f32], s: &Settings) {
        self.configure(s);
        for m in main.iter_mut() {
            let (l, _r) = self.process_sample(*m, *m, s);
            *m = l;
        }
    }
}

#[path = "tests.rs"]
#[cfg(test)]
mod tests;
