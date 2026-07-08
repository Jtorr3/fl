//! suite_core::classify — instrument auto-classification and session-theme inference
//! from audio features (OVERSEER-ENRICH v2).
//!
//! Two layers, both allocation-free after construction:
//!
//! - [`FeatureExtractor`] runs on the audio thread. It taps a Node's own input and keeps
//!   rolling (~4 s) statistics — low-band ratio, spectral centroid + tilt, onset rate +
//!   crest, pitch confidence + pitched-frame ratio, a 5–9 kHz sibilance ratio, a sustain
//!   estimate and stereo width — via a cheap SVF filterbank, two envelope followers and
//!   the suite [`crate::pitch::PitchTracker`]. It produces a [`FeatureSummary`] (a small
//!   `Copy` struct of scalars) which is published to atomics for the GUI/bus tick. It also
//!   supports a LEARN capture: an exact-N-sample window that finalises once and freezes
//!   (so a Learn commits the type played *during* the window even if the audio changes
//!   after).
//!
//! - [`classify`] (rule/score) maps a [`FeatureSummary`] to an [`InstrumentType`] +
//!   confidence, and [`infer_theme`] aggregates several Nodes' summaries + a master mix
//!   analysis into a [`SessionTheme`]. These are cheap and run on the GUI/bus tick — never
//!   in `process()`.
//!
//! Nothing here changes the audio: the extractor only reads samples.

use crate::dsp::Svf;
use crate::pitch::PitchTracker;

// ===========================================================================
// Types
// ===========================================================================

/// Instrument classes the Node can auto-detect (SPECS OVERSEER-ENRICH). `Generic` is the
/// low-confidence fallback (used when no class clears the confidence margin), not a scored
/// class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstrumentType {
    Kick,
    Bass,
    Rumble,
    Perc,
    Hats,
    Snare,
    Breaks,
    Vocal,
    Pad,
    Lead,
    Atmos,
    Fx,
    Bus,
    Generic,
}

impl InstrumentType {
    /// Short display label (Master grid badge, Node header).
    pub fn label(self) -> &'static str {
        match self {
            InstrumentType::Kick => "KICK",
            InstrumentType::Bass => "BASS",
            InstrumentType::Rumble => "RUMBLE",
            InstrumentType::Perc => "PERC",
            InstrumentType::Hats => "HATS",
            InstrumentType::Snare => "SNARE",
            InstrumentType::Breaks => "BREAKS",
            InstrumentType::Vocal => "VOCAL",
            InstrumentType::Pad => "PAD",
            InstrumentType::Lead => "LEAD",
            InstrumentType::Atmos => "ATMOS",
            InstrumentType::Fx => "FX",
            InstrumentType::Bus => "BUS",
            InstrumentType::Generic => "GENERIC",
        }
    }

    /// Stable index used to publish the type over the bus (round-trips via [`from_index`]).
    pub fn index(self) -> u32 {
        match self {
            InstrumentType::Kick => 0,
            InstrumentType::Bass => 1,
            InstrumentType::Rumble => 2,
            InstrumentType::Perc => 3,
            InstrumentType::Hats => 4,
            InstrumentType::Snare => 5,
            InstrumentType::Breaks => 6,
            InstrumentType::Vocal => 7,
            InstrumentType::Pad => 8,
            InstrumentType::Lead => 9,
            InstrumentType::Atmos => 10,
            InstrumentType::Fx => 11,
            InstrumentType::Bus => 12,
            InstrumentType::Generic => 13,
        }
    }

    pub fn from_index(i: u32) -> InstrumentType {
        match i {
            0 => InstrumentType::Kick,
            1 => InstrumentType::Bass,
            2 => InstrumentType::Rumble,
            3 => InstrumentType::Perc,
            4 => InstrumentType::Hats,
            5 => InstrumentType::Snare,
            6 => InstrumentType::Breaks,
            7 => InstrumentType::Vocal,
            8 => InstrumentType::Pad,
            9 => InstrumentType::Lead,
            10 => InstrumentType::Atmos,
            11 => InstrumentType::Fx,
            12 => InstrumentType::Bus,
            _ => InstrumentType::Generic,
        }
    }

    /// A stable RGB tint for the Master grid strip (SPECS: type-colored strip per Node).
    pub fn color_rgb(self) -> (u8, u8, u8) {
        match self {
            InstrumentType::Kick => (224, 96, 64),    // rust-orange
            InstrumentType::Bass => (196, 128, 64),   // amber-brown
            InstrumentType::Rumble => (140, 90, 70),  // dark ochre
            InstrumentType::Perc => (96, 176, 208),   // cyan
            InstrumentType::Hats => (128, 208, 224),  // pale cyan
            InstrumentType::Snare => (208, 176, 96),  // sand
            InstrumentType::Breaks => (176, 128, 208),// violet
            InstrumentType::Vocal => (224, 176, 128), // warm skin
            InstrumentType::Pad => (128, 176, 128),   // sage
            InstrumentType::Lead => (224, 208, 96),   // yellow
            InstrumentType::Atmos => (128, 160, 200), // slate blue
            InstrumentType::Fx => (160, 160, 176),    // grey-violet
            InstrumentType::Bus => (176, 176, 176),   // neutral grey
            InstrumentType::Generic => (128, 128, 128),
        }
    }
}

/// Session-wide musical theme (Master, SPECS OVERSEER-ENRICH).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionTheme {
    DarkTechno,
    DnbBreaks,
    Ambient,
    HouseGroove,
    Generic,
}

