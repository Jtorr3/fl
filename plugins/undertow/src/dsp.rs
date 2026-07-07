//! UNDERTOW — pure-DSP core for the kick-to-rumble generator (SPECS "UNDERTOW";
//! taste-tailored for hard/melodic techno low-end). Sits ON the kick track.
//!
//! ```text
//! in(kick) ─┬───────────────────────────────────────────────────────── dry ── + ── out
//!           └ transient strip (env-gated: strip the click, keep the body)
//!             → saturation (suite waveshaper bank, 2× oversampled)
//!             → Fdn8 small/dark (short delays, dark damping, short RT60; sum → mono)
//!             → LP 90–250 Hz (SVF, resonance) → resonant tune peak (key-lockable bell
//!               at note C0..B2 fundamental, amount)
//!             → ducker (keyed by the DRY kick envelope, att ~1 ms, rel 80–300 ms, depth)
//!             → rumble gain → (+ dry)
//! ```
//!
//! The rumble is **mono below ~150 Hz** (techno low-end stays mono); the `width` control
//! only spreads the FDN's stereo side-content **above** 150 Hz. Zero reported latency (the
//! wet path is a reverb — a time-smearing effect, not fixed processing latency); the null
//! convention is therefore "rumble muted → output == dry" rather than a lag-0 coherent peak.
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the
//! offline harness tests. Every buffer is preallocated in [`UndertowCore::new`]; the
//! per-sample path is allocation-free (safe under nih-plug's `assert_process_allocs`).

use suite_core::dsp::{Detector, EnvFollower, OnePole, Oversampler2x, Shaper, Svf};
use suite_core::fdn::{Fdn8, N};

// ---- FDN small/dark preset constants (STATUS.md MURMUR reuse tips) ----

/// Nominal shortest / longest FDN line length (ms) at `size = 0.5` before size scaling.
/// "small/dark" ⇒ short delays (8–25 ms) for a tight, dense low-end tail.
const DMIN_MS: f32 = 8.0;
const DMAX_MS: f32 = 25.0;
/// `size` maps to this multiplicative range on the nominal delay lengths.
const SIZE_MIN: f32 = 0.6;
const SIZE_MAX: f32 = 1.7;
/// Dark damping tilt (+ = dark) for the rumble tail.
const DAMP_TILT: f32 = 0.6;
/// Input-diffusion coefficient (dense, smeared onset).
const DIFFUSION: f32 = 0.6;
/// Absolute floor on any drawn line length (samples).
const MIN_DELAY: usize = 32;

/// Crossover (Hz) below which the rumble is forced mono (techno low-end stays mono);
/// `width` only spreads content above this.
const MONO_XOVER_HZ: f32 = 150.0;

/// Maximum ducking depth (dB) at `duck_depth = 1`.
const MAX_DUCK_DB: f32 = 24.0;
/// Peak-tracker release (ms) that normalises the sidechain so `sc_norm → 1` at each onset
/// regardless of kick amplitude (instant attack, slow release).
const SC_NORM_REL_MS: f32 = 1500.0;

/// Wet-path safety soft-clip knee: identity below this, tanh-compressed above (guarantees
/// |rumble| < 1 without touching the "rumble-muted → dry" null, which uses the dry path only).
const CLIP_KNEE: f32 = 0.9;

/// Smoothing time (ms) for the audible scalar params inside the core.
const SMOOTH_MS: f32 = 12.0;

// ---------------------------------------------------------------------------
// Settings snapshot
// ---------------------------------------------------------------------------

/// A full snapshot of UNDERTOW's controls (plain values). Cheap to copy. Gains are already
/// linear (the plugin converts dB → linear before snapshotting so the DSP stays API-agnostic).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// 0..1 — transient-strip amount (how hard the attack/click is attenuated).
    pub strip: f32,
    /// 0..1 — saturation drive into the waveshaper (pre-gain).
    pub drive: f32,
    /// 0..1 — rumble size (scales the FDN delay lengths).
    pub size: f32,
    /// FDN reverb time RT60 (seconds).
    pub decay: f32,
    /// Low-pass cutoff (Hz), ~90..250.
    pub lp_cutoff: f32,
    /// Low-pass resonance (Q).
    pub lp_res: f32,
    /// Resonant tune-peak fundamental (Hz), from the note param (C0..B2). Key-lockable.
    pub tune_hz: f32,
    /// 0..1 — resonant tune-peak amount.
    pub tune_amount: f32,
    /// 0..1 — ducking depth (→ dB reduction at each kick onset).
    pub duck_depth: f32,
    /// Ducker release (ms), ~80..300.
    pub duck_release_ms: f32,
    /// Linear rumble output gain.
    pub rumble_gain: f32,
    /// 0..1 — stereo width of the rumble above 150 Hz (0 = mono).
    pub width: f32,
    /// Linear dry gain (default 1.0 = unity; the plugin sits ON the kick track).
    pub dry_gain: f32,
    /// Linear output trim.
    pub out_gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strip: 0.5,
            drive: 0.35,
            size: 0.5,
            decay: 0.8,
            lp_cutoff: 140.0,
            lp_res: 1.2,
            tune_hz: 55.0, // A1
            tune_amount: 0.0,
            duck_depth: 0.5,
            duck_release_ms: 160.0,
            rumble_gain: db_to_gain(-2.0),
            width: 0.3,
            dry_gain: 1.0,
            out_gain: 1.0,
        }
    }
}

