//! Pitch detection (PRD/SPECS: TRACER; PLUCK and CHORALE reuse this).
//!
//! [`Mpm`] implements the **McLeod Pitch Method** — the normalized square difference
//! function (NSDF), key-maximum peak picking, and parabolic interpolation — producing
//! an `(f0_hz, confidence)` pair. [`PitchTracker`] wraps it with a decimating front-end
//! (~12 kHz analysis rate) and the SPECS post-processing chain (median-of-5, ±35-cent
//! hysteresis, Hz/ms slew limiting) for a clean streaming, sample-in API.
//!
//! Both are allocation-free after construction (all scratch preallocated), so the tracker
//! is safe to drive from a nih-plug `process` loop under `assert_process_allocs`.

use crate::dsp::Svf;

/// Result of one pitch analysis frame.
#[derive(Clone, Copy, Debug)]
pub struct PitchResult {
    /// Estimated fundamental in Hz (`0.0` when no pitch found).
    pub f0_hz: f32,
    /// Clarity / confidence in `0..1` (interpolated NSDF peak height).
    pub confidence: f32,
}

/// Cents between two frequencies (positive if `b > a`).
#[inline]
pub fn cents(a: f32, b: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        0.0
    } else {
        1200.0 * (b / a).log2()
    }
}

/// McLeod Pitch Method analyzer. Operates on a mono buffer at a fixed analysis sample
/// rate. `analyze` is allocation-free (NSDF + peak scratch preallocated in [`Mpm::new`]).
pub struct Mpm {
    sr: f32,
    window: usize,
    min_lag: usize,
    max_lag: usize,
    nsdf: Vec<f32>,
    peak_lags: Vec<usize>,
    peak_vals: Vec<f32>,
    /// Fraction of the global NSDF maximum a key peak must reach to be chosen (MPM `k`).
    peak_pick: f32,
}

impl Mpm {
    /// Create an analyzer over `window` samples at `sr` Hz, searching `f0_min..f0_max`.
    pub fn new(window: usize, sr: f32, f0_min: f32, f0_max: f32) -> Self {
        let window = window.max(64);
        let sr = sr.max(1.0);
        let min_lag = ((sr / f0_max.max(1.0)).floor() as usize).max(2);
        let max_lag = ((sr / f0_min.max(1.0)).ceil() as usize)
            .min(window - 2)
            .max(min_lag + 1);
        Self {
            sr,
            window,
            min_lag,
            max_lag,
            nsdf: vec![0.0; max_lag + 1],
            peak_lags: vec![0; max_lag + 2],
            peak_vals: vec![0.0; max_lag + 2],
            peak_pick: 0.85,
        }
    }

    /// Analysis window length (samples).
    #[inline]
    pub fn window(&self) -> usize {
        self.window
    }

