//! ITU-R BS.1770 loudness measurement — K-weighting + gated LUFS integrator.
//!
//! Reusable across the suite (OVERSEER's Master meter needs it; W7 REFERENCE-GAP will
//! reuse it later). Everything here is pure Rust, `no-alloc` after construction (the
//! integrated-gating block store is pre-reserved), and computed in `f64` internally for
//! meter-grade accuracy.
//!
//! # K-weighting
//! Two cascaded biquads, coefficients derived for the running sample rate the same way
//! `libebur128` does (bilinear-transformed analog prototypes), so the meter is correct at
//! 44.1 / 48 / 96 / 192 kHz — not just at the 48 kHz table values printed in the spec:
//! - **Stage 1** — a high-shelf "head" pre-filter (+~4 dB above ~1.5 kHz).
//! - **Stage 2** — the RLB high-pass (2nd-order, ~38 Hz).
//!
//! # Loudness
//! `L = -0.691 + 10·log10( Σ_ch G_ch · mean_square(K-weighted ch) )` (LUFS). Channel
//! weights `G` are 1.0 for L/R (surround weights are out of scope here). Momentary uses a
//! 400 ms window, short-term 3 s, and integrated applies the two-stage gating
//! (absolute −70 LUFS, then relative −10 LU) over 400 ms blocks with 75 % overlap.
//!
//! A test hook ([`LoudnessMeter::set_kweighting`]) disables the K-filters *and* the
//! −0.691 offset, turning the momentary reading into a plain mean-square level
//! (`10·log10(mean-square)` ≈ dBFS-RMS) so the meter can be checked against an analytic
//! value.

/// The K-weighting reference offset from BS.1770 (`-0.691 LU`).
pub const LUFS_OFFSET: f64 = -0.691;

/// A transposed-direct-form-II biquad in `f64`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// An identity (pass-through) biquad.
    pub fn identity() -> Self {
        Self {
            b0: 1.0,
            ..Self::default()
        }
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        // Transposed Direct Form II.
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Complex magnitude of the transfer function at normalized angular frequency `w`
    /// (rad/sample). Used to evaluate the K-weighting response analytically.
    pub fn magnitude(&self, w: f64) -> f64 {
        let (cos1, sin1) = (w.cos(), w.sin());
        let (cos2, sin2) = ((2.0 * w).cos(), (2.0 * w).sin());
        // Numerator b0 + b1 z^-1 + b2 z^-2 evaluated at z = e^{jw}.
        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = -(self.b1 * sin1 + self.b2 * sin2);
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = -(self.a1 * sin1 + self.a2 * sin2);
        let num = (num_re * num_re + num_im * num_im).sqrt();
        let den = (den_re * den_re + den_im * den_im).sqrt();
        if den <= 0.0 {
            0.0
        } else {
            num / den
        }
    }
}