/// dB → linear amplitude (kept local so the DSP core has no nih-plug dependency).
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Transient strip
// ---------------------------------------------------------------------------

/// Env-gated transient strip: a fast and a slow peak envelope; when the fast envelope leads
/// the slow one (the attack/click region) the signal is attenuated by up to `strip`, so the
/// rumble source is the kick's **body**, not its click. In the sustained body the two
/// envelopes converge and the gate opens back to unity.
struct TransientStrip {
    fast: EnvFollower,
    slow: EnvFollower,
}

impl TransientStrip {
    fn new(sr: f32) -> Self {
        let mut fast = EnvFollower::new(Detector::Peak);
        fast.set_times(0.3, 8.0, sr);
        let mut slow = EnvFollower::new(Detector::Peak);
        slow.set_times(18.0, 70.0, sr);
        Self { fast, slow }
    }

    fn reset(&mut self) {
        self.fast.reset();
        self.slow.reset();
    }

    #[inline]
    fn process(&mut self, x: f32, strip: f32) -> f32 {
        let f = self.fast.process(x);
        let s = self.slow.process(x);
        // Transient-ness: 0 in the body (f≈s), →1 during the attack (f≫s).
        let trans = ((f - s) / (f + 1.0e-4)).clamp(0.0, 1.0);
        let g = 1.0 - strip.clamp(0.0, 1.0) * trans;
        x * g
    }
}

// ---------------------------------------------------------------------------
// Ducker (keyed by the DRY kick envelope)
// ---------------------------------------------------------------------------

/// Sidechain ducker: a fast-attack peak follower on the DRY kick, normalised by a slow
/// peak-tracker so the reduction reaches full `depth` at each onset regardless of kick
/// level, then recovers over `release`. The rumble "breathes around" the kick.
struct Ducker {
    env: EnvFollower,
    /// Adaptive peak tracker (instant attack, slow release) → normaliser.
    peak: f32,
    peak_rel: f32,
    sr: f32,
}

impl Ducker {
    fn new(sr: f32) -> Self {
        let mut env = EnvFollower::new(Detector::Peak);
        env.set_times(1.0, 160.0, sr);
        Self {
            env,
            peak: 0.0,
            peak_rel: (-1.0 / (SC_NORM_REL_MS * 0.001 * sr).max(1.0)).exp(),
            sr,
        }
    }

    fn set_release(&mut self, release_ms: f32) {
        self.env.set_times(1.0, release_ms.clamp(5.0, 2000.0), self.sr);
    }

    fn reset(&mut self) {
        self.env.reset();
        self.peak = 0.0;
    }