impl SessionTheme {
    pub fn label(self) -> &'static str {
        match self {
            SessionTheme::DarkTechno => "DARK-TECHNO",
            SessionTheme::DnbBreaks => "DNB-BREAKS",
            SessionTheme::Ambient => "AMBIENT",
            SessionTheme::HouseGroove => "HOUSE-GROOVE",
            SessionTheme::Generic => "GENERIC",
        }
    }

    pub fn index(self) -> u32 {
        match self {
            SessionTheme::DarkTechno => 0,
            SessionTheme::DnbBreaks => 1,
            SessionTheme::Ambient => 2,
            SessionTheme::HouseGroove => 3,
            SessionTheme::Generic => 4,
        }
    }

    pub fn from_index(i: u32) -> SessionTheme {
        match i {
            0 => SessionTheme::DarkTechno,
            1 => SessionTheme::DnbBreaks,
            2 => SessionTheme::Ambient,
            3 => SessionTheme::HouseGroove,
            _ => SessionTheme::Generic,
        }
    }

    /// The preset-bank category slug the Master preset bar filters by for this theme.
    pub fn bank_category(self) -> &'static str {
        match self {
            SessionTheme::DarkTechno => "DARK-TECHNO",
            SessionTheme::DnbBreaks => "DNB-BREAKS",
            SessionTheme::Ambient => "AMBIENT",
            SessionTheme::HouseGroove => "HOUSE-GROOVE",
            SessionTheme::Generic => "GENERIC",
        }
    }
}

/// A compact, `Copy` snapshot of a Node's rolling audio features. All fields are plain
/// scalars so the whole thing can be published over the bus as a handful of atomics.
#[derive(Clone, Copy, Debug)]
pub struct FeatureSummary {
    /// Energy below 120 Hz as a fraction of total band energy, `0..1`.
    pub low_ratio: f32,
    /// Spectral centroid in Hz.
    pub centroid_hz: f32,
    /// Spectral tilt: `log10(upper-half energy / lower-half energy)`; <0 dark, >0 bright.
    pub tilt: f32,
    /// Discrete onset rate in onsets per second.
    pub onset_rate: f32,
    /// Crest factor `peak/rms` (linear).
    pub crest: f32,
    /// Fraction of recent analysis frames that were confidently pitched, `0..1`.
    pub pitched_ratio: f32,
    /// Current pitch confidence, `0..1`.
    pub pitch_conf: f32,
    /// Detected fundamental in Hz (0 when unpitched).
    pub pitch_hz: f32,
    /// Energy in the 5–9 kHz sibilance band as a fraction of total, `0..1`.
    pub sibilance_ratio: f32,
    /// Sustain estimate `mean_env / peak_env`, `0..1` (low = transient, high = sustained).
    pub sustain: f32,
    /// Stereo width `side / (mid + side)`, `0..1` (0 = mono).
    pub width: f32,
    /// Rough loudness (RMS) in dBFS — used only for the silence gate.
    pub level_db: f32,
}

impl Default for FeatureSummary {
    fn default() -> Self {
        Self {
            low_ratio: 0.0,
            centroid_hz: 1000.0,
            tilt: 0.0,
            onset_rate: 0.0,
            crest: 1.0,
            pitched_ratio: 0.0,
            pitch_conf: 0.0,
            pitch_hz: 0.0,
            sibilance_ratio: 0.0,
            sustain: 0.0,
            width: 0.0,
            level_db: f32::NEG_INFINITY,
        }
    }
}

// ===========================================================================
// Feature extractor (audio-thread, allocation-free)
// ===========================================================================

/// Log-spaced analysis band centres (Hz) for the cheap filterbank. Chosen to resolve the
/// sub (<120 Hz), the mids, and the 5–9 kHz sibilance region.
const BAND_HZ: [f32; 11] = [
    45.0, 90.0, 180.0, 360.0, 720.0, 1440.0, 2880.0, 5000.0, 7000.0, 10_000.0, 14_000.0,
];
const NBANDS: usize = BAND_HZ.len();

/// Rolling-statistics time constant (seconds) — the "~4 s" window of SPECS, realised as
/// exponential moving averages so the extractor stays allocation-free.
const WIN_S: f32 = 4.0;

fn ema_coeff(tau_s: f32, sr: f32) -> f32 {
    (-1.0 / (tau_s.max(1.0e-4) * sr)).exp()
}

/// Rolling audio-feature extractor. Feed it stereo blocks; read [`FeatureExtractor::summary`]
/// on the GUI/bus tick. Allocation-free after [`FeatureExtractor::new`].
pub struct FeatureExtractor {
    sr: f32,
    // Filterbank: one bandpass SVF per analysis band, and an EMA of its squared output.
    bank: [Svf; NBANDS],
    band_e: [f32; NBANDS],
    // Envelope followers for onset detection / crest / sustain.
    fast_env: f32,
    slow_env: f32,
    fast_a: f32,
    fast_r: f32,
    slow_c: f32,
    // Peak / mean-square EMAs for crest + level.
    peak_env: f32,
    peak_decay: f32,
    ms_ema: f32,
    // Sustain estimate from the SMOOTHED envelope (windowed mean / decaying peak) — robust
    // to pitch-rate ripple that makes a raw peak/mean read transient on sustained tones.
    sust_mean: f32,
    sust_peak: f32,
    // Mid / side energy EMAs for width.
    mid_e: f32,
    side_e: f32,
    // Onset detection state.
    refractory: usize,
    refractory_len: usize,
    onset_leaky: f32, // leaky integrator of onsets (steady state ≈ rate·WIN_S)
    // Pitch.
    pitch: PitchTracker,
    pitched_ema: f32,
    // EMA coefficient for the ~4 s window.
    win_c: f32,
    // --- LEARN capture ------------------------------------------------------
    cap_active: bool,
    cap_remaining: usize,
    cap_n: usize,
    cap_result: Option<FeatureSummary>,
    // Capture accumulators (sums over the window).
    cap_band_e: [f64; NBANDS],
    cap_mid_e: f64,
    cap_side_e: f64,
    cap_ms: f64,
    cap_peak: f32,
    cap_env_sum: f64,
    cap_slow_sum: f64,
    cap_slow_peak: f32,
    cap_onsets: u32,
    cap_pitched: u32,
    cap_pitch_hz_sum: f64,
    cap_pitch_conf_sum: f64,
    cap_frames: u32,
}

