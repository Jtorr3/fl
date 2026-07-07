//! GRIT — pure-DSP core for the sidechained distortion (SPECS "GRIT").
//!
//! Signal flow (per SPECS):
//! ```text
//! main in ─ trim ─ pre-filter(SVF HP/LP) ─┐
//!                                         ├─ DIST CORE ─ post-filter ─ auto-gain ─ mix ─ out
//! sidechain in ─ SC focus BP ─ env follower┘        ▲
//!                                                    └ mode selects how SC drives the core
//! ```
//! Modes shipped: **A (Env→Drive)** and **B (Waveshape-by-SC dynamic bias)**. Both run
//! the nonlinearity at 4x oversampling. Mode C (spectral STFT per-bin drive) is
//! deferred — see DEFERRED.md.
//!
//! This module is API-agnostic pure Rust and is shared verbatim between the nih-plug
//! `process` path and the offline harness tests, so the tested math is the shipped math.

use suite_core::dsp::{DelayLine, Detector, EnvFollower, Oversampler4x, Shaper, Svf};

/// How the sidechain drives the distortion core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A: sidechain envelope raises the drive amount.
    EnvDrive,
    /// B: sidechain envelope injects a dynamic bias into the waveshaper.
    WaveshapeSc,
}

impl Mode {
    pub fn from_index(i: usize) -> Mode {
        match i {
            1 => Mode::WaveshapeSc,
            _ => Mode::EnvDrive,
        }
    }
}

/// Which waveshaper from the suite bank the core uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Tube,
    Tape,
    Fold,
    Hard,
}

impl ShapeKind {
    pub fn from_index(i: usize) -> ShapeKind {
        match i {
            1 => ShapeKind::Tape,
            2 => ShapeKind::Fold,
            3 => ShapeKind::Hard,
            _ => ShapeKind::Tube,
        }
    }
    pub fn shaper(self) -> Shaper {
        match self {
            ShapeKind::Tube => Shaper::TubeTanh,
            ShapeKind::Tape => Shaper::TapeSoft,
            ShapeKind::Fold => Shaper::SineFold,
            ShapeKind::Hard => Shaper::HardClip,
        }
    }
}

/// A full snapshot of GRIT's controls (plain, un-normalized values). Cheap to copy;
/// the plugin builds one per sample from its smoothers, tests build them directly.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub mode: Mode,
    pub shape: ShapeKind,
    /// Input trim, dB.
    pub trim_db: f32,
    /// Base drive, dB.
    pub drive_db: f32,
    /// Sidechain modulation depth, 0..1.
    pub depth: f32,
    /// Envelope curve exponent (>0).
    pub curve: f32,
    /// Envelope attack / release, ms.
    pub attack_ms: f32,
    pub release_ms: f32,
    /// Sidechain focus-band center (Hz) and bandwidth (octaves).
    pub sc_focus_hz: f32,
    pub sc_width_oct: f32,
    /// Monitor the sidechain focus band instead of the output.
    pub sc_listen: bool,
    /// Pre / post filter cutoffs, Hz.
    pub pre_hp_hz: f32,
    pub pre_lp_hz: f32,
    pub post_hp_hz: f32,
    pub post_lp_hz: f32,
    /// Auto-gain (match post-RMS to pre-RMS over 300 ms, +/-12 dB clamp).
    pub auto_gain: bool,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim, dB.
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: Mode::EnvDrive,
            shape: ShapeKind::Tube,
            trim_db: 0.0,
            drive_db: 12.0,
            depth: 0.5,
            curve: 1.0,
            attack_ms: 5.0,
            release_ms: 120.0,
            sc_focus_hz: 100.0,
            sc_width_oct: 1.5,
            sc_listen: false,
            pre_hp_hz: 20.0,
            pre_lp_hz: 20_000.0,
            post_hp_hz: 20.0,
            post_lp_hz: 20_000.0,
            auto_gain: true,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// Convert a bandwidth in octaves to an SVF Q (bandpass).
#[inline]
fn octaves_to_q(bw_oct: f32) -> f32 {
    let bw = bw_oct.max(0.05);
    // Q = f0 / bandwidth; bandwidth = f0 (2^(bw/2) - 2^(-bw/2)).
    let span = 2.0f32.powf(bw * 0.5) - 2.0f32.powf(-bw * 0.5);
    (1.0 / span.max(1.0e-4)).clamp(0.2, 20.0)
}

/// First-order DC blocker (~5 Hz corner at 48 kHz), keeps bias-injected offsets out.
#[derive(Clone, Copy, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
    r: f32,
}