    /// Estimate `(f0, confidence)` from a mono buffer of at least [`Self::window`] samples.
    pub fn analyze(&mut self, buf: &[f32]) -> PitchResult {
        let none = PitchResult {
            f0_hz: 0.0,
            confidence: 0.0,
        };
        let w = self.window.min(buf.len());
        if w < self.min_lag + 4 {
            return none;
        }
        // Silence guard: no pitch in near-zero energy.
        let energy: f32 = buf[..w].iter().map(|v| v * v).sum();
        if energy < 1.0e-7 {
            return none;
        }
        let max_lag = self.max_lag.min(w - 2);

        // --- NSDF via the type-II autocorrelation and the running square sum m'(τ) ---
        for tau in 0..=max_lag {
            let mut acf = 0.0f32;
            let mut m = 0.0f32;
            for j in 0..(w - tau) {
                let a = buf[j];
                let b = buf[j + tau];
                acf += a * b;
                m += a * a + b * b;
            }
            self.nsdf[tau] = if m > 1.0e-12 { 2.0 * acf / m } else { 0.0 };
        }

        // --- Collect "key maxima": one maximum per positive lobe of the NSDF ---
        let mut count = 0usize;
        let mut i = 1usize;
        // Skip the τ=0 lobe (NSDF starts at 1) until the first descent through zero.
        while i < max_lag && self.nsdf[i] > 0.0 {
            i += 1;
        }
        while i < max_lag {
            if self.nsdf[i] > 0.0 && self.nsdf[i - 1] <= 0.0 {
                // Entered a positive lobe; track its maximum until it goes negative.
                let mut cur_i = i;
                let mut cur_v = self.nsdf[i];
                i += 1;
                while i < max_lag && self.nsdf[i] > 0.0 {
                    if self.nsdf[i] > cur_v {
                        cur_v = self.nsdf[i];
                        cur_i = i;
                    }
                    i += 1;
                }
                if cur_i >= self.min_lag {
                    self.peak_lags[count] = cur_i;
                    self.peak_vals[count] = cur_v;
                    count += 1;
                }
            } else {
                i += 1;
            }
        }
        if count == 0 {
            return none;
        }

        // Global max of the key maxima, then choose the FIRST peak clearing k·global.
        let mut global_max = 0.0f32;
        for k in 0..count {
            if self.peak_vals[k] > global_max {
                global_max = self.peak_vals[k];
            }
        }
        if global_max <= 0.0 {
            return none;
        }
        let threshold = self.peak_pick * global_max;
        let mut chosen_lag = self.peak_lags[0];
        for k in 0..count {
            if self.peak_vals[k] >= threshold {
                chosen_lag = self.peak_lags[k];
                break;
            }
        }

        // --- Parabolic interpolation around the chosen lag ---
        let t = chosen_lag;
        let (tau, clarity) = if t >= 1 && t + 1 <= max_lag {
            let a = self.nsdf[t - 1];
            let b = self.nsdf[t];
            let c = self.nsdf[t + 1];
            let denom = a - 2.0 * b + c;
            let delta = if denom.abs() > 1.0e-9 {
                0.5 * (a - c) / denom
            } else {
                0.0
            };
            // Interpolated peak height (vertex of the fitted parabola).
            let peak = b - 0.25 * (a - c) * delta;
            (t as f32 + delta, peak)
        } else {
            (t as f32, self.nsdf[t])
        };

        if tau <= 0.0 {
            return none;
        }
        PitchResult {
            f0_hz: self.sr / tau,
            confidence: clarity.clamp(0.0, 1.0),
        }
    }
}

/// Streaming pitch tracker: decimate → MPM → median-of-5 → ±35-cent hysteresis →
/// Hz/ms slew. Feed input-rate samples with [`PitchTracker::push`]; read the smoothed
/// pitch with [`PitchTracker::f0`] and the gated confidence with
/// [`PitchTracker::confidence`]. When a MIDI note is set the detector is bypassed.
pub struct PitchTracker {
    input_sr: f32,
    decim: usize,
    aa: Svf,
    decim_count: usize,

    ring: Vec<f32>,
    frame: Vec<f32>,
    write: usize,
    fill: usize,
    hop: usize,
    since_hop: usize,

    mpm: Mpm,

    med_f0: [f32; 5],
    med_conf: [f32; 5],
    med_pos: usize,
    med_n: usize,

    conf_gate: f32,
    default_f0: f32,
    held_f0: f32,
    locked_f0: f32,
    target_f0: f32,
    current_f0: f32,
    confidence: f32,
    slew_hz_per_ms: f32,

    midi_f0: Option<f32>,
}

impl PitchTracker {
    /// Build a tracker at input `sample_rate`. Analysis runs on a ~12 kHz decimated
    /// stream over a 1024-sample window. `default_f0` is the frozen pitch used before the
    /// first confident detection and whenever confidence drops (SPECS freeze rule).
    pub fn new(sample_rate: f32, default_f0: f32) -> Self {
        let input_sr = sample_rate.max(1.0);
        // Decimate to ~12 kHz.
        let decim = ((input_sr / 12_000.0).round() as usize).max(1);
        let analysis_sr = input_sr / decim as f32;
        let window = 1024usize;
        let hop = window / 4;

        let mut aa = Svf::new();
        // Anti-alias low-pass below the decimated Nyquist.
        aa.set((analysis_sr * 0.45).min(input_sr * 0.45), 0.707, input_sr);

        let mpm = Mpm::new(window, analysis_sr, 45.0, 1200.0);
        let default_f0 = default_f0.clamp(20.0, 5000.0);

        Self {
            input_sr,
            decim,
            aa,
            decim_count: 0,
            ring: vec![0.0; window],
            frame: vec![0.0; window],
            write: 0,
            fill: 0,
            hop,
            since_hop: 0,
            mpm,
            med_f0: [default_f0; 5],
            med_conf: [0.0; 5],
            med_pos: 0,
            med_n: 0,
            conf_gate: 0.6,
            default_f0,
            held_f0: default_f0,
            locked_f0: default_f0,
            target_f0: default_f0,
            current_f0: default_f0,
            confidence: 0.0,
            slew_hz_per_ms: 200.0,
            midi_f0: None,
        }
    }

