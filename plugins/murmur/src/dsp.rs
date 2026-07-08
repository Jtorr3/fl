//! MURMUR — pure-DSP core for the stochastic reverb (SPECS "MURMUR"; Hikari clone).
//!
//! ```text
//! in ─┬─ onset detector (band-energy rise, sensitivity) ── trigger ──┐
//!     │                                                              ▼
//!     ├─▶ Fdn8 #A ─┐   on each onset (or manual re-roll): draw a NEW random room
//!     └─▶ Fdn8 #B ─┤   (delays / diffusion / damping-color, deviating from nominal
//!                  │    by `randomness`) into the idle FDN, then 50 ms equal-power
//!                  └──▶ crossfade to it — every hit is a different room, click-free.
//! ```
//!
//! Two [`Fdn8`] instances **ping-pong**. BOTH always process the input, so the idle one is
//! pre-warmed with its own randomly-drawn room and already contains the incoming transient at
//! crossfade time. On an onset (or the manual re-roll button) we equal-power crossfade the
//! output from the current FDN to the pre-warmed idle one over 50 ms; when the crossfade
//! completes we reset the now-idle FDN and draw a fresh random room into it for next time.
//!
//! - **Onset detector**: a fast/slow band-energy-rise detector on a high-passed copy of the
//!   input (a cheap, zero-latency spectral-flux stand-in). `sensitivity` lowers the trigger
//!   ratio; a refractory gap prevents double-triggers.
//! - **Randomness**: at `randomness = 0` every draw is the deterministic *nominal* room
//!   (delays spread across the size range, damping = color, nominal diffusion), so the RT60
//!   is well-defined by `decay` — MURMUR's RT60 done-bar. As `randomness` rises, each draw
//!   deviates further (delay lengths, damping color, diffusion coeff), so two identical hits
//!   land in different rooms and their tails decorrelate — MURMUR's cross-correlation done-bar.
//! - **Freeze**: RT60 → (near-)infinite on both FDNs and the input is ducked into the tail, so
//!   the current wash sustains as an infinite pad.
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the offline
//! harness tests. All buffers are preallocated in [`MurmurCore::new`]; the per-sample path is
//! allocation-free (safe under nih-plug's `assert_process_allocs`).

use suite_core::dsp::{OnePole, Svf};
use suite_core::fdn::{Fdn8, N};
use suite_core::testsig::Rng;

use std::f32::consts::FRAC_PI_2;

// ---- Delay-range / draw constants ----

/// Nominal shortest / longest line length (ms) at `size = 0.5`, before size scaling.
const DMIN_MS: f32 = 15.0;
const DMAX_MS: f32 = 70.0;
/// `size` maps to this multiplicative range on the nominal delay lengths.
const SIZE_MIN: f32 = 0.4;
const SIZE_MAX: f32 = 1.8;
/// Fraction the per-line length may deviate from nominal at `randomness = 1`.
const DEV_RANGE: f32 = 0.4;
/// Nominal input-diffusion coefficient.
const NOMINAL_DIFF: f32 = 0.55;
/// How far the damping-color tilt / diffusion coeff deviate at `randomness = 1`.
const COLOR_DEV: f32 = 0.5;
const DIFF_DEV: f32 = 0.3;
/// Absolute floor on any drawn line length (samples).
const MIN_DELAY: usize = 64;

/// Equal-power crossfade duration between the two FDNs (ms) — click-free room swap.
const CROSSFADE_MS: f32 = 50.0;
/// Minimum time between onset-triggered re-rolls (ms) — refractory gap.
const REFRACTORY_MS: f32 = 120.0;
/// Freeze RT60 target (seconds) — effectively lossless (the FDN caps line gain < 1).
const FREEZE_RT60: f32 = 1.0e5;
/// Per-sample input-duck smoothing when toggling freeze.
const DUCK_MS: f32 = 30.0;

/// Wet-path safety soft-clip knee: identity below this, tanh-compressed above (guarantees
/// |wet| < 1 without touching the mix=0 null, which uses the dry path only).
const CLIP_KNEE: f32 = 0.9;

