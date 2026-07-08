//! Dynamics for OVERSEER: a feed-forward compressor (Node + Master bands), a 3-band
//! multiband compressor built on LR4 splits, and a lookahead brickwall limiter.
//!
//! All pure Rust, allocation-free after construction, shared between the plugin `process`
//! path and the offline harness tests.

use suite_core::dsp::{Oversampler4x, Svf};

const EPS: f32 = 1.0e-9;

#[inline]
fn coef_ms(time_ms: f32, fs: f32) -> f32 {
    let s = (time_ms * 0.001) * fs;
    if s <= 0.0 {
        0.0
    } else {
        (-1.0 / s).exp()
    }
}

#[inline]
fn lin_to_db(x: f32) -> f32 {
    20.0 * (x.max(EPS)).log10()
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Feed-forward compressor with RMS detection, soft knee, attack/release, and makeup.
/// Computes a linear gain from a detector sample; the caller applies it to the audio
/// (channel-linked by feeding a summed/max detector).
#[derive(Clone, Copy, Debug)]
pub struct Compressor {
    // Detector (mean-square one-pole).
    ms: f32,
    det_coef: f32,
    // Smoothed gain reduction in dB (<= 0).
    gr_db: f32,
    atk: f32,
    rel: f32,
    // Static curve.
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    makeup_db: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        Self {
            ms: 0.0,
            det_coef: 0.0,
            gr_db: 0.0,
            atk: 0.0,
            rel: 0.0,
            threshold_db: 0.0,
            ratio: 2.0,
            knee_db: 6.0,
            makeup_db: 0.0,
        }
    }
}

impl Compressor {
    pub fn new(fs: f32) -> Self {
        let mut c = Self::default();
        c.det_coef = coef_ms(10.0, fs); // 10 ms RMS window
        c.configure(-18.0, 2.0, 6.0, 10.0, 120.0, 0.0, fs);
        c
    }

    #[allow(clippy::too_many_arguments)]
    pub fn configure(
        &mut self,
        threshold_db: f32,
        ratio: f32,
        knee_db: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_db: f32,
        fs: f32,
    ) {
        self.threshold_db = threshold_db;
        self.ratio = ratio.max(1.0);
        self.knee_db = knee_db.max(0.0);
        self.makeup_db = makeup_db;
        self.atk = coef_ms(attack_ms, fs);
        self.rel = coef_ms(release_ms, fs);
        // Attack-honesty (SOUND-PASS FIX-3): the RMS detector window used to be a fixed 10 ms,
        // which floored the realized attack near ~15 ms regardless of the displayed value — a
        // fast-attack setting did essentially nothing. Tie the detector time to the attack so a
        // fast attack is actually fast, while capping at 10 ms keeps slow settings free of
        // low-frequency detector ripple. Floors at 0.5 ms.
        let det_ms = attack_ms.clamp(0.5, 10.0);
        self.det_coef = coef_ms(det_ms, fs);
    }

    pub fn reset(&mut self) {
        self.ms = 0.0;
        self.gr_db = 0.0;
    }

    /// Current gain reduction in dB (negative), for metering.
    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    /// Feed a detector sample, return the linear gain to apply (includes makeup).
    #[inline]
    pub fn process(&mut self, detector: f32) -> f32 {
        // RMS detector.
        self.ms = detector * detector + self.det_coef * (self.ms - detector * detector);
        let level_db = lin_to_db(self.ms.max(0.0).sqrt());

        // Soft-knee static gain computer (downward compression).
        let over = level_db - self.threshold_db;
        let target_gr = if 2.0 * over < -self.knee_db {
            0.0
        } else if 2.0 * over > self.knee_db && self.knee_db > 0.0 {
            over * (1.0 / self.ratio - 1.0)
        } else if self.knee_db <= 0.0 {
            if over > 0.0 {
                over * (1.0 / self.ratio - 1.0)
            } else {
                0.0
            }
        } else {
            let x = over + self.knee_db * 0.5;
            (1.0 / self.ratio - 1.0) * x * x / (2.0 * self.knee_db)
        };

        // Attack/release smoothing in dB (moving toward more gain reduction = attack).
        let c = if target_gr < self.gr_db { self.atk } else { self.rel };
        self.gr_db = target_gr + c * (self.gr_db - target_gr);

        db_to_lin(self.gr_db + self.makeup_db)
    }
}