    /// Slew limit in Hz per millisecond (SPECS post-processing).
    pub fn set_slew(&mut self, hz_per_ms: f32) {
        self.slew_hz_per_ms = hz_per_ms.max(0.001);
    }

    /// Confidence gate below which crossovers freeze (SPECS: `< 0.6`).
    pub fn set_confidence_gate(&mut self, gate: f32) {
        self.conf_gate = gate.clamp(0.0, 1.0);
    }

    /// Enter MIDI mode: the detector is bypassed and `note_hz` drives the pitch
    /// (still slew-limited). Pass `None` to return to audio detection.
    pub fn set_midi_note(&mut self, note_hz: Option<f32>) {
        self.midi_f0 = note_hz.map(|f| f.clamp(20.0, 5000.0));
    }

    /// Current smoothed fundamental (Hz).
    #[inline]
    pub fn f0(&self) -> f32 {
        self.current_f0
    }

    /// Current gated confidence in `0..1`.
    #[inline]
    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    /// Reset all state to the default pitch.
    pub fn reset(&mut self) {
        self.aa.reset();
        self.decim_count = 0;
        for v in self.ring.iter_mut() {
            *v = 0.0;
        }
        self.write = 0;
        self.fill = 0;
        self.since_hop = 0;
        self.med_f0 = [self.default_f0; 5];
        self.med_conf = [0.0; 5];
        self.med_pos = 0;
        self.med_n = 0;
        self.held_f0 = self.default_f0;
        self.locked_f0 = self.default_f0;
        self.target_f0 = self.default_f0;
        self.current_f0 = self.default_f0;
        self.confidence = 0.0;
    }

    /// Feed one input-rate sample. Runs an analysis at hop boundaries and advances the
    /// per-sample slew toward the current target pitch.
    #[inline]
    pub fn push(&mut self, x: f32) {
        // Anti-alias + decimate into the analysis ring.
        let lp = self.aa.process(x).lp;
        self.decim_count += 1;
        if self.decim_count >= self.decim {
            self.decim_count = 0;
            let n = self.ring.len();
            self.ring[self.write] = lp;
            self.write = (self.write + 1) % n;
            if self.fill < n {
                self.fill += 1;
            }
            self.since_hop += 1;
            if self.since_hop >= self.hop && self.fill >= n {
                self.since_hop = 0;
                self.run_analysis();
            }
        }

        // Per-sample slew toward the target (MIDI or detected).
        let target = if let Some(f) = self.midi_f0 {
            self.confidence = 1.0;
            f
        } else {
            self.target_f0
        };
        let max_step = self.slew_hz_per_ms * 1000.0 / self.input_sr;
        let d = (target - self.current_f0).clamp(-max_step, max_step);
        self.current_f0 = (self.current_f0 + d).clamp(20.0, 8000.0);
    }

    fn run_analysis(&mut self) {
        // Copy the ring into a contiguous, time-ordered frame (oldest → newest).
        let n = self.ring.len();
        for i in 0..n {
            self.frame[i] = self.ring[(self.write + i) % n];
        }
        let r = self.mpm.analyze(&self.frame);

        // Median-of-5 on both f0 and confidence.
        self.med_f0[self.med_pos] = if r.f0_hz > 0.0 { r.f0_hz } else { self.locked_f0 };
        self.med_conf[self.med_pos] = r.confidence;
        self.med_pos = (self.med_pos + 1) % 5;
        if self.med_n < 5 {
            self.med_n += 1;
        }
        let med_f0 = median5(&self.med_f0);
        let med_conf = median5(&self.med_conf);

        if self.midi_f0.is_some() {
            // MIDI mode drives pitch elsewhere; keep detector state coherent but idle.
            self.confidence = 1.0;
            return;
        }

        let confident = med_conf >= self.conf_gate && med_f0 >= 20.0 && med_f0 <= 5000.0;
        if confident {
            // ±35-cent hysteresis: ignore sub-threshold wobble, otherwise re-lock.
            if cents(self.held_f0, med_f0).abs() > 35.0 {
                self.held_f0 = med_f0;
            }
            self.locked_f0 = self.held_f0;
            self.target_f0 = self.held_f0;
        } else {
            // Freeze at the last confident value (SPECS: confidence < gate).
            self.target_f0 = self.locked_f0;
        }
        self.confidence = med_conf;
    }
}

