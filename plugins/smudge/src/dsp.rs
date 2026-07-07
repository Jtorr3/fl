//! SMUDGE — pure-DSP core for the spectral-chaos processor (SPECS "SMUDGE").
//!
//! ```text
//! in ─ STFT(2048, hop 512, Hann) ─ per-frame ops (fixed order 1→4) ─ iSTFT/OLA ─ mix ─ out
//!        1 scramble → 2 spectral delay → 3 blur → 4 smear/stretch
//!        chaos macro = slow S&H (Rng) modulating op params every `chaos_rate` frames
//! ```
//!
//! Four independent per-frame spectral ops, applied in the FIXED order 1→4 (documented in
//! SPECS + docs/SMUDGE.md). **Each op has its own amount param and is EXACTLY bypassed when
//! its amount is 0** (the op is skipped, the spectrum is untouched), so with all four amounts
//! at 0 the wet path is the STFT's own identity reconstruction — it nulls against the
//! latency-delayed dry below −60 dB (SMUDGE done-bar 1).
//!
//! 1. **Scramble** — permute bins inside contiguous neighbourhoods of width `2N+1` (N scales
//!    with the scramble range param). The permutation is redrawn every `scramble_rate` frames
//!    (per-frame = chaos; every 4–8 frames = musical). A permutation is energy-preserving, so
//!    scramble>0 keeps total energy within ±3 dB while decorrelating the spectrum (done-bar 2).
//! 2. **Spectral delay** — bins grouped into ~1/3-octave bands; each band has its own
//!    frame-delay (0..32 frames from a tilt curve — lows-short/highs-long or inverse) with
//!    soft-limited bounded feedback. Echo is ADDED: `out = cur + amount·delayed`.
//! 3. **Blur** — per-bin temporal magnitude one-pole (τ per band, blur tilt); phase is
//!    interpolated toward a phase-vocoder advance as blur dominates (EMBER's approach).
//! 4. **Smear/stretch** — bin-index remap ×0.5–2 (linear interp between source bins,
//!    energy-normalised), crossfaded in by the stretch amount.
//!
//! The **chaos macro** holds a set of sample-and-hold random values (redrawn every
//! `chaos_rate` frames from `suite_core::testsig::Rng`) that modulate the op parameters by
//! `chaos_depth`. Amount modulation is MULTIPLICATIVE (a random gain ≤ 1), so a base amount of
//! 0 stays exactly 0 under any chaos setting — the exact-bypass guarantee survives chaos.
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the offline
//! harness tests. All scratch is preallocated — the per-sample path is allocation-free (safe
//! under nih-plug's `assert_process_allocs`).

use std::f32::consts::{PI, TAU};
use suite_core::dsp::OnePole;
use suite_core::stft::{Complex, Stft};

pub const FFT_SIZE: usize = 2048;
pub const HOP: usize = 512;
/// ~1/3-octave bands spanning 20 Hz .. 20 kHz (≈10 octaves × 3).
pub const N_BANDS: usize = 30;
pub const F_LO: f32 = 20.0;
pub const F_HI: f32 = 20_000.0;

/// Max scramble neighbourhood half-width in bins (range param 1.0 → N = MAX_NEIGH).
pub const MAX_NEIGH: usize = 48;
/// Max spectral-delay depth in STFT frames.
pub const MAX_DELAY_FRAMES: usize = 32;

pub const MIN_STRETCH: f32 = 0.5;
pub const MAX_STRETCH: f32 = 2.0;
pub const MIN_BLUR_TAU_MS: f32 = 5.0;
pub const MAX_BLUR_TAU_MS: f32 = 2000.0;
pub const MAX_DELAY_FEEDBACK: f32 = 0.95;

