//! EMBER — pure-DSP core for the spectral fader / temporal smoother (SPECS "EMBER").
//!
//! ```text
//! in ─ STFT(2048, hop 512, Hann) ─ per-bin state machine ─ fitting ─ iSTFT/OLA ─ mix ─ out
//!               factor-band curves: attack(f), decay(f)  (log-freq, 8 editable breakpoints)
//! ```
//!
//! Per bin `k`, each STFT frame (hop time `T = hop/sr`):
//!   `state[k] += coef · (in_mag[k] − state[k])`
//! with `coef = 1 − exp(−T/τ)` and `τ` chosen by whether the bin is rising (attack) or
//! falling (decay); τ interpolated across 8 log-frequency breakpoints. Decay τ runs up to
//! 60 s so spectral tails keep ringing long after the input stops. **Freeze** sets τ→∞
//! (coef 0) so the captured spectrum is held indefinitely.
//!
//! Phase strategy (keeps tails tonal): while a bin's input magnitude is above the gate,
//! the output phase locks to the measured input phase and the per-hop phase advance is
//! recorded. Once the bin falls silent (generated tail), the output phase is advanced by
//! that recorded per-hop increment — a phase-vocoder advance — so the ringing tail stays
//! coherent at the bin's tonal frequency instead of smearing.
//!
//! Fitting blends each bin toward a ~1/3-octave spectral-envelope moving average.
//!
//! This module is API-agnostic pure Rust, shared verbatim between the nih-plug `process`
//! path and the offline harness tests. All scratch is preallocated — the per-sample path
//! is allocation-free (safe under nih-plug's `assert_process_allocs`).

use std::f32::consts::{PI, TAU};
use suite_core::stft::{Complex, Stft};

pub const FFT_SIZE: usize = 2048;
pub const HOP: usize = 512;
pub const N_BANDS: usize = 8;
/// Factor-band breakpoint frequencies span this log-frequency range.
pub const F_LO: f32 = 20.0;
pub const F_HI: f32 = 20_000.0;

/// Reference bin magnitude for a 0 dBFS spectral component (≈ Hann peak-bin magnitude of
/// a full-scale sine: `sum(window)/2 = n/4`). The gate threshold is relative to this.
const REF_MAG: f32 = FFT_SIZE as f32 * 0.25;

/// 2^(1/6): half of a 1/3-octave, used for the fitting envelope window in bin index.
const SIXTH_OCT: f32 = 1.122_462_f32;

/// A full snapshot of EMBER's controls (plain, un-normalized values). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Attack time constant per factor band (ms), low→high frequency.
    pub attack_ms: [f32; N_BANDS],
    /// Decay time constant per factor band (ms, up to 60 000 = 60 s), low→high frequency.
    pub decay_ms: [f32; N_BANDS],
    /// Fitting amount 0..1 — blend bins toward the 1/3-oct spectral envelope.
    pub fitting: f32,
    /// Freeze: hold the captured spectrum (τ→∞).
    pub freeze: bool,
    /// Gate threshold (dB relative to a full-scale component). Bins whose input magnitude
    /// is above this lock to input phase; below it they become phase-vocoder tails.
    pub gate_db: f32,
    /// Extra gain applied to generated-tail bins (dB).
    pub tail_gain_db: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            attack_ms: [20.0; N_BANDS],
            decay_ms: [800.0; N_BANDS],
            fitting: 0.0,
            freeze: false,
            gate_db: -60.0,
            tail_gain_db: 0.0,
            mix: 1.0,
        }
    }
}

/// Interpolate a per-band time constant (ms) at frequency `f` across the 8 log-frequency
/// breakpoints. Interpolation is linear in log-frequency and log-time (smooth, monotone).
fn interp_band(bands: &[f32; N_BANDS], f: f32) -> f32 {
    let f = f.clamp(F_LO, F_HI);
    let pos =
        (f.ln() - F_LO.ln()) / (F_HI.ln() - F_LO.ln()) * (N_BANDS - 1) as f32;
    let j = (pos.floor() as usize).min(N_BANDS - 2);
    let frac = (pos - j as f32).clamp(0.0, 1.0);
    let a = bands[j].max(1.0e-3).ln();
    let b = bands[j + 1].max(1.0e-3).ln();
    (a + frac * (b - a)).exp()
}

/// Derived, per-bin configuration recomputed at block rate from a [`Settings`].
struct Cfg {
    atk_coef: Vec<f32>,
    dec_coef: Vec<f32>,
    gate_lin: f32,
    tail_gain: f32,
    fitting: f32,
    freeze: bool,
}

