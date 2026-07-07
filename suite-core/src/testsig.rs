//! Generated-in-code test signals (PRD §4). No external audio files, ever.
//!
//! All generators return `Vec<f32>` mono at [`crate::TEST_SR`] unless a sample rate
//! is passed explicitly.

use std::f32::consts::PI;

/// Tiny deterministic PRNG (xorshift32) so noise renders are reproducible.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform float in [-1, 1).
    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Unit impulse: a single 1.0 at sample 0, silence after.
pub fn impulse(len: usize) -> Vec<f32> {
    let mut v = vec![0.0; len.max(1)];
    v[0] = 1.0;
    v
}

/// Sine at `freq` Hz, `amp` linear, `len` samples.
pub fn sine(freq: f32, amp: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    (0..len)
        .map(|n| amp * (2.0 * PI * freq * n as f32 / sample_rate).sin())
        .collect()
}

/// Logarithmic (exponential) sine sweep from `f0` to `f1` Hz.
pub fn log_chirp(f0: f32, f1: f32, amp: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    let n = len.max(1) as f32;
    let t_total = n / sample_rate;
    let k = (f1 / f0).ln();
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate;
            // Instantaneous-phase integral of an exponential frequency sweep.
            let phase = 2.0 * PI * f0 * t_total / k * ((k * t / t_total).exp() - 1.0);
            amp * phase.sin()
        })
        .collect()
}

/// White-noise burst (uniform), deterministic for a given seed.
pub fn white_noise(amp: f32, len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..len).map(|_| amp * rng.next_bipolar()).collect()
}

/// Pink-noise burst via Paul Kellet's economical filter, normalized to `amp`.
pub fn pink_noise(amp: f32, len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) =
        (0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    (0..len)
        .map(|_| {
            let white = rng.next_bipolar();
            b0 = 0.99886 * b0 + white * 0.0555179;
            b1 = 0.99332 * b1 + white * 0.0750759;
            b2 = 0.96900 * b2 + white * 0.1538520;
            b3 = 0.86650 * b3 + white * 0.3104856;
            b4 = 0.55000 * b4 + white * 0.5329522;
            b5 = -0.7616 * b5 - white * 0.0168980;
            let pink = b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362;
            b6 = white * 0.115926;
            amp * pink * 0.11
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Stubs required by later plugins (IMPACT, SEANCE, etc.). Real synthesis lands
// with the owning plugin; these keep the harness API stable and are intentionally
// simple placeholders (PRD §4).
// ---------------------------------------------------------------------------

/// STUB: synthetic kick placeholder — a decaying low sine. Replaced by IMPACT's
/// own math when that plugin is built.
pub fn synth_kick_stub(len: usize, sample_rate: f32) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / sample_rate;
            let env = (-t * 12.0).exp();
            let f = 55.0 + 80.0 * (-t * 40.0).exp();
            env * (2.0 * PI * f * t).sin()
        })
        .collect()
}

/// STUB: synthetic vocal placeholder — a sawtooth with light vibrato. Replaced by a
/// proper formant model (saw + 5 Hz vibrato through 3 band-passes) when needed.
pub fn synth_vocal_stub(freq: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / sample_rate;
            let vib = 1.0 + 0.01 * (2.0 * PI * 5.0 * t).sin();
            let phase = (freq * vib * t).fract();
            0.4 * (2.0 * phase - 1.0)
        })
        .collect()
}