/// Build the BS.1770 stage-1 high-shelf ("head") filter for `fs`.
pub fn shelf_biquad(fs: f64) -> Biquad {
    let f0 = 1681.974450955533_f64;
    let g = 3.999843853973347_f64;
    let q = 0.7071752369554196_f64;
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let vh = 10.0_f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    Biquad {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

/// Build the BS.1770 stage-2 RLB high-pass filter for `fs`.
pub fn highpass_biquad(fs: f64) -> Biquad {
    let f0 = 38.13547087602444_f64;
    let q = 0.5003270373238773_f64;
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    Biquad {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

/// A single-channel K-weighting cascade (shelf → high-pass).
#[derive(Clone, Copy, Debug)]
pub struct KWeight {
    shelf: Biquad,
    hp: Biquad,
    enabled: bool,
}

impl KWeight {
    pub fn new(fs: f32) -> Self {
        Self {
            shelf: shelf_biquad(fs as f64),
            hp: highpass_biquad(fs as f64),
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn reset(&mut self) {
        self.shelf.reset();
        self.hp.reset();
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        self.hp.process(self.shelf.process(x as f64)) as f32
    }
}

/// Linear magnitude of the K-weighting cascade at `freq` Hz for sample rate `fs`.
pub fn k_response(freq: f32, fs: f32) -> f32 {
    let w = 2.0 * std::f64::consts::PI * freq as f64 / fs as f64;
    (shelf_biquad(fs as f64).magnitude(w) * highpass_biquad(fs as f64).magnitude(w)) as f32
}

/// K-weighting magnitude in dB at `freq` Hz.
pub fn k_response_db(freq: f32, fs: f32) -> f32 {
    20.0 * k_response(freq, fs).max(1.0e-12).log10()
}

/// A sliding-window running sum over the summed instantaneous K-weighted power.
struct PowerWindow {
    ring: Vec<f64>,
    pos: usize,
    filled: usize,
    sum: f64,
}

impl PowerWindow {
    fn new(len: usize) -> Self {
        Self {
            ring: vec![0.0; len.max(1)],
            pos: 0,
            filled: 0,
            sum: 0.0,
        }
    }

    #[inline]
    fn push(&mut self, power: f64) {
        let old = self.ring[self.pos];
        self.ring[self.pos] = power;
        self.sum += power - old;
        self.pos += 1;
        if self.pos == self.ring.len() {
            self.pos = 0;
        }
        if self.filled < self.ring.len() {
            self.filled += 1;
        }
    }

    /// Mean power over the window (once it has filled; before that, over what exists).
    #[inline]
    fn mean(&self) -> f64 {
        if self.filled == 0 {
            0.0
        } else {
            (self.sum / self.filled as f64).max(0.0)
        }
    }

    fn reset(&mut self) {
        for v in self.ring.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
        self.filled = 0;
        self.sum = 0.0;
    }
}

/// A fixed-size log-power histogram over completed 400 ms block powers. Bounds the cost
/// of the integrated-loudness gating so it is **O(number of bins)** — independent of how
/// long the meter has been running — rather than an O(n) rescan of an ever-growing Vec on
/// every 100 ms step inside the audio callback (HARD CHECKPOINT 1, MINOR 6).
///
/// Each bin keeps the exact `f64` sum of the powers that fell in it, so gated averages are
/// numerically exact; only the *gate threshold* decision is quantised to the bin width
/// (0.05 dB → well under the ±0.5 LU tolerance).
struct PowerHistogram {
    counts: Vec<u64>,
    sums: Vec<f64>,
    lo_db: f64,
    bin_db: f64,
}

impl PowerHistogram {
    fn new() -> Self {
        // Power expressed as 10·log10(power) ("power dB"): cover −120..+20, 0.05 dB bins.
        let lo_db: f64 = -120.0;
        let hi_db: f64 = 20.0;
        let bin_db: f64 = 0.05;
        let nbins = (((hi_db - lo_db) / bin_db).round() as usize) + 1;
        Self {
            counts: vec![0; nbins],
            sums: vec![0.0; nbins],
            lo_db,
            bin_db,
        }
    }

    #[inline]
    fn bin_of(&self, power: f64) -> usize {
        let d = 10.0 * power.max(1.0e-30).log10();
        let idx = ((d - self.lo_db) / self.bin_db).floor() as isize;
        idx.clamp(0, self.counts.len() as isize - 1) as usize
    }

    #[inline]
    fn center_power(&self, bin: usize) -> f64 {
        let d = self.lo_db + (bin as f64 + 0.5) * self.bin_db;
        10.0_f64.powf(d / 10.0)
    }

    #[inline]
    fn add(&mut self, power: f64) {
        if power <= 0.0 {
            return;
        }
        let b = self.bin_of(power);
        self.counts[b] += 1;
        self.sums[b] += power;
    }

    fn clear(&mut self) {
        for c in self.counts.iter_mut() {
            *c = 0;
        }
        for s in self.sums.iter_mut() {
            *s = 0.0;
        }
    }

    /// Two-stage gated integrated power (absolute −70 LUFS, then relative −10 LU) folded
    /// with the K-weighting `offset` into a LUFS value.
    fn integrated_lufs(&self, offset: f64) -> f64 {
        let abs_gate = 10.0_f64.powf((-70.0 - offset) / 10.0);
        // Pass 1 — absolute gate: running sums over bins above the gate.
        let mut sum = 0.0;
        let mut count = 0u64;
        for b in 0..self.counts.len() {
            if self.counts[b] > 0 && self.center_power(b) > abs_gate {
                sum += self.sums[b];
                count += self.counts[b];
            }
        }
        if count == 0 {
            return f64::NEG_INFINITY;
        }
        let abs_mean = sum / count as f64;
        // Relative gate at abs_mean − 10 LU == abs_mean / 10 in linear power.
        let gate = abs_gate.max(abs_mean * 0.1);
        let mut sum2 = 0.0;
        let mut count2 = 0u64;
        for b in 0..self.counts.len() {
            if self.counts[b] > 0 && self.center_power(b) > gate {
                sum2 += self.sums[b];
                count2 += self.counts[b];
            }
        }
        if count2 == 0 {
            f64::NEG_INFINITY
        } else {
            offset + 10.0 * (sum2 / count2 as f64).log10()
        }
    }
}

/// Full BS.1770 loudness meter: momentary (400 ms), short-term (3 s), and gated
/// integrated loudness with reset. Feeds on per-sample channel frames.
pub struct LoudnessMeter {
    channels: Vec<KWeight>,
    momentary: PowerWindow,
    short: PowerWindow,
    kweighting: bool,

    /// When true (constructed via [`LoudnessMeter::new_momentary`]) the integrated /
    /// short-term gating machinery is skipped entirely — used by consumers that only read
    /// momentary loudness (e.g. OVERSEER Node), so `push` stays O(1) with no histogram
    /// bookkeeping in the audio callback.
    momentary_only: bool,

    // Integrated gating: 400 ms blocks with 75 % overlap (a new block every 100 ms step,
    // each covering the last 400 ms — read straight off the momentary window's mean).
    block_len: usize,
    step_len: usize,
    step_counter: usize,
    samples_seen: usize,
    // Bounded histogram of completed-block mean powers (see `PowerHistogram`): keeps the
    // integrated recompute O(bins), not O(elapsed time), inside `process`.
    hist: PowerHistogram,
    integrated_cache: f64,
}

impl LoudnessMeter {
    /// Build a full meter (momentary + short + gated integrated) for `channels` channels
    /// at `fs`.
    pub fn new(fs: f32, channels: usize) -> Self {
        Self::build(fs, channels, false)
    }

    /// Build a **momentary-only** meter: momentary loudness is tracked, but the short-term
    /// window and the integrated-gating histogram are inert, so `push` does no per-100 ms
    /// gating work. For consumers that only read [`momentary_lufs`](Self::momentary_lufs).
    pub fn new_momentary(fs: f32, channels: usize) -> Self {
        Self::build(fs, channels, true)
    }

    fn build(fs: f32, channels: usize, momentary_only: bool) -> Self {
        let fsf = fs.max(1.0) as f64;
        let ch = channels.max(1);
        let block_len = (0.4 * fsf).round() as usize;
        let step_len = (0.1 * fsf).round() as usize;
        // A momentary-only meter does not need the 3 s short window.
        let short_len = if momentary_only {
            1
        } else {
            (3.0 * fsf).round() as usize
        };
        Self {
            channels: (0..ch).map(|_| KWeight::new(fs)).collect(),
            momentary: PowerWindow::new((0.4 * fsf).round() as usize),
            short: PowerWindow::new(short_len),
            kweighting: true,
            momentary_only,
            block_len,
            step_len,
            step_counter: 0,
            samples_seen: 0,
            hist: PowerHistogram::new(),
            integrated_cache: f64::NEG_INFINITY,
        }
    }

    /// Enable/disable K-weighting. When disabled the −0.691 offset is also dropped, so a
    /// reading becomes a plain `10·log10(mean-square)` level (test hook, see module docs).
    pub fn set_kweighting(&mut self, enabled: bool) {
        self.kweighting = enabled;
        for c in self.channels.iter_mut() {
            c.set_enabled(enabled);
        }
    }

    #[inline]
    fn offset(&self) -> f64 {
        if self.kweighting {
            LUFS_OFFSET
        } else {
            0.0
        }
    }

    /// Feed one multichannel sample frame (`frame.len()` == channel count; extra channels
    /// ignored, missing channels treated as 0).
    #[inline]
    pub fn push(&mut self, frame: &[f32]) {
        let mut power = 0.0f64;
        for (i, c) in self.channels.iter_mut().enumerate() {
            let x = frame.get(i).copied().unwrap_or(0.0);
            let w = c.process(x) as f64;
            power += w * w; // G = 1.0 for L/R
        }
        self.momentary.push(power);
        if self.momentary_only {
            return;
        }
        self.short.push(power);

        // Integrated gating: every 100 ms, once at least one full 400 ms block exists,
        // record the 400 ms mean power (the momentary window already holds exactly that)
        // into the bounded histogram and refresh the (now O(bins)) integrated value.
        self.samples_seen += 1;
        self.step_counter += 1;
        if self.step_counter >= self.step_len {
            self.step_counter = 0;
            if self.samples_seen >= self.block_len {
                let mean = self.momentary.mean();
                self.hist.add(mean);
                self.recompute_integrated();
            }
        }
    }

    /// Momentary loudness (LUFS) over the last 400 ms.
    pub fn momentary_lufs(&self) -> f32 {
        Self::mean_to_lufs(self.momentary.mean(), self.offset())
    }

    /// Short-term loudness (LUFS) over the last 3 s.
    pub fn short_lufs(&self) -> f32 {
        Self::mean_to_lufs(self.short.mean(), self.offset())
    }

    /// Gated integrated loudness (LUFS) since the last [`reset`](Self::reset).
    pub fn integrated_lufs(&self) -> f32 {
        self.integrated_cache as f32
    }

    #[inline]
    fn mean_to_lufs(mean: f64, offset: f64) -> f32 {
        if mean <= 1.0e-12 {
            f32::NEG_INFINITY
        } else {
            (offset + 10.0 * mean.log10()) as f32
        }
    }

    /// Two-stage gated integration (BS.1770-4) over the histogram of 400 ms block powers.
    /// O(number of histogram bins), independent of elapsed time.
    fn recompute_integrated(&mut self) {
        let offset = self.offset();
        self.integrated_cache = self.hist.integrated_lufs(offset);
    }

    /// Reset every window, filter, and the integrated history.
    pub fn reset(&mut self) {
        for c in self.channels.iter_mut() {
            c.reset();
        }
        self.momentary.reset();
        self.short.reset();
        self.step_counter = 0;
        self.samples_seen = 0;
        self.hist.clear();
        self.integrated_cache = f64::NEG_INFINITY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, rms: f32, len: usize, fs: f32) -> Vec<f32> {
        let amp = rms * 2.0f32.sqrt(); // peak for a given RMS
        (0..len)
            .map(|n| amp * (2.0 * PI * freq * n as f32 / fs).sin())
            .collect()
    }

    #[test]
    fn unweighted_momentary_equals_rms_level() {
        // A -20 dBFS-RMS sine, K-weighting disabled → momentary == 10log10(meansq) = -20.
        let fs = 48_000.0f32;
        let rms = 10f32.powf(-20.0 / 20.0); // 0.1
        let x = sine(997.0, rms, (fs * 2.0) as usize, fs);
        let mut m = LoudnessMeter::new(fs, 1);
        m.set_kweighting(false);
        for &s in &x {
            m.push(&[s]);
        }
        let lu = m.momentary_lufs();
        assert!((lu - (-20.0)).abs() < 0.1, "unweighted momentary {lu} != -20.0");
    }

    #[test]
    fn weighted_momentary_matches_analytic_k_response() {
        // Self-consistency: the meter's momentary reading of a -20 dBFS-RMS 997 Hz sine
        // must equal the analytic value computed from this module's own K-filter response.
        let fs = 48_000.0f32;
        let rms = 10f32.powf(-20.0 / 20.0);
        let meansq = (rms as f64) * (rms as f64);
        let x = sine(997.0, rms, (fs * 2.0) as usize, fs);
        let mut m = LoudnessMeter::new(fs, 1);
        for &s in &x {
            m.push(&[s]);
        }
        let kmag = k_response(997.0, fs) as f64;
        let analytic = LUFS_OFFSET + 10.0 * (kmag * kmag * meansq).log10();
        let lu = m.momentary_lufs() as f64;
        assert!(
            (lu - analytic).abs() < 0.5,
            "weighted momentary {lu:.3} vs analytic {analytic:.3} (K={:.3} dB)",
            k_response_db(997.0, fs)
        );
    }

    #[test]
    fn integrated_tracks_momentary_for_steady_tone() {
        let fs = 48_000.0f32;
        let rms = 10f32.powf(-23.0 / 20.0);
        let x = sine(1_000.0, rms, (fs * 4.0) as usize, fs);
        let mut m = LoudnessMeter::new(fs, 1);
        for &s in &x {
            m.push(&[s]);
        }
        let integ = m.integrated_lufs();
        let mom = m.momentary_lufs();
        assert!(integ.is_finite(), "integrated is not finite");
        assert!(
            (integ - mom).abs() < 0.6,
            "integrated {integ} should track momentary {mom} on a steady tone"
        );
    }

    #[test]
    fn momentary_only_matches_full_momentary_and_skips_integrated() {
        // A momentary-only meter must read the same momentary loudness as a full meter,
        // while leaving integrated at -inf (no gating work done).
        let fs = 48_000.0f32;
        let rms = 10f32.powf(-23.0 / 20.0);
        let x = sine(1_000.0, rms, (fs * 2.0) as usize, fs);
        let mut full = LoudnessMeter::new(fs, 1);
        let mut mom = LoudnessMeter::new_momentary(fs, 1);
        for &s in &x {
            full.push(&[s]);
            mom.push(&[s]);
        }
        assert!(
            (full.momentary_lufs() - mom.momentary_lufs()).abs() < 1e-4,
            "momentary-only {} != full {}",
            mom.momentary_lufs(),
            full.momentary_lufs()
        );
        assert!(
            !mom.integrated_lufs().is_finite(),
            "momentary-only meter should not compute integrated loudness"
        );
        assert!(full.integrated_lufs().is_finite());
    }

    #[test]
    fn k_response_is_near_flat_around_1k_and_rolls_off_lows() {
        let fs = 48_000.0f32;
        // High-pass kills sub-bass.
        assert!(k_response_db(20.0, fs) < -10.0);
        // Shelf lifts highs by ~4 dB.
        assert!(k_response_db(10_000.0, fs) > 3.0);
    }
}