/// One LR4 (4th-order Linkwitz-Riley) crossover section built from two cascaded
/// Butterworth TPT-SVF stages. Splits an input into a lowpass and highpass output that
/// sum to a flat magnitude.
#[derive(Clone, Copy, Debug, Default)]
struct Lr4 {
    lp1: Svf,
    lp2: Svf,
    hp1: Svf,
    hp2: Svf,
}

impl Lr4 {
    fn set(&mut self, cutoff: f32, fs: f32) {
        let q = std::f32::consts::FRAC_1_SQRT_2; // Butterworth
        self.lp1.set(cutoff, q, fs);
        self.lp2.set(cutoff, q, fs);
        self.hp1.set(cutoff, q, fs);
        self.hp2.set(cutoff, q, fs);
    }

    fn reset(&mut self) {
        self.lp1.reset();
        self.lp2.reset();
        self.hp1.reset();
        self.hp2.reset();
    }

    /// Returns (low, high).
    #[inline]
    fn split(&mut self, x: f32) -> (f32, f32) {
        let low = self.lp2.process(self.lp1.process(x).lp).lp;
        let high = self.hp2.process(self.hp1.process(x).hp).hp;
        (low, high)
    }
}

/// 3-band multiband compressor: two LR4 crossovers → low / mid / high, each with its own
/// [`Compressor`], summed back. One instance per channel; detection is per-band on that
/// channel (linking across channels handled by the caller if desired).
#[derive(Clone, Copy, Debug, Default)]
pub struct MultibandComp {
    xo_low: Lr4,
    xo_high: Lr4,
    pub comps: [Compressor; 3],
}

impl MultibandComp {
    pub fn new(fs: f32) -> Self {
        Self {
            xo_low: Lr4::default(),
            xo_high: Lr4::default(),
            comps: [Compressor::new(fs), Compressor::new(fs), Compressor::new(fs)],
        }
    }

    pub fn set_crossovers(&mut self, low_hz: f32, high_hz: f32, fs: f32) {
        // Fuzzer-extreme guard: keep the clamp bounds ordered for every input —
        // `f32::clamp` PANICS on inverted min/max, and pluginval fuzzes params to the
        // range edges (xo_low = 20 kHz at 44.1 kHz makes `lo + 10 > 0.45·fs`).
        let max_hz = (fs * 0.45).max(100.0);
        let lo = low_hz.clamp(30.0, max_hz - 20.0);
        let hi = high_hz.clamp(lo + 10.0, max_hz);
        self.xo_low.set(lo, fs);
        self.xo_high.set(hi, fs);
    }

    pub fn reset(&mut self) {
        self.xo_low.reset();
        self.xo_high.reset();
        for c in self.comps.iter_mut() {
            c.reset();
        }
    }

    /// Process one sample: split into 3 bands, compress each, sum.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let (low, rest) = self.xo_low.split(x);
        let (mid, high) = self.xo_high.split(rest);
        let gl = self.comps[0].process(low);
        let gm = self.comps[1].process(mid);
        let gh = self.comps[2].process(high);
        low * gl + mid * gm + high * gh
    }
}

/// A sliding maximum over a fixed window, implemented with a monotonic deque of
/// (value, index) pairs. Pre-reserved so `push` never allocates.
struct SlidingMax {
    // Monotonic-decreasing deque stored in a ring buffer of (value, expiry_index).
    vals: Vec<f32>,
    idxs: Vec<u64>,
    head: usize,
    tail: usize,
    len: usize,
    cap: usize,
    window: usize,
    counter: u64,
}

impl SlidingMax {
    fn new(window: usize) -> Self {
        let cap = window + 2;
        Self {
            vals: vec![0.0; cap],
            idxs: vec![0; cap],
            head: 0,
            tail: 0,
            len: 0,
            cap,
            window: window.max(1),
            counter: 0,
        }
    }

    fn reset(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.len = 0;
        self.counter = 0;
    }

    #[inline]
    fn push_back(&mut self, v: f32, i: u64) {
        self.vals[self.tail] = v;
        self.idxs[self.tail] = i;
        self.tail = (self.tail + 1) % self.cap;
        self.len += 1;
    }

    #[inline]
    fn pop_back(&mut self) {
        self.tail = (self.tail + self.cap - 1) % self.cap;
        self.len -= 1;
    }

    #[inline]
    fn back(&self) -> f32 {
        let i = (self.tail + self.cap - 1) % self.cap;
        self.vals[i]
    }