impl Cfg {
    fn new(num_bins: usize) -> Self {
        Self {
            atk_coef: vec![0.0; num_bins],
            dec_coef: vec![0.0; num_bins],
            gate_lin: 0.0,
            tail_gain: 1.0,
            fitting: 0.0,
            freeze: false,
        }
    }

    fn update(&mut self, s: &Settings, bin_freq: &[f32], sr: f32) {
        let hop_time = HOP as f32 / sr;
        for (k, &f) in bin_freq.iter().enumerate() {
            let atk_tau = interp_band(&s.attack_ms, f) * 1.0e-3;
            let dec_tau = interp_band(&s.decay_ms, f) * 1.0e-3;
            self.atk_coef[k] = 1.0 - (-hop_time / atk_tau.max(1.0e-6)).exp();
            self.dec_coef[k] = 1.0 - (-hop_time / dec_tau.max(1.0e-6)).exp();
        }
        self.gate_lin = REF_MAG * 10.0_f32.powf(s.gate_db / 20.0);
        self.tail_gain = 10.0_f32.powf(s.tail_gain_db / 20.0);
        self.fitting = s.fitting.clamp(0.0, 1.0);
        self.freeze = s.freeze;
    }
}

/// Per-bin state for one channel.
struct ChanState {
    state: Vec<f32>,
    out_phase: Vec<f32>,
    prev_phase: Vec<f32>,
    freq_est: Vec<f32>,
}

impl ChanState {
    fn new(nb: usize) -> Self {
        Self {
            state: vec![0.0; nb],
            out_phase: vec![0.0; nb],
            prev_phase: vec![0.0; nb],
            freq_est: vec![0.0; nb],
        }
    }
    fn reset(&mut self) {
        for v in self.state.iter_mut() {
            *v = 0.0;
        }
        for v in self.out_phase.iter_mut() {
            *v = 0.0;
        }
        for v in self.prev_phase.iter_mut() {
            *v = 0.0;
        }
        for v in self.freq_est.iter_mut() {
            *v = 0.0;
        }
    }
}

/// Reused per-frame scratch (shared across L/R since channels are processed sequentially).
struct Scratch {
    out_mag: Vec<f32>,
    active: Vec<bool>,
    prefix: Vec<f32>,
}

impl Scratch {
    fn new(nb: usize) -> Self {
        Self {
            out_mag: vec![0.0; nb],
            active: vec![false; nb],
            prefix: vec![0.0; nb + 1],
        }
    }
}

/// One STFT channel plus its per-bin state.
struct EmberChan {
    stft: Stft,
    st: ChanState,
}

impl EmberChan {
    fn new(nb: usize) -> Self {
        Self {
            stft: Stft::new(FFT_SIZE, HOP),
            st: ChanState::new(nb),
        }
    }
    fn reset(&mut self) {
        self.stft.reset();
        self.st.reset();
    }
    #[inline]
    fn process(&mut self, x: f32, cfg: &Cfg, scr: &mut Scratch) -> f32 {
        let EmberChan { stft, st } = self;
        let mut cb = |spec: &mut [Complex<f32>]| frame(spec, st, cfg, scr);
        stft.process(x, &mut cb)
    }
}

/// Wrap a phase to (−π, π].
#[inline]
fn wrap(mut p: f32) -> f32 {
    while p > PI {
        p -= TAU;
    }
    while p < -PI {
        p += TAU;
    }
    p
}

/// The per-frame spectral op: magnitude state machine + phase-vocoder tails + fitting.
fn frame(spec: &mut [Complex<f32>], st: &mut ChanState, cfg: &Cfg, scr: &mut Scratch) {
    let nb = spec.len();

    // --- Pass 1: magnitude state machine + phase tracking / vocoder advance -----------
    for k in 0..nb {
        let re = spec[k].re;
        let im = spec[k].im;
        let mag = (re * re + im * im).sqrt();
        let ph = im.atan2(re);

        let active = mag > cfg.gate_lin && !cfg.freeze;
        scr.active[k] = active;

        let coef = if cfg.freeze {
            0.0
        } else if active {
            if mag > st.state[k] {
                cfg.atk_coef[k]
            } else {
                cfg.dec_coef[k]
            }
        } else {
            // Generated tail: keep decaying toward the (silent) input.
            cfg.dec_coef[k]
        };
        st.state[k] += coef * (mag - st.state[k]);

        let dphi = wrap(ph - st.prev_phase[k]);
        st.prev_phase[k] = ph;
        if active {
            st.freq_est[k] = dphi;
            st.out_phase[k] = ph;
        } else {
            // Phase-vocoder advance at the last-measured tonal rate → coherent tail.
            st.out_phase[k] = wrap(st.out_phase[k] + st.freq_est[k]);
        }
    }

    // --- Fitting: blend toward the ~1/3-octave spectral-envelope moving average --------
    if cfg.fitting > 1.0e-4 {
        scr.prefix[0] = 0.0;
        for k in 0..nb {
            scr.prefix[k + 1] = scr.prefix[k] + st.state[k];
        }
        for k in 0..nb {
            let lo = ((k as f32) / SIXTH_OCT).floor() as usize;
            let hi = (((k as f32) * SIXTH_OCT).ceil() as usize).min(nb - 1);
            let cnt = (hi - lo + 1) as f32;
            let env = (scr.prefix[hi + 1] - scr.prefix[lo]) / cnt;
            scr.out_mag[k] = st.state[k] + cfg.fitting * (env - st.state[k]);
        }
    } else {
        for k in 0..nb {
            scr.out_mag[k] = st.state[k];
        }
    }

    // --- Pass 2: reconstruct complex bins ---------------------------------------------
    for k in 0..nb {
        let mut m = scr.out_mag[k];
        if !scr.active[k] {
            m *= cfg.tail_gain;
        }
        let ph = st.out_phase[k];
        spec[k] = Complex::new(m * ph.cos(), m * ph.sin());
    }
}

