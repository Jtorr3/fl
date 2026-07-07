//! DRIFT — pure-DSP core for the infinity filter (SPECS "DRIFT", Sweep clone).
//!
//! The Shepard-tone illusion, in the filter domain. `N` peak (bell) filters are placed on
//! the log-frequency axis, evenly spaced across a `[range_lo, range_hi]` window, and all
//! glide together up (or down) at `Rate`. Each filter's center **wraps** at the range
//! edges, and its boost follows a **raised-cosine window over its log-frequency position**
//! so every filter fades in silently at the bottom, swells through the middle, and fades
//! out at the top. Because a filter reaching the top has already faded to unity gain, the
//! wrap is inaudible and the ear hears an *endless* rise (or fall) — the Shepard illusion.
//!
//! ```text
//!   log-freq position u_i(t) = frac( phase(t) + i/N )         (i = 0..N, evenly spaced)
//!   center       fc_i = 2^( lo_oct + u_i · span )
//!   window gain  g_i  = depth_dB · (0.5 - 0.5·cos(2π·u_i))    (Hann over log-freq)
//!   wet = ( bell_{N-1} ∘ … ∘ bell_0 )(x)                       (series cascade, per channel)
//! ```
//!
//! The bells are **TPT (topology-preserving transform) state-variable** peaking filters —
//! the same Cytomic SVF topology TRACER uses for its time-varying crossovers, which is
//! unconditionally stable under per-block coefficient modulation. Coefficients recompute
//! per 32-sample control block from a **smoothed** phase and smoothed range/resonance/depth;
//! filter state is preserved across recomputes so the glide is click-free. At a window edge
//! `g_i → 0 dB` so the bell is a pass-through — the fc wrap there carries no energy.
//!
//! DRIFT is pure minimum-phase IIR: dry and wet stay sample-aligned, so latency is 0 and no
//! delay compensation is needed (a suite convention noted in the build brief). API-agnostic
//! pure Rust, shared verbatim between the nih-plug `process` path and the offline harness.

use std::f32::consts::PI;
use suite_core::dsp::OnePole;

/// Maximum number of simultaneous peak filters (param range 2..=8).
pub const MAX_PEAKS: usize = 8;
/// Control-block length: bell coefficients recompute this often (samples).
const CTRL_BLOCK: usize = 32;

/// Glide direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    pub fn from_index(i: usize) -> Direction {
        match i {
            1 => Direction::Down,
            _ => Direction::Up,
        }
    }
    #[inline]
    fn sign(self) -> f32 {
        match self {
            Direction::Up => 1.0,
            Direction::Down => -1.0,
        }
    }
}

/// BPM-sync cycle length (one full glide over the range = this many beats).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncDivision {
    FourBars,
    TwoBars,
    OneBar,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

impl SyncDivision {
    pub fn from_index(i: usize) -> SyncDivision {
        match i {
            0 => SyncDivision::FourBars,
            1 => SyncDivision::TwoBars,
            2 => SyncDivision::OneBar,
            3 => SyncDivision::Half,
            4 => SyncDivision::Quarter,
            5 => SyncDivision::Eighth,
            _ => SyncDivision::Sixteenth,
        }
    }
    /// Beats (quarter notes) per full glide cycle, assuming 4/4.
    #[inline]
    pub fn beats(self) -> f32 {
        match self {
            SyncDivision::FourBars => 16.0,
            SyncDivision::TwoBars => 8.0,
            SyncDivision::OneBar => 4.0,
            SyncDivision::Half => 2.0,
            SyncDivision::Quarter => 1.0,
            SyncDivision::Eighth => 0.5,
            SyncDivision::Sixteenth => 0.25,
        }
    }
}

