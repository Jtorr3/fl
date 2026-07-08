//! SEANCE — pure-DSP core for the ethereal vocal machine (SPECS "SEANCE";
//! Cynthoni/Sewerslvt-style ghost vocals). API-agnostic Rust, shared verbatim between the
//! nih-plug `process` path and the offline harness tests.
//!
//! ```text
//! in ─┬─ delay(latency) ───────────────────────────────────── dry ──┐
//!     └ ShiftEngine (pitch ±12 st + formant, envelope-preserve)      │  mix
//!       → chopper (BPM-synced gate: 4 shapes + random, smooth edges) │   ↓
//!       → shimmer FDN verb (Fdn8 + +12 st shifter in feedback,       ├── + ── out
//!         soft-limit + DC block)                                     │
//!       → wash (LP + wow: slow fractional-delay pitch drift)         │
//!       → ducker (keyed by the DRY env — wet SWELLS when the vocal   │
//!         pauses: the drowned-vocal trick) ───────────── wet ────────┘
//! ```
//!
//! Latency = the main [`ShiftEngine`] FFT size (2048); the dry path is delayed to match so
//! the `mix` knob aligns. Two shift engines run for stereo; the shimmer verb runs one cheap
//! mono +12 st engine in its feedback loop. Everything is preallocated in [`SeanceCore::new`];
//! the per-sample path is allocation-free (safe under nih-plug's `assert_process_allocs`).

use std::f32::consts::FRAC_PI_2;

use suite_core::dsp::{DelayLine, Detector, EnvFollower, OnePole, Svf};
use suite_core::fdn::{Fdn8, N};
use suite_core::shift::{ShiftEngine, DEFAULT_FFT, DEFAULT_HOP};
use suite_core::testsig::{Rng, TransportFrame};

/// Main analysis FFT for the formant-preserving shift (== reported latency).
pub const MAIN_FFT: usize = DEFAULT_FFT;
const MAIN_HOP: usize = DEFAULT_HOP;
/// Smaller/cheaper FFT for the shimmer-loop octave shifter.
const SHIMMER_FFT: usize = 1024;
const SHIMMER_HOP: usize = 256;

/// Chopper gate-edge smoothing (ms) — the "smooth edges 3–8 ms" spec.
const CHOP_EDGE_MS: f32 = 5.0;
/// Wash wow base delay (ms) and max modulation depth (ms).
const WOW_BASE_MS: f32 = 22.0;
const WOW_DEPTH_MS: f32 = 3.0;
/// Sidechain normaliser release (ms) — makes the duck level-independent.
const SC_NORM_REL_MS: f32 = 1200.0;
/// Audible-scalar smoothing time (ms).
const SMOOTH_MS: f32 = 12.0;

/// Beat length (in quarter-note beats) for each chop rate division, menu order.
pub const CHOP_DIVISIONS: [(&str, f32); 6] = [
    ("1/2", 2.0),
    ("1/4", 1.0),
    ("1/8", 0.5),
    ("1/8T", 1.0 / 3.0),
    ("1/16", 0.25),
    ("1/32", 0.125),
];
/// Number of chop pattern shapes (0..=3 shapes + 4 = Random).
pub const CHOP_PATTERNS: [&str; 5] = ["Square", "Stutter", "Ramp", "Double", "Random"];

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Settings snapshot (already macro-resolved, plain units, ratios linear)
// ---------------------------------------------------------------------------

/// A full snapshot of SEANCE's *effective* controls (macros already folded in). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Pitch shift ratio (2^(st/12)).
    pub pitch_ratio: f32,
    /// Formant shift ratio (2^(st/12)), independent of pitch.
    pub formant_ratio: f32,
    /// Envelope (formant) preservation on/off for the main shifter.
    pub preserve: bool,

    /// Chop pattern index (0..=4; 4 = Random).
    pub chop_pattern: usize,
    /// Chop rate division index into [`CHOP_DIVISIONS`].
    pub chop_rate: usize,
    /// Chop depth 0..1 (0 = no chopping).
    pub chop_depth: f32,
    /// Host tempo (BPM) for the synced chopper.
    pub tempo_bpm: f32,

    /// Verb size 0..1 (scales FDN delay lengths).
    pub verb_size: f32,
    /// Verb decay RT60 (seconds).
    pub verb_decay: f32,
    /// Shimmer amount 0..1 (octave-up feedback into the FDN loop).
    pub verb_shimmer: f32,
    /// Verb wet send 0..1.
    pub verb_wet: f32,

    /// Wash amount 0..1 (LP darkening + wow depth). 0 = wash bypassed.
    pub wash: f32,

    /// Duck depth 0..1 (wet is pulled down while the dry vocal is active).
    pub duck_depth: f32,
    /// Duck release (ms).
    pub duck_release_ms: f32,

    /// Dry/wet mix 0..1 (0 = pure dry, latency-matched).
    pub mix: f32,
    /// Output trim (linear).
    pub out_gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pitch_ratio: 1.0,
            formant_ratio: 1.0,
            preserve: true,
            chop_pattern: 0,
            chop_rate: 2, // 1/8
            chop_depth: 0.0,
            tempo_bpm: 120.0,
            verb_size: 0.6,
            verb_decay: 2.2,
            verb_shimmer: 0.35,
            verb_wet: 0.35,
            wash: 0.3,
            duck_depth: 0.4,
            duck_release_ms: 260.0,
            mix: 0.5,
            out_gain: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Raw controls → macro resolution → Settings