    #[inline]
    fn front_idx(&self) -> u64 {
        self.idxs[self.head]
    }

    /// Push a new sample, return the maximum over the last `window` samples.
    #[inline]
    fn push(&mut self, x: f32) -> f32 {
        let i = self.counter;
        self.counter += 1;
        // Drop from back while smaller-or-equal (keep monotonic decreasing).
        while self.len > 0 && self.back() <= x {
            self.pop_back();
        }
        self.push_back(x, i);
        // Expire from front.
        while self.len > 0 && self.front_idx() + self.window as u64 <= i {
            self.head = (self.head + 1) % self.cap;
            self.len -= 1;
        }
        self.vals[self.head]
    }
}

/// Lookahead brickwall limiter. A `lookahead`-sample delay line lets the gain envelope
/// reach the required attenuation before a peak arrives at the output; a sliding maximum
/// over the lookahead window makes the reduction anticipatory (no overshoot on attack),
/// and a final ceiling clamp guarantees the output never exceeds the ceiling.
pub struct Limiter {
    lookahead: usize,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    dpos: usize,
    smax: SlidingMax,
    gain: f32,
    atk: f32,
    rel: f32,
    ceiling: f32,
    gr_db: f32,
    // True-peak (inter-sample) detection: the peak that drives the gain is the 4x-oversampled
    // peak of the incoming signal, not the raw sample peak, so inter-sample overs are caught.
    // These live in the *sidechain only* — the audio path and its reported latency are
    // unchanged (the oversampler's ~22-sample group delay stays inside the 96-sample lookahead
    // window, so the anticipatory gain still lands before the peak reaches the output).
    tp_l: Oversampler4x,
    tp_r: Oversampler4x,
}

impl Limiter {
    pub fn new(fs: f32) -> Self {
        let lookahead = ((0.002 * fs).round() as usize).max(1); // 2 ms
        // Attack settles well within the lookahead window; release is musical.
        let atk = coef_ms(0.05, fs); // fast, but the sliding-max is what prevents overshoot
        let rel = coef_ms(100.0, fs);
        Self {
            lookahead,
            delay_l: vec![0.0; lookahead + 1],
            delay_r: vec![0.0; lookahead + 1],
            dpos: 0,
            smax: SlidingMax::new(lookahead),
            gain: 1.0,
            atk,
            rel,
            ceiling: db_to_lin(-1.0),
            gr_db: 0.0,
            tp_l: Oversampler4x::new(),
            tp_r: Oversampler4x::new(),
        }
    }

    pub fn lookahead_samples(&self) -> usize {
        self.lookahead
    }

    pub fn set_ceiling_db(&mut self, ceiling_db: f32) {
        self.ceiling = db_to_lin(ceiling_db);
    }

    pub fn set_release_ms(&mut self, release_ms: f32, fs: f32) {
        self.rel = coef_ms(release_ms.max(1.0), fs);
    }

    pub fn gain_reduction_db(&self) -> f32 {
        self.gr_db
    }

    pub fn reset(&mut self) {
        for v in self.delay_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.delay_r.iter_mut() {
            *v = 0.0;
        }
        self.dpos = 0;
        self.smax.reset();
        self.gain = 1.0;
        self.gr_db = 0.0;
        self.tp_l.reset();
        self.tp_r.reset();
    }

    /// Process one stereo sample, returning the limited (delayed) pair.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        // True-peak of the *incoming* (undelayed) sample across channels: oversample each
        // channel 4x and take the largest inter-sample magnitude. This makes the limiter
        // catch inter-sample overs that a raw sample-peak detector would miss.
        let mut tp_l = 0.0f32;
        self.tp_l.process(l, |u| {
            tp_l = tp_l.max(u.abs());
            u
        });
        let mut tp_r = 0.0f32;
        self.tp_r.process(r, |u| {
            tp_r = tp_r.max(u.abs());
            u
        });
        let peak_in = tp_l.max(tp_r);
        let win_peak = self.smax.push(peak_in);

        // Gain required so the loudest sample within the lookahead window hits the ceiling.
        let needed = if win_peak > self.ceiling {
            self.ceiling / win_peak.max(EPS)
        } else {
            1.0
        };
        // Anticipatory smoothing: attack downward fast, release upward slow.
        let c = if needed < self.gain { self.atk } else { self.rel };
        self.gain = needed + c * (self.gain - needed);

