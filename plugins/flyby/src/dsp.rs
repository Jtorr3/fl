//! FLYBY — pure-DSP core for the doppler spatializer (SPECS "FLYBY", Transfer clone).
//!
//! A mono source is flown around a **closed path** on an XY plane with the listener fixed at
//! the origin. As the source moves, its distance `r` and azimuth `θ` to the listener change,
//! and four physically-motivated cues are synthesised per sample:
//!
//! ```text
//!  in(mono) ─► fractional delay line  ─► distance gain 1/max(r,r0) ─► air LP(cutoff ∝ 1/r)
//!               read @ delay = r/c              (level)                    (absorption)
//!               (Catmull-Rom, rate-clamped)                                     │
//!                                                                               ▼
//!                                   equal-power pan(θ) ─► micro-ITD(θ) ─► width ─► mix ─► out
//! ```
//!
//! * **Doppler** is the moving fractional read: `delay = r/c`. As `r` changes the read pointer
//!   slews, so the pitch bends (approach → up, recede → down) exactly like a real fly-by. The
//!   read position is **rate-clamped** so a sharp path corner can't produce a pitch spike
//!   (done-bar 4). The interpolation is 4-point Catmull-Rom.
//! * **Distance**: `gain = r0 / max(r, r0)` (inverse-distance falloff, clamped near the origin).
//! * **Air absorption**: a one-pole low-pass whose cutoff falls with distance (far = dark).
//! * **Pan**: equal-power from the horizontal direction cosine, plus an optional opposite-ear
//!   **micro-ITD** (≤ 0.6 ms) for extra externalisation.
//! * **Width** is a post-pan mid/side control.
//!
//! ## Path
//! The path is a **closed Catmull-Rom loop** through 4–8 control points (nodes) in normalized
//! `[-1.5, 1.5]` space; the view is scaled by `size`. Three starting layouts (Circle / Ellipse /
//! Figure-8) are placed **off-centre** from the listener so even the Circle sweeps the source
//! nearer and farther (a genuine fly-by, not a constant-radius orbit → real doppler). The
//! traversal position is phase-driven: a free rate in Hz or a BPM-synced loop length.
//!
//! ## Latency
//! The delay line **is the effect** (distance = delay), not fixed processing latency, so FLYBY
//! reports **zero** latency, exactly like OUROBOROS. The lag-0 single-coherent-peak regression
//! therefore does not apply (there is no lag-0 wet to align); instead `mix = 0` **nulls against
//! the dry input** (see `tests.rs`).
//!
//! Pure Rust, shared verbatim between the nih-plug `process` path and the offline/done-bar tests.

use std::f32::consts::{FRAC_PI_2, PI};
use suite_core::dsp::OnePole;

/// Max number of path control points.
pub const MAX_NODES: usize = 8;
/// Min number of path control points.
pub const MIN_NODES: usize = 4;

/// Scaled speed of sound (units/second). Real air is 343 m/s; we run slower so that musical
/// path sizes and traversal rates give an audible doppler bend (SPECS: "c scaled so Size gives
/// audible doppler at sane rates"). `delay_seconds = r / SPEED`.
pub const SPEED: f32 = 100.0;

/// Reference distance (path units) for the inverse-distance law and the near clamp. The source
/// never gets louder than `1/r0`-scaled unity, and `r` is clamped to this to keep `1/r` finite.
pub const R0: f32 = 0.6;

/// Minimum (base) read delay in seconds — keeps the fractional read causal behind the write head
/// even when `r → 0`, and is the delay when `doppler = 0`.
pub const MIN_DELAY_S: f32 = 0.002;

/// Rate clamp: the maximum change in the read delay per sample (samples of delay / sample). At
/// 0.5 the read pointer speed stays within `[0.5, 1.5]×` → doppler pitch is bounded to roughly a
/// fifth up / octave down, and a sharp corner can never produce a hard step (done-bar 4).
pub const RATE_CLAMP: f32 = 0.5;

/// Micro-ITD maximum, seconds (≤ 0.6 ms per SPECS).
pub const ITD_MAX_S: f32 = 0.0006;

/// Control-block length: `r`, `θ`, and the air cutoff recompute this often (samples). Per-sample
/// we still slew the delay and smooth the gains, so nothing steps at the block boundary.
const CTRL_BLOCK: usize = 16;

/// Path starting layouts (SPECS: Circle / Ellipse / Figure-8). Each fills a node array with a
/// shape placed off-centre from the listener so the source flies *by* (near then far).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathShape {
    Circle,
    Ellipse,
    Figure8,
}

