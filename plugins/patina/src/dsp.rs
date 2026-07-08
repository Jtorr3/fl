//! PATINA — pure-DSP core for the analog lo-fi character processor (SPECS "PATINA").
//!
//! Signal flow (stereo; shared "tape transport" modulators + per-channel voices):
//! ```text
//!  x ─ wow/flutter (FracDelay ← 0.4 Hz wow + 8 Hz flutter + slow random walk)
//!    ─ saturation (tape_soft, 2x OS, dry blended against a 15-sample delay line)
//!    ─ head-bump EQ (low shelf: y = x + (g−1)·LP(x))
//!    ─ azimuth (R channel HF through a first-order allpass, blended)
//!    ─ dropouts (shared random gain dips, 8 ms-smoothed edges)
//!    ─ + noise (hiss + hum + crackle, KEYED to the input RMS envelope)
//!    ─ AGE macro (adds to every section on a curated curve)
//!    ─ mix (against a latency-matched dry) ─ out
//! ```
//!
//! **Latency / null contract.** Every section is an *exact identity* when its amount is 0:
//! the wow line reads a constant integer base delay (Catmull-Rom at frac 0 = the stored
//! sample), the saturation blends against a clean [`DelayLine`] (never the oversampler's
//! filtered output) so at drive 0 it is a pure delay, the head-bump adds `(g−1)·LP` with
//! `g−1 == 0`, the azimuth adds `amount·(…)` with `amount == 0`, dropouts multiply by a
//! gain primed to 1, and the noise levels are 0. So with **age 0 and all sections 0** the
//! wet path is a *bit-exact* delay of the input by `L = BASE_DELAY + OS_DELAY` samples; the
//! dry path is delayed by the same `L`, so `out = (1−mix)·dry_L + mix·wet` **nulls exactly**
//! against the latency-matched dry for any mix (PRD §4 done-bar). The plugin reports `L` via
//! `set_latency_samples`. Wow/flutter add delay *on top of* the base (one-sided), so with wow
//! active the wet mean-delay exceeds `L`; at partial mix that dry/wet detune is an intended
//! lo-fi flange (documented).

use suite_core::db_to_lin;
use suite_core::dsp::{DelayLine, Detector, EnvFollower, OnePole, Oversampler2x, Svf, tape_soft};
use suite_core::testsig::Rng;

/// Base (constant) wow delay in samples — the modulation zero point. Small, so reported
/// latency stays low; wow/flutter add *more* delay on top of it.
const BASE_DELAY: usize = 16;
/// The 2x oversampler's linear-phase group delay in base samples (`(31−1)/2`). Matches
/// [`Oversampler2x::measure_group_delay`].
const OS_DELAY: usize = 15;
/// Total reported latency = actual wow base delay + saturation oversampler delay.
///
/// [`FracDelay`] writes-then-reads, so an integer read argument of `BASE_DELAY` yields an
/// *actual* delay of `BASE_DELAY − 1` samples; the saturation dry [`DelayLine`] adds `OS_DELAY`.
pub const LATENCY: usize = (BASE_DELAY - 1) + OS_DELAY;

/// Final safety clamp — a pure runaway/NaN guard well above full scale (±8.0 ≈ +18 dBFS),
/// NOT a level ceiling. The old ±0.999 clamp digitally clipped legitimate >0 dBFS float
/// headroom (routine on FL buses) even at mix=0; identity for any real signal.
const CEILING: f32 = 8.0;

/// Wow LFO base frequency (Hz). The done-bar tracks f0 modulation at this rate.
const WOW_HZ: f32 = 0.4;
/// Flutter LFO frequency (Hz).
const FLUTTER_HZ: f32 = 8.0;

/// Max delay (ms) each modulation source contributes at full depth — sizes the delay buffer.
const WOW_MAX_MS: f32 = 8.0;
const FLUT_MAX_MS: f32 = 2.0;
/// The slow random walk is a fraction of the wow depth (no dedicated param; part of "wow").
const WALK_FRACTION: f32 = 0.12;

/// Key-envelope reference amplitude: input RMS at/above this fully opens the keyed noise.
const KEY_REF: f32 = 0.15;

/// Crackle impulse rate (events/second, before the level/key gate).
const CRACKLE_RATE: f32 = 45.0;