/// A full snapshot of MURMUR's controls (plain values). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// 0..1 — scales the delay-length range (room size). Takes effect on the next draw.
    pub size: f32,
    /// Reverb time RT60 (seconds). Applied live to both FDNs.
    pub decay: f32,
    /// −1..1 — damping color tilt (bright→dark). Takes effect on the next draw (+ live on change).
    pub color: f32,
    /// 0..1 — how far each draw deviates from the nominal room. 0 = deterministic nominal.
    pub randomness: f32,
    /// 0..1 — onset-detector sensitivity (higher = triggers on smaller rises).
    pub sensitivity: f32,
    /// Freeze: sustain the current tail (RT60 → ∞, input ducked).
    pub freeze: bool,
    /// 0..1 — while Freeze is engaged, blend the output between the live input (0) and the
    /// fully-frozen tail (1). 1.0 = classic hard freeze; lower keeps the live source audible.
    pub freeze_mix: f32,
    /// 0..1 — stereo width of the wet signal (0 = mono, 1 = full).
    pub width: f32,
    /// 0..1 — dry/wet mix.
    pub mix: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            size: 0.5,
            decay: 2.5,
            color: 0.1,
            randomness: 0.4,
            sensitivity: 0.5,
            freeze: false,
            freeze_mix: 1.0,
            width: 1.0,
            mix: 0.35,
        }
    }
}

// ---------------------------------------------------------------------------
// Onset detector (band-energy rise)
// ---------------------------------------------------------------------------

/// Zero-latency onset detector: high-pass the input to emphasise transients, track a fast and
/// a slow envelope, and fire when the fast envelope rises far enough above the slow one (a
/// cheap spectral-flux stand-in). `sensitivity` lowers the required ratio.
struct OnsetDetector {
    hp: Svf,
    fast: f32,
    slow: f32,
    fast_atk: f32,
    fast_rel: f32,
    slow_atk: f32,
    slow_rel: f32,
    refractory: usize,
    since_last: usize,
    ratio_thresh: f32,
}

impl OnsetDetector {
    fn new(sr: f32) -> Self {
        let mut hp = Svf::new();
        hp.set(800.0, 0.707, sr);
        let coef = |ms: f32| (-1.0 / (ms * 0.001 * sr).max(1.0)).exp();
        Self {
            hp,
            fast: 0.0,
            slow: 0.0,
            fast_atk: coef(0.5),
            fast_rel: coef(20.0),
            slow_atk: coef(40.0),
            slow_rel: coef(250.0),
            refractory: (REFRACTORY_MS * 0.001 * sr) as usize,
            since_last: usize::MAX / 2,
            ratio_thresh: 2.0,
        }
    }

    fn set_sensitivity(&mut self, s: f32) {
        // sensitivity 0 → ratio 3.0 (only big transients), 1 → 1.2 (very twitchy).
        self.ratio_thresh = 3.0 + (1.2 - 3.0) * s.clamp(0.0, 1.0);
    }

    fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
        self.since_last = usize::MAX / 2;
        self.hp.reset();
    }

    /// Feed one (mono) sample; returns true on a detected onset.
    #[inline]
    fn process(&mut self, x: f32) -> bool {
        let h = self.hp.process(x).hp.abs();
        let fc = if h > self.fast { self.fast_atk } else { self.fast_rel };
        self.fast = h + fc * (self.fast - h);
        let sc = if h > self.slow { self.slow_atk } else { self.slow_rel };
        self.slow = h + sc * (self.slow - h);

        self.since_last = self.since_last.saturating_add(1);
        let floor = 1.0e-4;
        if self.since_last >= self.refractory
            && self.fast > floor
            && self.fast > self.slow * self.ratio_thresh + floor
        {
            self.since_last = 0;
            true
        } else {
            false
        }
    }
}

/// First-order DC blocker (`y = x − x₋₁ + R·y₋₁`) for the wet output.
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

/// Wet safety clip: exact identity for |x| ≤ [`CLIP_KNEE`]; tanh-compressed above so |y| < 1.
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

// ---------------------------------------------------------------------------
// MurmurCore
// ---------------------------------------------------------------------------