/// A short delay line used to align the dry path with the STFT's reported latency.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }
    #[inline]
    fn push(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }
}

/// EMBER's full stereo DSP core.
pub struct EmberCore {
    sr: f32,
    num_bins: usize,
    bin_freq: Vec<f32>,
    cfg: Cfg,
    chan_l: EmberChan,
    chan_r: EmberChan,
    scr: Scratch,
    dry_l: Delay,
    dry_r: Delay,
}

impl EmberCore {
    pub fn new(sample_rate: f32) -> Self {
        let nb = FFT_SIZE / 2 + 1;
        let sr = sample_rate.max(1.0);
        // Bin-center frequencies (bin 0 clamped up to F_LO for the band interpolation).
        let bin_freq: Vec<f32> = (0..nb)
            .map(|k| (k as f32 * sr / FFT_SIZE as f32).max(F_LO))
            .collect();
        Self {
            sr,
            num_bins: nb,
            bin_freq,
            cfg: Cfg::new(nb),
            chan_l: EmberChan::new(nb),
            chan_r: EmberChan::new(nb),
            scr: Scratch::new(nb),
            dry_l: Delay::new(FFT_SIZE),
            dry_r: Delay::new(FFT_SIZE),
        }
    }

    /// Latency (samples) this core adds — equal to the STFT frame size.
    pub fn latency(&self) -> usize {
        FFT_SIZE
    }

    pub fn reset(&mut self) {
        self.chan_l.reset();
        self.chan_r.reset();
        self.dry_l.reset();
        self.dry_r.reset();
    }

    /// Recompute derived per-bin config from a settings snapshot (call at block rate).
    pub fn configure(&mut self, s: &Settings) {
        self.cfg.update(s, &self.bin_freq, self.sr);
    }

    /// Process one stereo sample. `mix` is passed per-sample so it can be smoothed.
    #[inline]
    pub fn process_sample(&mut self, l: f32, r: f32, mix: f32) -> (f32, f32) {
        let wet_l = self.chan_l.process(l, &self.cfg, &mut self.scr);
        let wet_r = self.chan_r.process(r, &self.cfg, &mut self.scr);
        let dry_l = self.dry_l.push(l);
        let dry_r = self.dry_r.push(r);
        let m = mix.clamp(0.0, 1.0);
        (dry_l + m * (wet_l - dry_l), dry_r + m * (wet_r - dry_r))
    }