/// An op is "active" (not exactly bypassed) once its smoothed amount exceeds this.
const AMOUNT_EPS: f32 = 1.0e-5;
/// Per-bin magnitude ceiling for the delay feedback soft-limiter (tanh knee). ≈2× a
/// full-scale sine's peak-bin magnitude (FFT_SIZE/4 = 512), so a runaway resonant bin is
/// bounded while normal bins pass linearly.
const DELAY_CLIP: f32 = 1024.0;
/// Frame-rate smoothing time (ms) for the audible op params (amounts, tilt, factor).
const PARAM_SMOOTH_MS: f32 = 40.0;
/// Makeup applied to the permuted (scrambled) content: a bin permutation decorrelates
/// overlapping WOLA frames, so their overlap-add sums in POWER (×√overlap amplitude) rather
/// than coherently (×overlap) — a fixed −6 dB (√(fft/hop)=2) loss that this restores.
const SCRAMBLE_MAKEUP: f32 = 2.0;
/// Per-frame release factor for the spectral-delay output-energy envelope (≈1 s tail at the
/// 512-hop frame rate). Bounds the feedback tail without killing it.
const DELAY_ENV_RELEASE: f32 = 0.992;
/// Energy headroom on the released delay ceiling: the feedback tail's energy is held below
/// this fraction of the input envelope for musical taming (the hard ≤ 0 dBFS guarantee is the
/// wet-path safety clip below).
const DELAY_ENV_HEADROOM: f32 = 0.7;
/// Wet-path safety soft-clip knee: below this the clip is EXACT identity (so the all-amounts-0
/// STFT-identity null and mix=0 null are untouched — normal reconstruction stays well under
/// it); above it, overshoot is tanh-compressed into [KNEE, 1), guaranteeing |wet| < 1.
const CLIP_KNEE: f32 = 0.9;

/// A full snapshot of SMUDGE's controls (plain, un-normalized values). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    // ---- op 1: scramble ----
    /// 0..1 — crossfade amount toward the permuted spectrum. 0 = exact bypass.
    pub scramble_amt: f32,
    /// 0..1 — neighbourhood size; N = round(range · MAX_NEIGH) bins.
    pub scramble_range: f32,
    /// Frames between permutation redraws (≥1; 1 = per-frame chaos, 4–8 = musical).
    pub scramble_rate: u32,

    // ---- op 2: spectral delay ----
    /// 0..1 — wet echo level (`out = cur + amount·delayed`). 0 = exact bypass.
    pub delay_amt: f32,
    /// −1..1 — per-band delay tilt. +1 lows-short/highs-long, −1 inverse, 0 flat.
    pub delay_tilt: f32,
    /// 0..MAX_DELAY_FEEDBACK — in-loop feedback (soft-limited, always decays).
    pub delay_feedback: f32,

    // ---- op 3: blur ----
    /// 0..1 — blend toward the temporally-averaged magnitude. 0 = exact bypass.
    pub blur_amt: f32,
    /// Base blur time constant (ms), MIN..MAX_BLUR_TAU_MS.
    pub blur_tau_ms: f32,
    /// −1..1 — τ tilt across frequency (+1 = highs smoothed more).
    pub blur_tilt: f32,

    // ---- op 4: smear / stretch ----
    /// 0..1 — crossfade amount toward the remapped spectrum. 0 = exact bypass.
    pub stretch_amt: f32,
    /// MIN..MAX_STRETCH — bin-index remap factor (source bin = k / factor).
    pub stretch_factor: f32,

    // ---- chaos macro ----
    /// Frames between sample-and-hold redraws (≥1).
    pub chaos_rate: u32,
    /// 0..1 — chaos modulation depth.
    pub chaos_depth: f32,

    /// Dry/wet mix, 0..1.
    pub mix: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scramble_amt: 0.0,
            scramble_range: 0.5,
            scramble_rate: 4,
            delay_amt: 0.0,
            delay_tilt: 0.0,
            delay_feedback: 0.3,
            blur_amt: 0.0,
            blur_tau_ms: 200.0,
            blur_tilt: 0.0,
            stretch_amt: 0.0,
            stretch_factor: 1.0,
            chaos_rate: 64,
            chaos_depth: 0.0,
            mix: 1.0,
        }
    }
}

/// Wrap a phase to (−π, π].
#[inline]
fn wrap(mut p: f32) -> f32 {
    while p > PI {
        p -= TAU;
    }
    while p < -PI {
        p += TAU;
    }
    p
}

/// Linear interpolation between two complex bins.
#[inline]
fn lerp_c(a: Complex<f32>, b: Complex<f32>, t: f32) -> Complex<f32> {
    a + (b - a) * t
}