// ---------------------------------------------------------------------------

/// Raw (pre-macro) control values, in their natural param units. Both the plugin's
/// param snapshot and the preset loader build this, then call [`RawControls::resolve`] so
/// the macro math lives in exactly one place.
#[derive(Clone, Copy, Debug)]
pub struct RawControls {
    pub pitch_st: f32,
    pub formant_st: f32,
    pub preserve: bool,
    pub chop_pattern: usize,
    pub chop_rate: usize,
    pub chop_depth: f32,
    pub verb_size: f32,
    pub verb_decay: f32,
    pub verb_shimmer: f32,
    pub verb_wet: f32,
    pub wash: f32,
    pub duck_depth: f32,
    pub duck_release_ms: f32,
    pub ghost: f32,
    pub drown: f32,
    pub chop_macro: f32,
    pub mix: f32,
    pub out_gain: f32,
    pub tempo_bpm: f32,
}

impl Default for RawControls {
    fn default() -> Self {
        Self {
            pitch_st: 0.0,
            formant_st: 0.0,
            preserve: true,
            chop_pattern: 0,
            chop_rate: 2,
            chop_depth: 0.0,
            verb_size: 0.6,
            verb_decay: 2.2,
            verb_shimmer: 0.35,
            verb_wet: 0.35,
            wash: 0.3,
            duck_depth: 0.4,
            duck_release_ms: 260.0,
            ghost: 0.0,
            drown: 0.0,
            chop_macro: 0.0,
            mix: 0.5,
            out_gain: 1.0,
            tempo_bpm: 120.0,
        }
    }
}

impl RawControls {
    /// Fold the three macros into the underlying params and produce effective [`Settings`].
    ///
    /// - **GHOST** (`ghost`): formant up (+ up to 7 st) + wash + shift blend (mix).
    /// - **DROWN** (`drown`): verb size + wet + duck depth.
    /// - **CHOP** (`chop_macro`): chop depth (pattern density/depth).
    pub fn resolve(&self) -> Settings {
        let clamp01 = |v: f32| v.clamp(0.0, 1.0);
        let formant_eff = self.formant_st + self.ghost * 7.0;
        let wash_eff = clamp01(self.wash + self.ghost * 0.5);
        let mix_eff = clamp01(self.mix + self.ghost * 0.3);
        let size_eff = clamp01(self.verb_size + self.drown * 0.4);
        let wet_eff = clamp01(self.verb_wet + self.drown * 0.5);
        let duck_eff = clamp01(self.duck_depth + self.drown * 0.5);
        let chop_depth_eff = clamp01(self.chop_depth + self.chop_macro * 0.6);
        Settings {
            pitch_ratio: 2.0f32.powf(self.pitch_st / 12.0),
            formant_ratio: 2.0f32.powf(formant_eff / 12.0),
            preserve: self.preserve,
            chop_pattern: self.chop_pattern.min(CHOP_PATTERNS.len() - 1),
            chop_rate: self.chop_rate.min(CHOP_DIVISIONS.len() - 1),
            chop_depth: chop_depth_eff,
            tempo_bpm: self.tempo_bpm,
            verb_size: size_eff,
            verb_decay: self.verb_decay,
            verb_shimmer: clamp01(self.verb_shimmer),
            verb_wet: wet_eff,
            wash: wash_eff,
            duck_depth: duck_eff,
            duck_release_ms: self.duck_release_ms,
            mix: mix_eff,
            out_gain: self.out_gain,
        }
    }
}

// ---------------------------------------------------------------------------
// Chopper — BPM-synced gate
// ---------------------------------------------------------------------------