impl DcBlock {
    fn set(&mut self, sample_rate: f32) {
        // R chosen for ~5 Hz corner, scaled with sample rate.
        self.r = 1.0 - (std::f32::consts::TAU * 5.0 / sample_rate);
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Per-channel filter + oversampler state.
struct Channel {
    pre_hp: Svf,
    pre_lp: Svf,
    post_hp: Svf,
    post_lp: Svf,
    dc: DcBlock,
    os: Oversampler4x,
}

impl Channel {
    fn new() -> Self {
        Channel {
            pre_hp: Svf::new(),
            pre_lp: Svf::new(),
            post_hp: Svf::new(),
            post_lp: Svf::new(),
            dc: DcBlock::default(),
            os: Oversampler4x::new(),
        }
    }
    fn reset(&mut self) {
        self.pre_hp.reset();
        self.pre_lp.reset();
        self.post_hp.reset();
        self.post_lp.reset();
        self.dc = DcBlock::default();
        self.dc.set(48_000.0);
        self.os.reset();
    }
}

/// Stereo GRIT core (also usable mono by passing R = L). Holds all filter/oversampler
/// state plus the shared (mono-summed) sidechain path and auto-gain trackers.
pub struct GritCore {
    sr: f32,
    ch: [Channel; 2],
    sc_bp: Svf,
    sc_env: EnvFollower,
    // Auto-gain: 300 ms one-pole running mean-square of pre / post (mono sum).
    ag_coef: f32,
    pre_ms: f32,
    post_ms: f32,
    // Dry-path delay compensation: the wet path runs the distortion core through a 4x
    // oversampler whose linear-phase halfband FIRs impose a fixed group delay. The dry
    // path is delayed by the same integer amount so dry and wet stay sample-aligned at
    // partial mix (no comb filtering); this delay is reported to the host as latency.
    dry_delay: [DelayLine; 2],
    latency: usize,
}

impl GritCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        // Empirically-measured group delay of the wet path's 4x oversampler (SR-independent
        // — a fixed number of FIR taps). Using the measured integer peak lag makes the dry
        // path align to the wet path with zero-sample error.
        let latency = Oversampler4x::measure_group_delay();
        let mut core = GritCore {
            sr,
            ch: [Channel::new(), Channel::new()],
            sc_bp: Svf::new(),
            sc_env: EnvFollower::new(Detector::Peak),
            ag_coef: 0.0,
            pre_ms: 0.0,
            post_ms: 0.0,
            dry_delay: [DelayLine::new(latency), DelayLine::new(latency)],
            latency,
        };
        core.set_sample_rate(sr);
        core
    }

    /// Reported plugin latency (samples) = the oversampler group delay the dry path is
    /// compensated by. Constant across sample rates.
    pub fn latency_samples(&self) -> u32 {
        self.latency as u32
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        // 300 ms auto-gain averaging window.
        let n = 0.300 * self.sr;
        self.ag_coef = (-1.0 / n).exp();
        for c in self.ch.iter_mut() {
            c.dc.set(self.sr);
        }
    }

    pub fn reset(&mut self) {
        for c in self.ch.iter_mut() {
            c.reset();
            c.dc.set(self.sr);
        }
        self.sc_bp.reset();
        self.sc_env.reset();
        self.pre_ms = 0.0;
        self.post_ms = 0.0;
        for d in self.dry_delay.iter_mut() {
            d.reset();
        }
    }

    /// Reconfigure filters + envelope coefficients from a settings snapshot. Cheap
    /// enough for once-per-block (control-rate) use; avoids per-sample `tan()`.
    pub fn configure(&mut self, s: &Settings) {
        for c in self.ch.iter_mut() {
            c.pre_hp.set(s.pre_hp_hz, 0.707, self.sr);
            c.pre_lp.set(s.pre_lp_hz, 0.707, self.sr);
            c.post_hp.set(s.post_hp_hz, 0.707, self.sr);
            c.post_lp.set(s.post_lp_hz, 0.707, self.sr);
        }
        self.sc_bp
            .set(s.sc_focus_hz, octaves_to_q(s.sc_width_oct), self.sr);
        self.sc_env
            .set_times(s.attack_ms, s.release_ms, self.sr);
    }

    /// Distort one already-trimmed, pre-filtered sample for channel `ci`.
    #[inline]
    fn distort(&mut self, ci: usize, x: f32, env: f32, s: &Settings) -> f32 {
        let shaper = s.shape.shaper();
        match s.mode {
            Mode::EnvDrive => {
                // drive_dB(t) = base + depth*36dB * env^curve
                let extra = s.depth * 36.0 * env.max(0.0).powf(s.curve.max(0.05));
                let drive = db_to_lin(s.drive_db + extra);
                self.ch[ci].os.process(x, |v| shaper.apply(v, drive))
            }
            Mode::WaveshapeSc => {
                let drive = db_to_lin(s.drive_db);
                let bias = s.depth * 2.0 * env;
                self.ch[ci]
                    .os
                    .process(x, |v| shaper.apply(v * drive + bias, 1.0))
            }
        }
    }