/// A full snapshot of DRIFT's controls (plain, un-normalized values).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Free glide rate in Hz (full range traversals per second) when not synced.
    pub rate_hz: f32,
    /// BPM-sync on/off.
    pub sync: bool,
    /// Sync cycle length.
    pub division: SyncDivision,
    /// Host tempo (BPM) for sync; falls back to 120 if the host reports none.
    pub tempo_bpm: f32,
    pub direction: Direction,
    /// Shared bell resonance (Q).
    pub resonance: f32,
    /// Range lower edge (Hz).
    pub range_lo: f32,
    /// Range upper edge (Hz).
    pub range_hi: f32,
    /// Active filter count (2..=8).
    pub peaks: usize,
    /// Right-channel glide phase offset, 0..0.5 of the cycle.
    pub stereo_offset: f32,
    /// Peak boost at window center (dB).
    pub depth_db: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            rate_hz: 0.1,
            sync: false,
            division: SyncDivision::OneBar,
            tempo_bpm: 120.0,
            direction: Direction::Up,
            resonance: 3.0,
            range_lo: 50.0,
            range_hi: 3200.0, // exactly 6 octaves above 50 Hz ⇒ default N=6 ≈ 1 octave apart
            peaks: 6,
            stereo_offset: 0.25,
            depth_db: 12.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// TPT (Cytomic) state-variable **peaking / bell** filter. Time-varying-safe: coefficients
/// may be recomputed every control block while state is preserved. At `gain_db == 0` the
/// bell degenerates to a pass-through (`m1 == 0`), so a filter parked at a window edge
/// contributes nothing — the property that makes the fc wrap silent.
#[derive(Clone, Copy, Default)]
struct Bell {
    ic1eq: f32,
    ic2eq: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    /// Bandpass mix coefficient `k·(A² − 1)`.
    m1: f32,
}

impl Bell {
    /// Set center `fc` (Hz), quality `q`, and peak gain `gain_db` at `sr`.
    fn set(&mut self, fc: f32, q: f32, gain_db: f32, sr: f32) {
        let a = 10.0f32.powf(gain_db / 40.0);
        let nyq = sr * 0.5;
        let fc = fc.clamp(1.0, nyq - 1.0);
        let g = (PI * fc / sr).tan();
        // Cytomic bell: k = 1/(Q·A); the same k appears in a1 and in the mix m1.
        let k = 1.0 / (q.max(1.0e-4) * a);
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
        self.m1 = k * (a * a - 1.0);
    }
    fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }
    #[inline]
    fn process(&mut self, v0: f32) -> f32 {
        let v3 = v0 - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        v0 + self.m1 * v1
    }
}

/// The endless Shepard-filter core (stereo; usable mono by passing R = L).
///
/// Owns the per-channel bell cascade and the shared, per-sample-advanced glide phase. The
/// left channel rides `phase`; the right channel rides `phase + stereo_offset`.
pub struct DriftCore {
    sr: f32,
    /// Per-channel bell cascades.
    bells: [[Bell; MAX_PEAKS]; 2],
    /// Master glide phase in cycles, wrapped to [0, 1).
    phase: f32,
    ctrl_count: usize,
    // Smoothed controls (glide-critical values are smoothed to avoid zipper on automation).
    range_lo_s: OnePole,
    range_hi_s: OnePole,
    res_s: OnePole,
    depth_s: OnePole,
    offset_s: OnePole,
    mix_s: OnePole,
    out_s: OnePole,
    primed: bool,
    /// Last-computed center frequencies (Hz) of the left channel — exposed for tests.
    centers: [f32; MAX_PEAKS],
}

