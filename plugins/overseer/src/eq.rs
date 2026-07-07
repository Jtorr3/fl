//! Shared 4-band EQ for OVERSEER (used by both the Node strip and the Master bus).
//!
//! Low shelf → bell → bell → high shelf, each an RBJ (Audio-EQ-Cookbook) biquad in Direct
//! Form I. Coefficients are recomputed only when a band's settings change (cheap, at block
//! rate). Processes one channel per call; the caller keeps one [`FourBandEq`] per channel.

use std::f32::consts::PI;

/// One RBJ biquad section (Direct Form I).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

impl Biquad {
    fn set_coeffs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        let inv = 1.0 / a0;
        self.b0 = b0 * inv;
        self.b1 = b1 * inv;
        self.b2 = b2 * inv;
        self.a1 = a1 * inv;
        self.a2 = a2 * inv;
    }

    /// Low-shelf at `f0` Hz, `gain_db`, shelf slope `s` (1.0 = max-flat).
    pub fn low_shelf(&mut self, f0: f32, gain_db: f32, s: f32, fs: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0.clamp(10.0, fs * 0.49) / fs;
        let (sw, cw) = (w0.sin(), w0.cos());
        let alpha = sw / 2.0 * ((a + 1.0 / a) * (1.0 / s.max(0.1) - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let b0 = a * ((a + 1.0) - (a - 1.0) * cw + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cw);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cw - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cw + two_sqrt_a_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cw);
        let a2 = (a + 1.0) + (a - 1.0) * cw - two_sqrt_a_alpha;
        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    /// High-shelf at `f0` Hz, `gain_db`, shelf slope `s`.
    pub fn high_shelf(&mut self, f0: f32, gain_db: f32, s: f32, fs: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0.clamp(10.0, fs * 0.49) / fs;
        let (sw, cw) = (w0.sin(), w0.cos());
        let alpha = sw / 2.0 * ((a + 1.0 / a) * (1.0 / s.max(0.1) - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let b0 = a * ((a + 1.0) + (a - 1.0) * cw + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cw);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cw - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cw + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cw);
        let a2 = (a + 1.0) - (a - 1.0) * cw - two_sqrt_a_alpha;
        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    /// Peaking bell at `f0` Hz, `gain_db`, quality `q`.
    pub fn peaking(&mut self, f0: f32, gain_db: f32, q: f32, fs: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0.clamp(10.0, fs * 0.49) / fs;
        let (sw, cw) = (w0.sin(), w0.cos());
        let alpha = sw / (2.0 * q.max(0.05));
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cw;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cw;
        let a2 = 1.0 - alpha / a;
        self.set_coeffs(b0, b1, b2, a0, a1, a2);
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Per-band settings for the 4-band EQ.
#[derive(Clone, Copy, Debug)]
pub struct EqSettings {
    pub low_freq: f32,
    pub low_gain: f32,
    pub b1_freq: f32,
    pub b1_gain: f32,
    pub b1_q: f32,
    pub b2_freq: f32,
    pub b2_gain: f32,
    pub b2_q: f32,
    pub high_freq: f32,
    pub high_gain: f32,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            low_freq: 90.0,
            low_gain: 0.0,
            b1_freq: 300.0,
            b1_gain: 0.0,
            b1_q: 0.9,
            b2_freq: 2500.0,
            b2_gain: 0.0,
            b2_q: 0.9,
            high_freq: 9000.0,
            high_gain: 0.0,
        }
    }
}

/// A 4-band EQ for one channel: low shelf → bell → bell → high shelf.
#[derive(Clone, Copy, Debug, Default)]
pub struct FourBandEq {
    low: Biquad,
    b1: Biquad,
    b2: Biquad,
    high: Biquad,
}

impl FourBandEq {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recompute all four sections' coefficients from `s` and `fs` (call at block rate).
    pub fn configure(&mut self, s: &EqSettings, fs: f32) {
        self.low.low_shelf(s.low_freq, s.low_gain, 0.7, fs);
        self.b1.peaking(s.b1_freq, s.b1_gain, s.b1_q, fs);
        self.b2.peaking(s.b2_freq, s.b2_gain, s.b2_q, fs);
        self.high.high_shelf(s.high_freq, s.high_gain, 0.7, fs);
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.b1.reset();
        self.b2.reset();
        self.high.reset();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.high
            .process(self.b2.process(self.b1.process(self.low.process(x))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_eq_is_transparent() {
        let mut eq = FourBandEq::new();
        eq.configure(&EqSettings::default(), 48_000.0);
        // 0 dB on every band → essentially unity.
        let mut max_dev = 0.0f32;
        for n in 0..2000 {
            let x = (n as f32 * 0.01).sin();
            let y = eq.process(x);
            max_dev = max_dev.max((y - x).abs());
        }
        assert!(max_dev < 1e-3, "flat EQ deviated by {max_dev}");
    }

    #[test]
    fn low_shelf_boosts_dc_region() {
        let mut eq = FourBandEq::new();
        let mut s = EqSettings::default();
        s.low_gain = 12.0;
        eq.configure(&s, 48_000.0);
        // A very low tone should be boosted (~+12 dB → ~4x amplitude).
        let fs = 48_000.0f32;
        let mut peak = 0.0f32;
        for n in 0..48_000 {
            let x = 0.25 * (2.0 * std::f32::consts::PI * 20.0 * n as f32 / fs).sin();
            let y = eq.process(x);
            if n > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak > 0.7, "low shelf did not boost: peak {peak}");
    }
}
