//! Streaming STFT engine (PRD/SPECS: EMBER, reused by SMUDGE / SEANCE / CARVE / DRIFT).
//!
//! Design goals:
//! - **Streaming, sample-in / sample-out** with a fixed, reported latency so it drops
//!   straight into a per-sample `process` loop.
//! - **Callback per frame** exposing the mutable complex spectrum (`&mut [Complex<f32>]`,
//!   length `fft_size/2 + 1`). Callers read magnitude/phase, rewrite the bins, and the
//!   engine resynthesizes them. This is the most general primitive (a phase-vocoder or a
//!   magnitude-only op both fit).
//! - **Window-sum-compensated overlap-add** (WOLA): the same window is applied on
//!   analysis and synthesis (Hann²), and the overlap sum is normalized to unity so an
//!   identity callback reconstructs the input (a delayed copy).
//! - **Allocation-free** in [`Stft::process`] (all scratch preallocated in [`Stft::new`]).
//!   Safe under nih-plug's `assert_process_allocs`.
//!
//! Latency: with a symmetric analysis/synthesis window and the front-emit OLA scheme
//! below, the algorithmic latency is exactly `fft_size` samples — see
//! [`Stft::latency`] and the `identity_latency_is_fft_size` test.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// A streaming short-time Fourier transform with per-frame callback resynthesis.
pub struct Stft {
    n: usize,
    hop: usize,
    window: Vec<f32>,
    /// Combined synthesis scale: (1 / n) for the unnormalized inverse FFT × (1 / COLA)
    /// window-overlap compensation. Applied together with the synthesis window.
    synth_scale: f32,

    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,

    /// Sliding analysis buffer: index 0 = oldest, `n-1` = newest of the last `n` samples.
    analysis: Vec<f32>,
    /// Windowed frame / inverse-FFT time output scratch (length `n`).
    time: Vec<f32>,
    /// Complex spectrum handed to the callback (length `n/2 + 1`).
    spectrum: Vec<Complex<f32>>,
    /// Overlap-add accumulator (length `n`); front `hop` samples finalize each frame.
    ola: Vec<f32>,
    /// Output staging: the `hop` finalized samples emitted over the next hop period.
    out_stage: Vec<f32>,

    fwd_scratch: Vec<Complex<f32>>,
    inv_scratch: Vec<Complex<f32>>,

    /// Count of input samples within the current hop (0..hop).
    fill: usize,
}

impl Stft {
    /// Create an STFT with `fft_size` (power of two recommended) and `hop` (must divide
    /// `fft_size` for constant-overlap-add; e.g. 2048 / 512). Uses a periodic Hann window.
    pub fn new(fft_size: usize, hop: usize) -> Self {
        assert!(fft_size >= 4 && hop >= 1 && hop <= fft_size, "invalid STFT geometry");
        let n = fft_size;

        // Periodic Hann window (DFT-even): w[i] = 0.5 - 0.5 cos(2π i / N).
        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();

        // COLA compensation for applying the window on BOTH analysis and synthesis
        // (i.e. w²). In steady state the squared-window contributions of all overlapping
        // frames sum to a constant; compute it and invert. Circular accumulation gives
        // the exact steady-state sum for a hop that divides n.
        let mut acc = vec![0.0f32; n];
        let mut start = 0usize;
        while start < n {
            for j in 0..n {
                acc[(start + j) % n] += window[j] * window[j];
            }
            start += hop;
        }
        let cola = acc.iter().copied().sum::<f32>() / n as f32;
        let synth_scale = 1.0 / (n as f32 * cola.max(1.0e-12));

        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let spectrum = fwd.make_output_vec();
        let fwd_scratch = fwd.make_scratch_vec();
        let inv_scratch = inv.make_scratch_vec();

        Self {
            n,
            hop,
            window,
            synth_scale,
            fwd,
            inv,
            analysis: vec![0.0; n],
            time: vec![0.0; n],
            spectrum,
            ola: vec![0.0; n],
            out_stage: vec![0.0; hop],
            fwd_scratch,
            inv_scratch,
            fill: 0,
        }
    }

    /// FFT size (number of bins in the callback spectrum is `fft_size/2 + 1`).
    #[inline]
    pub fn fft_size(&self) -> usize {
        self.n
    }

    /// Hop size in samples.
    #[inline]
    pub fn hop(&self) -> usize {
        self.hop
    }

    /// Number of frequency bins the callback receives (`fft_size/2 + 1`).
    #[inline]
    pub fn num_bins(&self) -> usize {
        self.n / 2 + 1
    }

    /// Constant end-to-end latency of the streaming transform, in samples (`fft_size`).
    #[inline]
    pub fn latency(&self) -> usize {
        self.n
    }

    /// Bin-center frequency (Hz) for bin `k` at `sample_rate`.
    #[inline]
    pub fn bin_freq(&self, k: usize, sample_rate: f32) -> f32 {
        k as f32 * sample_rate / self.n as f32
    }