impl DriftCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let mut core = DriftCore {
            sr,
            bells: [[Bell::default(); MAX_PEAKS]; 2],
            phase: 0.0,
            ctrl_count: 0,
            range_lo_s: OnePole::new(),
            range_hi_s: OnePole::new(),
            res_s: OnePole::new(),
            depth_s: OnePole::new(),
            offset_s: OnePole::new(),
            mix_s: OnePole::new(),
            out_s: OnePole::new(),
            primed: false,
            centers: [0.0; MAX_PEAKS],
        };
        core.set_sample_rate(sr);
        core
    }

    /// DRIFT is pure minimum-phase IIR: zero reported latency, no dry-path compensation.
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        // ~15 ms control smoothing for the audible glide/range/depth parameters.
        let t = 15.0;
        for s in [
            &mut self.range_lo_s,
            &mut self.range_hi_s,
            &mut self.res_s,
            &mut self.depth_s,
            &mut self.offset_s,
            &mut self.mix_s,
            &mut self.out_s,
        ] {
            s.set_time(t, self.sr);
        }
        self.primed = false;
    }

    pub fn reset(&mut self) {
        for ch in self.bells.iter_mut() {
            for b in ch.iter_mut() {
                b.reset();
            }
        }
        self.phase = 0.0;
        self.ctrl_count = 0;
        self.primed = false;
    }

    /// Current left-channel center frequencies (Hz). Exposed for the done-bar tests.
    pub fn centers(&self) -> [f32; MAX_PEAKS] {
        self.centers
    }

    /// Current master glide phase in cycles (0..1).
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Per-sample phase increment (cycles/sample) for the current settings.
    #[inline]
    fn phase_inc(&self, s: &Settings) -> f32 {
        let cycles_per_sec = if s.sync {
            // beats/cycle → seconds/cycle → cycles/sec.
            let bpm = s.tempo_bpm.clamp(20.0, 999.0);
            let beats = s.division.beats().max(1.0e-4);
            (bpm / 60.0) / beats
        } else {
            s.rate_hz.clamp(0.0, 40.0)
        };
        (cycles_per_sec / self.sr) * s.direction.sign()
    }

    /// Snap all smoothers to the incoming settings (first block / preset load).
    fn prime(&mut self, s: &Settings) {
        self.range_lo_s.reset(s.range_lo);
        self.range_hi_s.reset(s.range_hi);
        self.res_s.reset(s.resonance);
        self.depth_s.reset(s.depth_db);
        self.offset_s.reset(s.stereo_offset);
        self.mix_s.reset(s.mix.clamp(0.0, 1.0));
        self.out_s.reset(s.out_db);
        self.primed = true;
    }

    /// Latch per-block targets. Call once per audio block before the sample loop.
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.prime(s);
        }
    }

    /// Recompute every bell's coefficients from the current smoothed phase + params.
    fn recompute(&mut self, s: &Settings) {
        let lo = self.range_lo_s.value().clamp(20.0, self.sr * 0.45);
        let hi_raw = self.range_hi_s.value().clamp(20.0, self.sr * 0.48);
        // Enforce a non-degenerate, ordered range (at least ~1/4 octave of span).
        let hi = hi_raw.max(lo * 1.19);
        let lo_oct = lo.log2();
        let span = (hi.log2() - lo_oct).max(0.25);
        let q = self.res_s.value().clamp(0.2, 24.0);
        let depth = self.depth_s.value().clamp(0.0, 36.0);
        let offset = self.offset_s.value().clamp(0.0, 0.5);
        let n = s.peaks.clamp(2, MAX_PEAKS);
        let inv_n = 1.0 / n as f32;

        for ci in 0..2 {
            let ch_phase = if ci == 0 {
                self.phase
            } else {
                // Right channel rides a phase-shifted glide (stereo width of the illusion).
                let p = self.phase + offset;
                p - p.floor()
            };
            for i in 0..MAX_PEAKS {
                let bell = &mut self.bells[ci][i];
                if i >= n {
                    // Inactive filter: force pass-through and idle its state.
                    bell.set(1_000.0, q, 0.0, self.sr);
                    continue;
                }
                // Evenly-spaced fractional log positions, wrapping in [0,1).
                let mut u = ch_phase + i as f32 * inv_n;
                u -= u.floor();
                let fc = 2.0f32.powf(lo_oct + u * span);
                // Raised-cosine (Hann) window over log-freq position ⇒ silent fade at edges.
                let win = 0.5 - 0.5 * (2.0 * PI * u).cos();
                let gain_db = depth * win;
                bell.set(fc, q, gain_db, self.sr);
                if ci == 0 {
                    self.centers[i] = fc;
                }
            }
            for i in n..MAX_PEAKS {
                if ci == 0 {
                    self.centers[i] = 0.0;
                }
            }
        }
    }

    /// Process one stereo sample. Cutoffs recompute internally every [`CTRL_BLOCK`] samples
    /// from the smoothed, per-sample-advanced glide phase.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        // Advance the parameter smoothers once per sample so the 32-sample coefficient
        // recompute samples them at the right rate (the TRACER/OVERSEER pattern).
        self.range_lo_s.process(s.range_lo);
        self.range_hi_s.process(s.range_hi);
        self.res_s.process(s.resonance);
        self.depth_s.process(s.depth_db);
        self.offset_s.process(s.stereo_offset);
        let mix = self.mix_s.process(s.mix.clamp(0.0, 1.0));
        let out_lin = db_to_lin(self.out_s.process(s.out_db));

        if self.ctrl_count == 0 {
            self.recompute(s);
        }
        self.ctrl_count += 1;
        if self.ctrl_count >= CTRL_BLOCK {
            self.ctrl_count = 0;
        }

        // Wet: series bell cascade per channel.
        let n = s.peaks.clamp(2, MAX_PEAKS);
        let mut wet_l = l_in;
        let mut wet_r = r_in;
        for i in 0..n {
            wet_l = self.bells[0][i].process(wet_l);
            wet_r = self.bells[1][i].process(wet_r);
        }

        // Advance the glide phase, wrapped to [0,1).
        self.phase += self.phase_inc(s);
        if self.phase >= 1.0 || self.phase < 0.0 {
            self.phase -= self.phase.floor();
        }

        let out_l = ((l_in * (1.0 - mix) + wet_l * mix) * out_lin).clamp(-0.999, 0.999);
        let out_r = ((r_in * (1.0 - mix) + wet_r * mix) * out_lin).clamp(-0.999, 0.999);
        (out_l, out_r)
    }

    /// Convenience for the mono offline harness: process `main` in place (R = L).
    pub fn process_mono(&mut self, main: &mut [f32], s: &Settings) {
        self.configure(s);
        for m in main.iter_mut() {
            let (l, _r) = self.process_sample(*m, *m, s);
            *m = l;
        }
    }

    /// Render a stereo pair from a mono input (for tests needing the L/R phase offset).
    pub fn process_stereo(&mut self, input: &[f32], s: &Settings) -> (Vec<f32>, Vec<f32>) {
        self.configure(s);
        let mut l = Vec::with_capacity(input.len());
        let mut r = Vec::with_capacity(input.len());
        for &x in input {
            let (ol, or) = self.process_sample(x, x, s);
            l.push(ol);
            r.push(or);
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::stft::{Complex, Stft};
    use suite_core::testsig;

    /// Run `sig` through a streaming [`Stft`] and collect one per-bin magnitude spectrum per
    /// frame (fires every `hop` samples). This is the suite's STFT engine (PRD §4), used for
    /// both DRIFT done-bar assertions.
    fn stft_frames(sig: &[f32], fft: usize, hop: usize) -> Vec<Vec<f32>> {
        let mut stft = Stft::new(fft, hop);
        let mut frames: Vec<Vec<f32>> = Vec::new();
        let mut cb = |spec: &mut [Complex<f32>]| {
            frames.push(spec.iter().map(|c| c.norm()).collect());
        };
        for &x in sig {
            stft.process(x, &mut cb);
        }
        frames
    }

    /// Dominant spectral-peak frequency (Hz) of a single magnitude frame within `[f_lo, f_hi]`.
    fn dominant_bin_hz(frame: &[f32], sr: f32, fft: usize, f_lo: f32, f_hi: f32) -> f32 {
        let bin_hz = sr / fft as f32;
        let mut best = 0.0f32;
        let mut best_hz = 0.0f32;
        for (k, &m) in frame.iter().enumerate() {
            let hz = k as f32 * bin_hz;
            if hz < f_lo || hz > f_hi {
                continue;
            }
            if m > best {
                best = m;
                best_hz = hz;
            }
        }
        best_hz
    }

    /// Welch-averaged power spectrum over `count` frames starting at `first`, then lightly
    /// smoothed across frequency (±`sm` bins). Averaging beats down the noise realization so
    /// what remains is the *filter bank's* imprint — the thing whose periodicity we test.
    fn avg_psd(frames: &[Vec<f32>], first: usize, count: usize, sm: usize) -> Vec<f32> {
        let bins = frames[0].len();
        let mut psd = vec![0.0f32; bins];
        let last = (first + count).min(frames.len());
        for f in &frames[first..last] {
            for (k, &m) in f.iter().enumerate() {
                psd[k] += m * m;
            }
        }
        // Frequency smoothing (moving average) to further reduce variance.
        let mut out = vec![0.0f32; bins];
        for k in 0..bins {
            let lo = k.saturating_sub(sm);
            let hi = (k + sm + 1).min(bins);
            let mut acc = 0.0f32;
            for v in &psd[lo..hi] {
                acc += *v;
            }
            out[k] = acc / (hi - lo) as f32;
        }
        out
    }

    /// Pearson correlation of two equal-length slices.
    fn corr(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len().min(b.len());
        let ma = a[..n].iter().sum::<f32>() / n as f32;
        let mb = b[..n].iter().sum::<f32>() / n as f32;
        let mut num = 0.0f32;
        let mut da = 0.0f32;
        let mut db = 0.0f32;
        for i in 0..n {
            let x = a[i] - ma;
            let y = b[i] - mb;
            num += x * y;
            da += x * x;
            db += y * y;
        }
        if da <= 0.0 || db <= 0.0 {
            return 0.0;
        }
        num / (da.sqrt() * db.sqrt())
    }

    #[test]
    fn bell_is_passthrough_at_zero_gain() {
        let mut b = Bell::default();
        b.set(1000.0, 4.0, 0.0, 48_000.0);
        let x = [0.3, -0.7, 0.1, 0.9, -0.2, 0.5];
        for &v in &x {
            let y = b.process(v);
            assert!((y - v).abs() < 1e-6, "bell not pass-through at 0 dB: {y} vs {v}");
        }
    }

    #[test]
    fn bell_boosts_at_center() {
        // A +12 dB bell at 1 kHz should amplify a 1 kHz sine by ~4x.
        let sr = 48_000.0f32;
        let mut b = Bell::default();
        b.set(1000.0, 4.0, 12.0, sr);
        let mut peak_in = 0.0f32;
        let mut peak_out = 0.0f32;
        for n in 0..9_600usize {
            let x = 0.2 * (2.0 * PI * 1000.0 * n as f32 / sr).sin();
            let y = b.process(x);
            if n > 4_800 {
                peak_in = peak_in.max(x.abs());
                peak_out = peak_out.max(y.abs());
            }
        }
        let g = peak_out / peak_in;
        assert!(g > 3.0 && g < 5.0, "bell gain {g} not ~4x (+12 dB) at center");
    }

    /// Settings tuned for spectral testing: mid-range, generous resolution, strong peaks.
    fn test_settings(sr: f32) -> Settings {
        let mut s = Settings::default();
        s.range_lo = 300.0;
        s.range_hi = 4800.0; // 4 octaves
        s.peaks = 4;
        s.resonance = 4.0;
        s.depth_db = 18.0;
        s.mix = 1.0;
        s.rate_hz = 0.5; // period = 2 s at these settings
        s.sync = false;
        s.direction = Direction::Up;
        let _ = sr;
        s
    }

    /// DONE-BAR (1): white-noise input, direction = up → the dominant STFT peak position
    /// strictly advances over time and wraps at the range edge.
    #[test]
    fn dominant_peak_advances_and_wraps() {
        let sr = 48_000.0f32;
        let s = test_settings(sr);
        let period = (sr / s.rate_hz) as usize; // samples per full glide
        let len = period + period / 2; // 1.5 periods so a wrap is guaranteed
        let noise = testsig::white_noise(0.25, len + 8192, 1234);

        let mut core = DriftCore::new(sr);
        let (out, _r) = core.process_stereo(&noise, &s);

        // Track the dominant peak within the active range across the STFT frames. A coarse
        // hop keeps the per-frame glide step (~0.08 octave) well above the bin quantization.
        let fft = 4096usize;
        let hop = 2048usize;
        let frames = stft_frames(&out, fft, hop);
        let peaks: Vec<f32> = frames
            .iter()
            .map(|f| dominant_bin_hz(f, sr, fft, 250.0, 5200.0).max(1.0).log2())
            .collect();
        assert!(peaks.len() > 20, "not enough frames ({})", peaks.len());
        let _ = len;

        // Steps between consecutive frames: forward (rise) steps vs. wrap (large drop) steps.
        let mut forward = 0;
        let mut wraps = 0;
        let mut backward_nonwrap = 0;
        for w in peaks.windows(2) {
            let d = w[1] - w[0];
            if d > 0.02 {
                forward += 1;
            } else if d < -0.4 {
                wraps += 1; // a wrap drops the dominant peak by ~span/N octaves
            } else if d < -0.02 {
                backward_nonwrap += 1;
            }
        }
        assert!(
            forward >= peaks.len() / 3,
            "dominant peak did not mostly advance (forward {forward} of {})",
            peaks.len()
        );
        assert!(wraps >= 1, "no wrap detected (peak never jumped back to range low)");
        assert!(
            backward_nonwrap <= forward,
            "too many non-wrap regressions ({backward_nonwrap}) vs forward ({forward})"
        );

        // All tracked peaks stay inside the range (with a small guard).
        let lo_oct = 250.0f32.log2();
        let hi_oct = 5200.0f32.log2();
        for &p in &peaks {
            assert!(p >= lo_oct - 0.1 && p <= hi_oct + 0.1, "peak {p} octaves out of range");
        }
    }

    /// DONE-BAR (1) — structural companion: an individual filter center glides across the
    /// FULL range and wraps at the edge (range_hi → range_lo), tracked via [`DriftCore::centers`].
    #[test]
    fn filter_center_sweeps_full_range_and_wraps() {
        let sr = 48_000.0f32;
        let mut s = test_settings(sr);
        s.rate_hz = 2.0; // fast so one filter traverses the range within the buffer
        let period = (sr / s.rate_hz) as usize;
        let len = period + period / 2;
        let mut core = DriftCore::new(sr);
        let mut min_c = f32::INFINITY;
        let mut max_c = 0.0f32;
        let mut wrapped = false;
        let mut prev = 0.0f32;
        for n in 0..len {
            let _ = core.process_sample(0.0, 0.0, &s);
            if n % 32 == 0 {
                let c0 = core.centers()[0]; // filter 0's center
                if prev > 0.0 && c0 < prev * 0.5 {
                    wrapped = true; // dropped by ≥ 1 octave ⇒ wrapped past range_hi to range_lo
                }
                min_c = min_c.min(c0);
                max_c = max_c.max(c0);
                prev = c0;
            }
        }
        assert!(wrapped, "filter 0 center never wrapped at the range edge");
        // It should have visited near both range edges (within ~1 octave).
        assert!(min_c < s.range_lo * 2.0, "min center {min_c:.1} never neared range_lo {}", s.range_lo);
        assert!(max_c > s.range_hi * 0.5, "max center {max_c:.1} never neared range_hi {}", s.range_hi);
    }

    /// DONE-BAR (2): periodicity / self-similarity. With `N` evenly-spaced filters, the
    /// output spectrum at `t` and at `t + period/N` strongly correlate (each filter has
    /// advanced into its neighbour's former position).
    #[test]
    fn spectra_self_similar_at_period_over_n() {
        let sr = 48_000.0f32;
        let mut s = test_settings(sr);
        s.rate_hz = 0.2; // slow glide ⇒ the filter bank is ~constant across each averaging window
        let n = s.peaks;
        let period = sr / s.rate_hz; // samples/cycle (240_000)
        let shift = (period / n as f32).round() as usize; // period/N ⇒ +1/N cycle of phase

        // Flat white noise so the averaged periodogram reveals the filter bank's imprint.
        let fft = 4096usize;
        let hop = 512usize;
        let avg_frames = 40usize; // Welch average span ≈ 24k samples ≈ 0.1 cycle of glide
        let sm = 6usize; // ± bins of frequency smoothing
        let t0 = period as usize;
        let total = t0 + shift + avg_frames * hop + fft * 2;
        let noise = testsig::white_noise(0.3, total, 777);

        let mut core = DriftCore::new(sr);
        let (out, _r) = core.process_stereo(&noise, &s);
        let frames = stft_frames(&out, fft, hop);

        // Convert sample offsets to frame indices (frame k ≈ input sample fft + k·hop).
        let f0 = (t0.saturating_sub(fft)) / hop;
        let f_shift = shift / hop;
        let f_half = f_shift / 2;

        let psd_t = avg_psd(&frames, f0, avg_frames, sm);
        let psd_shift = avg_psd(&frames, f0 + f_shift, avg_frames, sm);
        let psd_half = avg_psd(&frames, f0 + f_half, avg_frames, sm);

        // Correlate over the active band (bins spanning ~250..5200 Hz).
        let bin_hz = sr / fft as f32;
        let k_lo = (250.0 / bin_hz) as usize;
        let k_hi = (5200.0 / bin_hz) as usize;
        let c_self = corr(&psd_t[k_lo..k_hi], &psd_shift[k_lo..k_hi]);
        let c_half = corr(&psd_t[k_lo..k_hi], &psd_half[k_lo..k_hi]);

        assert!(
            c_self > 0.9,
            "spectra at t and t+period/N not self-similar (corr {c_self:.3})"
        );
        assert!(
            c_self > c_half,
            "period/N self-similarity ({c_self:.3}) not stronger than period/2N ({c_half:.3})"
        );
    }

    #[test]
    fn extreme_params_stay_finite_and_bounded() {
        // Fuzz-like: huge depth, tiny Q, degenerate/inverted range, all peaks, fast rate.
        let sr = 44_100.0f32;
        let x = testsig::white_noise(0.95, 20_000, 9);
        let mut s = Settings::default();
        s.peaks = MAX_PEAKS;
        s.depth_db = 36.0;
        s.resonance = 24.0;
        s.range_lo = 19_000.0; // inverted vs. hi on purpose
        s.range_hi = 25.0;
        s.rate_hz = 40.0;
        s.stereo_offset = 0.5;
        let mut core = DriftCore::new(sr);
        let mut out = x.clone();
        core.process_mono(&mut out, &s);
        assert!(out.iter().all(|v| v.is_finite()));
        let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak <= 1.0, "peak {peak} exceeded 0 dBFS");
    }

    #[test]
    fn mix_zero_nulls_against_dry() {
        // Minimum-phase IIR: dry/wet are sample-aligned, so mix=0 is an exact dry copy.
        let sr = 48_000.0f32;
        let n = 24_000usize;
        let main: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 220.0 * i as f32 / sr).sin())
            .collect();
        let mut s = Settings::default();
        s.mix = 0.0;
        s.out_db = 0.0;
        let mut core = DriftCore::new(sr);
        let mut out = main.clone();
        core.process_mono(&mut out, &s);
        let mse = (0..n).map(|i| (main[i] - out[i]).powi(2)).sum::<f32>() / n as f32;
        let resid = 20.0 * mse.sqrt().max(1.0e-12).log10();
        assert!(resid < -80.0, "mix=0 did not null: residual {resid:.1} dB");
    }

    /// Partial-mix coherence (build brief): minimum-phase filters keep dry/wet aligned, so a
    /// unit impulse at mix=0.5 with near-unity wet forms a SINGLE coherent peak (no comb).
    #[test]
    fn partial_mix_impulse_is_single_coherent_peak() {
        use suite_core::harness::assert_single_coherent_peak;
        let sr = 48_000.0f32;
        let n = 256usize;
        let mut s = Settings::default();
        s.mix = 0.5;
        s.depth_db = 0.0; // window×0 ⇒ every bell is pass-through ⇒ wet == dry
        let mut main = vec![0.0f32; n];
        main[0] = 1.0;
        let mut core = DriftCore::new(sr);
        core.process_mono(&mut main, &s);
        assert_single_coherent_peak(&main, 2, 0.5);
    }

    #[test]
    fn stereo_offset_decorrelates_channels() {
        let sr = 48_000.0f32;
        let mut s = test_settings(sr);
        // Offset must NOT be a multiple of 1/N, else the R filter set maps exactly onto L
        // (the Shepard self-similarity) and the channels are identical.
        s.stereo_offset = 0.13;
        let pink = testsig::pink_noise(0.4, 48_000, 5);
        let mut core = DriftCore::new(sr);
        let (l, r) = core.process_stereo(&pink, &s);
        // With a non-commensurate L/R phase offset the channels must not be identical.
        let d: f32 = l.iter().zip(&r).map(|(a, b)| (a - b).abs()).sum::<f32>() / l.len() as f32;
        assert!(d > 1.0e-4, "stereo offset produced ~identical channels (mean |L-R| {d})");
    }
}
