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

// ---------------------------------------------------------------------------
// Oversampling — polyphase-style halfband FIR up/down-samplers for nonlinear
// stages (PRD §3 "2–4x oversampling on nonlinear stages"). Linear-phase windowed
// -sinc halfband lowpass at Fs/4; a 2x stage cascades to 4x. Fully preallocated,
// allocation-free in `process` (safe under nih-plug's assert_process_allocs).
// ---------------------------------------------------------------------------

/// Tap count of the halfband lowpass used by every oversampler stage. Forced odd in
/// [`design_lowpass`] so the kernel is symmetric (linear phase) with an integer group
/// delay of `(N-1)/2` samples at the rate the FIR runs.
pub const HALFBAND_TAPS: usize = 31;

/// Design a windowed-sinc (Hamming) FIR lowpass of `num_taps` taps with cutoff
/// `fc_norm` in cycles/sample (0..0.5), normalized to unity DC gain.
fn design_lowpass(num_taps: usize, fc_norm: f32) -> Vec<f32> {
    let n = num_taps.max(3) | 1; // force odd for a symmetric linear-phase kernel
    let m = (n - 1) as f32 / 2.0;
    let mut h = vec![0.0f32; n];
    let mut sum = 0.0f32;
    for (i, hi) in h.iter_mut().enumerate() {
        let k = i as f32 - m;
        let sinc = if k.abs() < 1.0e-6 {
            2.0 * fc_norm
        } else {
            (2.0 * PI * fc_norm * k).sin() / (PI * k)
        };
        let w = 0.54 - 0.46 * (2.0 * PI * i as f32 / (n - 1) as f32).cos();
        *hi = sinc * w;
        sum += *hi;
    }
    if sum.abs() > 1.0e-12 {
        for hi in h.iter_mut() {
            *hi /= sum;
        }
    }
    h
}

/// Streaming direct-form FIR with a ring-buffer history. Allocation-free `process`.
#[derive(Clone)]
struct Fir {
    h: Vec<f32>,
    z: Vec<f32>,
    pos: usize,
}

impl Fir {
    fn new(h: Vec<f32>) -> Self {
        let n = h.len();
        Self {
            h,
            z: vec![0.0; n],
            pos: 0,
        }
    }

    fn reset(&mut self) {
        for v in self.z.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let n = self.h.len();
        self.z[self.pos] = x;
        let mut acc = 0.0f32;
        let mut idx = self.pos;
        for &hk in self.h.iter() {
            acc += hk * self.z[idx];
            idx = if idx == 0 { n - 1 } else { idx - 1 };
        }
        self.pos += 1;
        if self.pos == n {
            self.pos = 0;
        }
        acc
    }
}

/// 2x oversampler: zero-stuffing upsample → nonlinearity at 2x → halfband decimate.
#[derive(Clone)]
pub struct Oversampler2x {
    up: Fir,
    down: Fir,
}

impl Default for Oversampler2x {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler2x {
    pub fn new() -> Self {
        let h = design_lowpass(HALFBAND_TAPS, 0.25);
        Self {
            up: Fir::new(h.clone()),
            down: Fir::new(h),
        }
    }

    /// Group delay (base-rate samples) introduced by this 2x stage's linear-phase
    /// halfband FIRs. Both the up- and down-sampling FIRs run at the 2x rate and each
    /// contribute `(N-1)/2` samples of delay there, so together they add `(N-1)` samples
    /// at 2x = `(N-1)/2` samples at the base rate. Verified empirically by the
    /// `oversampler_group_delay_matches_analytic` test.
    #[inline]
    pub fn group_delay_samples() -> f32 {
        (HALFBAND_TAPS as f32 - 1.0) / 2.0
    }

    /// Empirically measured base-rate group delay: the integer peak lag of a unit impulse
    /// pushed through this oversampler with an identity shaper. Equals
    /// [`group_delay_samples`](Self::group_delay_samples) to within 1 sample; used for
    /// dry-path delay compensation so the alignment is exact (0-sample error).
    pub fn measure_group_delay() -> usize {
        let mut os = Oversampler2x::new();
        let mut peak = 0.0f32;
        let mut idx = 0usize;
        for n in 0..256usize {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y = os.process(x, |v| v);
            if y.abs() > peak {
                peak = y.abs();
                idx = n;
            }
        }
        idx
    }

    pub fn reset(&mut self) {
        self.up.reset();
        self.down.reset();
    }

    /// Process one input sample: upsample 2x, apply `f` per high-rate sample,
    /// decimate back. `f` sees the same nonlinearity at twice the rate.
    #[inline]
    pub fn process<F: FnMut(f32) -> f32>(&mut self, x: f32, mut f: F) -> f32 {
        // Zero-stuff and compensate the halving of energy with a gain of 2.
        let u0 = self.up.process(x * 2.0);
        let u1 = self.up.process(0.0);
        let y0 = f(u0);
        let y1 = f(u1);
        // Anti-alias then decimate by 2 (advance on both, keep the second).
        self.down.process(y0);
        self.down.process(y1)
    }
}

/// 4x oversampler = two cascaded 2x halfband stages.
#[derive(Clone, Default)]
pub struct Oversampler4x {
    s1: Oversampler2x,
    s2: Oversampler2x,
}

impl Oversampler4x {
    pub fn new() -> Self {
        Self::default()
    }