    /// Clear all internal state (history, overlap-add, staging).
    pub fn reset(&mut self) {
        for v in self.analysis.iter_mut() {
            *v = 0.0;
        }
        for v in self.ola.iter_mut() {
            *v = 0.0;
        }
        for v in self.out_stage.iter_mut() {
            *v = 0.0;
        }
        self.fill = 0;
    }

    /// Push one input sample and return one output sample (delayed by [`Self::latency`]).
    /// Every `hop` samples the internal frame fires: analysis-window → forward FFT →
    /// `callback(&mut spectrum)` → inverse FFT → synthesis-window → overlap-add.
    ///
    /// Allocation-free. `callback` must keep `spectrum.len()` unchanged.
    #[inline]
    pub fn process<F: FnMut(&mut [Complex<f32>])>(&mut self, x: f32, callback: &mut F) -> f32 {
        let idx = self.fill;
        // Newest hop of input occupies the tail region [n-hop .. n) of the analysis buffer.
        self.analysis[self.n - self.hop + idx] = x;
        let y = self.out_stage[idx];

        self.fill += 1;
        if self.fill == self.hop {
            self.fill = 0;
            self.run_frame(callback);
            // Slide the analysis history left by one hop so the next hop of input appends
            // to the tail while the previous n-hop samples are retained.
            self.analysis.copy_within(self.hop.., 0);
        }
        y
    }

    #[inline]
    fn run_frame<F: FnMut(&mut [Complex<f32>])>(&mut self, callback: &mut F) {
        // Analysis window.
        for i in 0..self.n {
            self.time[i] = self.analysis[i] * self.window[i];
        }
        // Forward transform (consumes `time` as scratch).
        self.fwd
            .process_with_scratch(&mut self.time, &mut self.spectrum, &mut self.fwd_scratch)
            .expect("realfft forward");

        // User frame op.
        callback(&mut self.spectrum);

        // A real inverse requires the DC and Nyquist bins to be purely real.
        let last = self.spectrum.len() - 1;
        self.spectrum[0].im = 0.0;
        self.spectrum[last].im = 0.0;

        // Inverse transform → time domain (consumes spectrum as scratch).
        self.inv
            .process_with_scratch(&mut self.spectrum, &mut self.time, &mut self.inv_scratch)
            .expect("realfft inverse");

        // Synthesis window + normalization, overlap-add.
        for i in 0..self.n {
            self.ola[i] += self.time[i] * self.window[i] * self.synth_scale;
        }

        // Emit the finalized front hop, then slide the OLA accumulator down by one hop.
        self.out_stage.copy_from_slice(&self.ola[..self.hop]);
        self.ola.copy_within(self.hop.., 0);
        for v in self.ola[self.n - self.hop..].iter_mut() {
            *v = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An identity callback (spectrum untouched) must reconstruct the input delayed by
    /// exactly `fft_size` samples, at unity gain (WOLA reconstruction).
    #[test]
    fn identity_latency_is_fft_size() {
        let n = 2048usize;
        let hop = 512usize;
        let mut stft = Stft::new(n, hop);
        let sr = 48_000.0f32;

        // A 1 kHz sine, long enough to fill the pipeline.
        let len = n * 8;
        let input: Vec<f32> = (0..len)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / sr).sin())
            .collect();

        let mut id = |_s: &mut [Complex<f32>]| {};
        let out: Vec<f32> = input.iter().map(|&x| stft.process(x, &mut id)).collect();

        // Compare steady-state region: out[i] ≈ input[i - latency].
        let lat = stft.latency();
        assert_eq!(lat, n);
        let mut max_err = 0.0f32;
        for i in (lat + n)..(len - 1) {
            max_err = max_err.max((out[i] - input[i - lat]).abs());
        }
        assert!(max_err < 1.0e-3, "WOLA identity error too large: {max_err}");
    }

    /// Impulse in → the reconstructed impulse peak sits at exactly `latency` samples.
    #[test]
    fn impulse_peak_at_reported_latency() {
        let n = 2048usize;
        let hop = 512usize;
        let mut stft = Stft::new(n, hop);
        let len = n * 6;
        let mut input = vec![0.0f32; len];
        // Put the impulse a few hops in so the analysis buffer is aligned.
        let imp_at = n;
        input[imp_at] = 1.0;

        let mut id = |_s: &mut [Complex<f32>]| {};
        let out: Vec<f32> = input.iter().map(|&x| stft.process(x, &mut id)).collect();

        let (peak_idx, _) = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .unwrap();
        assert_eq!(peak_idx - imp_at, stft.latency(), "impulse delay != reported latency");
    }

    /// A magnitude-only callback (zeroing phase → keep real part) must still be finite and
    /// bounded — exercises the callback path and DC/Nyquist handling.
    #[test]
    fn callback_path_is_finite() {
        let n = 1024usize;
        let mut stft = Stft::new(n, 256);
        let len = n * 6;
        let mut cb = |spec: &mut [Complex<f32>]| {
            for b in spec.iter_mut() {
                let m = (b.re * b.re + b.im * b.im).sqrt();
                *b = Complex::new(m, 0.0);
            }
        };
        for i in 0..len {
            let x = 0.3 * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin();
            let y = stft.process(x, &mut cb);
            assert!(y.is_finite());
        }
    }
}