/// Wet-path safety soft-clip: exact identity for |x| ≤ [`CLIP_KNEE`]; above the knee the
/// overshoot is tanh-compressed so the result stays in [−1, 1). Because normal STFT
/// reconstruction of program material sits well below the knee, this is a true bypass for the
/// null/identity tests and only engages on loud resonant peaks.
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

/// Soft-limit a complex value's magnitude with a tanh knee at `clip`.
#[inline]
fn soft_limit_c(c: Complex<f32>, clip: f32) -> Complex<f32> {
    let m = c.norm();
    if m <= 1.0e-20 {
        return c;
    }
    let m2 = clip * (m / clip).tanh();
    c * (m2 / m)
}

/// Derived, sample-rate-dependent config recomputed only in [`SmudgeCore::configure`]
/// (block rate). Holds the base settings, the bin→band map, and hop time.
struct Cfg {
    settings: Settings,
    /// Band index (0..N_BANDS-1) for each bin.
    bin_band: Vec<usize>,
    hop_time: f32,
}

impl Cfg {
    fn new(num_bins: usize, sr: f32) -> Self {
        let bin_band: Vec<usize> = (0..num_bins)
            .map(|k| {
                let f = (k as f32 * sr / FFT_SIZE as f32).clamp(F_LO, F_HI);
                let pos = (f.ln() - F_LO.ln()) / (F_HI.ln() - F_LO.ln());
                ((pos * N_BANDS as f32) as usize).min(N_BANDS - 1)
            })
            .collect();
        Self {
            settings: Settings::default(),
            bin_band,
            hop_time: HOP as f32 / sr.max(1.0),
        }
    }
}

/// Chaos sample-and-hold state: random modulators held between redraws.
#[derive(Clone, Copy)]
struct Chaos {
    /// Multiplicative gains (≤1) for the 4 op amounts.
    amt_gain: [f32; 4],
    /// Stretch-factor multiplier (2^(±depth)).
    factor_mult: f32,
    /// Delay-tilt additive offset.
    tilt_off: f32,
    /// Scramble-N multiplier (2^(±depth)).
    neigh_mult: f32,
}

impl Chaos {
    fn neutral() -> Self {
        Self {
            amt_gain: [1.0; 4],
            factor_mult: 1.0,
            tilt_off: 0.0,
            neigh_mult: 1.0,
        }
    }
}

/// Smoothed effective (post-chaos) op parameters, shared by both channels each frame.
struct Eff {
    scramble_amt: OnePole,
    delay_amt: OnePole,
    blur_amt: OnePole,
    stretch_amt: OnePole,
    stretch_factor: OnePole,
    delay_tilt: OnePole,
    /// Scramble neighbourhood half-width (bins), integer — used only at perm redraw.
    scramble_n: usize,
    /// Per-band spectral-delay depth (frames, ≥1 when used).
    delay_frames_band: [usize; N_BANDS],
    /// Per-band blur one-pole coefficient.
    blur_coef_band: [f32; N_BANDS],
}

impl Eff {
    fn new() -> Self {
        Self {
            scramble_amt: OnePole::new(),
            delay_amt: OnePole::new(),
            blur_amt: OnePole::new(),
            stretch_amt: OnePole::new(),
            stretch_factor: OnePole::new(),
            delay_tilt: OnePole::new(),
            scramble_n: 0,
            delay_frames_band: [1; N_BANDS],
            blur_coef_band: [0.0; N_BANDS],
        }
    }

    fn set_smooth_rate(&mut self, frame_rate: f32) {
        for p in [
            &mut self.scramble_amt,
            &mut self.delay_amt,
            &mut self.blur_amt,
            &mut self.stretch_amt,
            &mut self.stretch_factor,
            &mut self.delay_tilt,
        ] {
            p.set_time(PARAM_SMOOTH_MS, frame_rate);
        }
    }
}

/// State shared by the two channels (computed once per frame by the primary channel).
struct Shared {
    rng: Rng,
    chaos: Chaos,
    frame_idx: u64,
    /// Current permutation (identity-initialised).
    perm: Vec<usize>,
    /// Complex scratch reused by scramble (op1) and stretch (op4).
    scratch: Vec<Complex<f32>>,
    eff: Eff,
    primed: bool,
}