    /// Group delay (base-rate samples) of the 4x oversampler. Stage 1's up/down FIRs run
    /// at 2x and add `(N-1)/2` base samples; the inner stage 2 is itself a 2x oversampler
    /// running at the 2x rate, so its own `(N-1)/2`-at-its-base delay is `(N-1)/4` base
    /// samples. Total = `(N-1)/2 + (N-1)/4 = 3(N-1)/4` base samples (= 22.5 at N=31).
    /// Verified empirically by the `oversampler_group_delay_matches_analytic` test.
    #[inline]
    pub fn group_delay_samples() -> f32 {
        3.0 * (HALFBAND_TAPS as f32 - 1.0) / 4.0
    }

    /// Empirically measured base-rate group delay (integer impulse peak lag through an
    /// identity shaper). Equals [`group_delay_samples`](Self::group_delay_samples) to
    /// within 1 sample; used for exact dry-path delay compensation.
    pub fn measure_group_delay() -> usize {
        let mut os = Oversampler4x::new();
        let mut peak = 0.0f32;
        let mut idx = 0usize;
        for n in 0..256usize {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y = os.process(x, |v| v);
            if y.abs() > peak {
                peak = y.abs();
                idx = n;
            }
        }
        idx
    }

    pub fn reset(&mut self) {
        self.s1.reset();
        self.s2.reset();
    }

    /// Process one input sample at 4x oversampling, applying `f` at the 4x rate.
    #[inline]
    pub fn process<F: FnMut(f32) -> f32>(&mut self, x: f32, mut f: F) -> f32 {
        let s2 = &mut self.s2;
        self.s1.process(x, |u| s2.process(u, &mut f))
    }
}

/// A fixed integer-sample delay line (allocation-free `process`). Used to delay-compensate
/// a dry path so it stays sample-aligned with a wet path that passes through an oversampler
/// or other fixed-latency stage — the alignment that prevents comb filtering at partial
/// dry/wet mix (PRD §3, HARD CHECKPOINT 1).
#[derive(Clone)]
pub struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
    delay: usize,
}

impl DelayLine {
    /// Create a delay line that can delay by up to `max_delay` samples (initialised to
    /// exactly `max_delay`).
    pub fn new(max_delay: usize) -> Self {
        Self {
            buf: vec![0.0; max_delay + 1],
            pos: 0,
            delay: max_delay,
        }
    }

    /// Set the active delay (clamped to the allocated maximum). Does not clear history.
    pub fn set_delay(&mut self, delay: usize) {
        self.delay = delay.min(self.buf.len().saturating_sub(1));
    }

    pub fn delay(&self) -> usize {
        self.delay
    }

    pub fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }

    /// Push `x`, return the sample from `delay` samples ago.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let len = self.buf.len();
        let read = (self.pos + len - self.delay) % len;
        let y = self.buf[read];
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == len {
            self.pos = 0;
        }
        y
    }
}

/// RAII guard that enables SSE flush-to-zero (FTZ) + denormals-are-zero (DAZ) for the
/// duration of an audio `process()` scope, restoring the previous MXCSR mode on drop.
///
/// Denormal floats (e.g. the tails of IIR filters, reverbs, and envelopes decaying to
/// silence) are hundreds of times slower to compute on x86; a single leaked denormal in a
/// feedback path can spike CPU. Enabling FTZ/DAZ once at the top of every plugin's
/// `process()` mitigates this suite-wide (PRD §3 / HARD CHECKPOINT 1, MINOR 7).
///
/// On non-x86_64 targets this is a no-op.
pub struct ScopedFtz {
    #[cfg(target_arch = "x86_64")]
    saved_mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
impl ScopedFtz {
    /// MXCSR bit 15 — flush-to-zero (denormal *results* are flushed to 0).
    const FTZ: u32 = 1 << 15;
    /// MXCSR bit 6 — denormals-are-zero (denormal *inputs* are treated as 0). Supported on
    /// all x86_64 CPUs in practice; SSE2 is mandatory on the architecture.
    const DAZ: u32 = 1 << 6;
}

impl ScopedFtz {
    /// Enable FTZ + DAZ, saving the prior MXCSR for restoration on drop.
    ///
    /// (This toolchain does not expose the `_MM_SET_DENORMALS_ZERO_MODE` intrinsic, so the
    /// MXCSR control word is read/written directly — both bits live in it.)
    #[inline]
    pub fn enable() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            // SSE/SSE2 are guaranteed on x86_64, so these MXCSR intrinsics are always safe
            // to call here; the `unsafe` is only for the target-feature contract.
            #[allow(deprecated)]
            unsafe {
                let saved_mxcsr = core::arch::x86_64::_mm_getcsr();
                core::arch::x86_64::_mm_setcsr(saved_mxcsr | Self::FTZ | Self::DAZ);
                Self { saved_mxcsr }
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Self {}
        }
    }
}