impl FeatureExtractor {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let mut bank: [Svf; NBANDS] = std::array::from_fn(|_| Svf::new());
        for (i, b) in bank.iter_mut().enumerate() {
            // Constant-Q-ish bandpass; wider Q at the extremes keeps it stable.
            b.set(BAND_HZ[i].min(sr * 0.45), 2.0, sr);
        }
        Self {
            sr,
            bank,
            band_e: [0.0; NBANDS],
            fast_env: 0.0,
            slow_env: 0.0,
            // Sensitive attack follower for reliable hit/kick detection.
            fast_a: ema_coeff(0.001, sr),
            fast_r: ema_coeff(0.030, sr),
            // Short reference envelope (~25 ms): a *sustained* tone equalises (one onset at
            // its start) while a *transient* attack spikes the fast env above it (a fresh
            // onset each hit). Pitch-rate ripple on pitched material can trip extra onsets,
            // but the onset-driven classes (PERC/HATS/SNARE/BREAKS) are gated on `le(pr)`
            // (unpitched) so that ripple never mislabels a pitched pad/vocal.
            slow_c: ema_coeff(0.025, sr),
            peak_env: 0.0,
            peak_decay: ema_coeff(0.400, sr),
            ms_ema: 0.0,
            sust_mean: 0.0,
            sust_peak: 0.0,
            mid_e: 0.0,
            side_e: 0.0,
            refractory: 0,
            refractory_len: (0.040 * sr) as usize,
            onset_leaky: 0.0,
            pitch: PitchTracker::new(sr, 110.0),
            pitched_ema: 0.0,
            win_c: ema_coeff(WIN_S, sr),
            cap_active: false,
            cap_remaining: 0,
            cap_n: 0,
            cap_result: None,
            cap_band_e: [0.0; NBANDS],
            cap_mid_e: 0.0,
            cap_side_e: 0.0,
            cap_ms: 0.0,
            cap_peak: 0.0,
            cap_env_sum: 0.0,
            cap_slow_sum: 0.0,
            cap_slow_peak: 0.0,
            cap_onsets: 0,
            cap_pitched: 0,
            cap_pitch_hz_sum: 0.0,
            cap_pitch_conf_sum: 0.0,
            cap_frames: 0,
        }
    }

    pub fn reset(&mut self) {
        for b in self.bank.iter_mut() {
            b.reset();
        }
        self.band_e = [0.0; NBANDS];
        self.fast_env = 0.0;
        self.slow_env = 0.0;
        self.peak_env = 0.0;
        self.ms_ema = 0.0;
        self.sust_mean = 0.0;
        self.sust_peak = 0.0;
        self.mid_e = 0.0;
        self.side_e = 0.0;
        self.refractory = 0;
        self.onset_leaky = 0.0;
        self.pitch.reset();
        self.pitched_ema = 0.0;
        // A learn capture in flight is abandoned on reset (transport stop/relocate).
        self.cap_active = false;
        self.cap_remaining = 0;
    }

    /// Begin a LEARN capture of exactly `n_samples` samples. Any previous unread result is
    /// cleared. The window finalises once and freezes (see [`FeatureExtractor::take_capture`]).
    pub fn begin_capture(&mut self, n_samples: usize) {
        self.cap_active = n_samples > 0;
        self.cap_remaining = n_samples;
        self.cap_n = n_samples;
        self.cap_result = None;
        self.cap_band_e = [0.0; NBANDS];
        self.cap_mid_e = 0.0;
        self.cap_side_e = 0.0;
        self.cap_ms = 0.0;
        self.cap_peak = 0.0;
        self.cap_env_sum = 0.0;
        self.cap_slow_sum = 0.0;
        self.cap_slow_peak = 0.0;
        self.cap_onsets = 0;
        self.cap_pitched = 0;
        self.cap_pitch_hz_sum = 0.0;
        self.cap_pitch_conf_sum = 0.0;
        self.cap_frames = 0;
    }

    /// True while a LEARN capture is accumulating.
    pub fn capturing(&self) -> bool {
        self.cap_active
    }

    /// Progress of the current capture in `0..1` (1.0 when idle/finished).
    pub fn capture_progress(&self) -> f32 {
        if self.cap_n == 0 {
            1.0
        } else {
            1.0 - (self.cap_remaining as f32 / self.cap_n as f32)
        }
    }

    /// Take the finalised LEARN summary exactly once (returns `None` until the window
    /// completes, and `None` again after it has been taken).
    pub fn take_capture(&mut self) -> Option<FeatureSummary> {
        self.cap_result.take()
    }

    /// Process one stereo block, updating all rolling statistics (and the capture window if
    /// active). Allocation-free.
    pub fn process_block(&mut self, l: &[f32], r: &[f32]) {
        let n = l.len().min(r.len());
        for i in 0..n {
            self.push_sample(l[i], r[i]);
        }
    }

    #[inline]
    fn push_sample(&mut self, xl: f32, xr: f32) {
        let mono = 0.5 * (xl + xr);
        let mid = mono;
        let side = 0.5 * (xl - xr);

        // Filterbank band energies (EMA of squared bandpass output).
        for i in 0..NBANDS {
            let bp = self.bank[i].process(mono).bp;
            let e = bp * bp;
            self.band_e[i] = e + self.win_c * (self.band_e[i] - e);
            if self.cap_active {
                self.cap_band_e[i] += e as f64;
            }
        }

        // Mid/side energy.
        let me = mid * mid;
        let se = side * side;
        self.mid_e = me + self.win_c * (self.mid_e - me);
        self.side_e = se + self.win_c * (self.side_e - se);

        // Envelopes.
        let a = mono.abs();
        // Fast envelope (attack/release).
        if a > self.fast_env {
            self.fast_env = a + self.fast_a * (self.fast_env - a);
        } else {
            self.fast_env = a + self.fast_r * (self.fast_env - a);
        }
        self.slow_env = a + self.slow_c * (self.slow_env - a);

        // Peak (decaying) + mean-square (windowed) for crest / level / sustain.
        if a > self.peak_env {
            self.peak_env = a;
        } else {
            self.peak_env *= self.peak_decay;
        }
        let ms = mono * mono;
        self.ms_ema = ms + self.win_c * (self.ms_ema - ms);

        // Sustain from the smoothed envelope: windowed mean vs decaying peak.
        self.sust_mean = self.slow_env + self.win_c * (self.sust_mean - self.slow_env);
        if self.slow_env > self.sust_peak {
            self.sust_peak = self.slow_env;
        } else {
            self.sust_peak *= self.peak_decay;
        }

        // Onset detection: a rising-edge transient — the fast envelope spikes well above
        // the short reference envelope and a fraction of the recent peak, with a refractory
        // hold. Rising-edge (vs level) so sustained tones fire once, not continuously.
        if self.refractory > 0 {
            self.refractory -= 1;
        } else if self.fast_env > 0.05
            && self.fast_env > self.slow_env * 1.8
            && self.fast_env > 0.12 * self.peak_env
        {
            // New onset.
            self.onset_leaky += 1.0;
            self.refractory = self.refractory_len;
            if self.cap_active {
                self.cap_onsets += 1;
            }
        }
        // Leak the onset integrator toward zero over the window.
        self.onset_leaky *= self.win_c;

        // Pitch.
        self.pitch.push(mono);
        let conf = self.pitch.confidence();
        let pitched = if conf >= 0.6 { 1.0 } else { 0.0 };
        self.pitched_ema = pitched + self.win_c * (self.pitched_ema - pitched);

        // Capture accumulation.
        if self.cap_active {
            self.cap_mid_e += me as f64;
            self.cap_side_e += se as f64;
            self.cap_ms += ms as f64;
            self.cap_peak = self.cap_peak.max(a);
            self.cap_env_sum += a as f64;
            self.cap_slow_sum += self.slow_env as f64;
            self.cap_slow_peak = self.cap_slow_peak.max(self.slow_env);
            self.cap_pitch_conf_sum += conf as f64;
            if pitched > 0.5 {
                self.cap_pitched += 1;
                self.cap_pitch_hz_sum += self.pitch.f0() as f64;
            }
            self.cap_frames += 1;
            if self.cap_remaining > 0 {
                self.cap_remaining -= 1;
            }
            if self.cap_remaining == 0 {
                self.finalize_capture();
            }
        }
    }

    fn finalize_capture(&mut self) {
        let frames = self.cap_frames.max(1) as f64;
        let dur_s = (self.cap_n as f32 / self.sr).max(1.0e-3);
        // Band energies (mean).
        let mut band = [0.0f32; NBANDS];
        let mut total = 0.0f64;
        for i in 0..NBANDS {
            band[i] = (self.cap_band_e[i] / frames) as f32;
            total += self.cap_band_e[i];
        }
        let total = (total / frames) as f32;
        let mid = (self.cap_mid_e / frames) as f32;
        let side = (self.cap_side_e / frames) as f32;
        let meansq = (self.cap_ms / frames) as f32;
        let sustain = if self.cap_slow_peak > 1.0e-6 {
            ((self.cap_slow_sum / frames) as f32 / self.cap_slow_peak).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let _ = self.cap_env_sum;
        let onset_rate = self.cap_onsets as f32 / dur_s;
        let pitched_ratio = self.cap_pitched as f32 / frames as f32;
        let pitch_conf = (self.cap_pitch_conf_sum / frames) as f32;
        let pitch_hz = if self.cap_pitched > 0 {
            (self.cap_pitch_hz_sum / self.cap_pitched as f64) as f32
        } else {
            0.0
        };
        let peak = self.cap_peak;

        self.cap_result = Some(summarize(
            &band,
            total,
            mid,
            side,
            meansq,
            sustain,
            peak,
            onset_rate,
            pitched_ratio,
            pitch_conf,
            pitch_hz,
        ));
        self.cap_active = false;
    }

    /// The current rolling feature summary (continuous AUTO display).
    pub fn summary(&self) -> FeatureSummary {
        // Onset rate from the leaky integrator: steady state ≈ rate · WIN_S.
        let onset_rate = self.onset_leaky / WIN_S;
        let sustain = if self.sust_peak > 1.0e-6 {
            (self.sust_mean / self.sust_peak).clamp(0.0, 1.0)
        } else {
            0.0
        };
        summarize(
            &self.band_e,
            self.band_e.iter().sum::<f32>(),
            self.mid_e,
            self.side_e,
            self.ms_ema,
            sustain,
            self.peak_env,
            onset_rate,
            self.pitched_ema,
            self.pitch.confidence(),
            self.pitch.f0(),
        )
    }
}