impl Shared {
    fn new(num_bins: usize) -> Self {
        Self {
            rng: Rng::new(0x5EED_5309),
            chaos: Chaos::neutral(),
            frame_idx: 0,
            perm: (0..num_bins).collect(),
            scratch: vec![Complex::new(0.0, 0.0); num_bins],
            eff: Eff::new(),
            primed: false,
        }
    }

    fn reset(&mut self) {
        self.rng = Rng::new(0x5EED_5309);
        self.chaos = Chaos::neutral();
        self.frame_idx = 0;
        for (k, p) in self.perm.iter_mut().enumerate() {
            *p = k;
        }
        self.primed = false;
    }

    /// Advance one frame (called by the primary/L channel): update chaos S&H, compute the
    /// smoothed effective params + per-band tables, and redraw the scramble permutation on
    /// schedule. The secondary channel reuses this state unchanged.
    fn advance_frame(&mut self, cfg: &Cfg) {
        let s = &cfg.settings;
        let depth = s.chaos_depth.clamp(0.0, 1.0);

        // ---- chaos sample & hold ----
        let chaos_rate = s.chaos_rate.max(1) as u64;
        if depth <= 0.0 {
            self.chaos = Chaos::neutral();
        } else if self.frame_idx % chaos_rate == 0 {
            let mut g = [1.0f32; 4];
            for gi in g.iter_mut() {
                // Random gain in [1-depth, 1] — never lifts a zero base amount.
                *gi = 1.0 - depth * self.rng.next_unipolar();
            }
            self.chaos = Chaos {
                amt_gain: g,
                factor_mult: 2.0f32.powf(depth * self.rng.next_bipolar()),
                tilt_off: depth * self.rng.next_bipolar(),
                neigh_mult: 2.0f32.powf(depth * self.rng.next_bipolar()),
            };
        }

        // ---- effective (post-chaos) targets ----
        let c = self.chaos;
        let t_scramble = (s.scramble_amt * c.amt_gain[0]).clamp(0.0, 1.0);
        let t_delay = (s.delay_amt * c.amt_gain[1]).clamp(0.0, 1.0);
        let t_blur = (s.blur_amt * c.amt_gain[2]).clamp(0.0, 1.0);
        let t_stretch = (s.stretch_amt * c.amt_gain[3]).clamp(0.0, 1.0);
        let t_factor =
            (s.stretch_factor * c.factor_mult).clamp(MIN_STRETCH, MAX_STRETCH);
        let t_tilt = (s.delay_tilt + c.tilt_off).clamp(-1.0, 1.0);

        if !self.primed {
            self.eff.scramble_amt.reset(t_scramble);
            self.eff.delay_amt.reset(t_delay);
            self.eff.blur_amt.reset(t_blur);
            self.eff.stretch_amt.reset(t_stretch);
            self.eff.stretch_factor.reset(t_factor);
            self.eff.delay_tilt.reset(t_tilt);
            self.primed = true;
        }
        self.eff.scramble_amt.process(t_scramble);
        self.eff.delay_amt.process(t_delay);
        self.eff.blur_amt.process(t_blur);
        self.eff.stretch_amt.process(t_stretch);
        self.eff.stretch_factor.process(t_factor);
        let tilt = self.eff.delay_tilt.process(t_tilt);

        // ---- scramble N (integer, chaos-modulated) ----
        let base_n = (s.scramble_range.clamp(0.0, 1.0) * MAX_NEIGH as f32).round();
        self.eff.scramble_n =
            (base_n * c.neigh_mult).round().clamp(0.0, MAX_NEIGH as f32) as usize;

        // ---- per-band delay depth from tilt ----
        for b in 0..N_BANDS {
            let f = if N_BANDS > 1 {
                b as f32 / (N_BANDS - 1) as f32
            } else {
                0.5
            };
            // tilt +1 → highs long (depth_frac = f); −1 → lows long (1−f); 0 → 0.5.
            let depth_frac = (0.5 + 0.5 * tilt * (2.0 * f - 1.0)).clamp(0.0, 1.0);
            let frames = (depth_frac * MAX_DELAY_FRAMES as f32).round() as usize;
            self.eff.delay_frames_band[b] = frames.clamp(1, MAX_DELAY_FRAMES);
        }

        // ---- per-band blur coefficient from τ + blur tilt ----
        let tau_tilt = s.blur_tilt.clamp(-1.0, 1.0);
        for b in 0..N_BANDS {
            let f = if N_BANDS > 1 {
                b as f32 / (N_BANDS - 1) as f32
            } else {
                0.5
            };
            // ±2 octaves of τ scaling across the band range.
            let scale = 2.0f32.powf(tau_tilt * (2.0 * f - 1.0) * 2.0);
            let tau = (s.blur_tau_ms * 1.0e-3 * scale).max(1.0e-4);
            self.eff.blur_coef_band[b] = 1.0 - (-cfg.hop_time / tau).exp();
        }

        // ---- scramble permutation redraw ----
        let rate = s.scramble_rate.max(1) as u64;
        if self.frame_idx % rate == 0 {
            self.redraw_perm();
        }

        self.frame_idx = self.frame_idx.wrapping_add(1);
    }