impl Drop for ScopedFtz {
    #[inline]
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        {
            #[allow(deprecated)]
            unsafe {
                core::arch::x86_64::_mm_setcsr(self.saved_mxcsr);
            }
        }
    }
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
    fn oversampler4x_passes_low_frequency_cleanly() {
        // A 1 kHz sine put through a 4x oversampler with an identity nonlinearity
        // should come back with nearly the same amplitude (allowing for filter
        // group delay) and no blow-up.
        let mut os = Oversampler4x::new();
        let sr = 48_000.0f32;
        let mut peak_in = 0.0f32;
        let mut peak_out = 0.0f32;
        // Prime the filters, then measure a steady window.
        for n in 0..9_600usize {
            let x = 0.5 * (2.0 * PI * 1_000.0 * n as f32 / sr).sin();
            let y = os.process(x, |v| v);
            if n > 4_800 {
                peak_in = peak_in.max(x.abs());
                peak_out = peak_out.max(y.abs());
            }
        }
        assert!(peak_out.is_finite());
        assert!(
            (peak_out - peak_in).abs() < 0.06,
            "OS changed amplitude too much: in {peak_in:.3} out {peak_out:.3}"
        );
    }

    /// Empirically confirm the analytic `group_delay_samples()` values by measuring the
    /// impulse peak lag through each oversampler with an identity (passthrough) shaper.
    /// This is the measurement the GRIT/TRACER dry-path delay compensation relies on.
    #[test]
    fn oversampler_group_delay_matches_analytic() {
        fn peak_lag_2x() -> usize {
            let mut os = Oversampler2x::new();
            let mut peak = 0.0f32;
            let mut idx = 0usize;
            for n in 0..256usize {
                let x = if n == 0 { 1.0 } else { 0.0 };
                let y = os.process(x, |v| v);
                if y.abs() > peak {
                    peak = y.abs();
                    idx = n;
                }
            }
            idx
        }
        fn peak_lag_4x() -> usize {
            let mut os = Oversampler4x::new();
            let mut peak = 0.0f32;
            let mut idx = 0usize;
            for n in 0..256usize {
                let x = if n == 0 { 1.0 } else { 0.0 };
                let y = os.process(x, |v| v);
                if y.abs() > peak {
                    peak = y.abs();
                    idx = n;
                }
            }
            idx
        }

        let lag2 = peak_lag_2x() as f32;
        let a2 = Oversampler2x::group_delay_samples();
        assert!(
            (lag2 - a2).abs() <= 1.0,
            "2x: measured peak lag {lag2} vs analytic {a2}"
        );
        // The exposed empirical measurement must equal the peak-lag probe here.
        assert_eq!(Oversampler2x::measure_group_delay(), lag2 as usize);

        let lag4 = peak_lag_4x() as f32;
        let a4 = Oversampler4x::group_delay_samples();
        assert!(
            (lag4 - a4).abs() <= 1.0,
            "4x: measured peak lag {lag4} vs analytic {a4}"
        );
        assert_eq!(Oversampler4x::measure_group_delay(), lag4 as usize);
    }

    #[test]
    fn delay_line_delays_by_exact_samples() {
        let mut d = DelayLine::new(23);
        d.set_delay(23);
        let mut out = Vec::new();
        for n in 0..64 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            out.push(d.process(x));
        }
        // The impulse must reappear exactly 23 samples later, nowhere else.
        for (n, &v) in out.iter().enumerate() {
            if n == 23 {
                assert!((v - 1.0).abs() < 1e-9, "delayed impulse missing at 23: {v}");
            } else {
                assert!(v.abs() < 1e-9, "spurious output at {n}: {v}");
            }
        }
    }

    #[test]
    fn oversampler2x_bounded_under_hard_drive() {
        let mut os = Oversampler2x::new();
        let sr = 48_000.0f32;
        for n in 0..4_800usize {
            let x = (2.0 * PI * 3_000.0 * n as f32 / sr).sin();
            let y = os.process(x, |v| (v * 4.0).tanh());
            assert!(y.is_finite() && y.abs() < 1.5, "os2x out of range: {y}");
        }
    }

    #[test]
    fn env_follower_tracks_level() {
        // Symmetric smoothing >> signal period so the follower averages x^2 and
        // reports true RMS (a fast attack + slow release would peak-track instead).
        let mut e = EnvFollower::new(Detector::Rms);
        e.set_times(50.0, 50.0, 48_000.0);
        let mut env = 0.0;
        for n in 0..48_000 {
            let x = (2.0 * PI * 1_000.0 * n as f32 / 48_000.0).sin() * 0.5;
            env = e.process(x);
        }
        // RMS of a 0.5-amplitude sine is ~0.354.
        assert!((env - 0.354).abs() < 0.05, "env was {env}");
    }
}