/// Assemble a [`FeatureSummary`] from the raw rolling/captured accumulators.
#[allow(clippy::too_many_arguments)]
fn summarize(
    band: &[f32; NBANDS],
    _total_ignored: f32,
    mid: f32,
    side: f32,
    meansq: f32,
    sustain: f32,
    peak: f32,
    onset_rate: f32,
    pitched_ratio: f32,
    pitch_conf: f32,
    pitch_hz: f32,
) -> FeatureSummary {
    let total: f32 = band.iter().sum::<f32>().max(1.0e-12);

    // Low-band ratio (<120 Hz → bands 0,1 centred 45/90 Hz).
    let low = band[0] + band[1];
    let low_ratio = (low / total).clamp(0.0, 1.0);

    // Sibilance 5–9 kHz → bands centred 5000/7000 Hz (indices 7,8).
    let sib = band[7] + band[8];
    let sibilance_ratio = (sib / total).clamp(0.0, 1.0);

    // Spectral centroid (energy-weighted band centre).
    let mut cw = 0.0f32;
    for i in 0..NBANDS {
        cw += BAND_HZ[i] * band[i];
    }
    let centroid_hz = (cw / total).clamp(20.0, 20_000.0);

    // Tilt: upper-half vs lower-half band energy.
    let half = NBANDS / 2;
    let lo_e: f32 = band[..half].iter().sum::<f32>().max(1.0e-12);
    let hi_e: f32 = band[half..].iter().sum::<f32>().max(1.0e-12);
    let tilt = (hi_e / lo_e).log10();

    let rms = meansq.max(0.0).sqrt();
    let crest = if rms > 1.0e-6 {
        (peak / rms).clamp(1.0, 100.0)
    } else {
        1.0
    };
    let sustain = sustain.clamp(0.0, 1.0);
    let width = {
        let denom = mid + side;
        if denom > 1.0e-12 {
            (side / denom).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let level_db = if rms > 1.0e-9 {
        20.0 * rms.log10()
    } else {
        f32::NEG_INFINITY
    };

    FeatureSummary {
        low_ratio,
        centroid_hz,
        tilt,
        onset_rate,
        crest,
        pitched_ratio,
        pitch_conf,
        pitch_hz,
        sibilance_ratio,
        sustain,
        width,
        level_db,
    }
}

// ===========================================================================
// Rule / score classifier (GUI/bus-tick, cheap)
// ===========================================================================

/// Confidence a class score must reach to be a confident classification (SPECS "margin").
/// Below this the caller keeps its last confident type or falls back to Generic.
pub const CONF_MARGIN: f32 = 0.4;

/// Silence gate: below this rough loudness the input is treated as silent (no confidence).
pub const SILENCE_DB: f32 = -60.0;

/// Smooth "x is at least `t`" ramp over a soft width `w` → `0..1`.
#[inline]
fn ge(x: f32, t: f32, w: f32) -> f32 {
    ((x - (t - w)) / w.max(1.0e-6)).clamp(0.0, 1.0)
}
/// Smooth "x is at most `t`" ramp.
#[inline]
fn le(x: f32, t: f32, w: f32) -> f32 {
    ((t + w - x) / w.max(1.0e-6)).clamp(0.0, 1.0)
}
/// Smooth "x is within `[lo, hi]`" (soft edges of width `w`).
#[inline]
fn band(x: f32, lo: f32, hi: f32, w: f32) -> f32 {
    ge(x, lo, w).min(le(x, hi, w))
}

/// Per-class raw scores for a feature summary, in class-index order (excludes Generic).
/// Exposed for tests / diagnostics; [`classify`] picks the argmax.
pub fn scores(f: &FeatureSummary) -> [(InstrumentType, f32); 13] {
    // Convenience aliases.
    let low = f.low_ratio;
    let cen = f.centroid_hz;
    let onset = f.onset_rate;
    let crest = f.crest;
    let pr = f.pitched_ratio;
    let sus = f.sustain;
    let wid = f.width;
    let sib = f.sibilance_ratio;
    let pitch_hz = f.pitch_hz;

    // KICK: low-band, low centroid, RHYTHMIC discrete hits (onsets present). The onset
    // rate is the key separator from a sustained bass note in the same register.
    let kick = ge(low, 0.4, 0.25) * le(cen, 350.0, 300.0) * ge(onset, 1.2, 1.5);

    // BASS: low-band + strongly pitched + sustained + FEW onsets (one sustained note, not a
    // rhythmic hit train).
    let bass =
        ge(low, 0.3, 0.25) * ge(pr, 0.45, 0.3) * ge(sus, 0.35, 0.25) * le(onset, 1.5, 1.5);

    // RUMBLE: very low-band + no onsets + not strongly pitched (a sub drone / bed).
    let rumble =
        ge(low, 0.55, 0.25) * le(onset, 0.8, 0.8) * le(pr, 0.5, 0.4) * ge(sus, 0.35, 0.3);

    // PERC: bright + dense onsets + unpitched + transient.
    let perc =
        ge(cen, 1500.0, 1200.0) * ge(onset, 2.5, 2.5) * le(pr, 0.5, 0.3) * le(sus, 0.55, 0.3);

    // HATS: very bright + dense onsets + unpitched.
    let hats = ge(cen, 5000.0, 3000.0) * ge(onset, 4.0, 3.0) * le(pr, 0.4, 0.3);

    // SNARE/CLAP: mid burst + noisy (low pitch) + moderate onset rate + transient.
    let snare = band(cen, 800.0, 4000.0, 800.0)
        * ge(onset, 1.5, 2.0)
        * le(pr, 0.5, 0.3)
        * le(sus, 0.5, 0.3)
        * ge(crest, 2.0, 2.0);

    // BREAKS: broadband + dense onsets + noisy/unpitched top — mid tilt.
    let breaks =
        ge(onset, 5.0, 4.0) * band(cen, 600.0, 4000.0, 1000.0) * le(sus, 0.6, 0.3) * le(pr, 0.55, 0.3);

    // VOCAL: pitched + vocal-range fundamental + mid centroid + not low-dominant, small
    // sibilance/formant bonus.
    let vocal = ge(pr, 0.4, 0.3)
        * band(cen, 400.0, 3000.0, 700.0)
        * le(low, 0.4, 0.3)
        * band(pitch_hz, 80.0, 500.0, 120.0)
        * le(wid, 0.45, 0.3)
        * (0.7 + 0.3 * ge(sib, 0.01, 0.05));

    // PAD/ATMOS: sustained + WIDE (the separator from a mono vocal/lead). Onset-independent
    // because pitch-rate ripple on a sustained pad can trip spurious onsets. PAD is pitched,
    // ATMOS is more diffuse/unpitched.
    let pad_base = ge(sus, 0.5, 0.3) * ge(wid, 0.22, 0.18);
    let pad = pad_base * ge(pr, 0.35, 0.35);
    let atmos = pad_base * le(pr, 0.45, 0.35) * le(low, 0.6, 0.4);

    // LEAD: pitched + mid-high centroid + rhythmic + not wide.
    let lead = ge(pr, 0.45, 0.3)
        * band(cen, 1500.0, 6000.0, 1500.0)
        * le(wid, 0.4, 0.3)
        * band(pitch_hz, 150.0, 1500.0, 300.0);

    // FX / BUS: deliberately low, capped so they never clear the confidence margin on their
    // own (fallbacks only).
    let fx = 0.20 * le(pr, 0.3, 0.3) * ge(onset, 0.5, 2.0);
    let bus = 0.20 * band(cen, 500.0, 4000.0, 2000.0) * ge(pr, 0.2, 0.3);

    [
        (InstrumentType::Kick, kick),
        (InstrumentType::Bass, bass),
        (InstrumentType::Rumble, rumble),
        (InstrumentType::Perc, perc),
        (InstrumentType::Hats, hats),
        (InstrumentType::Snare, snare),
        (InstrumentType::Breaks, breaks),
        (InstrumentType::Vocal, vocal),
        (InstrumentType::Pad, pad),
        (InstrumentType::Lead, lead),
        (InstrumentType::Atmos, atmos),
        (InstrumentType::Fx, fx),
        (InstrumentType::Bus, bus),
    ]
}

/// Classify a feature summary into `(type, confidence)`. Confidence is the winning class
/// score; when it is below [`CONF_MARGIN`] (or the input is silent) the type is
/// [`InstrumentType::Generic`] and the confidence is returned as-is so callers can gate.
pub fn classify(f: &FeatureSummary) -> (InstrumentType, f32) {
    if !f.level_db.is_finite() || f.level_db < SILENCE_DB {
        return (InstrumentType::Generic, 0.0);
    }
    let s = scores(f);
    let mut best = InstrumentType::Generic;
    let mut best_v = 0.0f32;
    for (ty, v) in s.iter() {
        if *v > best_v {
            best_v = *v;
            best = *ty;
        }
    }
    if best_v < CONF_MARGIN {
        (InstrumentType::Generic, best_v)
    } else {
        (best, best_v)
    }
}

// ===========================================================================
// Session-theme inference (Master)
// ===========================================================================

/// One live Node's contribution to theme inference: its classified type + key features.
#[derive(Clone, Copy, Debug)]
pub struct NodeReport {
    pub ty: InstrumentType,
    pub features: FeatureSummary,
}

/// Master mix analysis fed into [`infer_theme`] alongside the per-Node reports.
#[derive(Clone, Copy, Debug)]
pub struct MixAnalysis {
    /// Transport tempo (BPM), or 0 if unknown.
    pub tempo_bpm: f32,
    /// Overall spectral tilt of the master bus (<0 dark).
    pub tilt: f32,
    /// Total onset density across the mix (onsets/s).
    pub onset_density: f32,
    /// Dynamic range estimate (dB) — crest of the master.
    pub dynamic_range_db: f32,
}

impl Default for MixAnalysis {
    fn default() -> Self {
        Self {
            tempo_bpm: 0.0,
            tilt: 0.0,
            onset_density: 0.0,
            dynamic_range_db: 12.0,
        }
    }
}

/// Infer the session theme from the live Node reports + master mix analysis. Returns
/// `(theme, confidence)`. Below [`CONF_MARGIN`] the theme is [`SessionTheme::Generic`].
pub fn infer_theme(nodes: &[NodeReport], mix: &MixAnalysis) -> (SessionTheme, f32) {
    if nodes.is_empty() {
        return (SessionTheme::Generic, 0.0);
    }
    let has = |t: InstrumentType| nodes.iter().any(|n| n.ty == t);
    let count = |t: InstrumentType| nodes.iter().filter(|n| n.ty == t).count() as f32;

    let has_kick = has(InstrumentType::Kick);
    let has_low = has(InstrumentType::Bass) || has(InstrumentType::Rumble);
    let has_pad = has(InstrumentType::Pad) || has(InstrumentType::Atmos);
    let has_breaks = has(InstrumentType::Breaks) || count(InstrumentType::Perc) >= 2.0;

    let tempo = mix.tempo_bpm;
    let dark = le(mix.tilt, -0.1, 0.6); // dark tilt
    let onset = mix.onset_density;

    // DARK-TECHNO: 4-floor kick + low end (rumble/bass) + sparse tops, slowish, dark.
    let techno = (if has_kick { 1.0 } else { 0.3 })
        * (if has_low { 1.0 } else { 0.4 })
        * band(tempo, 118.0, 140.0, 14.0).max(if tempo <= 0.0 { 0.6 } else { 0.0 })
        * (0.5 + 0.5 * dark)
        * le(onset, 8.0, 6.0);

    // DNB/BREAKS: fast tempo + break density + sub.
    let dnb = (if has_breaks { 1.0 } else { 0.4 })
        * (if has_low { 1.0 } else { 0.5 })
        * band(tempo, 160.0, 180.0, 18.0).max(if tempo <= 0.0 { 0.3 } else { 0.0 })
        * ge(onset, 6.0, 6.0);

    // AMBIENT/ATMOS: few onsets + wide/sustained pads + no driving kick.
    let ambient = (if has_pad { 1.0 } else { 0.4 })
        * le(onset, 2.0, 2.0)
        * (if has_kick { 0.4 } else { 1.0 });

    // HOUSE/GROOVE: four-on-the-floor kick, ~120–128, brighter than techno.
    let house = (if has_kick { 1.0 } else { 0.4 })
        * band(tempo, 118.0, 128.0, 8.0).max(if tempo <= 0.0 { 0.4 } else { 0.0 })
        * ge(mix.tilt, -0.2, 0.5)
        * band(onset, 2.0, 8.0, 4.0);

    let cands = [
        (SessionTheme::DarkTechno, techno),
        (SessionTheme::DnbBreaks, dnb),
        (SessionTheme::Ambient, ambient),
        (SessionTheme::HouseGroove, house),
    ];
    let mut best = SessionTheme::Generic;
    let mut best_v = 0.0f32;
    for (t, v) in cands.iter() {
        if *v > best_v {
            best_v = *v;
            best = *t;
        }
    }
    if best_v < CONF_MARGIN {
        (SessionTheme::Generic, best_v)
    } else {
        (best, best_v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsig;

    const SR: f32 = 48_000.0;

    /// Run a mono signal through the extractor in 512-sample blocks and return the rolling
    /// summary at the end.
    fn summarize_signal(sig: &[f32]) -> FeatureSummary {
        let mut fx = FeatureExtractor::new(SR);
        for chunk in sig.chunks(512) {
            fx.process_block(chunk, chunk);
        }
        fx.summary()
    }

    // ---- Fixture builders --------------------------------------------------

    /// A four-on-the-floor kick train (default kick every 0.25 s).
    fn kick_train(secs: f32) -> Vec<f32> {
        let n = (SR * secs) as usize;
        let mut out = vec![0.0f32; n];
        let period = (SR * 0.25) as usize;
        let one = testsig::synth_kick_stub((SR * 0.24) as usize, SR);
        let mut t = 0;
        while t < n {
            for (i, &v) in one.iter().enumerate() {
                if t + i < n {
                    out[t + i] += v;
                }
            }
            t += period;
        }
        out
    }

    /// A dense noise-burst train (percussion stand-in): short bright bursts ~12/s.
    fn noise_burst_train(secs: f32) -> Vec<f32> {
        let n = (SR * secs) as usize;
        let mut out = vec![0.0f32; n];
        let mut rng = testsig::Rng::new(0x9E37);
        let period = (SR / 12.0) as usize;
        let burst = (SR * 0.02) as usize;
        let mut t = 0;
        while t < n {
            for i in 0..burst {
                if t + i < n {
                    let env = 1.0 - (i as f32 / burst as f32);
                    out[t + i] += rng.next_bipolar() * env * 0.8;
                }
            }
            t += period;
        }
        out
    }

    /// A slow, wide, sustained chord pad (A-minor-ish), lowpassed, decorrelated L/R.
    fn wide_pad(secs: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (SR * secs) as usize;
        let freqs = [220.0f32, 261.63, 329.63];
        let mut lp_l = Svf::new();
        let mut lp_r = Svf::new();
        lp_l.set(1200.0, 0.707, SR);
        lp_r.set(1200.0, 0.707, SR);
        let mut l = vec![0.0f32; n];
        let mut r = vec![0.0f32; n];
        for i in 0..n {
            let t = i as f32 / SR;
            let mut sl = 0.0f32;
            let mut sr_ = 0.0f32;
            for (k, &fr) in freqs.iter().enumerate() {
                // Detune + phase offset between channels → stereo width.
                let ph = (fr * t).fract();
                sl += (2.0 * ph - 1.0) * 0.2;
                let ph_r = ((fr * 1.003) * t + 0.25 * (k as f32 + 1.0)).fract();
                sr_ += (2.0 * ph_r - 1.0) * 0.2;
            }
            // Slow attack over 0.5 s, then sustain.
            let env = (t / 0.5).min(1.0);
            l[i] = lp_l.process(sl * env).lp;
            r[i] = lp_r.process(sr_ * env).lp;
        }
        (l, r)
    }

    // ---- Done-bar: classifier fixtures ------------------------------------

    #[test]
    fn fixture_kick_classifies_as_kick() {
        let f = summarize_signal(&kick_train(4.0));
        let (ty, conf) = classify(&f);
        assert_eq!(ty, InstrumentType::Kick, "features: {f:?}");
        assert!(conf >= CONF_MARGIN, "kick confidence {conf} below margin");
    }

    #[test]
    fn fixture_bass_classifies_as_bass() {
        // Sustained/sliding saw in the bass register.
        let sig = testsig::sliding_saw(70.0, 62.0, 0.7, (SR * 4.0) as usize, SR);
        let f = summarize_signal(&sig);
        let (ty, conf) = classify(&f);
        assert_eq!(ty, InstrumentType::Bass, "features: {f:?}");
        assert!(conf >= CONF_MARGIN, "bass confidence {conf} below margin");
    }

    #[test]
    fn fixture_vocal_classifies_as_vocal() {
        let sig = testsig::synth_vocal(180.0, (SR * 4.0) as usize, SR);
        let f = summarize_signal(&sig);
        let (ty, conf) = classify(&f);
        assert_eq!(ty, InstrumentType::Vocal, "features: {f:?}");
        assert!(conf >= CONF_MARGIN, "vocal confidence {conf} below margin");
    }

    #[test]
    fn fixture_perc_classifies_as_perc_family() {
        let f = summarize_signal(&noise_burst_train(4.0));
        let (ty, conf) = classify(&f);
        assert!(
            matches!(ty, InstrumentType::Perc | InstrumentType::Hats),
            "noise-burst train classified {ty:?}, features: {f:?}"
        );
        assert!(conf >= CONF_MARGIN, "perc confidence {conf} below margin");
    }

    #[test]
    fn fixture_pad_classifies_as_pad_family() {
        let (l, r) = wide_pad(4.0);
        let mut fx = FeatureExtractor::new(SR);
        for (cl, cr) in l.chunks(512).zip(r.chunks(512)) {
            fx.process_block(cl, cr);
        }
        let f = fx.summary();
        let (ty, conf) = classify(&f);
        assert!(
            matches!(ty, InstrumentType::Pad | InstrumentType::Atmos),
            "wide pad classified {ty:?}, features: {f:?}"
        );
        assert!(conf >= CONF_MARGIN, "pad confidence {conf} below margin");
    }

    #[test]
    fn white_noise_and_silence_stay_below_margin() {
        // Steady white noise: broadband, unpitched, NO discrete onsets → no confident class.
        let noise = testsig::white_noise(0.5, (SR * 4.0) as usize, 4242);
        let (ty, conf) = classify(&summarize_signal(&noise));
        assert!(
            conf < CONF_MARGIN,
            "steady white noise falsely confident: {ty:?} @ {conf}"
        );

        // Silence.
        let silence = vec![0.0f32; (SR * 2.0) as usize];
        let (_, cs) = classify(&summarize_signal(&silence));
        assert!(cs < CONF_MARGIN, "silence produced confidence {cs}");
    }

    // ---- Done-bar: 4/5 correct above margin --------------------------------

    #[test]
    fn at_least_four_of_five_fixtures_correct() {
        let mut correct = 0;
        // KICK
        if classify(&summarize_signal(&kick_train(4.0))).0 == InstrumentType::Kick {
            correct += 1;
        }
        // BASS
        if classify(&summarize_signal(&testsig::sliding_saw(
            70.0,
            62.0,
            0.7,
            (SR * 4.0) as usize,
            SR,
        )))
        .0 == InstrumentType::Bass
        {
            correct += 1;
        }
        // VOCAL
        if classify(&summarize_signal(&testsig::synth_vocal(
            180.0,
            (SR * 4.0) as usize,
            SR,
        )))
        .0 == InstrumentType::Vocal
        {
            correct += 1;
        }
        // PERC
        if matches!(
            classify(&summarize_signal(&noise_burst_train(4.0))).0,
            InstrumentType::Perc | InstrumentType::Hats
        ) {
            correct += 1;
        }
        // PAD
        {
            let (l, r) = wide_pad(4.0);
            let mut fx = FeatureExtractor::new(SR);
            for (cl, cr) in l.chunks(512).zip(r.chunks(512)) {
                fx.process_block(cl, cr);
            }
            if matches!(
                classify(&fx.summary()).0,
                InstrumentType::Pad | InstrumentType::Atmos
            ) {
                correct += 1;
            }
        }
        assert!(correct >= 4, "only {correct}/5 fixtures classified correctly");
    }

    // ---- Done-bar: LEARN captures exactly N seconds, freezes on commit -----

    #[test]
    fn learn_captures_exactly_n_seconds_and_freezes() {
        let mut fx = FeatureExtractor::new(SR);
        let n = (SR * 2.0) as usize; // capture exactly 2 s
        fx.begin_capture(n);
        assert!(fx.capturing());

        // Play KICK during the window.
        let kick = kick_train(2.0);
        let mut fed = 0;
        for chunk in kick.chunks(512) {
            fx.process_block(chunk, chunk);
            fed += chunk.len();
            if fed >= n {
                break;
            }
        }
        // The capture must have finalised right at N samples.
        assert!(!fx.capturing(), "capture did not finalise at N samples");
        let captured = fx.take_capture().expect("capture result available");
        assert_eq!(
            classify(&captured).0,
            InstrumentType::Kick,
            "learned type must match the fixture played DURING the window: {captured:?}"
        );

        // Now play VOCAL AFTER commit — the already-taken result must be unchanged (there is
        // no second result to take).
        let vocal = testsig::synth_vocal(180.0, (SR * 2.0) as usize, SR);
        for chunk in vocal.chunks(512) {
            fx.process_block(chunk, chunk);
        }
        assert!(
            fx.take_capture().is_none(),
            "post-commit audio must not produce a new capture result"
        );
    }

    // ---- Done-bar: theme inference -----------------------------------------

    #[test]
    fn techno_session_infers_dark_techno() {
        // Kick + rumble + pad node streams; dark tilt; 130 BPM.
        let kick_f = summarize_signal(&kick_train(4.0));
        let rumble = testsig::sine(45.0, 0.5, (SR * 4.0) as usize, SR);
        let rumble_f = summarize_signal(&rumble);
        let (pl, pr) = wide_pad(4.0);
        let mut pfx = FeatureExtractor::new(SR);
        for (cl, cr) in pl.chunks(512).zip(pr.chunks(512)) {
            pfx.process_block(cl, cr);
        }
        let pad_f = pfx.summary();

        let nodes = [
            NodeReport {
                ty: classify(&kick_f).0,
                features: kick_f,
            },
            NodeReport {
                ty: InstrumentType::Rumble,
                features: rumble_f,
            },
            NodeReport {
                ty: classify(&pad_f).0,
                features: pad_f,
            },
        ];
        let mix = MixAnalysis {
            tempo_bpm: 130.0,
            tilt: -0.3,
            onset_density: 4.0,
            dynamic_range_db: 10.0,
        };
        let (theme, conf) = infer_theme(&nodes, &mix);
        assert_eq!(theme, SessionTheme::DarkTechno, "got {theme:?} @ {conf}");
        assert!(conf >= CONF_MARGIN);
    }

    #[test]
    fn empty_session_theme_is_generic() {
        let (t, c) = infer_theme(&[], &MixAnalysis::default());
        assert_eq!(t, SessionTheme::Generic);
        assert!(c < CONF_MARGIN);
    }
}