    /// Offline mono convenience for the harness: process `buf` in place through the core
    /// with a fixed `Settings`. Returns nothing (in-place).
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        self.configure(s);
        for x in buf.iter_mut() {
            let (y, _) = self.process_sample(*x, *x, s.mix);
            *x = y;
        }
    }

    pub fn num_bins(&self) -> usize {
        self.num_bins
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        if x.is_empty() {
            return 0.0;
        }
        (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }
    fn db(x: f32) -> f32 {
        20.0 * x.max(1.0e-12).log10()
    }

    /// τ=10 s decay: after a noise burst stops, the tail at +2 s stays above −40 dBFS and
    /// frame-RMS decreases (nearly) monotonically. This is EMBER's core done-bar.
    #[test]
    fn long_decay_tail_persists_and_decays_monotonically() {
        let sr = 48_000.0f32;
        let mut s = Settings::default();
        s.attack_ms = [10.0; N_BANDS];
        s.decay_ms = [10_000.0; N_BANDS]; // 10 s
        s.mix = 1.0;

        let burst_len = (sr * 1.0) as usize;
        let tail_len = (sr * 3.0) as usize;
        let total = burst_len + tail_len;

        // White-noise burst then silence.
        let mut rng = suite_core::testsig::Rng::new(1234);
        let mut input = vec![0.0f32; total];
        for v in input.iter_mut().take(burst_len) {
            *v = 0.4 * rng.next_bipolar();
        }

        let mut core = EmberCore::new(sr);
        core.configure(&s);
        let mut out = vec![0.0f32; total];
        for i in 0..total {
            let (y, _) = core.process_sample(input[i], input[i], s.mix);
            out[i] = y;
        }

        // Tail energy at +2 s after input stops (over a 0.25 s window), accounting for the
        // 2048-sample latency.
        let lat = core.latency();
        let probe = burst_len + lat + (sr * 2.0) as usize;
        let win = (sr * 0.25) as usize;
        let tail_rms_db = db(rms(&out[probe..(probe + win).min(total)]));
        assert!(
            tail_rms_db > -40.0,
            "tail at +2 s was {tail_rms_db:.1} dBFS (need > -40)"
        );

        // Frame-RMS over the tail (post-input, post-latency) must be monotone-decreasing
        // within a ±1 dB tolerance.
        let tail_start = burst_len + lat;
        let frame = (sr * 0.25) as usize;
        let mut prev = f32::INFINITY;
        let mut frames = 0;
        let mut i = tail_start;
        while i + frame <= total {
            let r = db(rms(&out[i..i + frame]));
            if prev.is_finite() {
                assert!(
                    r <= prev + 1.0,
                    "tail frame RMS rose {:.2}->{:.2} dB (>1 dB) at frame {frames}",
                    prev,
                    r
                );
            }
            prev = r;
            frames += 1;
            i += frame;
        }
        assert!(frames >= 4, "not enough tail frames measured: {frames}");
    }

    /// Freeze: after capturing a tone, the held tail RMS is flat within ±1 dB over 5 s.
    #[test]
    fn freeze_holds_flat_tail() {
        let sr = 48_000.0f32;
        let mut s = Settings::default();
        s.attack_ms = [5.0; N_BANDS];
        s.mix = 1.0;

        let mut core = EmberCore::new(sr);

        // Feed ~0.5 s of a tone with freeze OFF to build state, then engage freeze and
        // feed silence for 5 s.
        let pre = (sr * 0.5) as usize;
        s.freeze = false;
        core.configure(&s);
        for i in 0..pre {
            let x = 0.4 * (TAU * 220.0 * i as f32 / sr).sin();
            core.process_sample(x, x, s.mix);
        }
        s.freeze = true;
        core.configure(&s);
        let hold = (sr * 5.0) as usize;
        let mut out = vec![0.0f32; hold];
        for i in 0..hold {
            let (y, _) = core.process_sample(0.0, 0.0, s.mix);
            out[i] = y;
        }

        // Skip the first latency+settle window, then check frame RMS flatness.
        let start = FFT_SIZE * 2;
        let frame = (sr * 0.5) as usize;
        let mut mn = f32::INFINITY;
        let mut mx = f32::NEG_INFINITY;
        let mut i = start;
        while i + frame <= hold {
            let r = db(rms(&out[i..i + frame]));
            mn = mn.min(r);
            mx = mx.max(r);
            i += frame;
        }
        assert!(
            (mx - mn) <= 2.0,
            "freeze tail not flat: spread {:.2} dB ({mn:.1}..{mx:.1})",
            mx - mn
        );
        assert!(db(rms(&out[start..])) > -40.0, "freeze tail too quiet");
    }

    /// mix=0 nulls against the dry input delayed by the reported latency (< −80 dB).
    #[test]
    fn mix_zero_nulls_against_delayed_dry() {
        let sr = 48_000.0f32;
        let s = Settings {
            mix: 0.0,
            ..Settings::default()
        };
        let n = (sr * 1.0) as usize;
        let input = suite_core::testsig::log_chirp(50.0, 12_000.0, 0.5, n, sr);

        let mut core = EmberCore::new(sr);
        core.configure(&s);
        let mut out = vec![0.0f32; n];
        for i in 0..n {
            let (y, _) = core.process_sample(input[i], input[i], s.mix);
            out[i] = y;
        }

        let lat = core.latency();
        // Residual of out[i] vs input[i-lat], over the region where the delay is filled.
        let mut acc = 0.0f32;
        let mut cnt = 0usize;
        for i in lat..n {
            let d = out[i] - input[i - lat];
            acc += d * d;
            cnt += 1;
        }
        let resid_db = db((acc / cnt as f32).sqrt());
        assert!(resid_db < -80.0, "mix=0 null was {resid_db:.1} dB (need < -80)");
    }
}