    /// Feed the DRY sidechain sample; returns the current duck gain (≤ 1) for `depth_db`.
    #[inline]
    fn process(&mut self, sidechain: f32, depth_db: f32) -> f32 {
        let e = self.env.process(sidechain);
        // Instant-attack / slow-release peak tracker.
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
        let reduction_db = depth_db * sc_norm;
        db_to_gain(-reduction_db)
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// First-order DC blocker (`y = x − x₋₁ + R·y₋₁`) for the rumble path.
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

/// Nudge the eight FDN delay lengths mutually prime-ish (kills commensurate-delay flutter).
fn make_coprime_ish(delays: &mut [usize; N], max_delay: usize) {
    for i in 0..N {
        if delays[i] % 2 == 0 {
            delays[i] += 1;
        }
        let mut tries = 0;
        while tries < 64 && (0..i).any(|j| delays[i] == delays[j] || gcd(delays[i], delays[j]) > 1)
        {
            delays[i] += 2;
            if delays[i] > max_delay {
                delays[i] = (MIN_DELAY | 1) + (i * 2);
            }
            tries += 1;
        }
    }
}

/// A per-sample one-pole scalar smoother wrapper (audible-param de-zippering).
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
// UndertowCore
// ---------------------------------------------------------------------------

/// UNDERTOW's full stereo DSP core.
pub struct UndertowCore {
    sr: f32,
    settings: Settings,

    strip: TransientStrip,
    os: Oversampler2x,
    fdn: Fdn8,
    dc: DcBlock,
    lp: Svf,
    tune: Svf,
    /// Steep (cascaded) high-pass for the width side-signal (keeps < 150 Hz mono).
    side_hp: [Svf; 2],
    ducker: Ducker,

    max_delay: usize,
    /// Change-detection cache so the FDN is only reconfigured when structure actually moves.
    prev_size: f32,
    prev_decay: f32,

    // Smoothed audible scalars.
    sm_strip: Smooth,
    sm_drive: Smooth,
    sm_tune_amt: Smooth,
    sm_depth: Smooth,
    sm_rumble: Smooth,
    sm_width: Smooth,
    sm_dry: Smooth,
    sm_out: Smooth,
}

impl UndertowCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_ms = DMAX_MS * SIZE_MAX * 1.1;
        let max_delay = ((max_ms * 0.001 * sr).ceil() as usize).max(MIN_DELAY + 1);

        let d = Settings::default();
        let mut side_hp = [Svf::new(), Svf::new()];
        for f in side_hp.iter_mut() {
            f.set(MONO_XOVER_HZ, 0.707, sr);
        }
        let mut lp = Svf::new();
        lp.set(d.lp_cutoff, d.lp_res, sr);
        let mut tune = Svf::new();
        tune.set(d.tune_hz, 10.0, sr);

        let mut core = Self {
            sr,
            settings: d,
            strip: TransientStrip::new(sr),
            os: Oversampler2x::new(),
            fdn: Fdn8::new(max_delay, sr),
            dc: DcBlock::default(),
            lp,
            tune,
            side_hp,
            ducker: Ducker::new(sr),
            max_delay,
            prev_size: -1.0,
            prev_decay: -1.0,
            sm_strip: Smooth::new(sr, d.strip),
            sm_drive: Smooth::new(sr, d.drive),
            sm_tune_amt: Smooth::new(sr, d.tune_amount),
            sm_depth: Smooth::new(sr, d.duck_depth),
            sm_rumble: Smooth::new(sr, d.rumble_gain),
            sm_width: Smooth::new(sr, d.width),
            sm_dry: Smooth::new(sr, d.dry_gain),
            sm_out: Smooth::new(sr, d.out_gain),
        };
        core.fdn.set_damping(DAMP_TILT);
        core.fdn.set_diffusion(DIFFUSION);
        core.configure_fdn(true);
        core
    }

    /// The wet path is a reverb (time-smearing) ⇒ zero reported latency (SPECS / build brief).
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn reset(&mut self) {
        self.strip.reset();
        self.os.reset();
        self.fdn.reset();
        self.dc.reset();
        self.lp.reset();
        self.tune.reset();
        for f in self.side_hp.iter_mut() {
            f.reset();
        }
        self.ducker.reset();
        self.sm_strip.reset(self.settings.strip);
        self.sm_drive.reset(self.settings.drive);
        self.sm_tune_amt.reset(self.settings.tune_amount);
        self.sm_depth.reset(self.settings.duck_depth);
        self.sm_rumble.reset(self.settings.rumble_gain);
        self.sm_width.reset(self.settings.width);
        self.sm_dry.reset(self.settings.dry_gain);
        self.sm_out.reset(self.settings.out_gain);
    }

    /// FDN delay lengths from the current size (geometric spread over 8–25 ms × size scale).
    fn configure_fdn(&mut self, force: bool) {
        let s = self.settings;
        if !force && (s.size - self.prev_size).abs() < 1.0e-4 && (s.decay - self.prev_decay).abs() < 1.0e-4
        {
            return;
        }
        let scale = SIZE_MIN + (SIZE_MAX - SIZE_MIN) * s.size.clamp(0.0, 1.0);
        let mut delays = [0usize; N];
        for i in 0..N {
            let frac = if N > 1 { i as f32 / (N as f32 - 1.0) } else { 0.5 };
            let ms = DMIN_MS * (DMAX_MS / DMIN_MS).powf(frac) * scale;
            let d = (ms * 0.001 * self.sr).round() as usize;
            delays[i] = d.clamp(MIN_DELAY, self.max_delay);
        }
        make_coprime_ish(&mut delays, self.max_delay);
        self.fdn.set_delays(&delays);
        self.fdn.set_rt60(s.decay.max(0.05));
        self.prev_size = s.size;
        self.prev_decay = s.decay;
    }

    /// Latch a settings snapshot (call at block rate). Updates filter coeffs, the FDN structure
    /// on change, the ducker release, and the smoothed-scalar targets.
    pub fn configure(&mut self, s: &Settings) {
        self.settings = *s;
        self.configure_fdn(false);
        self.lp.set(s.lp_cutoff.clamp(20.0, self.sr * 0.45), s.lp_res.max(0.4), self.sr);
        self.tune.set(s.tune_hz.clamp(10.0, self.sr * 0.45), 10.0, self.sr);
        self.ducker.set_release(s.duck_release_ms);

        self.sm_strip.set(s.strip);
        self.sm_drive.set(s.drive);
        self.sm_tune_amt.set(s.tune_amount);
        self.sm_depth.set(s.duck_depth);
        self.sm_rumble.set(s.rumble_gain);
        self.sm_width.set(s.width);
        self.sm_dry.set(s.dry_gain);
        self.sm_out.set(s.out_gain);
    }