/// A tempo-synced gate. When the host transport is **playing**, the gate phase is derived
/// directly from the absolute playhead (`pos_beats` modulo the chop division), so chop
/// boundaries land exactly on the bar grid and every playback/bounce is identical. When the
/// transport is **stopped**, it free-runs from a local clock (the old behaviour) so the
/// chopper still works while auditioning. One of four pattern shapes (or a per-division
/// sample-and-hold random level) is one-pole slewed for click-free 3–8 ms edges; `depth`
/// blends the gate toward unity.
pub struct Chopper {
    sr: f32,
    // Free-run clock — used only while the transport is stopped.
    phase: f32, // 0..1 within the current division
    inc: f32,   // per-sample phase increment
    // Transport-locked grid.
    playing: bool,
    beat_pos: f64,          // absolute playhead in quarter-note beats (advanced per sample)
    beats_per_sample: f64,  // playhead advance per output sample
    beats_per_div: f32,     // length of one chop division in beats
    prev_phase: f32,        // last division phase (for S&H boundary detection)
    edge: OnePole,          // gate-edge smoother
    rng: Rng,
    rand_level: f32, // S&H random level for the Random pattern
    pattern: usize,
    depth: f32,
}

impl Chopper {
    pub fn new(sr: f32) -> Self {
        let mut edge = OnePole::new();
        edge.set_time(CHOP_EDGE_MS, sr);
        edge.reset(1.0);
        Self {
            sr,
            phase: 0.0,
            inc: 0.0,
            playing: false,
            beat_pos: 0.0,
            beats_per_sample: 0.0,
            beats_per_div: CHOP_DIVISIONS[2].1, // 1/8 default
            prev_phase: 0.0,
            edge,
            rng: Rng::new(0x5EA9CE01),
            rand_level: 1.0,
            pattern: 0,
            depth: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.beat_pos = 0.0;
        self.prev_phase = 0.0;
        self.edge.reset(1.0);
        self.rand_level = 1.0;
    }

    /// Recompute the phase increment / division for the current tempo + rate (block rate).
    pub fn configure(&mut self, tempo_bpm: f32, rate: usize, pattern: usize, depth: f32) {
        let beats = CHOP_DIVISIONS[rate.min(CHOP_DIVISIONS.len() - 1)].1;
        let period_s = (beats * 60.0 / tempo_bpm.max(20.0)).max(1.0e-4);
        self.inc = 1.0 / (period_s * self.sr);
        self.beats_per_div = beats;
        self.pattern = pattern.min(CHOP_PATTERNS.len() - 1);
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// Latch the host transport (block rate). While playing, the authoritative playhead
    /// (`ppq_pos`, in beats) is snapped in so the grid phase re-aligns every block; the
    /// per-sample advance then keeps it sample-accurate within the block.
    pub fn set_transport(&mut self, playing: bool, ppq_pos: f64, beats_per_sample: f64) {
        self.playing = playing;
        self.beats_per_sample = beats_per_sample.max(0.0);
        if playing {
            self.beat_pos = ppq_pos.max(0.0);
        }
    }

    /// The raw pattern gate (0..1) for a phase within one division.
    #[inline]
    fn raw_gate(&self, p: f32) -> f32 {
        match self.pattern {
            0 => if p < 0.5 { 1.0 } else { 0.0 },          // Square
            1 => if p < 0.25 { 1.0 } else { 0.0 },         // Stutter (gappier)
            2 => 1.0 - p,                                  // Ramp (tremolo down)
            3 => {                                         // Double pulse
                if p < 0.25 || (0.5..0.75).contains(&p) { 1.0 } else { 0.0 }
            }
            _ => self.rand_level,                          // Random (S&H per division)
        }
    }

    /// Advance one sample; return the smoothed gate multiplier.
    #[inline]
    pub fn process(&mut self) -> f32 {
        // Division phase: grid-locked to the playhead while playing, else the free-run clock.
        let phase = if self.playing && self.beats_per_div > 0.0 {
            ((self.beat_pos / self.beats_per_div as f64).rem_euclid(1.0)) as f32
        } else {
            self.phase
        };
        // A wrap (phase steps back toward 0) is a new division → redraw the S&H random level.
        if phase < self.prev_phase {
            self.rand_level = if (self.rng.next_u32() & 1) == 0 { 0.0 } else { 1.0 };
        }
        let raw = self.raw_gate(phase);
        self.prev_phase = phase;

        if self.playing {
            self.beat_pos += self.beats_per_sample;
            // Keep the free-run clock synced so a play→stop transition continues seamlessly.
            self.phase = phase;
        } else {
            self.phase += self.inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }

        let g = self.edge.process(raw);
        // depth blends gate → unity.
        1.0 - self.depth * (1.0 - g)
    }
}

// ---------------------------------------------------------------------------
// Shimmer FDN verb
// ---------------------------------------------------------------------------

/// First-order DC blocker.
#[derive(Clone, Copy, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
}
impl DcBlock {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + 0.995 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// Lush stereo FDN reverb with an octave-up phase-vocoder shifter in the feedback loop
/// (the "shimmer"). The shifted feedback is soft-limited (`tanh`) and DC-blocked so the
/// loop is bounded and drift-free even at high shimmer.
///
/// **Size crossfade (MURMUR pattern).** Changing the FDN delay lengths live snaps the
/// [`Fdn8`] read pointers, so automating SIZE / the DROWN macro crackles. To avoid this the
/// verb runs **two** `Fdn8` instances that both always process the input (so the idle one is
/// pre-warmed); on a SIZE change we reconfigure the idle FDN to the new lengths and
/// equal-power crossfade to it over [`VERB_XFADE_MS`]. Decay changes only recompute per-line
/// gains (no pointer jump), so they are applied live to both FDNs without a crossfade. Both
/// FDNs are preallocated, so the whole path stays allocation-free at process time.
pub struct ShimmerVerb {
    sr: f32,
    fdn: [Fdn8; 2],
    /// Index of the currently-audible FDN (crossfade origin).
    cur: usize,
    /// Crossfade position 0→1 toward the idle FDN (`1 − cur`). 0 = not crossfading.
    xf: f32,
    xf_inc: f32,
    crossfading: bool,
    /// SIZE the audible FDN is configured to, and the SIZE the idle FDN was just loaded with.
    cur_size: f32,
    idle_size: f32,
    /// Latest requested SIZE / decay (from `configure`).
    req_size: f32,
    decay: f32,
    prev_decay: f32,
    /// First-configure force flag (set the size directly at startup — buffers are empty, so
    /// there is nothing to click, and existing steady-state renders are unchanged).
    primed: bool,
    shifter: ShiftEngine, // mono, +12 st, in the feedback path
    dc: DcBlock,
    shimmer_fb: f32,      // one-sample-delayed shifted feedback
    max_delay: usize,
}

/// Nominal shortest / longest FDN line (ms) at size = 0.5 — a medium-large ethereal space.
const VMIN_MS: f32 = 20.0;
const VMAX_MS: f32 = 75.0;
const VSIZE_MIN: f32 = 0.5;
const VSIZE_MAX: f32 = 1.8;
const VMIN_DELAY: usize = 48;
/// Equal-power crossfade duration on a SIZE change (ms) — click-free delay-length swap.
const VERB_XFADE_MS: f32 = 60.0;
/// SIZE change (0..1) that triggers a new crossfade.
const VSIZE_EPS: f32 = 5.0e-4;

impl ShimmerVerb {
    pub fn new(sr: f32) -> Self {
        let max_ms = VMAX_MS * VSIZE_MAX * 1.1;
        let max_delay = ((max_ms * 0.001 * sr).ceil() as usize).max(VMIN_DELAY + 1);
        let mut shifter = ShiftEngine::new(SHIMMER_FFT, SHIMMER_HOP, sr);
        shifter.set_pitch_ratio(2.0); // +12 st
        shifter.set_envelope_preserve(false); // classic bright chipmunk shimmer
        let mk_fdn = || {
            let mut f = Fdn8::new(max_delay, sr);
            f.set_damping(0.35);
            f.set_diffusion(0.7);
            f
        };
        let mut v = Self {
            sr,
            fdn: [mk_fdn(), mk_fdn()],
            cur: 0,
            xf: 0.0,
            xf_inc: 1.0 / (VERB_XFADE_MS * 0.001 * sr).max(1.0),
            crossfading: false,
            cur_size: 0.6,
            idle_size: 0.6,
            req_size: 0.6,
            decay: 2.2,
            prev_decay: -1.0,
            primed: false,
            shifter,
            dc: DcBlock::default(),
            shimmer_fb: 0.0,
            max_delay,
        };
        // Load both FDNs with the default room directly (no crossfade at construction).
        v.configure(0.6, 2.2, true);
        v
    }

