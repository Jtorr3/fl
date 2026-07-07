//! Minimal-but-real DSP building blocks shared across the suite.
//!
//! - [`OnePole`]  : one-pole exponential smoother (parameter/control smoothing)
//! - [`Svf`]      : TPT (topology-preserving transform) state-variable filter
//! - [`EnvFollower`] : peak / RMS envelope follower with attack+release
//! - [`Shaper`]   : small waveshaper bank (tanh tube, tape soft-knee, hard clip, sine fold)

use std::f32::consts::{FRAC_PI_2, PI};

/// One-pole exponential smoother. Coefficient derived from a time constant so the
/// smoother reaches ~63% of a step change in `time_ms`.
#[derive(Clone, Copy, Debug)]
pub struct OnePole {
    z: f32,
    a: f32,
}

impl Default for OnePole {
    fn default() -> Self {
        Self { z: 0.0, a: 0.0 }
    }
}

impl OnePole {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the smoothing time constant. `time_ms <= 0` means no smoothing
    /// (pass-through).
    pub fn set_time(&mut self, time_ms: f32, sample_rate: f32) {
        let samples = (time_ms * 0.001) * sample_rate;
        self.a = if samples <= 0.0 {
            0.0
        } else {
            (-1.0 / samples).exp()
        };
    }

    /// Jump the internal state directly to `value` (no glide).
    pub fn reset(&mut self, value: f32) {
        self.z = value;
    }

    /// Advance one sample toward `target`.
    #[inline]
    pub fn process(&mut self, target: f32) -> f32 {
        self.z = target + self.a * (self.z - target);
        self.z
    }

    pub fn value(&self) -> f32 {
        self.z
    }
}

/// TPT state-variable filter (Zavalishin / Cytomic). Produces lowpass, bandpass and
/// highpass simultaneously from one `process` call. Stable across the full audio range.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1eq: f32,
    ic2eq: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

/// The three simultaneous outputs of the SVF.
#[derive(Clone, Copy, Debug)]
pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set cutoff (Hz) and resonance Q. Recomputes the fixed coefficients.
    pub fn set(&mut self, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let nyq = sample_rate * 0.5;
        let fc = cutoff_hz.clamp(1.0, nyq - 1.0);
        self.g = (PI * fc / sample_rate).tan();
        self.k = 1.0 / q.max(1.0e-4);
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> SvfOut {
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        let hp = x - self.k * v1 - v2;
        SvfOut { lp: v2, bp: v1, hp }
    }
}

/// Detection mode for [`EnvFollower`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detector {
    Peak,
    Rms,
}

/// Peak / RMS envelope follower with independent attack and release coefficients.
#[derive(Clone, Copy, Debug)]
pub struct EnvFollower {
    detector: Detector,
    atk: f32,
    rel: f32,
    env: f32,
}

impl EnvFollower {
    pub fn new(detector: Detector) -> Self {
        Self {
            detector,
            atk: 0.0,
            rel: 0.0,
            env: 0.0,
        }
    }

    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, sample_rate: f32) {
        self.atk = coef_from_ms(attack_ms, sample_rate);
        self.rel = coef_from_ms(release_ms, sample_rate);
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    /// Feed one sample; returns the current envelope (linear amplitude).
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let target = match self.detector {
            Detector::Peak => x.abs(),
            Detector::Rms => x * x,
        };
        let c = if target > self.env { self.atk } else { self.rel };
        self.env = target + c * (self.env - target);
        match self.detector {
            Detector::Peak => self.env,
            Detector::Rms => self.env.max(0.0).sqrt(),
        }
    }
}

#[inline]
fn coef_from_ms(time_ms: f32, sample_rate: f32) -> f32 {
    let samples = (time_ms * 0.001) * sample_rate;
    if samples <= 0.0 {
        0.0
    } else {
        (-1.0 / samples).exp()
    }
}

/// Small waveshaper bank. Each variant maps an input sample to a shaped output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shaper {
    /// Tube-style odd-harmonic saturation via `tanh`.
    TubeTanh,
    /// Tape-style soft-knee saturation (smooth, gentle compression of peaks).
    TapeSoft,
    /// Hard clipper at +/- 1.0.
    HardClip,
    /// Sine wavefolder — folds back on overdrive.
    SineFold,
}

impl Shaper {
    /// Apply the shaper with a pre-gain `drive` (>= 1.0 pushes harder into the curve).
    #[inline]
    pub fn apply(self, x: f32, drive: f32) -> f32 {
        let d = x * drive;
        match self {
            Shaper::TubeTanh => d.tanh(),
            Shaper::TapeSoft => tape_soft(d),
            Shaper::HardClip => d.clamp(-1.0, 1.0),
            Shaper::SineFold => (d * FRAC_PI_2).sin(),
        }
    }
}

/// Cubic soft-knee saturation, roughly unity for small signals and smoothly
/// saturating toward +/- 1 for larger ones.
#[inline]
pub fn tape_soft(x: f32) -> f32 {
    let t = x.clamp(-3.0, 3.0);
    t * (27.0 + t * t) / (27.0 + 9.0 * t * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onepole_settles_to_target() {
        let mut s = OnePole::new();
        s.set_time(1.0, 48_000.0);
        s.reset(0.0);
        for _ in 0..48_000 {
            s.process(1.0);
        }
        assert!((s.value() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn svf_lp_passes_dc_blocks_nyquist() {
        let mut f = Svf::new();
        f.set(1_000.0, 0.707, 48_000.0);
        // DC through lowpass should approach the input level.
        let mut y = 0.0;
        for _ in 0..48_000 {
            y = f.process(1.0).lp;
        }
        assert!((y - 1.0).abs() < 1e-2);
    }

    #[test]
    fn shapers_are_bounded() {
        for &s in &[
            Shaper::TubeTanh,
            Shaper::TapeSoft,
            Shaper::HardClip,
            Shaper::SineFold,
        ] {
            for i in -100..=100 {
                let x = i as f32 * 0.1;
                let y = s.apply(x, 2.0);
                assert!(y.is_finite());
                assert!(y.abs() <= 1.05, "{s:?} exceeded bound at x={x}: {y}");
            }
        }
    }

    #[test]
    fn env_follower_tracks_level() {
        let mut e = EnvFollower::new(Detector::Rms);
        e.set_times(1.0, 50.0, 48_000.0);
        let mut env = 0.0;
        for n in 0..48_000 {
            let x = (2.0 * PI * 1_000.0 * n as f32 / 48_000.0).sin() * 0.5;
            env = e.process(x);
        }
        // RMS of a 0.5-amplitude sine is ~0.354.
        assert!((env - 0.354).abs() < 0.05, "env was {env}");
    }
}