    /// Process one stereo sample. `sc` is the (already mono-summed) sidechain sample.
    /// Returns the processed `(l, r)`. Call [`configure`] once per block first.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, sc: f32, s: &Settings) -> (f32, f32) {
        // Advance the dry-delay lines every sample so the dry path stays group-delay
        // aligned with the oversampled wet path (done before any early return so the delay
        // state never drifts).
        let dry_l = self.dry_delay[0].process(l_in);
        let dry_r = self.dry_delay[1].process(r_in);

        // --- Sidechain path (mono, shared) ---
        let sc_band = self.sc_bp.process(sc).bp;
        let env = self.sc_env.process(sc_band).clamp(0.0, 4.0);

        if s.sc_listen {
            let m = (sc_band).clamp(-0.999, 0.999);
            return (m, m);
        }

        let trim = db_to_lin(s.trim_db);
        let inputs = [l_in, r_in];
        let mut wet = [0.0f32; 2];
        let mut pre_sum = 0.0f32;
        let mut post_sum = 0.0f32;

        for ci in 0..2 {
            // trim -> pre-filter (HP then LP)
            let x = inputs[ci] * trim;
            let x = self.ch[ci].pre_hp.process(x).hp;
            let x = self.ch[ci].pre_lp.process(x).lp;
            pre_sum += x * x;
            // distortion core (4x oversampled) -> DC block -> post-filter
            let y = self.distort(ci, x, env, s);
            let y = self.ch[ci].dc.process(y);
            let y = self.ch[ci].post_hp.process(y).hp;
            let y = self.ch[ci].post_lp.process(y).lp;
            post_sum += y * y;
            wet[ci] = y;
        }

        // --- Auto-gain: match post-RMS to pre-RMS over 300 ms, +/-12 dB clamp ---
        let mut ag = 1.0f32;
        if s.auto_gain {
            self.pre_ms = pre_sum + self.ag_coef * (self.pre_ms - pre_sum);
            self.post_ms = post_sum + self.ag_coef * (self.post_ms - post_sum);
            let ratio = (self.pre_ms.max(1.0e-12) / self.post_ms.max(1.0e-12)).sqrt();
            ag = ratio.clamp(db_to_lin(-12.0), db_to_lin(12.0));
        }