    pub fn reset(&mut self) {
        for f in self.fdn.iter_mut() {
            f.reset();
        }
        self.shifter.reset();
        self.dc.reset();
        self.shimmer_fb = 0.0;
        self.cur = 0;
        self.xf = 0.0;
        self.crossfading = false;
        self.primed = false;
    }

    /// Compute the eight (coprime-ish) delay lengths for a given SIZE.
    fn delays_for(&self, size: f32) -> [usize; N] {
        let scale = VSIZE_MIN + (VSIZE_MAX - VSIZE_MIN) * size.clamp(0.0, 1.0);
        let mut delays = [0usize; N];
        for i in 0..N {
            let frac = if N > 1 { i as f32 / (N as f32 - 1.0) } else { 0.5 };
            let ms = VMIN_MS * (VMAX_MS / VMIN_MS).powf(frac) * scale;
            let d = (ms * 0.001 * self.sr).round() as usize;
            delays[i] = d.clamp(VMIN_DELAY, self.max_delay);
        }
        make_coprime_ish(&mut delays, self.max_delay, VMIN_DELAY);
        delays
    }

    /// Load `size` into FDN `idx` immediately (delay lengths + current decay).
    fn load_fdn(&mut self, idx: usize, size: f32) {
        let delays = self.delays_for(size);
        self.fdn[idx].set_delays(&delays);
        self.fdn[idx].set_rt60(self.decay.max(0.1));
    }