// --- AGE macro contributions (added to each base section at age = 1) ---
const AGE_WOW_MS: f32 = 3.0;
const AGE_FLUT_MS: f32 = 1.0;
const AGE_SAT: f32 = 0.5;
const AGE_DROP_RATE: f32 = 2.5;
const AGE_DROP_DEPTH: f32 = 0.5;
const AGE_HISS: f32 = 0.5;
const AGE_HUM: f32 = 0.3;
const AGE_CRACKLE: f32 = 0.5;

/// Block-rate settings snapshot (filled from the params / NERVE listen layer each block). All
/// section amounts are 0-based so that 0 == exact identity.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Wow depth 0..1 (pitch-wobble amount; also scales the slow random walk).
    pub wow_depth: f32,
    /// Wow rate trim (0.5..2.0 × the 0.4 Hz base).
    pub wow_rate: f32,
    /// Flutter depth 0..1 (fast ~8 Hz wobble).
    pub flutter: f32,
    /// Saturation drive 0..1 (0 = clean bypass).
    pub sat_drive: f32,
    /// Head-bump low-shelf boost amount 0..1 (→ up to +9 dB).
    pub bump_amount: f32,
    /// Head-bump shelf corner (Hz, 60..120).
    pub bump_freq: f32,
    /// Azimuth HF phase-skew amount 0..1 (skews the right channel's highs).
    pub azimuth: f32,
    /// Dropout rate 0..1 (→ events/second).
    pub dropout_rate: f32,
    /// Dropout depth 0..1 (how deep each dip cuts).
    pub dropout_depth: f32,
    /// Hiss level 0..1 (filtered white noise).
    pub hiss: f32,
    /// Hum level 0..1 (50/60 Hz + harmonics).
    pub hum: f32,
    /// Crackle level 0..1 (sparse band-passed pops).
    pub crackle: f32,
    /// Hum uses 60 Hz when true, else 50 Hz.
    pub hum_60: bool,
    /// Noise key amount 0..1 (0 = constant floor, 1 = fully gated by the input envelope).
    pub key_amount: f32,
    /// AGE macro 0..1 (adds degradation to every section on a curated curve).
    pub age: f32,
    /// Dry↔wet blend (1 = fully processed). `0` returns the latency-matched dry.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wow_depth: 0.0,
            wow_rate: 1.0,
            flutter: 0.0,
            sat_drive: 0.0,
            bump_amount: 0.0,
            bump_freq: 90.0,
            azimuth: 0.0,
            dropout_rate: 0.0,
            dropout_depth: 0.0,
            hiss: 0.0,
            hum: 0.0,
            crackle: 0.0,
            hum_60: true,
            key_amount: 1.0,
            age: 0.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

/// Effective (post-AGE) per-section amounts, in the units the per-sample path consumes.
#[derive(Clone, Copy, Debug, Default)]
struct Eff {
    wow_samp: f32,
    walk_samp: f32,
    flut_samp: f32,
    wow_rate: f32,
    sat_drive: f32,
    bump_gain: f32,
    bump_hz: f32,
    azimuth: f32,
    drop_rate: f32,
    drop_depth: f32,
    hiss: f32,
    hum: f32,
    crackle: f32,
    hum_hz: f32,
    key_amount: f32,
}

/// First-order allpass (frequency-dependent phase shift) for the azimuth HF skew.
/// `H(z) = (a + z⁻¹)/(1 + a·z⁻¹)`, magnitude-flat for `|a| < 1`.
#[derive(Clone, Copy, Default)]
struct Allpass1 {
    a: f32,
    x1: f32,
    y1: f32,
}