        // --- Mix + output trim, with a hard safety ceiling at +/-0.999 (<= 0 dBFS) ---
        // Dry uses the latency-compensated input so partial mix does not comb-filter.
        let out_lin = db_to_lin(s.out_db);
        let mix = s.mix.clamp(0.0, 1.0);
        let dry = [dry_l, dry_r];
        let mut out = [0.0f32; 2];
        for ci in 0..2 {
            let w = wet[ci] * ag;
            let mixed = dry[ci] * (1.0 - mix) + w * mix;
            out[ci] = (mixed * out_lin).clamp(-0.999, 0.999);
        }
        (out[0], out[1])
    }

    /// Convenience for the mono offline harness: process `main` in place against `sc`.
    pub fn process_mono(&mut self, main: &mut [f32], sc: &[f32], s: &Settings) {
        self.configure(s);
        for (i, m) in main.iter_mut().enumerate() {
            let scv = sc.get(i).copied().unwrap_or(0.0);
            let (l, _r) = self.process_sample(*m, *m, scv, s);
            *m = l;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thd_ratio(x: &[f32], fund_hz: f32, sr: f32) -> f32 {
        // Goertzel magnitude^2 at a given frequency.
        fn power(x: &[f32], f: f32, sr: f32) -> f32 {
            let w = std::f32::consts::TAU * f / sr;
            let cw = 2.0 * w.cos();
            let (mut s1, mut s2) = (0.0f32, 0.0f32);
            for &v in x {
                let s0 = v + cw * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            s1 * s1 + s2 * s2 - cw * s1 * s2
        }
        let fund = power(x, fund_hz, sr).max(1.0e-20);
        let mut harm = 0.0f32;
        for h in 2..=8 {
            let f = fund_hz * h as f32;
            if f < sr * 0.5 {
                harm += power(x, f, sr);
            }
        }
        (harm / fund).sqrt()
    }

    #[test]
    fn thd_rises_during_sidechain_pulses() {
        let sr = 48_000.0f32;
        let n = sr as usize; // 1 s
        let main: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / sr).sin())
            .collect();
        // Pulsed sidechain: 50 ms bursts of a 100 Hz tone every 250 ms.
        let sc: Vec<f32> = (0..n)
            .map(|i| {
                let phase = (i as f32 / sr) % 0.25;
                if phase < 0.05 {
                    0.9 * (std::f32::consts::TAU * 100.0 * i as f32 / sr).sin()
                } else {
                    0.0
                }
            })
            .collect();

        let mut s = Settings::default();
        s.mode = Mode::EnvDrive;
        s.drive_db = 3.0;
        s.depth = 0.9;
        s.attack_ms = 2.0;
        s.release_ms = 30.0;
        s.auto_gain = true;

        let mut core = GritCore::new(sr);
        let mut out = main.clone();
        core.process_mono(&mut out, &sc, &s);

        // Window centered in the first pulse vs. a window between pulses.
        let win = (0.03 * sr) as usize;
        let during = &out[(0.015 * sr) as usize..(0.015 * sr) as usize + win];
        let between = &out[(0.16 * sr) as usize..(0.16 * sr) as usize + win];
        let thd_during = thd_ratio(during, 1_000.0, sr);
        let thd_between = thd_ratio(between, 1_000.0, sr);
        assert!(
            thd_during > thd_between * 1.5,
            "THD during pulse ({thd_during:.4}) not clearly > between ({thd_between:.4})"
        );
    }

    #[test]
    fn autogain_holds_output_rms_near_input_rms() {
        let sr = 48_000.0f32;
        let n = (sr * 1.5) as usize;
        let main: Vec<f32> = (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 1_000.0 * i as f32 / sr).sin())
            .collect();
        let sc: Vec<f32> = (0..n)
            .map(|i| 0.7 * (std::f32::consts::TAU * 90.0 * i as f32 / sr).sin())
            .collect();

        let mut s = Settings::default();
        s.mode = Mode::EnvDrive;
        s.drive_db = 18.0;
        s.depth = 0.8;
        s.auto_gain = true;
        s.mix = 1.0;

        let mut core = GritCore::new(sr);
        let mut out = main.clone();
        core.process_mono(&mut out, &sc, &s);

        // Measure over the settled second half (skip the 300 ms auto-gain ramp).
        let start = (sr * 0.6) as usize;
        let rms = |x: &[f32]| {
            (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt()
        };
        let pre_db = 20.0 * rms(&main[start..]).log10();
        let post_db = 20.0 * rms(&out[start..]).log10();
        assert!(
            (pre_db - post_db).abs() <= 1.0,
            "auto-gain off: pre {pre_db:.2} dB vs post {post_db:.2} dB"
        );
    }

    #[test]
    fn mix_zero_nulls_against_latency_matched_dry() {
        let sr = 48_000.0f32;
        let n = 24_000usize;
        let main: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / sr).sin())
            .collect();
        let sc = vec![0.3f32; n];
        let mut s = Settings::default();
        s.mix = 0.0;
        s.out_db = 0.0;
        let mut core = GritCore::new(sr);
        let lat = core.latency_samples() as usize;
        let mut out = main.clone();
        core.process_mono(&mut out, &sc, &s);
        // At mix=0 the output is the dry path delayed by the reported latency.
        let m = n - lat;
        let resid: f32 = {
            let mse = (0..m)
                .map(|i| {
                    let d = main[i] - out[i + lat];
                    d * d
                })
                .sum::<f32>()
                / m as f32;
            20.0 * mse.sqrt().max(1.0e-12).log10()
        };
        assert!(resid < -80.0, "mix=0 did not null: residual {resid:.1} dB");
    }

    /// Regression (HARD CHECKPOINT 1): at mix=0.5 with a unit impulse and a near-identity
    /// wet setting, dry and wet must land as a SINGLE coherent peak — the uncompensated
    /// oversampler group delay would otherwise split it into two peaks (comb filtering).
    #[test]
    fn partial_mix_impulse_is_single_coherent_peak() {
        use suite_core::harness::assert_single_coherent_peak;
        let sr = 48_000.0f32;
        let n = 256usize;
        let mut s = Settings::default();
        s.mix = 0.5;
        s.drive_db = 0.0; // ~unity wet: no added drive
        s.depth = 0.0;
        s.auto_gain = false;
        s.mode = Mode::EnvDrive;
        let mut main = vec![0.0f32; n];
        main[0] = 1.0;
        let sc = vec![0.0f32; n];
        let mut core = GritCore::new(sr);
        core.process_mono(&mut main, &sc, &s);
        // Dry (0.5) and wet (~0.5) coincide at the group-delay lag → one cluster.
        assert_single_coherent_peak(&main, 2, 0.5);
    }
}