    /// If the requested SIZE has drifted from the audible SIZE and no crossfade is in flight,
    /// load the idle FDN with the new size and begin an equal-power crossfade to it.
    fn maybe_start_size_xfade(&mut self) {
        if self.crossfading || (self.req_size - self.cur_size).abs() <= VSIZE_EPS {
            return;
        }
        let idle = 1 - self.cur;
        self.load_fdn(idle, self.req_size);
        self.idle_size = self.req_size;
        self.crossfading = true;
        self.xf = 0.0;
    }

    pub fn configure(&mut self, size: f32, decay: f32, force: bool) {
        self.req_size = size.clamp(0.0, 1.0);
        self.decay = decay;
        // Decay only recomputes per-line gains (no read-pointer jump) → apply live to both.
        if force || (decay - self.prev_decay).abs() > 1.0e-4 {
            for f in self.fdn.iter_mut() {
                f.set_rt60(decay.max(0.1));
            }
            self.prev_decay = decay;
        }
        if force || !self.primed {
            // Startup / reset: set both FDNs to the size directly, no crossfade.
            let delays = self.delays_for(self.req_size);
            for f in self.fdn.iter_mut() {
                f.set_delays(&delays);
                f.set_rt60(decay.max(0.1));
                // Anti-metallic delay modulation (SOUND-PASS): smears the ghost-verb's
                // discrete FDN modes so the drowned tail is a wash, not a ringing tone.
                f.set_modulation(0.0002 * self.sr, 0.9);
            }
            self.cur_size = self.req_size;
            self.idle_size = self.req_size;
            self.crossfading = false;
            self.xf = 0.0;
            self.primed = true;
            return;
        }
        // Live SIZE move → click-free crossfade.
        self.maybe_start_size_xfade();
    }

    /// Process one stereo pair through the shimmer verb. `shimmer` scales the octave
    /// feedback amount (0 = plain verb).
    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32, shimmer: f32) -> (f32, f32) {
        // Inject the (delayed) shimmer feedback into both FDN inputs.
        let fb = self.shimmer_fb;
        let il = in_l + fb;
        let ir = in_r + fb;
        // Both FDNs always run so the idle one stays pre-warmed for the next crossfade.
        let nxt = 1 - self.cur;
        let (cl, cr) = self.fdn[self.cur].process(il, ir);
        let (nl, nr) = self.fdn[nxt].process(il, ir);

        // Equal-power blend cur → nxt.
        let theta = self.xf.clamp(0.0, 1.0) * FRAC_PI_2;
        let (ca, cb) = (theta.cos(), theta.sin());
        let vl = ca * cl + cb * nl;
        let vr = ca * cr + cb * nr;

        // Advance the crossfade; on completion, swap and re-arm for further SIZE moves.
        if self.crossfading {
            self.xf += self.xf_inc;
            if self.xf >= 1.0 {
                self.xf = 0.0;
                self.crossfading = false;
                self.cur = nxt;
                self.cur_size = self.idle_size;
                // A continuing sweep may already need the next hop.
                self.maybe_start_size_xfade();
            }
        }

        let vmono = 0.5 * (vl + vr);
        // Octave-up shifter on the wet, soft-limited + DC-blocked, scaled by shimmer amount.
        let shifted = self.shifter.process(vmono);
        let makeup = 1.6; // recover PV-shifter loss so the shimmer blooms without runaway
        let lim = (shifted * makeup).tanh();
        self.shimmer_fb = self.dc.process(shimmer.clamp(0.0, 0.95) * lim);
        (vl, vr)
    }
}

// ---------------------------------------------------------------------------
// Wash — LP + wow (fractional-delay pitch drift)
// ---------------------------------------------------------------------------

/// A slow fractional-delay "wow" plus a low-pass, per channel. `amount` 0 = bypass.
struct Wash {
    sr: f32,
    lp: Svf,
    buf: Vec<f32>,
    w: usize,
    lfo_phase: f32,
    base_delay: f32,
}

impl Wash {
    fn new(sr: f32, lfo_offset: f32) -> Self {
        let len = ((WOW_BASE_MS + WOW_DEPTH_MS + 2.0) * 0.001 * sr).ceil() as usize + 4;
        let mut lp = Svf::new();
        lp.set(18_000.0f32.min(sr * 0.45), 0.707, sr);
        Self {
            sr,
            lp,
            buf: vec![0.0; len.max(8)],
            w: 0,
            lfo_phase: lfo_offset,
            base_delay: WOW_BASE_MS * 0.001 * sr,
        }
    }

    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.w = 0;
        self.lp.reset();
    }