impl PathShape {
    pub fn from_index(i: usize) -> PathShape {
        match i {
            0 => PathShape::Circle,
            1 => PathShape::Ellipse,
            _ => PathShape::Figure8,
        }
    }

    /// Fill `nodes` with this shape using `count` control points and return `count` (clamped to
    /// [`MIN_NODES`, `MAX_NODES`]). Coordinates are in path units; `(0,0)` is the listener.
    pub fn layout(self, nodes: &mut [(f32, f32); MAX_NODES], count: usize) -> usize {
        let n = count.clamp(MIN_NODES, MAX_NODES);
        match self {
            PathShape::Circle => {
                // Circle radius 0.85 centred at (0, 0.5): the listener sits off-centre inside it,
                // so r sweeps ~0.35 → 1.35 (near pass + far side) with a full pan rotation.
                for (i, slot) in nodes.iter_mut().take(n).enumerate() {
                    let a = 2.0 * PI * i as f32 / n as f32;
                    *slot = (0.85 * a.sin(), 0.5 + 0.85 * a.cos());
                }
            }
            PathShape::Ellipse => {
                // Wide horizontal ellipse (a=1.1, b=0.5) centred at (0, 0.45): a broad left→right
                // fly-past close to the listener at the bottom of the arc.
                for (i, slot) in nodes.iter_mut().take(n).enumerate() {
                    let a = 2.0 * PI * i as f32 / n as f32;
                    *slot = (1.1 * a.sin(), 0.45 + 0.5 * a.cos());
                }
            }
            PathShape::Figure8 => {
                // Lemniscate of Gerono centred on the listener: the source crosses close to the
                // origin twice per loop with sharp direction changes (the rate-clamp stress path).
                for (i, slot) in nodes.iter_mut().take(n).enumerate() {
                    let a = 2.0 * PI * i as f32 / n as f32;
                    *slot = (1.15 * a.sin(), 0.75 * (2.0 * a).sin());
                }
            }
        }
        n
    }
}

/// BPM-sync loop length (how many beats one full traversal of the path takes).
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
    /// Beats (quarter notes) per full loop.
    #[inline]
    pub fn beats(self) -> f32 {
        match self {
            SyncDivision::Half => 2.0,
            SyncDivision::Bar => 4.0,
            SyncDivision::TwoBars => 8.0,
            SyncDivision::FourBars => 16.0,
        }
    }
}