    /// Rebuild the permutation: identity, then Fisher-Yates within contiguous neighbourhoods
    /// of width `2N+1` over bins [1 .. nb-1) (DC and Nyquist stay fixed). N=0 ⇒ identity.
    fn redraw_perm(&mut self) {
        let nb = self.perm.len();
        for (k, p) in self.perm.iter_mut().enumerate() {
            *p = k;
        }
        let n = self.eff.scramble_n;
        if n == 0 || nb < 4 {
            return;
        }
        let w = 2 * n + 1;
        let mut start = 1usize;
        while start < nb - 1 {
            let end = (start + w).min(nb - 1);
            // Fisher-Yates over perm[start..end).
            let mut i = end;
            while i > start + 1 {
                i -= 1;
                let span = (i - start + 1) as u32;
                let j = start + (self.rng.next_u32() % span) as usize;
                self.perm.swap(i, j);
            }
            start = end;
        }
    }
}

/// Per-channel spectral-delay ring + blur state.
struct ChanState {
    /// Complex frame ring, layout `[frame * nb + bin]`, `head` = next write slot.
    delay_ring: Vec<Complex<f32>>,
    head: usize,
    /// Decaying envelope of input frame-energy — the ceiling the delay output is normalised
    /// to, so the feedback tail stays bounded (≤ recent input level) even into silence.
    delay_env_e: f32,
    /// Blur temporal magnitude state per bin.
    blur_mag: Vec<f32>,
    /// Phase tracking (EMBER-style) for the blur phase-vocoder advance.
    prev_ph: Vec<f32>,
    out_ph: Vec<f32>,
}

impl ChanState {
    fn new(nb: usize) -> Self {
        Self {
            delay_ring: vec![Complex::new(0.0, 0.0); nb * MAX_DELAY_FRAMES],
            head: 0,
            delay_env_e: 0.0,
            blur_mag: vec![0.0; nb],
            prev_ph: vec![0.0; nb],
            out_ph: vec![0.0; nb],
        }
    }
    fn reset(&mut self) {
        for v in self.delay_ring.iter_mut() {
            *v = Complex::new(0.0, 0.0);
        }
        self.head = 0;
        self.delay_env_e = 0.0;
        for v in self.blur_mag.iter_mut() {
            *v = 0.0;
        }
        for v in self.prev_ph.iter_mut() {
            *v = 0.0;
        }
        for v in self.out_ph.iter_mut() {
            *v = 0.0;
        }
    }
}