    /// `amount` 0..1: darker LP + deeper wow. `rate_hz` = wow LFO rate.
    ///
    /// The write path (LP + wow buffer) runs **always**, even when the wash is bypassed, so
    /// the delay line is continuously filled with fresh material. Re-engaging the wash then
    /// reads recent audio instead of a stale/zeroed buffer — no dropout/click at engage. Only
    /// the read/mix is skipped while bypassed (bypass stays coloration- and delay-free).
    #[inline]
    fn process(&mut self, x: f32, amount: f32, rate_hz: f32) -> f32 {
        let bypass = amount < 1.0e-4;
        // LP cutoff: bright (18 k) → dark (2.5 k) as amount rises. Bright at/near bypass, so
        // the buffered content is continuous across the bypass↔engage boundary.
        let cutoff = (18_000.0 - amount * 15_500.0).clamp(500.0, self.sr * 0.45);
        self.lp.set(cutoff, 0.707, self.sr);
        let low = self.lp.process(x).lp;

        // Wow write path: always fill the buffer and advance the LFO (kept running so it
        // never jumps on re-engage).
        let len = self.buf.len();
        self.buf[self.w] = low;
        let depth = WOW_DEPTH_MS * 0.001 * self.sr * amount;
        let lfo = (std::f32::consts::TAU * self.lfo_phase).sin();
        self.lfo_phase += rate_hz / self.sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        if bypass {
            // Bypassed: buffer stays fresh (above), but emit the dry input with no added
            // delay/coloration.
            self.w = (self.w + 1) % len;
            return x;
        }

        // Read a fractionally-modulated delay from the (freshly-written) buffer.
        let delay = (self.base_delay + depth * lfo).clamp(1.0, len as f32 - 2.0);
        let read = self.w as f32 - delay;
        let read = if read < 0.0 { read + len as f32 } else { read };
        let i0 = read.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = read - read.floor();
        let out = self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac;
        self.w = (self.w + 1) % len;
        out
    }
}

// ---------------------------------------------------------------------------
// Ducker (keyed by the DRY env — wet SWELLS when the vocal pauses)
// ---------------------------------------------------------------------------

/// Inverse sidechain: the wet is attenuated by up to `depth` while the DRY input is active,
/// and swells back to full in the silence after — the "drowned-vocal" trick. Normalised so
/// the effect is level-independent.
struct Ducker {
    env: EnvFollower,
    peak: f32,
    peak_rel: f32,
    sr: f32,
}

impl Ducker {
    fn new(sr: f32) -> Self {
        let mut env = EnvFollower::new(Detector::Peak);
        env.set_times(5.0, 260.0, sr);
        Self {
            env,
            peak: 0.0,
            peak_rel: (-1.0 / (SC_NORM_REL_MS * 0.001 * sr).max(1.0)).exp(),
            sr,
        }
    }

    fn set_release(&mut self, release_ms: f32) {
        self.env.set_times(5.0, release_ms.clamp(20.0, 3000.0), self.sr);
    }

    fn reset(&mut self) {
        self.env.reset();
        self.peak = 0.0;
    }

    /// Feed the DRY key; return the wet gain (≤ 1) — full when the dry is silent.
    #[inline]
    fn process(&mut self, dry_key: f32, depth: f32) -> f32 {
        let e = self.env.process(dry_key);
        self.peak = if e > self.peak {
            e
        } else {
            e + self.peak_rel * (self.peak - e)
        };
        let sc_norm = if self.peak > 1.0e-5 {
            (e / self.peak).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // wet gain: 1 when dry silent (sc_norm 0), (1-depth) when dry fully active.
        1.0 - depth.clamp(0.0, 1.0) * sc_norm
    }
}

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

#[inline]
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Nudge FDN delay lengths mutually prime-ish (kills commensurate flutter).
fn make_coprime_ish(delays: &mut [usize; N], max_delay: usize, min_delay: usize) {
    for i in 0..N {
        if delays[i] % 2 == 0 {
            delays[i] += 1;
        }
        let mut tries = 0;
        while tries < 64 && (0..i).any(|j| delays[i] == delays[j] || gcd(delays[i], delays[j]) > 1) {
            delays[i] += 2;
            if delays[i] > max_delay {
                delays[i] = (min_delay | 1) + i * 2;
            }
            tries += 1;
        }
    }
}

/// Wet-path safety clip: exact identity for |x| ≤ 0.9, tanh-compressed above so |y| < 1.
/// The identity region keeps the `mix=0` (dry) null exact — the dry vocal sits well under
/// the knee — while bounding the shimmer/verb build-up to ≤ 0 dBFS.
const CLIP_KNEE: f32 = 0.9;
#[inline]
fn safety_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= CLIP_KNEE {
        x
    } else {
        let over = (a - CLIP_KNEE) / (1.0 - CLIP_KNEE);
        x.signum() * (CLIP_KNEE + (1.0 - CLIP_KNEE) * over.tanh())
    }
}