/// MURMUR's full stereo DSP core.
pub struct MurmurCore {
    sr: f32,
    fdn: [Fdn8; 2],
    /// Index of the currently-audible FDN (the crossfade origin).
    cur: usize,
    /// Crossfade position 0→1 toward the idle FDN (`1 − cur`). 0 = not crossfading.
    xf: f32,
    xf_inc: f32,
    crossfading: bool,
    onset: OnsetDetector,
    rng: Rng,
    settings: Settings,
    max_delay: usize,
    pending_reroll: bool,
    /// Smoothed input-duck gain (1 normal, →0 under freeze).
    duck: OnePole,
    /// Smoothed Freeze-Mix (live↔frozen blend, applied only while frozen).
    fm: OnePole,
    /// Smoothed freeze engage/release (0=live path, 1=frozen blend) — driven by the freeze
    /// bool so toggling FREEZE crossfades the blend instead of stepping it in one sample.
    freeze_blend: OnePole,
    prev_color: f32,
    prev_freeze: bool,
    dc_l: DcBlock,
    dc_r: DcBlock,
    /// Draw counter (deterministic — every room comes from a distinct RNG state).
    draws: u64,
}

impl MurmurCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        // Preallocate each line for the longest possible drawn delay at this SR.
        let max_ms = DMAX_MS * SIZE_MAX * (1.0 + DEV_RANGE);
        let max_delay = ((max_ms * 0.001 * sr).ceil() as usize).max(MIN_DELAY + 1);
        let fdn = [Fdn8::new(max_delay, sr), Fdn8::new(max_delay, sr)];
        let mut duck = OnePole::new();
        duck.set_time(DUCK_MS, sr);
        duck.reset(1.0);
        let mut fm = OnePole::new();
        fm.set_time(15.0, sr);
        fm.reset(1.0);
        let mut freeze_blend = OnePole::new();
        freeze_blend.set_time(20.0, sr);
        freeze_blend.reset(0.0);
        let d = Settings::default();
        let mut core = Self {
            sr,
            fdn,
            cur: 0,
            xf: 0.0,
            xf_inc: 1.0 / (CROSSFADE_MS * 0.001 * sr).max(1.0),
            crossfading: false,
            onset: OnsetDetector::new(sr),
            rng: Rng::new(0x4D55_524D), // "MURM"
            settings: d,
            max_delay,
            pending_reroll: false,
            duck,
            fm,
            freeze_blend,
            prev_color: d.color,
            prev_freeze: d.freeze,
            dc_l: DcBlock::default(),
            dc_r: DcBlock::default(),
            draws: 0,
        };
        core.onset.set_sensitivity(d.sensitivity);
        // Draw an initial room into each FDN so both are ready (idle is pre-warmed).
        core.draw_room(0);
        core.draw_room(1);
        core
    }

    /// A reverb is a time-smearing effect: zero reported latency (SPECS / build brief).
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn reset(&mut self) {
        for f in self.fdn.iter_mut() {
            f.reset();
        }
        self.onset.reset();
        self.duck.reset(1.0);
        self.fm.reset(self.settings.freeze_mix);
        self.freeze_blend.reset(if self.settings.freeze { 1.0 } else { 0.0 });
        self.dc_l.reset();
        self.dc_r.reset();
        self.cur = 0;
        self.xf = 0.0;
        self.crossfading = false;
        self.pending_reroll = false;
        self.rng = Rng::new(0x4D55_524D);
        self.draws = 0;
        self.draw_room(0);
        self.draw_room(1);
    }

    /// Request a manual re-roll (rising edge of the GUI button). Consumed at the next sample.
    pub fn request_reroll(&mut self) {
        self.pending_reroll = true;
    }

    /// Latch a settings snapshot (call at block rate). Applies decay/freeze live to both FDNs
    /// and color only when it changes (so per-draw color randomness survives a still knob).
    pub fn configure(&mut self, s: &Settings) {
        self.settings = *s;
        self.onset.set_sensitivity(s.sensitivity);

        let rt60 = if s.freeze { FREEZE_RT60 } else { s.decay };
        // Anti-metallic delay modulation (SOUND-PASS): ~0.2 ms of slow per-line wobble
        // smears the FDN's discrete tail modes into a dense diffuse band. Inaudible as
        // vibrato; turns the ringing "tin-can" tail into a smooth cathedral wash.
        let mod_depth = 0.0002 * self.sr;
        for f in self.fdn.iter_mut() {
            f.set_rt60(rt60);
            f.set_modulation(mod_depth, 0.8);
        }
        // Color: apply live to both only when the knob actually moves.
        if (s.color - self.prev_color).abs() > 1.0e-4 {
            for f in self.fdn.iter_mut() {
                f.set_damping(s.color);
            }
            self.prev_color = s.color;
        }
        // Input-duck target: 0 under freeze (let the tail sustain), 1 otherwise.
        self.duck.set_time(DUCK_MS, self.sr);
        self.fm.set_time(15.0, self.sr);
        self.freeze_blend.set_time(20.0, self.sr);
        self.prev_freeze = s.freeze;
    }

    /// Base (nominal) length of line `i` in samples for the current `size`.
    fn base_delay_samples(&self, i: usize, size: f32) -> f32 {
        let scale = SIZE_MIN + (SIZE_MAX - SIZE_MIN) * size.clamp(0.0, 1.0);
        let frac = if N > 1 { i as f32 / (N as f32 - 1.0) } else { 0.5 };
        let ms = DMIN_MS * (DMAX_MS / DMIN_MS).powf(frac) * scale;
        ms * 0.001 * self.sr
    }

    /// Draw a new random room into FDN `idx`: delay lengths (deviating from nominal by
    /// `randomness`, nudged mutually-prime-ish), damping color, and diffusion coeff.
    fn draw_room(&mut self, idx: usize) {
        let s = self.settings;
        let r = s.randomness.clamp(0.0, 1.0);

        let mut delays = [0usize; N];
        for i in 0..N {
            let nominal = self.base_delay_samples(i, s.size);
            let dev = if r > 0.0 {
                r * DEV_RANGE * self.rng.next_bipolar()
            } else {
                0.0
            };
            let d = (nominal * (1.0 + dev)).round() as isize;
            delays[i] = d.clamp(MIN_DELAY as isize, self.max_delay as isize) as usize;
        }
        make_coprime_ish(&mut delays, self.max_delay);
        self.fdn[idx].set_delays(&delays);

        self.fdn[idx].set_rt60(if s.freeze { FREEZE_RT60 } else { s.decay });

        let color = if r > 0.0 {
            s.color + r * COLOR_DEV * self.rng.next_bipolar()
        } else {
            s.color
        };
        self.fdn[idx].set_damping(color.clamp(-1.0, 1.0));

        let diff = if r > 0.0 {
            NOMINAL_DIFF + r * DIFF_DEV * self.rng.next_bipolar()
        } else {
            NOMINAL_DIFF
        };
        self.fdn[idx].set_diffusion(diff.clamp(0.0, 0.9));

        self.draws += 1;
    }

    /// Begin a crossfade to the idle FDN (which is already pre-warmed with a fresh room).
    fn start_crossfade(&mut self) {
        if !self.crossfading {
            self.crossfading = true;
            self.xf = 0.0;
        }
    }

    /// Process one stereo sample. `mix` is passed per-sample so it can be smoothed by the host.
    #[inline]
    pub fn process_sample(&mut self, l: f32, r: f32, mix: f32) -> (f32, f32) {
        let mono = 0.5 * (l + r);
        let onset = self.onset.process(mono);
        let reroll = std::mem::take(&mut self.pending_reroll);
        if (onset || reroll) && !self.crossfading && !self.settings.freeze {
            self.start_crossfade();
        }

        // Input duck (→0 under freeze so the frozen tail is not overwritten).
        let duck_target = if self.settings.freeze { 0.0 } else { 1.0 };
        let g_in = self.duck.process(duck_target);
        let (il, ir) = (l * g_in, r * g_in);

        // Both FDNs always process the input (the idle one stays pre-warmed).
        let nxt = 1 - self.cur;
        let (cur_l, cur_r) = self.fdn[self.cur].process(il, ir);
        let (nxt_l, nxt_r) = self.fdn[nxt].process(il, ir);

        // Equal-power blend cur → nxt.
        let theta = self.xf.clamp(0.0, 1.0) * FRAC_PI_2;
        let (ca, cb) = (theta.cos(), theta.sin());
        let mut wet_l = ca * cur_l + cb * nxt_l;
        let mut wet_r = ca * cur_r + cb * nxt_r;

        // Advance the crossfade; on completion, swap and re-draw the now-idle FDN.
        if self.crossfading {
            self.xf += self.xf_inc;
            if self.xf >= 1.0 {
                self.xf = 0.0;
                self.crossfading = false;
                let old = self.cur;
                self.cur = nxt;
                // The old FDN is now idle (silent in the blend): reset + load a fresh room.
                self.fdn[old].reset();
                self.draw_room(old);
            }
        }

        // Stereo width (mid/side) on the wet signal.
        let w = self.settings.width.clamp(0.0, 1.0);
        let mid = 0.5 * (wet_l + wet_r);
        let side = 0.5 * (wet_l - wet_r) * w;
        wet_l = mid + side;
        wet_r = mid - side;

        // DC block + safety clip (wet only — mix=0 uses the dry path, untouched).
        wet_l = safety_clip(self.dc_l.process(wet_l));
        wet_r = safety_clip(self.dc_r.process(wet_r));

        let m = mix.clamp(0.0, 1.0);
        let out_l = l + m * (wet_l - l);
        let out_r = r + m * (wet_r - r);

        // Freeze Mix: while frozen, crossfade the (fully-frozen) output back toward the live
        // input so the freeze isn't an all-or-nothing jump. fm=1 → classic hard freeze; fm<1
        // keeps the live source audible under the frozen tail. (Always smoothed; when not
        // frozen the smoother still tracks the target but the branch below is inactive.)
        let fm = self.fm.process(self.settings.freeze_mix.clamp(0.0, 1.0));
        // Smoothed engage/release: crossfade the live path ↔ the frozen blend over ~20 ms so
        // toggling FREEZE (in particular releasing it with fm<1) doesn't step the output in
        // one sample. fz=1 → fully frozen blend, fz=0 → live path. At fm=1 the two paths are
        // identical so classic hard-freeze behaviour is unchanged.
        let fz = self
            .freeze_blend
            .process(if self.settings.freeze { 1.0 } else { 0.0 });
        let frozen_l = fm * out_l + (1.0 - fm) * l;
        let frozen_r = fm * out_r + (1.0 - fm) * r;
        (out_l + fz * (frozen_l - out_l), out_r + fz * (frozen_r - out_r))
    }

    /// Offline mono convenience for the harness: process `buf` in place with a fixed `Settings`.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        self.configure(s);
        for x in buf.iter_mut() {
            let (y, _) = self.process_sample(*x, *x, s.mix);
            *x = y;
        }
    }

    /// Offline stereo convenience: process interleaved-by-channel slices in place.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], s: &Settings) {
        self.configure(s);
        let n = left.len().min(right.len());
        for i in 0..n {
            let (yl, yr) = self.process_sample(left[i], right[i], s.mix);
            left[i] = yl;
            right[i] = yr;
        }
    }

    /// Number of rooms drawn so far (test/diagnostic hook).
    pub fn draws(&self) -> u64 {
        self.draws
    }
}

/// Greatest common divisor (Euclid).
#[inline]
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Nudge the eight delay lengths to be mutually prime-ish (no shared factors, all distinct):
/// force each odd, then bump by 2 (bounded) until it is coprime with every earlier line.
/// Avoids the resonant flutter of commensurate delay lengths. Deterministic, allocation-free.
fn make_coprime_ish(delays: &mut [usize; N], max_delay: usize) {
    for i in 0..N {
        if delays[i] % 2 == 0 {
            delays[i] += 1;
        }
        let mut tries = 0;
        while tries < 64
            && (0..i).any(|j| delays[i] == delays[j] || gcd(delays[i], delays[j]) > 1)
        {
            delays[i] += 2;
            if delays[i] > max_delay {
                // Wrap back down (still odd) rather than exceed the allocation.
                delays[i] = (MIN_DELAY | 1) + (i * 2);
            }
            tries += 1;
        }
    }
}