/// The per-frame spectral op chain (order 1→4), operating in place on the complex spectrum.
fn frame(
    spec: &mut [Complex<f32>],
    primary: bool,
    shared: &mut Shared,
    chan: &mut ChanState,
    cfg: &Cfg,
) {
    let nb = spec.len();
    if primary {
        shared.advance_frame(cfg);
    }

    // Read the smoothed effective params computed by the primary this frame.
    let scramble_amt = shared.eff.scramble_amt.value();
    let delay_amt = shared.eff.delay_amt.value();
    let blur_amt = shared.eff.blur_amt.value();
    let stretch_amt = shared.eff.stretch_amt.value();
    let stretch_factor = shared.eff.stretch_factor.value();
    let delay_feedback = cfg.settings.delay_feedback.clamp(0.0, MAX_DELAY_FEEDBACK);

    // ---- Op 1: SCRAMBLE (permute bins in neighbourhoods) ----
    if scramble_amt > AMOUNT_EPS {
        for k in 0..nb {
            shared.scratch[k] = spec[shared.perm[k]] * SCRAMBLE_MAKEUP;
        }
        for k in 0..nb {
            spec[k] = lerp_c(spec[k], shared.scratch[k], scramble_amt);
        }
    }

    // ---- Op 2: SPECTRAL DELAY (per-band frame delays + feedback) ----
    if delay_amt > AMOUNT_EPS {
        let head = chan.head;
        let mut in_e = 0.0f32;
        let mut out_e = 0.0f32;
        for k in 0..nb {
            let cur = spec[k];
            in_e += cur.norm_sqr();
            let d = shared.eff.delay_frames_band[cfg.bin_band[k]];
            let read = (head + MAX_DELAY_FRAMES - d) % MAX_DELAY_FRAMES;
            let delayed = chan.delay_ring[read * nb + k];
            // Write current + feedback·delayed (soft-limited) into the ring.
            let w = soft_limit_c(cur + delayed * delay_feedback, DELAY_CLIP);
            chan.delay_ring[head * nb + k] = w;
            // Add the delayed echo to the output.
            let o = cur + delayed * delay_amt;
            spec[k] = o;
            out_e += o.norm_sqr();
        }
        chan.head = (head + 1) % MAX_DELAY_FRAMES;
        // Track a decaying envelope of the input frame-energy and normalise the echoed
        // spectrum DOWN to it. During input the ceiling == current energy (echo can't raise
        // the level past dry, no ducking); into silence the ceiling is the released envelope
        // scaled by a crest headroom so the (peakier, resonant) feedback tail is audible but
        // bounded ≤ 0 dBFS. Attenuate-only.
        chan.delay_env_e = in_e.max(chan.delay_env_e * DELAY_ENV_RELEASE);
        let ceil_e = in_e.max(chan.delay_env_e * DELAY_ENV_HEADROOM);
        if out_e > ceil_e && ceil_e > 0.0 {
            let norm = (ceil_e / out_e).sqrt();
            for k in 0..nb {
                spec[k] = spec[k] * norm;
            }
        }
    }

    // ---- Op 3: BLUR (temporal magnitude averaging + phase-vocoder advance) ----
    if blur_amt > AMOUNT_EPS {
        let va = ((blur_amt - 0.5) * 2.0).clamp(0.0, 1.0);
        for k in 0..nb {
            let c = spec[k];
            let mag = c.norm();
            let ph = c.im.atan2(c.re);
            let coef = shared.eff.blur_coef_band[cfg.bin_band[k]];
            chan.blur_mag[k] += coef * (mag - chan.blur_mag[k]);
            let out_mag = mag + blur_amt * (chan.blur_mag[k] - mag);

            // Phase-vocoder advance, blended in along the shortest arc as blur dominates.
            let dphi = wrap(ph - chan.prev_ph[k]);
            chan.prev_ph[k] = ph;
            let advanced = wrap(chan.out_ph[k] + dphi);
            let phase_out = wrap(ph + va * wrap(advanced - ph));
            chan.out_ph[k] = phase_out;

            spec[k] = Complex::from_polar(out_mag, phase_out);
        }
    }

    // ---- Op 4: SMEAR / STRETCH (bin-index remap, energy-normalised) ----
    if stretch_amt > AMOUNT_EPS {
        let inv = 1.0 / stretch_factor;
        for k in 0..nb {
            let src = k as f32 * inv;
            let j = src.floor() as isize;
            let frac = src - j as f32;
            let a = get_bin(spec, j, nb);
            let b = get_bin(spec, j + 1, nb);
            shared.scratch[k] = a * (1.0 - frac) + b * frac;
        }
        // Energy normalise the remapped spectrum to the original.
        let mut in_e = 0.0f32;
        let mut out_e = 0.0f32;
        for k in 0..nb {
            in_e += spec[k].norm_sqr();
            out_e += shared.scratch[k].norm_sqr();
        }
        let norm = if out_e > 1.0e-20 {
            (in_e / out_e).sqrt()
        } else {
            1.0
        };
        for k in 0..nb {
            let stretched = shared.scratch[k] * norm;
            spec[k] = lerp_c(spec[k], stretched, stretch_amt);
        }
    }
}