/// Per-sample one-pole scalar smoother.
#[derive(Clone, Copy)]
struct Smooth {
    op: OnePole,
    target: f32,
}
impl Smooth {
    fn new(sr: f32, init: f32) -> Self {
        let mut op = OnePole::new();
        op.set_time(SMOOTH_MS, sr);
        op.reset(init);
        Self { op, target: init }
    }
    #[inline]
    fn set(&mut self, t: f32) {
        self.target = t;
    }
    #[inline]
    fn next(&mut self) -> f32 {
        self.op.process(self.target)
    }
    fn reset(&mut self, v: f32) {
        self.op.reset(v);
        self.target = v;
    }
}

// ---------------------------------------------------------------------------
// SeanceCore
// ---------------------------------------------------------------------------

/// SEANCE's full stereo DSP core.
pub struct SeanceCore {
    sr: f32,
    settings: Settings,

    // Main formant-preserving shifters (one per channel).
    shift_l: ShiftEngine,
    shift_r: ShiftEngine,
    // Latency-matched dry path.
    dry_l: DelayLine,
    dry_r: DelayLine,

    chopper: Chopper,
    verb: ShimmerVerb,
    wash_l: Wash,
    wash_r: Wash,
    ducker: Ducker,

    // Smoothed audible scalars.
    sm_verb_wet: Smooth,
    sm_shimmer: Smooth,
    sm_wash: Smooth,
    sm_duck: Smooth,
    sm_mix: Smooth,
    sm_out: Smooth,
}