    /// Process one stereo sample pair.
    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let strip_a = self.sm_strip.next();
        let drive = self.sm_drive.next();
        let tune_amt = self.sm_tune_amt.next();
        let depth = self.sm_depth.next();
        let rumble_g = self.sm_rumble.next();
        let width = self.sm_width.next();
        let dry_g = self.sm_dry.next();
        let out_g = self.sm_out.next();

        // Mono source for the rumble chain + the DRY sidechain key (pre-strip).
        let mono = 0.5 * (in_l + in_r);

        // Ducker keyed by the DRY kick envelope.
        let depth_db = depth * MAX_DUCK_DB;
        let duck = self.ducker.process(mono, depth_db);

        // Transient strip → saturation (suite waveshaper bank, 2× oversampled) → FDN.
        let stripped = self.strip.process(mono, strip_a);
        let pre = 1.0 + drive * 9.0;
        let sat = self.os.process(stripped, |v| Shaper::TubeTanh.apply(v, pre));

        let (fl, fr) = self.fdn.process(sat, sat);
        // Sum the FDN pair toward mono for the sub; keep the side for width (> 150 Hz only).
        let fmono = self.dc.process(0.5 * (fl + fr));
        let fside = 0.5 * (fl - fr);

        // LP (90–250 Hz, resonant) → resonant tune peak (key-lockable bell).
        let low = self.lp.process(fmono).lp;
        let bp = self.tune.process(low).bp;
        let tuned = low + tune_amt * 4.0 * bp;

        // Ducked, gained mono sub (this is the mono low-end — always summed to both channels).
        let sub = tuned * duck * rumble_g;

        // Width side-content: steeply high-passed at 150 Hz so the sub stays mono.
        let mut sh = fside;
        for f in self.side_hp.iter_mut() {
            sh = f.process(sh).hp;
        }
        let side = sh * duck * rumble_g * width;

        // Safety-clip each output channel's rumble so |rumble| < 1 (identity below the knee, so
        // "rumble muted → dry" nulls exactly). Below 150 Hz side ≈ 0 ⇒ both channels ≈ clip(sub),
        // keeping the low-end mono.
        let rumble_l = safety_clip(sub + side);
        let rumble_r = safety_clip(sub - side);

        // Sum with the (unaffected) dry kick and apply the output trim.
        let out_l = out_g * (dry_g * in_l + rumble_l);
        let out_r = out_g * (dry_g * in_r + rumble_r);
        (out_l, out_r)
    }

    /// Offline mono convenience for the harness: process `buf` in place with fixed `Settings`.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        self.configure(s);
        for x in buf.iter_mut() {
            let (l, r) = self.process_sample(*x, *x);
            *x = 0.5 * (l + r);
        }
    }

    /// Offline stereo convenience: process interleaved-by-channel slices in place.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], s: &Settings) {
        self.configure(s);
        let n = left.len().min(right.len());
        for i in 0..n {
            let (l, r) = self.process_sample(left[i], right[i]);
            left[i] = l;
            right[i] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coprime_ish_makes_distinct_delays() {
        let mut d = [100usize; N];
        make_coprime_ish(&mut d, 4000);
        for i in 0..N {
            for j in 0..i {
                assert_ne!(d[i], d[j], "delays {i},{j} collided");
            }
        }
    }

    #[test]
    fn rumble_muted_output_equals_dry() {
        // rumble_gain = 0, width = 0, dry/out unity ⇒ output must equal the dry input exactly.
        let sr = 48_000.0f32;
        let mut core = UndertowCore::new(sr);
        let s = Settings {
            rumble_gain: 0.0,
            width: 0.0,
            dry_gain: 1.0,
            out_gain: 1.0,
            ..Settings::default()
        };
        core.configure(&s);
        // Snap the internal smoothers to their targets (rumble_gain = 0) so the wet path is
        // exactly silent — the "rumble muted → dry" null must be exact, not asymptotic.
        core.reset();
        let kick = suite_core::testsig::synth_kick_stub((sr * 0.3) as usize, sr);
        let mut max_err = 0.0f32;
        for &x in &kick {
            let (l, r) = core.process_sample(x, x);
            max_err = max_err.max((l - x).abs()).max((r - x).abs());
        }
        assert!(max_err < 1.0e-6, "rumble-muted output not equal to dry: err {max_err}");
    }
}