impl Allpass1 {
    fn new(a: f32) -> Self {
        Self { a, x1: 0.0, y1: 0.0 }
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.x1 - self.a * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Uniform Catmull-Rom (tension 0.5). At `t == 0` returns `p1` exactly (integer-tap identity).
#[inline]
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Mono fractional delay line, Catmull-Rom read. Preallocated, alloc-free `process`.
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
    /// Read `delay` samples in the past. `delay == BASE_DELAY` (integer) returns the exact
    /// stored sample (Catmull at frac 0).
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

/// One-pole lowpass helper (head-bump shelf component + hiss tilt).
#[derive(Clone, Copy, Default)]
struct OnePoleLp {
    z: f32,
    a: f32,
}
impl OnePoleLp {
    fn set(&mut self, cutoff_hz: f32, sr: f32) {
        let fc = cutoff_hz.clamp(1.0, sr * 0.49);
        let x = (-2.0 * std::f32::consts::PI * fc / sr).exp();
        self.a = x;
    }
    fn reset(&mut self) {
        self.z = 0.0;
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.z = x * (1.0 - self.a) + self.a * self.z;
        self.z
    }
}

/// Shared "tape transport" modulators — advanced once per output frame so both channels see
/// the same wow/flutter/dropout (a real transport moves both channels together). Hiss/crackle
/// are per-channel (decorrelated); hum is shared (correlated).
struct Mods {
    sr: f32,
    wow_phase: f32,
    flut_phase: f32,
    hum_phase: f32,
    // slow random walk
    walk_val: f32,
    walk_target: f32,
    walk_count: u32,
    walk_period: u32,
    // dropouts
    drop_remaining: u32,
    drop_gain: OnePole,
    rng: Rng,
    // noise keying
    key_env: EnvFollower,
}

impl Mods {
    fn new(sr: f32) -> Self {
        let mut drop_gain = OnePole::new();
        drop_gain.set_time(8.0, sr); // 8 ms smoothed dropout edges (5–15 ms window)
        drop_gain.reset(1.0);
        let mut key_env = EnvFollower::new(Detector::Rms);
        key_env.set_times(15.0, 90.0, sr);
        Self {
            sr,
            wow_phase: 0.0,
            flut_phase: 0.0,
            hum_phase: 0.0,
            walk_val: 0.0,
            walk_target: 0.0,
            walk_count: 0,
            walk_period: (sr * 0.18) as u32, // new walk target every ~180 ms
            drop_remaining: 0,
            drop_gain,
            rng: Rng::new(0x2E1A_7C05),
            key_env,
        }
    }

    fn reset(&mut self) {
        self.wow_phase = 0.0;
        self.flut_phase = 0.0;
        self.hum_phase = 0.0;
        self.walk_val = 0.0;
        self.walk_target = 0.0;
        self.walk_count = 0;
        self.drop_remaining = 0;
        self.drop_gain.reset(1.0);
        self.key_env.reset();
        self.rng = Rng::new(0x2E1A_7C05);
    }

    /// Advance the shared modulators one sample. Returns `(delay_offset_samples, dropout_gain,
    /// hum_sample_unit)`.
    #[inline]
    fn advance(&mut self, e: &Eff) -> (f32, f32, f32) {
        use std::f32::consts::TAU;
        // Wow (0.4 Hz × rate trim) — one-sided so depth 0 ⇒ offset 0 (null-safe).
        self.wow_phase += WOW_HZ * e.wow_rate / self.sr;
        if self.wow_phase >= 1.0 {
            self.wow_phase -= self.wow_phase.floor();
        }
        let wow01 = 0.5 + 0.5 * (TAU * self.wow_phase).sin();
        // Flutter (~8 Hz).
        self.flut_phase += FLUTTER_HZ / self.sr;
        if self.flut_phase >= 1.0 {
            self.flut_phase -= self.flut_phase.floor();
        }
        let flut01 = 0.5 + 0.5 * (TAU * self.flut_phase).sin();
        // Slow random walk toward a redrawn target (part of the wow character).
        if self.walk_count == 0 {
            self.walk_target = self.rng.next_bipolar();
            self.walk_count = self.walk_period.max(1);
        }
        self.walk_count -= 1;
        // Glide ~1/period per sample toward the target.
        let step = 1.0 / self.walk_period.max(1) as f32;
        self.walk_val += (self.walk_target - self.walk_val) * step;
        let walk01 = 0.5 + 0.5 * self.walk_val;

        let offset =
            e.wow_samp * (1.0 - wow01) + e.flut_samp * (1.0 - flut01) + e.walk_samp * (1.0 - walk01);

        // Dropouts: Poisson-triggered dips, smoothed edges.
        if self.drop_remaining == 0 {
            let p = (e.drop_rate / self.sr).clamp(0.0, 1.0);
            if e.drop_depth > 1.0e-4 && p > 0.0 {
                let draw = self.rng.next_u32() as f32 / u32::MAX as f32;
                if draw < p {
                    // Random dropout length 20–140 ms.
                    let ms = 20.0 + 120.0 * (self.rng.next_u32() as f32 / u32::MAX as f32);
                    self.drop_remaining = (ms * 0.001 * self.sr) as u32;
                }
            }
        }
        let drop_target = if self.drop_remaining > 0 {
            self.drop_remaining -= 1;
            (1.0 - e.drop_depth).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let dgain = self.drop_gain.process(drop_target);

        // Hum (fundamental + 3 harmonics), shared phase.
        self.hum_phase += e.hum_hz / self.sr;
        if self.hum_phase >= 1.0 {
            self.hum_phase -= self.hum_phase.floor();
        }
        let hp = TAU * self.hum_phase;
        let hum = 0.60 * hp.sin() + 0.30 * (2.0 * hp).sin() + 0.18 * (3.0 * hp).sin()
            + 0.10 * (4.0 * hp).sin();

        (offset, dgain, hum)
    }

    /// Update + return the keyed-noise gain from the mono input sample.
    #[inline]
    fn key_gain(&mut self, mono_in: f32, key_amount: f32) -> f32 {
        let env = self.key_env.process(mono_in);
        let opened = (env / KEY_REF).clamp(0.0, 1.0);
        (1.0 - key_amount) + key_amount * opened
    }
}

/// Per-channel processing voice (delay line, saturation OS, filters, noise generators).
struct Channel {
    sr: f32,
    wow: FracDelay,
    sat_os: Oversampler2x,
    sat_dry: DelayLine, // 15-sample delay so the sat dry blend matches the OS group delay
    bump_lp: OnePoleLp,
    az_hp: Svf,
    az_ap: Allpass1,
    hiss_lp: OnePoleLp, // hiss tilt (subtract for a gentle HP shape)
    crackle_bp: Svf,
    dry_comp: DelayLine, // main dry path, delayed by LATENCY for the mix
    rng: Rng,
}

impl Channel {
    fn new(sr: f32, max_delay: usize, seed: u32) -> Self {
        let mut az_hp = Svf::new();
        az_hp.set(2000.0, 0.707, sr);
        let mut crackle_bp = Svf::new();
        crackle_bp.set(3000.0, 4.0, sr);
        let mut bump_lp = OnePoleLp::default();
        bump_lp.set(90.0, sr);
        let mut hiss_lp = OnePoleLp::default();
        hiss_lp.set(800.0, sr);
        let mut sat_dry = DelayLine::new(OS_DELAY);
        sat_dry.set_delay(OS_DELAY);
        let mut dry_comp = DelayLine::new(LATENCY);
        dry_comp.set_delay(LATENCY);
        Self {
            sr,
            wow: FracDelay::new(max_delay),
            sat_os: Oversampler2x::new(),
            sat_dry,
            bump_lp,
            az_hp,
            az_ap: Allpass1::new(0.6),
            hiss_lp,
            crackle_bp,
            dry_comp,
            rng: Rng::new(seed),
        }
    }

    fn reset(&mut self) {
        self.wow.reset();
        self.sat_os.reset();
        self.sat_dry.reset();
        self.bump_lp.reset();
        self.az_hp.reset();
        self.az_ap.reset();
        self.hiss_lp.reset();
        self.crackle_bp.reset();
        self.dry_comp.reset();
    }

    fn set_bump(&mut self, hz: f32, sr: f32) {
        self.bump_lp.set(hz, sr);
    }

    /// Process one channel sample. `offset`/`dgain`/`hum` come from the shared modulators;
    /// `keyg` is the keyed-noise gain; `apply_azimuth` skews this channel's HF (R only).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        x: f32,
        e: &Eff,
        offset: f32,
        dgain: f32,
        hum: f32,
        keyg: f32,
        apply_azimuth: bool,
    ) -> f32 {
        // 1) wow/flutter fractional delay (base + one-sided modulation).
        self.wow.write(x);
        let wet = self.wow.read(BASE_DELAY as f32 + offset);

        // 2) saturation — always oversampled; blend the OS-shaped signal against a clean
        //    15-sample delay of the wow output (NOT the OS output) so drive 0 is a pure delay.
        let dry15 = self.sat_dry.process(wet);
        let sat = if e.sat_drive > 1.0e-6 {
            let pre = 1.0 + e.sat_drive * 6.0;
            let makeup = 1.0 / (1.0 + e.sat_drive * 1.5);
            let shaped = self
                .sat_os
                .process(wet, |v| tape_soft(v * pre) * makeup);
            dry15 + e.sat_drive * (shaped - dry15)
        } else {
            // Keep the OS warm (state coherent) but discard its filtered output.
            let _ = self.sat_os.process(wet, |v| v);
            dry15
        };

        // 3) head-bump low shelf: y = x + (g−1)·LP(x). g−1 == 0 ⇒ identity.
        let bump = sat + (e.bump_gain - 1.0) * self.bump_lp.process(sat);

        // 4) azimuth HF skew (right channel only): y = x + amount·(allpass(hp) − hp).
        let az = if apply_azimuth && e.azimuth > 1.0e-6 {
            let hf = self.az_hp.process(bump).hp;
            let shifted = self.az_ap.process(hf);
            bump + e.azimuth * (shifted - hf)
        } else {
            bump
        };

        // 5) dropouts (shared gain).
        let dropped = az * dgain;

        // 6) + keyed noise layer (hiss + hum + crackle).
        let mut noise = 0.0f32;
        if e.hiss > 1.0e-6 {
            let w = self.rng.next_bipolar();
            let lp = self.hiss_lp.process(w);
            let hiss = w - lp * 0.6; // gentle HF tilt
            noise += hiss * e.hiss * 0.5;
        }
        if e.hum > 1.0e-6 {
            noise += hum * e.hum * 0.15;
        }
        if e.crackle > 1.0e-6 {
            let prob = (CRACKLE_RATE / self.sr).clamp(0.0, 1.0);
            let draw = self.rng.next_u32() as f32 / u32::MAX as f32;
            let imp = if draw < prob {
                (self.rng.next_bipolar()).signum()
                    * (0.4 + 0.6 * (self.rng.next_u32() as f32 / u32::MAX as f32))
            } else {
                0.0
            };
            let ring = self.crackle_bp.process(imp).bp;
            noise += ring * e.crackle * 0.6;
        }

        dropped + noise * keyg
    }
}

/// Stereo PATINA core. Shared verbatim with the harness tests (mono path) and the plugin
/// (stereo path).
pub struct PatinaCore {
    sr: f32,
    mods: Mods,
    ch: [Channel; 2],
    eff: Eff,
    mix: OnePole,
    out: OnePole,
    mix_t: f32,
    out_t: f32,
    primed: bool,
}

impl PatinaCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_off_ms = WOW_MAX_MS + AGE_WOW_MS + FLUT_MAX_MS + AGE_FLUT_MS + 5.0;
        let max_delay = BASE_DELAY + ((max_off_ms * 0.001 * sr) as usize) + 8;
        let mut mix = OnePole::new();
        mix.set_time(5.0, sr);
        mix.reset(1.0);
        let mut out = OnePole::new();
        out.set_time(5.0, sr);
        out.reset(1.0);
        Self {
            sr,
            mods: Mods::new(sr),
            ch: [
                Channel::new(sr, max_delay, 0x1357_9BDF),
                Channel::new(sr, max_delay, 0x2468_ACE0),
            ],
            eff: Eff::default(),
            mix,
            out,
            mix_t: 1.0,
            out_t: 1.0,
            primed: false,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Reported plugin latency (base wow delay + saturation oversampler delay).
    pub fn latency_samples(&self) -> u32 {
        LATENCY as u32
    }

    pub fn reset(&mut self) {
        self.mods.reset();
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.mix.reset(self.mix_t);
        self.out.reset(self.out_t);
    }

    /// Apply a block-rate settings snapshot (AGE folded into the effective section amounts).
    pub fn configure(&mut self, s: &Settings) {
        let age = s.age.clamp(0.0, 1.0);
        let a = age.powf(1.3); // curated: gentle at first, steepening toward "destroyed"
        let ms_to_samp = |ms: f32| ms * 0.001 * self.sr;

        let wow_depth = (s.wow_depth.clamp(0.0, 1.0) * WOW_MAX_MS + a * AGE_WOW_MS).max(0.0);
        let flut_depth = (s.flutter.clamp(0.0, 1.0) * FLUT_MAX_MS + a * AGE_FLUT_MS).max(0.0);

        self.eff = Eff {
            wow_samp: ms_to_samp(wow_depth),
            walk_samp: ms_to_samp(wow_depth * WALK_FRACTION),
            flut_samp: ms_to_samp(flut_depth),
            wow_rate: s.wow_rate.clamp(0.25, 4.0),
            sat_drive: (s.sat_drive.clamp(0.0, 1.0) + a * AGE_SAT).clamp(0.0, 1.0),
            bump_gain: db_to_lin(s.bump_amount.clamp(0.0, 1.0) * 9.0),
            bump_hz: s.bump_freq.clamp(40.0, 200.0),
            azimuth: s.azimuth.clamp(0.0, 1.0),
            drop_rate: (s.dropout_rate.clamp(0.0, 1.0) * 6.0 + a * AGE_DROP_RATE).max(0.0),
            drop_depth: (s.dropout_depth.clamp(0.0, 1.0) + a * AGE_DROP_DEPTH).clamp(0.0, 1.0),
            hiss: (s.hiss.clamp(0.0, 1.0) + a * AGE_HISS).clamp(0.0, 1.0),
            hum: (s.hum.clamp(0.0, 1.0) + a * AGE_HUM).clamp(0.0, 1.0),
            crackle: (s.crackle.clamp(0.0, 1.0) + a * AGE_CRACKLE).clamp(0.0, 1.0),
            hum_hz: if s.hum_60 { 60.0 } else { 50.0 },
            key_amount: s.key_amount.clamp(0.0, 1.0),
        };
        for c in self.ch.iter_mut() {
            c.set_bump(self.eff.bump_hz, self.sr);
        }

        self.mix_t = s.mix.clamp(0.0, 1.0);
        self.out_t = db_to_lin(s.out_db);
        if !self.primed {
            self.mix.reset(self.mix_t);
            self.out.reset(self.out_t);
            self.primed = true;
        }
    }

    /// Process one stereo frame. Advances the shared modulators once.
    #[inline]
    pub fn process_stereo(&mut self, inl: f32, inr: f32) -> (f32, f32) {
        let mono = 0.5 * (inl + inr);
        let keyg = self.mods.key_gain(mono, self.eff.key_amount);
        let (offset, dgain, hum) = self.mods.advance(&self.eff);

        let wl = self.ch[0].process(inl, &self.eff, offset, dgain, hum, keyg, false);
        let wr = self.ch[1].process(inr, &self.eff, offset, dgain, hum, keyg, true);

        let dryl = self.ch[0].dry_comp.process(inl);
        let dryr = self.ch[1].dry_comp.process(inr);

        let mix = self.mix.process(self.mix_t);
        let outg = self.out.process(self.out_t);
        let ol = (dryl + mix * (wl - dryl)) * outg;
        let or = (dryr + mix * (wr - dryr)) * outg;
        (ol.clamp(-CEILING, CEILING), or.clamp(-CEILING, CEILING))
    }

    /// Mono path for the offline harness: runs channel 0 (no azimuth) with the shared
    /// modulators advanced once per sample.
    #[inline]
    pub fn process_mono(&mut self, x: f32) -> f32 {
        let keyg = self.mods.key_gain(x, self.eff.key_amount);
        let (offset, dgain, hum) = self.mods.advance(&self.eff);
        let wet = self.ch[0].process(x, &self.eff, offset, dgain, hum, keyg, false);
        let dry = self.ch[0].dry_comp.process(x);
        let mix = self.mix.process(self.mix_t);
        let outg = self.out.process(self.out_t);
        let y = (dry + mix * (wet - dry)) * outg;
        y.clamp(-CEILING, CEILING)
    }
}

impl Clone for PatinaCore {
    fn clone(&self) -> Self {
        PatinaCore::new(self.sr)
    }
}

// The harness `Processor` runs the mono core over a block in place.
impl suite_core::harness::Processor for PatinaCore {
    #[inline]
    fn process(&mut self, block: &mut [f32]) {
        for s in block.iter_mut() {
            *s = self.process_mono(*s);
        }
    }
}