/// Bounds-checked complex-bin read: out-of-range indices read as zero (no wrap).
#[inline]
fn get_bin(spec: &[Complex<f32>], idx: isize, nb: usize) -> Complex<f32> {
    if idx < 0 || idx as usize >= nb {
        Complex::new(0.0, 0.0)
    } else {
        spec[idx as usize]
    }
}

/// A short delay line used to align the dry path with the STFT's reported latency.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }
    #[inline]
    fn push(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }
}

/// SMUDGE's full stereo DSP core.
pub struct SmudgeCore {
    num_bins: usize,
    cfg: Cfg,
    stft_l: Stft,
    stft_r: Stft,
    shared: Shared,
    chan_l: ChanState,
    chan_r: ChanState,
    dry_l: Delay,
    dry_r: Delay,
}

impl SmudgeCore {
    pub fn new(sample_rate: f32) -> Self {
        let nb = FFT_SIZE / 2 + 1;
        let sr = sample_rate.max(1.0);
        let mut shared = Shared::new(nb);
        let frame_rate = sr / HOP as f32;
        shared.eff.set_smooth_rate(frame_rate);
        Self {
            num_bins: nb,
            cfg: Cfg::new(nb, sr),
            stft_l: Stft::new(FFT_SIZE, HOP),
            stft_r: Stft::new(FFT_SIZE, HOP),
            shared,
            chan_l: ChanState::new(nb),
            chan_r: ChanState::new(nb),
            dry_l: Delay::new(FFT_SIZE),
            dry_r: Delay::new(FFT_SIZE),
        }
    }

    /// Latency (samples) this core adds — equal to the STFT frame size.
    pub fn latency(&self) -> usize {
        FFT_SIZE
    }

    pub fn num_bins(&self) -> usize {
        self.num_bins
    }

    pub fn reset(&mut self) {
        self.stft_l.reset();
        self.stft_r.reset();
        self.shared.reset();
        self.chan_l.reset();
        self.chan_r.reset();
        self.dry_l.reset();
        self.dry_r.reset();
    }

    /// Latch a settings snapshot (call at block rate). Chaos/smoothing run at frame rate.
    pub fn configure(&mut self, s: &Settings) {
        self.cfg.settings = *s;
    }

    /// Process one stereo sample. `mix` is passed per-sample so it can be smoothed.
    #[inline]
    pub fn process_sample(&mut self, l: f32, r: f32, mix: f32) -> (f32, f32) {
        let cfg = &self.cfg;
        // Left channel is the primary: it advances the shared per-frame state.
        let wet_l = {
            let shared = &mut self.shared;
            let chan = &mut self.chan_l;
            self.stft_l
                .process(l, &mut |spec| frame(spec, true, shared, chan, cfg))
        };
        let wet_r = {
            let shared = &mut self.shared;
            let chan = &mut self.chan_r;
            self.stft_r
                .process(r, &mut |spec| frame(spec, false, shared, chan, cfg))
        };
        // Wet-path safety clip (exact identity for normal levels — see `safety_clip`).
        let wet_l = safety_clip(wet_l);
        let wet_r = safety_clip(wet_r);
        let dry_l = self.dry_l.push(l);
        let dry_r = self.dry_r.push(r);
        let m = mix.clamp(0.0, 1.0);
        (dry_l + m * (wet_l - dry_l), dry_r + m * (wet_r - dry_r))
    }

    /// Offline mono convenience for the harness: process `buf` in place (left channel out)
    /// with a fixed `Settings`.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        self.configure(s);
        for x in buf.iter_mut() {
            let (y, _) = self.process_sample(*x, *x, s.mix);
            *x = y;
        }
    }
}

// Re-export the suite RNG under a local name so the module reads cleanly.
use suite_core::testsig::Rng as SuiteRng;

/// Thin wrapper adding a unipolar helper over the suite xorshift RNG.
struct Rng(SuiteRng);
impl Rng {
    fn new(seed: u32) -> Self {
        Rng(SuiteRng::new(seed))
    }
    #[inline]
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    #[inline]
    fn next_bipolar(&mut self) -> f32 {
        self.0.next_bipolar()
    }
    /// Uniform float in [0, 1).
    #[inline]
    fn next_unipolar(&mut self) -> f32 {
        self.0.next_u32() as f32 / u32::MAX as f32
    }
}