/// Median of five values (allocation-free; does not mutate the input).
#[inline]
fn median5(src: &[f32; 5]) -> f32 {
    let mut a = *src;
    // Insertion sort of five elements.
    for i in 1..5 {
        let v = a[i];
        let mut j = i;
        while j > 0 && a[j - 1] > v {
            a[j] = a[j - 1];
            j -= 1;
        }
        a[j] = v;
    }
    a[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsig;

    #[test]
    fn mpm_finds_sine_pitch() {
        let sr = 12_000.0f32;
        let f = 220.0f32;
        let buf: Vec<f32> = (0..2048)
            .map(|n| (std::f32::consts::TAU * f * n as f32 / sr).sin())
            .collect();
        let mut mpm = Mpm::new(1024, sr, 45.0, 1200.0);
        let r = mpm.analyze(&buf[..1024]);
        assert!(
            (r.f0_hz - f).abs() < 3.0,
            "MPM f0 {} not near {f}",
            r.f0_hz
        );
        assert!(r.confidence > 0.9, "low clarity {}", r.confidence);
    }

    #[test]
    fn mpm_finds_saw_pitch() {
        let sr = 12_000.0f32;
        let f = 130.0f32;
        let buf: Vec<f32> = (0..2048)
            .map(|n| {
                let ph = (f * n as f32 / sr).fract();
                2.0 * ph - 1.0
            })
            .collect();
        let mut mpm = Mpm::new(1024, sr, 45.0, 1200.0);
        let r = mpm.analyze(&buf[..1024]);
        assert!((r.f0_hz - f).abs() < 4.0, "saw f0 {} not near {f}", r.f0_hz);
        assert!(r.confidence > 0.8);
    }

    #[test]
    fn mpm_low_clarity_on_noise() {
        let noise = testsig::white_noise(0.8, 4096, 12345);
        let mut mpm = Mpm::new(1024, 12_000.0, 45.0, 1200.0);
        let r = mpm.analyze(&noise[..1024]);
        assert!(
            r.confidence < 0.6,
            "white noise should have low clarity, got {}",
            r.confidence
        );
    }

    #[test]
    fn tracker_locks_onto_steady_sine_and_freezes_on_noise() {
        let sr = 48_000.0f32;
        let mut t = PitchTracker::new(sr, 110.0);
        // 1 s of a 200 Hz sine → should lock near 200 Hz with good confidence.
        for n in 0..(sr as usize) {
            let x = (std::f32::consts::TAU * 200.0 * n as f32 / sr).sin();
            t.push(x);
        }
        assert!((t.f0() - 200.0).abs() < 6.0, "tracker f0 {} not near 200", t.f0());
        assert!(t.confidence() >= 0.6);

        // Now feed white noise: pitch must freeze (not wander), confidence collapses.
        let noise = testsig::white_noise(0.7, sr as usize, 999);
        let frozen = t.f0();
        let mut maxdev = 0.0f32;
        for &x in &noise {
            t.push(x);
            maxdev = maxdev.max((t.f0() - frozen).abs());
        }
        // f0 should stay essentially at the last confident value.
        assert!(maxdev < 2.0, "pitch drifted {maxdev} Hz on noise (freeze failed)");
    }

    #[test]
    fn tracker_freeze_holds_default_on_pure_noise() {
        let sr = 48_000.0f32;
        let mut t = PitchTracker::new(sr, 120.0);
        let noise = testsig::white_noise(0.8, (sr * 1.0) as usize, 7);
        for &x in &noise {
            t.push(x);
        }
        // Never confident → cutoffs source frozen at the default.
        assert!((t.f0() - 120.0).abs() < 1.0, "noise moved frozen pitch: {}", t.f0());
        assert!(t.confidence() < 0.6);
    }
}