/// A full snapshot of FLYBY's controls (plain, un-normalized values).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Path control points, path units. Only the first `node_count` are used.
    pub nodes: [(f32, f32); MAX_NODES],
    pub node_count: usize,
    /// Free traversal rate (Hz = loops/second) when not synced.
    pub speed_hz: f32,
    pub sync: bool,
    pub division: SyncDivision,
    /// Host tempo (BPM) for sync; falls back to 120 if the host reports none.
    pub tempo_bpm: f32,
    /// View / distance scale (multiplies the node coordinates).
    pub size: f32,
    /// Doppler amount 0..1 — scales how much distance maps to read delay (0 = no doppler).
    pub doppler: f32,
    /// Air absorption amount 0..1 (0 = no distance filtering).
    pub air: f32,
    /// Micro-ITD toggle.
    pub itd: bool,
    /// Stereo width, 0..2 (1.0 = as-panned).
    pub width: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
        let node_count = PathShape::Circle.layout(&mut nodes, 6);
        Settings {
            nodes,
            node_count,
            speed_hz: 0.5,
            sync: false,
            division: SyncDivision::Bar,
            tempo_bpm: 120.0,
            size: 8.0,
            doppler: 0.7,
            air: 0.5,
            itd: true,
            width: 1.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Path evaluation (closed Catmull-Rom loop through the control points)
// ---------------------------------------------------------------------------

#[inline]
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    // Standard uniform Catmull-Rom (tension 0.5).
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Evaluate the closed Catmull-Rom path at phase `p` in [0, 1). Returns the source position in
/// path units (before the `size` scale). Shared by the audio core and the GUI's moving-dot /
/// path drawing, so the picture always matches the sound.
pub fn path_position(nodes: &[(f32, f32); MAX_NODES], count: usize, p: f32) -> (f32, f32) {
    let n = count.clamp(MIN_NODES, MAX_NODES);
    let ph = p.rem_euclid(1.0);
    let seg_f = ph * n as f32;
    let seg = (seg_f.floor() as usize) % n;
    let t = seg_f - seg_f.floor();
    let idx = |k: isize| -> usize { (((seg as isize + k).rem_euclid(n as isize)) as usize) % n };
    let a = nodes[idx(-1)];
    let b = nodes[idx(0)];
    let c = nodes[idx(1)];
    let d = nodes[idx(2)];
    (
        catmull_rom(a.0, b.0, c.0, d.0, t),
        catmull_rom(a.1, b.1, c.1, d.1, t),
    )
}

// ---------------------------------------------------------------------------
// Fractional delay line with Catmull-Rom interpolation
// ---------------------------------------------------------------------------

/// A mono fractional delay line read with 4-point Catmull-Rom interpolation. Preallocated to a
/// fixed maximum, allocation-free in `process` (safe under nih-plug's alloc guard).
#[derive(Clone)]
struct FracDelay {
    buf: Vec<f32>,
    wpos: usize,
}

impl FracDelay {
    fn new(max_delay: usize) -> Self {
        Self {
            buf: vec![0.0; max_delay.max(8) + 4],
            wpos: 0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
    }
    /// Read `delay` samples in the past (Catmull-Rom over the 4 samples straddling the tap).
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let len = self.buf.len() as isize;
        let d = delay.clamp(1.0, (self.buf.len() - 3) as f32);
        let rpos = self.wpos as f32 - d;
        let base = rpos.floor();
        let frac = rpos - base;
        let i1 = base as isize;
        let get = |k: isize| -> f32 {
            let idx = ((i1 + k).rem_euclid(len)) as usize;
            self.buf[idx]
        };
        catmull_rom(get(-1), get(0), get(1), get(2), frac)
    }
}

/// One-pole low-pass (air absorption), coefficient set from a cutoff in Hz.
#[derive(Clone, Copy, Default)]
struct LpOnePole {
    a: f32,
    z: f32,
}
impl LpOnePole {
    #[inline]
    fn set_cutoff(&mut self, hz: f32, sr: f32) {
        let fc = hz.clamp(20.0, sr * 0.49);
        // One-pole LP coefficient: y += a*(x - y), a = 1 - exp(-2πfc/sr).
        self.a = 1.0 - (-2.0 * PI * fc / sr).exp();
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.z += self.a * (x - self.z);
        self.z
    }
    fn reset(&mut self) {
        self.z = 0.0;
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// FLYBY core
// ---------------------------------------------------------------------------

/// The full FLYBY processor. Mono source in (L/R summed), spatialised stereo out.
pub struct FlybyCore {
    sr: f32,
    delay: FracDelay,
    /// Opposite-ear ITD delay lines (short).
    itd_l: FracDelay,
    itd_r: FracDelay,
    air_l: LpOnePole,
    air_r: LpOnePole,
    max_delay: usize,

    /// Traversal phase [0, 1).
    phase: f32,
    ctrl_count: usize,

    // Per-control-block targets (recomputed every CTRL_BLOCK), slewed/smoothed per sample.
    target_delay: f32,
    delay_slew: f32, // rate-clamped read delay (samples)
    gain_target: f32,
    gain_s: OnePole,
    pan_l_target: f32,
    pan_r_target: f32,
    pan_l_s: OnePole,
    pan_r_s: OnePole,
    itd_l_target: f32, // opposite-ear delay in samples for L
    itd_r_target: f32,
    itd_l_s: OnePole,
    itd_r_s: OnePole,
    cutoff_target: f32,
    cutoff_s: OnePole,

    mix_s: OnePole,
    out_s: OnePole,
    width_s: OnePole,

    primed: bool,
}

impl FlybyCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        // Max delay: MIN_DELAY + the largest possible r/SPEED. r_max ≈ size_max(30) *
        // shape_extent(~2) = 60 units → 0.6 s; add headroom → 1.2 s buffer.
        let max_delay = ((1.2 * sr) as usize).max(64);
        let itd_max = ((ITD_MAX_S * sr) as usize + 4).max(8);
        let mut core = FlybyCore {
            sr,
            delay: FracDelay::new(max_delay),
            itd_l: FracDelay::new(itd_max),
            itd_r: FracDelay::new(itd_max),
            air_l: LpOnePole::default(),
            air_r: LpOnePole::default(),
            max_delay,
            phase: 0.0,
            ctrl_count: 0,
            target_delay: (MIN_DELAY_S * sr).max(1.0),
            delay_slew: (MIN_DELAY_S * sr).max(1.0),
            gain_target: 1.0,
            gain_s: OnePole::new(),
            pan_l_target: FRAC_PI_2.cos(),
            pan_r_target: FRAC_PI_2.sin(),
            pan_l_s: OnePole::new(),
            pan_r_s: OnePole::new(),
            itd_l_target: 0.0,
            itd_r_target: 0.0,
            itd_l_s: OnePole::new(),
            itd_r_s: OnePole::new(),
            cutoff_target: 18_000.0,
            cutoff_s: OnePole::new(),
            mix_s: OnePole::new(),
            out_s: OnePole::new(),
            width_s: OnePole::new(),
            primed: false,
        };
        core.set_sample_rate(sr);
        core
    }

    /// The delay line IS the effect ⇒ report zero latency.
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let t = 10.0;
        self.gain_s.set_time(t, self.sr);
        self.pan_l_s.set_time(t, self.sr);
        self.pan_r_s.set_time(t, self.sr);
        self.itd_l_s.set_time(t, self.sr);
        self.itd_r_s.set_time(t, self.sr);
        self.cutoff_s.set_time(t, self.sr);
        self.mix_s.set_time(t, self.sr);
        self.out_s.set_time(t, self.sr);
        self.width_s.set_time(t, self.sr);
        self.primed = false;
    }

    pub fn reset(&mut self) {
        self.delay.reset();
        self.itd_l.reset();
        self.itd_r.reset();
        self.air_l.reset();
        self.air_r.reset();
        self.phase = 0.0;
        self.ctrl_count = 0;
        self.primed = false;
    }

    /// Current traversal phase [0, 1) — for the GUI moving dot.
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Phase increment per sample for the current settings (loops/sample).
    #[inline]
    fn phase_inc(&self, s: &Settings) -> f32 {
        let f_trav = if s.sync {
            let bpm = s.tempo_bpm.clamp(20.0, 999.0);
            let loop_beats = s.division.beats().max(0.25);
            bpm / (60.0 * loop_beats)
        } else {
            s.speed_hz.clamp(0.0, 20.0)
        };
        f_trav / self.sr
    }

    /// Recompute the per-control-block spatial targets from the current phase.
    fn recompute(&mut self, s: &Settings) {
        let (px, py) = path_position(&s.nodes, s.node_count, self.phase);
        let x = px * s.size;
        let y = py * s.size;
        let r = (x * x + y * y).sqrt();
        let r_eff = r.max(R0);

        // Doppler: delay = r / SPEED, scaled by the doppler amount, plus the causal base.
        let phys_delay = (r / SPEED) * self.sr;
        self.target_delay = (MIN_DELAY_S * self.sr) + s.doppler.clamp(0.0, 1.0) * phys_delay;
        if self.target_delay > (self.max_delay - 4) as f32 {
            self.target_delay = (self.max_delay - 4) as f32;
        }

        // Distance gain: inverse-distance law, clamped near the origin.
        self.gain_target = R0 / r_eff;

        // Air absorption cutoff ∝ 1/r, musically mapped and blended by the air amount.
        let far_cut = 18_000.0 * (R0 / (R0 + r)); // falls with distance
        self.cutoff_target = lerp(18_000.0, far_cut.max(300.0), s.air.clamp(0.0, 1.0));

        // Pan: horizontal direction cosine in [-1, 1] → equal-power gains.
        let pan = (x / r_eff).clamp(-1.0, 1.0);
        let ang = (pan + 1.0) * FRAC_PI_2 * 0.5; // 0..π/2
        self.pan_l_target = ang.cos();
        self.pan_r_target = ang.sin();

        // Micro-ITD: the ear *away* from the source is farther → delay that channel. pan>0
        // (source right) delays L; pan<0 delays R. Magnitude ∝ |pan|.
        let itd_samps = if s.itd {
            (ITD_MAX_S * self.sr) * pan.abs()
        } else {
            0.0
        };
        if pan >= 0.0 {
            self.itd_l_target = itd_samps;
            self.itd_r_target = 0.0;
        } else {
            self.itd_l_target = 0.0;
            self.itd_r_target = itd_samps;
        }
    }

    fn prime(&mut self, s: &Settings) {
        let (px, py) = path_position(&s.nodes, s.node_count, self.phase);
        let x = px * s.size;
        let y = py * s.size;
        let r = (x * x + y * y).sqrt();
        let r_eff = r.max(R0);
        let phys_delay = (r / SPEED) * self.sr;
        let d = ((MIN_DELAY_S * self.sr) + s.doppler.clamp(0.0, 1.0) * phys_delay)
            .clamp(1.0, (self.max_delay - 4) as f32);
        self.target_delay = d;
        self.delay_slew = d;
        self.gain_target = R0 / r_eff;
        self.gain_s.reset(self.gain_target);
        let pan = (x / r_eff).clamp(-1.0, 1.0);
        let ang = (pan + 1.0) * FRAC_PI_2 * 0.5;
        self.pan_l_target = ang.cos();
        self.pan_r_target = ang.sin();
        self.pan_l_s.reset(self.pan_l_target);
        self.pan_r_s.reset(self.pan_r_target);
        self.itd_l_target = 0.0;
        self.itd_r_target = 0.0;
        self.itd_l_s.reset(0.0);
        self.itd_r_s.reset(0.0);
        let far_cut = 18_000.0 * (R0 / (R0 + r));
        self.cutoff_target = lerp(18_000.0, far_cut.max(300.0), s.air.clamp(0.0, 1.0));
        self.cutoff_s.reset(self.cutoff_target);
        self.mix_s.reset(s.mix.clamp(0.0, 1.0));
        self.out_s.reset(s.out_db);
        self.width_s.reset(s.width.clamp(0.0, 2.0));
        self.primed = true;
    }

    /// Latch per-block config. Call once per block before the sample loop.
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.prime(s);
        }
    }

    /// Process one stereo sample.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        // Recompute spatial targets at the top of each control block.
        if self.ctrl_count == 0 {
            self.recompute(s);
        }
        self.ctrl_count += 1;
        if self.ctrl_count >= CTRL_BLOCK {
            self.ctrl_count = 0;
        }

        // Advance the traversal phase.
        self.phase += self.phase_inc(s);
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }

        // Rate-clamp the read delay toward its target (no pitch spikes at sharp corners).
        let dd = (self.target_delay - self.delay_slew).clamp(-RATE_CLAMP, RATE_CLAMP);
        self.delay_slew += dd;

        // Mono source.
        let mono = 0.5 * (l_in + r_in);
        self.delay.write(mono);
        let doppler = self.delay.read(self.delay_slew);

        // Distance gain (smoothed per sample toward R0/r).
        let gain = self.gain_s.process(self.gain_target);
        let far = doppler * gain;

        // Air absorption: shared cutoff, filtered per channel below (so ITD delays a filtered sig).
        let cutoff = self.cutoff_s.process(self.cutoff_target);
        self.air_l.set_cutoff(cutoff, self.sr);
        self.air_r.set_cutoff(cutoff, self.sr);

        // Equal-power pan (smoothed per sample).
        let gl = self.pan_l_s.process(self.pan_l_target);
        let gr = self.pan_r_s.process(self.pan_r_target);
        let mut wl = self.air_l.process(far * gl);
        let mut wr = self.air_r.process(far * gr);

        // Micro-ITD: delay the far ear's channel by a sub-ms amount (smoothed per sample).
        let dl = self.itd_l_s.process(self.itd_l_target);
        let dr = self.itd_r_s.process(self.itd_r_target);
        self.itd_l.write(wl);
        self.itd_r.write(wr);
        if dl > 0.01 {
            wl = self.itd_l.read(dl.max(1.0));
        }
        if dr > 0.01 {
            wr = self.itd_r.read(dr.max(1.0));
        }

        // Width (mid/side) post-pan.
        let width = self.width_s.process(s.width.clamp(0.0, 2.0));
        let mid = 0.5 * (wl + wr);
        let side = 0.5 * (wl - wr) * width;
        let mut ol = mid + side;
        let mut or = mid - side;

        // Dry/wet mix + output trim.
        let mix = self.mix_s.process(s.mix.clamp(0.0, 1.0));
        let out_lin = db_to_lin(self.out_s.process(s.out_db));
        ol = (l_in * (1.0 - mix) + ol * mix) * out_lin;
        or = (r_in * (1.0 - mix) + or * mix) * out_lin;

        (ol.clamp(-8.0, 8.0), or.clamp(-8.0, 8.0))
    }

    /// Stereo render from a mono input (each input sample fed to both channels of the source).
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

    /// Mono convenience renderer (returns the left channel in place) for quick checks.
    pub fn process_mono(&mut self, main: &mut [f32], s: &Settings) {
        self.configure(s);
        for m in main.iter_mut() {
            let (ol, _or) = self.process_sample(*m, *m, s);
            *m = ol;
        }
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