        // Read the delayed sample (delay == lookahead) and apply the gain.
        let read = (self.dpos + self.delay_l.len() - self.lookahead) % self.delay_l.len();
        let dl = self.delay_l[read];
        let dr = self.delay_r[read];
        self.delay_l[self.dpos] = l;
        self.delay_r[self.dpos] = r;
        self.dpos = (self.dpos + 1) % self.delay_l.len();

        let mut out_l = dl * self.gain;
        let mut out_r = dr * self.gain;
        // Final guarantee: never exceed the ceiling (catches any residual overshoot).
        out_l = out_l.clamp(-self.ceiling, self.ceiling);
        out_r = out_r.clamp(-self.ceiling, self.ceiling);

        self.gr_db = lin_to_db(self.gain);
        (out_l, out_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn compressor_reduces_gain_above_threshold() {
        let fs = 48_000.0;
        let mut c = Compressor::new(fs);
        c.configure(-24.0, 4.0, 6.0, 5.0, 80.0, 0.0, fs);
        let mut g = 1.0;
        for n in 0..48_000 {
            let x = 0.5 * (2.0 * PI * 1_000.0 * n as f32 / fs).sin(); // ~-9 dBFS RMS
            g = c.process(x);
        }
        assert!(g < 0.9, "compressor did not reduce gain: {g}");
    }

    #[test]
    fn limiter_holds_ceiling_on_hot_sine() {
        // +6 dBFS sine (amplitude 2.0), ceiling -1 dBFS → output peak <= ceiling + 0.1 dB.
        let fs = 48_000.0f32;
        let mut lim = Limiter::new(fs);
        lim.set_ceiling_db(-1.0);
        let mut peak = 0.0f32;
        for n in 0..(fs as usize) {
            let x = 2.0 * (2.0 * PI * 220.0 * n as f32 / fs).sin();
            let (ol, _) = lim.process(x, x);
            if n > (fs as usize) / 2 {
                peak = peak.max(ol.abs());
            }
        }
        let peak_db = lin_to_db(peak);
        assert!(
            peak_db <= -1.0 + 0.1,
            "limiter overshoot: peak {peak_db:.3} dBFS > ceiling -1 + 0.1"
        );
        assert!(peak_db > -3.0, "limiter over-attenuated: {peak_db:.3}");
    }

    #[test]
    fn sliding_max_matches_bruteforce() {
        let mut sm = SlidingMax::new(5);
        let xs = [0.1, 0.9, 0.3, 0.2, 0.5, 0.1, 0.05, 0.8, 0.2, 0.1, 0.0, 0.4];
        let mut got = Vec::new();
        for &x in &xs {
            got.push(sm.push(x));
        }
        for (i, &g) in got.iter().enumerate() {
            let lo = if i >= 4 { i - 4 } else { 0 };
            let brute = xs[lo..=i].iter().cloned().fold(0.0f32, f32::max);
            assert!((g - brute).abs() < 1e-6, "at {i}: got {g} want {brute}");
        }
    }

    #[test]
    fn set_crossovers_survives_fuzzer_extremes_at_44k() {
        // Regression: pluginval fuzzes xo_low to 20 kHz at fs=44.1 kHz, where a naive
        // clamp gets inverted bounds and panics.
        let fs = 44_100.0f32;
        let mut mb = MultibandComp::new(fs);
        for &(lo, hi) in &[
            (20_000.0f32, 20.0f32),
            (20_000.0, 20_000.0),
            (20.0, 20.0),
            (0.0, 0.0),
            (20_000.0, 0.0),
        ] {
            mb.set_crossovers(lo, hi, fs);
            let y = mb.process(0.5);
            assert!(y.is_finite());
        }
    }

    #[test]
    fn multiband_is_bounded_and_reconstructs_flat_at_unity() {
        let fs = 48_000.0f32;
        let mut mb = MultibandComp::new(fs);
        mb.set_crossovers(200.0, 2000.0, fs);
        // Ratio 1 → no compression → bands should sum to ~flat magnitude.
        for c in mb.comps.iter_mut() {
            c.configure(0.0, 1.0, 0.0, 5.0, 80.0, 0.0, fs);
        }
        let mut peak = 0.0f32;
        for n in 0..9600 {
            let x = 0.5 * (2.0 * PI * 1_000.0 * n as f32 / fs).sin();
            let y = mb.process(x);
            if n > 4800 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak.is_finite() && peak > 0.3 && peak < 0.7, "reconstruction peak {peak}");
    }
}