impl SeanceCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let lat = MAIN_FFT;
        let d = Settings::default();
        let mut dry_l = DelayLine::new(lat + 1);
        let mut dry_r = DelayLine::new(lat + 1);
        // Dry delay must equal the shifter latency for mix alignment.
        dry_l.set_delay(lat);
        dry_r.set_delay(lat);
        Self {
            sr,
            settings: d,
            shift_l: ShiftEngine::new(MAIN_FFT, MAIN_HOP, sr),
            shift_r: ShiftEngine::new(MAIN_FFT, MAIN_HOP, sr),
            dry_l,
            dry_r,
            chopper: Chopper::new(sr),
            verb: ShimmerVerb::new(sr),
            wash_l: Wash::new(sr, 0.0),
            wash_r: Wash::new(sr, 0.37), // decorrelated wow between channels
            ducker: Ducker::new(sr),
            sm_verb_wet: Smooth::new(sr, d.verb_wet),
            sm_shimmer: Smooth::new(sr, d.verb_shimmer),
            sm_wash: Smooth::new(sr, d.wash),
            sm_duck: Smooth::new(sr, d.duck_depth),
            sm_mix: Smooth::new(sr, d.mix),
            sm_out: Smooth::new(sr, d.out_gain),
        }
    }

    /// Reported constant latency in samples (the main shifter FFT size).
    pub fn latency_samples(&self) -> u32 {
        MAIN_FFT as u32
    }

    /// The core's sample rate (Hz).
    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn reset(&mut self) {
        self.shift_l.reset();
        self.shift_r.reset();
        self.dry_l.reset();
        self.dry_r.reset();
        self.chopper.reset();
        self.verb.reset();
        self.wash_l.reset();
        self.wash_r.reset();
        self.ducker.reset();
        self.sm_verb_wet.reset(self.settings.verb_wet);
        self.sm_shimmer.reset(self.settings.verb_shimmer);
        self.sm_wash.reset(self.settings.wash);
        self.sm_duck.reset(self.settings.duck_depth);
        self.sm_mix.reset(self.settings.mix);
        self.sm_out.reset(self.settings.out_gain);
    }

    /// Latch a settings snapshot (block rate).
    pub fn configure(&mut self, s: &Settings) {
        self.settings = *s;
        self.shift_l.set_pitch_ratio(s.pitch_ratio);
        self.shift_l.set_formant_ratio(s.formant_ratio);
        self.shift_l.set_envelope_preserve(s.preserve);
        self.shift_r.set_pitch_ratio(s.pitch_ratio);
        self.shift_r.set_formant_ratio(s.formant_ratio);
        self.shift_r.set_envelope_preserve(s.preserve);
        self.chopper
            .configure(s.tempo_bpm, s.chop_rate, s.chop_pattern, s.chop_depth);
        self.verb.configure(s.verb_size, s.verb_decay, false);
        self.ducker.set_release(s.duck_release_ms);

        self.sm_verb_wet.set(s.verb_wet);
        self.sm_shimmer.set(s.verb_shimmer);
        self.sm_wash.set(s.wash);
        self.sm_duck.set(s.duck_depth);
        self.sm_mix.set(s.mix);
        self.sm_out.set(s.out_gain);
    }

    /// Latch the host transport (block rate) so the chopper can phase-lock its gate to the
    /// playhead. Call after [`configure`](Self::configure) (which sets the chop division).
    /// When the transport is stopped the chopper free-runs. The offline harness convenience
    /// methods do not call this, so they render with a stopped (free-running) transport.
    pub fn set_transport(&mut self, t: &TransportFrame) {
        self.chopper
            .set_transport(t.playing, t.ppq_pos, t.beats_per_sample());
    }

    /// Process one stereo sample pair.
    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let verb_wet = self.sm_verb_wet.next();
        let shimmer = self.sm_shimmer.next();
        let wash_amt = self.sm_wash.next();
        let duck_depth = self.sm_duck.next();
        let mix = self.sm_mix.next();
        let out_g = self.sm_out.next();

        // DRY key (pre-shift) for the ducker + the latency-matched dry path.
        let dry_key = 0.5 * (in_l.abs() + in_r.abs());
        let dry_l = self.dry_l.process(in_l);
        let dry_r = self.dry_r.process(in_r);

        // 1. Formant-preserving pitch/formant shift.
        let mut wl = self.shift_l.process(in_l);
        let mut wr = self.shift_r.process(in_r);

        // 2. Chopper (shared gate).
        let gate = self.chopper.process();
        wl *= gate;
        wr *= gate;

        // 3. Shimmer FDN verb (blended over the dry-of-verb by verb_wet).
        let (vl, vr) = self.verb.process(wl, wr, shimmer);
        wl += verb_wet * vl;
        wr += verb_wet * vr;

        // 4. Wash (LP + wow).
        let rate = 0.45; // slow wow LFO (Hz)
        wl = self.wash_l.process(wl, wash_amt, rate);
        wr = self.wash_r.process(wr, wash_amt, rate * 0.9);

        // 5. Ducker keyed by the DRY env (wet swells on pause).
        let wet_gain = self.ducker.process(dry_key, duck_depth);
        wl *= wet_gain;
        wr *= wet_gain;

        // 6. Mix (linear crossfade; mix=0 → latency-matched dry, exact) + out trim.
        //    Safety-clip each channel so the shimmer/verb build-up stays ≤ 0 dBFS; the knee
        //    is above the dry level so mix=0 still nulls exactly.
        let out_l = safety_clip(out_g * ((1.0 - mix) * dry_l + mix * wl));
        let out_r = safety_clip(out_g * ((1.0 - mix) * dry_r + mix * wr));
        (out_l, out_r)
    }

    /// Offline convenience: process interleaved-by-channel slices in place with fixed settings.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], s: &Settings) {
        self.configure(s);
        let n = left.len().min(right.len());
        for i in 0..n {
            let (l, r) = self.process_sample(left[i], right[i]);
            left[i] = l;
            right[i] = r;
        }
    }

    /// Offline mono convenience: duplicate to stereo, process, return the L channel.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        self.configure(s);
        for x in buf.iter_mut() {
            let (l, _r) = self.process_sample(*x, *x);
            *x = l;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coprime_ish_distinct() {
        let mut d = [200usize; N];
        make_coprime_ish(&mut d, 8000, VMIN_DELAY);
        for i in 0..N {
            for j in 0..i {
                assert_ne!(d[i], d[j]);
            }
        }
    }

    #[test]
    fn mix_zero_equals_latency_delayed_dry() {
        let sr = 48_000.0f32;
        let mut core = SeanceCore::new(sr);
        let s = Settings { mix: 0.0, out_gain: 1.0, ..Settings::default() };
        core.configure(&s);
        core.reset();
        let input = suite_core::testsig::synth_vocal(150.0, (sr * 0.5) as usize, sr);
        let lat = MAIN_FFT;
        let mut max_err = 0.0f32;
        for (i, &x) in input.iter().enumerate() {
            let (l, _r) = core.process_sample(x, x);
            if i >= lat {
                max_err = max_err.max((l - input[i - lat]).abs());
            }
        }
        assert!(max_err < 1.0e-5, "mix=0 not equal to latency-delayed dry: {max_err}");
    }
}
