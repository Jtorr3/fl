//! One RBJ (Audio-EQ-Cookbook) biquad section in Direct Form I. Copied from `overseer::eq`
//! (the suite's proven shelf/bell implementation — kept local per the crate-boundary rule, with
//! a unity-peak band-pass added for VOXFIT's de-ess / dynamic-bell subtract paths). Coefficients
//! are recomputed only when a band changes (block rate); `process` is a cheap DF-I step.

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

    /// Constant-0-dB-peak band-pass at `f0` Hz, quality `q` (RBJ "BPF, constant 0 dB peak gain").
    /// Its output is unity at the centre so `x − k·bandpass(x)` is a well-behaved dynamic bell cut.
    pub fn bandpass_0db(&mut self, f0: f32, q: f32, fs: f32) {
        let w0 = 2.0 * PI * f0.clamp(10.0, fs * 0.49) / fs;
        let (sw, cw) = (w0.sin(), w0.cos());
        let alpha = sw / (2.0 * q.max(0.05));
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cw;
        let a2 = 1.0 - alpha;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_shelves_are_transparent() {
        let mut lo = Biquad::default();
        let mut hi = Biquad::default();
        lo.low_shelf(1000.0, 0.0, 0.7, 48_000.0);
        hi.high_shelf(1000.0, 0.0, 0.7, 48_000.0);
        let mut max_dev = 0.0f32;
        for n in 0..2000 {
            let x = (n as f32 * 0.01).sin();
            let y = hi.process(lo.process(x));
            max_dev = max_dev.max((y - x).abs());
        }
        assert!(max_dev < 1e-3, "flat shelves deviated by {max_dev}");
    }

    #[test]
    fn bandpass_unity_at_center() {
        let fs = 48_000.0f32;
        let fc = 3162.0f32;
        let mut bp = Biquad::default();
        bp.bandpass_0db(fc, 1.05, fs);
        // Drive a sine at the centre frequency; steady-state peak should be ~1.0 (0 dB).
        let mut peak = 0.0f32;
        for n in 0..48_000 {
            let x = (2.0 * PI * fc * n as f32 / fs).sin();
            let y = bp.process(x);
            if n > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - 1.0).abs() < 0.05, "bandpass centre gain {peak} not ~1.0");
    }
}
