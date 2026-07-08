//! CARVE — pure-DSP core for the spectral ducker (SPECS "CARVE", Trackspacer clone).
//!
//! ```text
//! main ─ STFT(2048, hop 512, Hann) ─┐
//!                                    ├─ per 1/3-oct band: gain = soft-knee(SC band energy vs
//! sidechain (mono-sum) ─ STFT ───────┘   threshold) → attack/release smoothed at hop rate
//!                                        → tilt / max-depth / sensitivity shaping
//! main bins ×= interpolated per-band gain ─ iSTFT/OLA ─ mix ─ out
//! ```
//!
//! The more energy the **sidechain** has in a ~1/3-octave band, the deeper CARVE cuts the
//! **main** signal's matching band — a spectral (Trackspacer-style) ducker that carves a
//! frequency-matched pocket for the sidechain instead of ducking the whole broadband level.
//!
//! Design mirrors SMUDGE's STFT recipe (streaming `suite_core::stft::Stft`, 2048/512, WOLA
//! identity, latency-matched dry `DelayLine` for the mix null). Two STFT *roles* run in
//! lockstep per sample: one **sidechain** analysis STFT (mono, computes per-band gains at the
//! frame boundary) and one **main** STFT per channel (applies those gains to its bins). Because
//! all STFTs share the 2048/512 geometry and are advanced together, the SC frame fires on the
//! same sample as the main frames — the SC callback updates the shared gain table first, then
//! the main callbacks read it (exactly SMUDGE's primary/secondary split-borrow pattern).
//!
//! **Exact-bypass / null guarantee:** when the sidechain is silent every band's reduction
//! envelope sits at 0 dB → the shared gain table is unity → the per-bin multiply is skipped
//! entirely (fast path) so the main path collapses to the STFT's own identity reconstruction,
//! which nulls against the latency-delayed dry below −60 dB (the honest STFT round-trip bound;
//! `mix=0` taps the delayed dry directly and nulls below −80 dB).
//!
//! **Δ-listen:** the per-bin gain `g` and its complement `1−g` partition the spectrum, so the
//! carved output and the Δ (residual = what's removed) sum back to the STFT reconstruction of
//! the dry — the energy-bookkeeping done-bar.
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the offline
//! harness tests. All scratch is preallocated — the per-sample path is allocation-free (safe
//! under nih-plug's `assert_process_allocs`).

use suite_core::dsp::DelayLine;
use suite_core::stft::{Complex, Stft};

pub const FFT_SIZE: usize = 2048;
pub const HOP: usize = 512;
/// ~1/3-octave band groups spanning 20 Hz .. 20 kHz (≈10 octaves × 3).
pub const N_BANDS: usize = 30;
pub const F_LO: f32 = 20.0;
pub const F_HI: f32 = 20_000.0;
/// Number of aggregated bars published to the GUI reduction meter.
pub const N_DISPLAY: usize = 14;
/// Hard ceiling on the max-depth param (dB) — also the meter's full-scale reference.
pub const MAX_DEPTH_LIMIT: f32 = 24.0;

/// A band's reduction envelope below this (dB) counts as "no reduction" → the whole per-bin
/// gain multiply is skipped, guaranteeing the STFT-identity null when the SC is silent.
const REDUCTION_EPS_DB: f32 = 1.0e-4;
/// Log-energy floor so silent bands map to a very negative dB, well under any threshold.
const ENERGY_EPS: f32 = 1.0e-12;
/// STFT bin-magnitude normalisation: a full-scale sine's peak bin ≈ N/4 (Hann coherent gain
/// 0.5 × N/2), so scaling magnitude by 4/N puts a full-scale in-band tone at ≈ 0 dB.
const MAG_SCALE: f32 = 4.0 / FFT_SIZE as f32;

/// Merge a per-sample smoothed parameter value with a block-rate NERVE modulation delta,
/// clamped to the param's plain `[min, max]` range. `delta` is `modulated_plain − base_plain`,
/// computed once per block from the listen layer; when no route is live `delta == 0` and the
/// result is exactly `smoothed` (bit-identical to the unmodulated path). Alloc-free.
#[inline]
pub fn apply_mod_delta(smoothed: f32, delta: f32, min: f32, max: f32) -> f32 {
    (smoothed + delta).clamp(min, max)
}

/// Monitoring / output mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenMode {
    /// Normal carved output (dry ducked by the SC-matched spectral gains).
    Off,
    /// Pass the sidechain signal straight through (audition what's controlling the duck).
    Sidechain,
    /// Output the carved residual — i.e. exactly what is being removed from the main.
    Delta,
}

/// A full snapshot of CARVE's controls (plain, un-normalized values). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// 0..1 — overall duck amount (scales the per-band reduction depth).
    pub amount: f32,
    /// 0..MAX_DEPTH_LIMIT — maximum reduction depth (dB) at full SC energy.
    pub max_depth_db: f32,
    /// SC band level (dB) above which ducking begins (soft-knee centre).
    pub threshold_db: f32,
    /// −1..1 — bias the depth toward lows (−) or highs (+). 0 = flat.
    pub tilt: f32,
    /// Attack time (ms) of the per-band reduction envelope (fast: reduction deepens).
    pub attack_ms: f32,
    /// Release time (ms) of the per-band reduction envelope (slow: reduction lets go).
    pub release_ms: f32,
    /// 0..1 — sensitivity: soft-knee width + how little SC excess reaches full depth.
    /// 0 = gentle/wide knee, 1 = aggressive/narrow knee.
    pub sens: f32,
    /// Monitoring / output mode.
    pub listen: ListenMode,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (linear gain).
    pub out_gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            amount: 0.7,
            max_depth_db: 12.0,
            threshold_db: -45.0,
            tilt: 0.0,
            attack_ms: 15.0,
            release_ms: 150.0,
            sens: 0.5,
            listen: ListenMode::Off,
            mix: 1.0,
            out_gain: 1.0,
        }
    }
}

/// Soft-knee "excess" mapper: 0 below the knee, linear above it, quadratic in-between. Returns
/// how far (dB, ≥0) the input sits above the (knee-softened) threshold.
#[inline]
fn soft_over(x: f32, knee: f32) -> f32 {
    let k = knee.max(1.0e-3);
    if x <= -0.5 * k {
        0.0
    } else if x >= 0.5 * k {
        x
    } else {
        let t = x + 0.5 * k;
        t * t / (2.0 * k)
    }
}

/// Per-band depth tilt weight in [1−|tilt|, 1]. `tilt > 0` spares lows / ducks highs more,
/// `tilt < 0` the inverse. Never exceeds 1, so it cannot push depth past `max_depth`.
#[inline]
fn tilt_weight(g: usize, tilt: f32) -> f32 {
    if tilt.abs() < 1.0e-6 || N_BANDS < 2 {
        return 1.0;
    }
    let f_norm = g as f32 / (N_BANDS - 1) as f32;
    let tside = 2.0 * f_norm - 1.0; // −1 = lowest band, +1 = highest
    let ramp = 0.5 - 0.5 * tside * tilt.signum();
    (1.0 - tilt.abs() * ramp).clamp(0.0, 1.0)
}

/// Derived, sample-rate-dependent config recomputed only in [`CarveCore::configure`]
/// (block rate). Holds the base settings, the bin→band maps, and the frame-rate env coefs.
struct Cfg {
    settings: Settings,
    /// Group index (0..N_BANDS-1) each bin's energy accumulates into.
    bin_band: Vec<usize>,
    /// Lower group index for the smooth per-bin gain interpolation.
    bin_g0: Vec<usize>,
    /// Interpolation fraction toward `bin_g0 + 1`.
    bin_frac: Vec<f32>,
    /// Per-frame attack / release one-pole coefficients (frame rate = sr / HOP).
    att_coef: f32,
    rel_coef: f32,
}

impl Cfg {
    fn new(num_bins: usize, sr: f32) -> Self {
        let ln_lo = F_LO.ln();
        let ln_span = F_HI.ln() - ln_lo;
        let mut bin_band = vec![0usize; num_bins];
        let mut bin_g0 = vec![0usize; num_bins];
        let mut bin_frac = vec![0.0f32; num_bins];
        for k in 0..num_bins {
            let f = (k as f32 * sr / FFT_SIZE as f32).clamp(F_LO, F_HI);
            let pos01 = (f.ln() - ln_lo) / ln_span; // 0..1 in log-freq
            // Accumulation group.
            bin_band[k] = ((pos01 * N_BANDS as f32) as usize).min(N_BANDS - 1);
            // Interpolation position: band *centres* sit at (g+0.5)/N, so shift by −0.5.
            let p = (pos01 * N_BANDS as f32 - 0.5).clamp(0.0, (N_BANDS - 1) as f32);
            let g0 = p.floor() as usize;
            bin_g0[k] = g0.min(N_BANDS - 1);
            bin_frac[k] = p - g0 as f32;
        }
        Self {
            settings: Settings::default(),
            bin_band,
            bin_g0,
            bin_frac,
            att_coef: 0.0,
            rel_coef: 0.0,
        }
    }

    /// Recompute the frame-rate attack/release coefficients from the current settings.
    fn update_coefs(&mut self, sr: f32) {
        let hop_time = HOP as f32 / sr.max(1.0);
        let coef = |ms: f32| {
            let tau = (ms * 1.0e-3).max(1.0e-5);
            (-hop_time / tau).exp()
        };
        self.att_coef = coef(self.settings.attack_ms);
        self.rel_coef = coef(self.settings.release_ms);
    }
}

/// Shared per-frame ducking state, computed by the SC callback and read by the main callbacks.
struct Shared {
    /// Per-group reduction envelope (dB, ≥0), attack/release-smoothed at hop rate.
    env_depth_db: [f32; N_BANDS],
    /// Per-group linear gain 10^(−env_depth/20), cached for the per-bin interpolation.
    gain_lin: [f32; N_BANDS],
    /// Scratch: per-group SC energy accumulator (rebuilt each SC frame).
    energy: [f32; N_BANDS],
    /// Largest current reduction (dB) — drives the fast bypass path + the meter.
    max_reduction_db: f32,
}

impl Shared {
    fn new() -> Self {
        Self {
            env_depth_db: [0.0; N_BANDS],
            gain_lin: [1.0; N_BANDS],
            energy: [0.0; N_BANDS],
            max_reduction_db: 0.0,
        }
    }

    fn reset(&mut self) {
        self.env_depth_db = [0.0; N_BANDS];
        self.gain_lin = [1.0; N_BANDS];
        self.energy = [0.0; N_BANDS];
        self.max_reduction_db = 0.0;
    }

    /// SC analysis frame: measure per-band SC energy, map to a soft-knee reduction target, and
    /// attack/release-smooth the per-group reduction envelope. Called once per hop.
    fn sc_frame(&mut self, spec: &[Complex<f32>], cfg: &Cfg) {
        let s = &cfg.settings;
        // Sensitivity → soft-knee width + excess span (dB to reach full depth).
        let knee_w = 12.0 - 10.0 * s.sens.clamp(0.0, 1.0); // 12 dB (gentle) → 2 dB (tight)
        let span = 30.0 - 22.0 * s.sens.clamp(0.0, 1.0); //   30 dB (gentle) → 8 dB (tight)
        let amount = s.amount.clamp(0.0, 1.0);
        let max_depth = s.max_depth_db.clamp(0.0, MAX_DEPTH_LIMIT);

        // Accumulate per-band SC energy from the (normalised) magnitude spectrum.
        for e in self.energy.iter_mut() {
            *e = 0.0;
        }
        for (k, c) in spec.iter().enumerate() {
            let m = c.norm() * MAG_SCALE;
            self.energy[cfg.bin_band[k]] += m * m;
        }

        let mut max_red = 0.0f32;
        for g in 0..N_BANDS {
            let e_db = 10.0 * (self.energy[g] + ENERGY_EPS).log10();
            let over = soft_over(e_db - s.threshold_db, knee_w);
            let frac = (over / span.max(1.0e-3)).clamp(0.0, 1.0);
            let target = max_depth * amount * frac * tilt_weight(g, s.tilt);

            // Attack when the cut is deepening, release when it lets go.
            let cur = self.env_depth_db[g];
            let coef = if target > cur { cfg.att_coef } else { cfg.rel_coef };
            let next = target + coef * (cur - target);
            self.env_depth_db[g] = next;
            self.gain_lin[g] = 10.0f32.powf(-next / 20.0);
            if next > max_red {
                max_red = next;
            }
        }
        self.max_reduction_db = max_red;
    }
}

/// CARVE's full stereo DSP core.
pub struct CarveCore {
    num_bins: usize,
    cfg: Cfg,
    sr: f32,
    /// Sidechain analysis STFT (mono).
    stft_sc: Stft,
    /// Main STFTs (per channel) — apply the carved gains.
    stft_l: Stft,
    stft_r: Stft,
    shared: Shared,
    dry_l: DelayLine,
    dry_r: DelayLine,
}

impl CarveCore {
    pub fn new(sample_rate: f32) -> Self {
        let nb = FFT_SIZE / 2 + 1;
        let sr = sample_rate.max(1.0);
        let mut cfg = Cfg::new(nb, sr);
        cfg.update_coefs(sr);
        Self {
            num_bins: nb,
            cfg,
            sr,
            stft_sc: Stft::new(FFT_SIZE, HOP),
            stft_l: Stft::new(FFT_SIZE, HOP),
            stft_r: Stft::new(FFT_SIZE, HOP),
            shared: Shared::new(),
            dry_l: DelayLine::new(FFT_SIZE),
            dry_r: DelayLine::new(FFT_SIZE),
        }
    }

    /// Latency (samples) this core adds — equal to the STFT frame size.
    pub fn latency(&self) -> usize {
        FFT_SIZE
    }

    pub fn num_bins(&self) -> usize {
        self.num_bins
    }

    pub fn reset(&mut self) {
        self.stft_sc.reset();
        self.stft_l.reset();
        self.stft_r.reset();
        self.shared.reset();
        self.dry_l.reset();
        self.dry_r.reset();
        // The delay lines default to their max delay; pin the dry delay to the STFT latency.
        self.dry_l.set_delay(FFT_SIZE);
        self.dry_r.set_delay(FFT_SIZE);
    }

    /// Latch a settings snapshot (call at block rate). Env coefs are recomputed here.
    pub fn configure(&mut self, s: &Settings) {
        self.cfg.settings = *s;
        self.cfg.update_coefs(self.sr);
        self.dry_l.set_delay(FFT_SIZE);
        self.dry_r.set_delay(FFT_SIZE);
    }

    /// Largest current reduction (dB) across all bands.
    pub fn max_reduction_db(&self) -> f32 {
        self.shared.max_reduction_db
    }

    /// Current per-group reduction envelope (dB). For tests / diagnostics.
    pub fn band_reduction_db(&self) -> [f32; N_BANDS] {
        self.shared.env_depth_db
    }

    /// Aggregated reduction bars (0..1, normalised to [`MAX_DEPTH_LIMIT`]) for the GUI meter.
    pub fn display_reductions(&self) -> [f32; N_DISPLAY] {
        let mut out = [0.0f32; N_DISPLAY];
        for (d, slot) in out.iter_mut().enumerate() {
            let lo = d * N_BANDS / N_DISPLAY;
            let hi = ((d + 1) * N_BANDS / N_DISPLAY).max(lo + 1).min(N_BANDS);
            let mut m = 0.0f32;
            for g in lo..hi {
                m = m.max(self.shared.env_depth_db[g]);
            }
            *slot = (m / MAX_DEPTH_LIMIT).clamp(0.0, 1.0);
        }
        out
    }

    /// Apply the shared per-band gains to one main spectrum in place (or its complement for
    /// Δ-listen). Skipped entirely when there is no reduction → exact STFT identity.
    #[inline]
    fn apply_gains(spec: &mut [Complex<f32>], shared: &Shared, cfg: &Cfg, delta: bool) {
        if shared.max_reduction_db < REDUCTION_EPS_DB && !delta {
            return; // no ducking → identity reconstruction (preserves the SC-silent null)
        }
        for (k, c) in spec.iter_mut().enumerate() {
            let g0 = cfg.bin_g0[k];
            let g1 = (g0 + 1).min(N_BANDS - 1);
            let frac = cfg.bin_frac[k];
            let g = shared.gain_lin[g0] + (shared.gain_lin[g1] - shared.gain_lin[g0]) * frac;
            let mul = if delta { 1.0 - g } else { g };
            *c = *c * mul;
        }
    }

    /// Process one stereo sample with a mono sidechain sample. Returns the final stereo output
    /// (mix + listen + out trim applied). `mix`/`out_gain` are per-sample so they smooth.
    #[inline]
    pub fn process_sample(
        &mut self,
        l: f32,
        r: f32,
        sc: f32,
        mix: f32,
        out_gain: f32,
    ) -> (f32, f32) {
        let listen = self.cfg.settings.listen;
        let delta = listen == ListenMode::Delta;

        // 1) Sidechain analysis STFT — updates the shared per-band gains on the frame boundary.
        {
            let shared = &mut self.shared;
            let cfg = &self.cfg;
            self.stft_sc
                .process(sc, &mut |spec| shared.sc_frame(spec, cfg));
        }
        // 2) Main STFTs — apply the (just-updated) carved gains.
        let wet_l = {
            let shared = &self.shared;
            let cfg = &self.cfg;
            self.stft_l
                .process(l, &mut |spec| Self::apply_gains(spec, shared, cfg, delta))
        };
        let wet_r = {
            let shared = &self.shared;
            let cfg = &self.cfg;
            self.stft_r
                .process(r, &mut |spec| Self::apply_gains(spec, shared, cfg, delta))
        };

        // Latency-matched dry for the mix null.
        let dry_l = self.dry_l.process(l);
        let dry_r = self.dry_r.process(r);

        match listen {
            ListenMode::Sidechain => {
                // Audition the sidechain (mono → both channels), still trimmed by Out.
                let y = sc * out_gain;
                (y, y)
            }
            ListenMode::Delta => {
                // The residual IS the output (what's being removed); mix is not applied.
                (wet_l * out_gain, wet_r * out_gain)
            }
            ListenMode::Off => {
                let m = mix.clamp(0.0, 1.0);
                (
                    (dry_l + m * (wet_l - dry_l)) * out_gain,
                    (dry_r + m * (wet_r - dry_r)) * out_gain,
                )
            }
        }
    }

    /// Offline stereo convenience for the harness/tests: process `main` (interleaved-free L/R
    /// vecs) against a mono `sc` with fixed `Settings`, returning (L, R).
    pub fn process_stereo(
        &mut self,
        left: &[f32],
        right: &[f32],
        sc: &[f32],
        s: &Settings,
    ) -> (Vec<f32>, Vec<f32>) {
        self.configure(s);
        let n = left.len().min(right.len()).min(sc.len());
        let mut ol = vec![0.0f32; n];
        let mut or = vec![0.0f32; n];
        for i in 0..n {
            let (a, b) = self.process_sample(left[i], right[i], sc[i], s.mix, s.out_gain);
            ol[i] = a;
            or[i] = b;
        }
        (ol, or)
    }

    /// Offline mono convenience: main = sidechain = `buf`, left channel out. (Used for the
    /// universal render assertions where a single carved signal is enough.)
    pub fn process_mono(&mut self, buf: &mut [f32], sc: &[f32], s: &Settings) {
        self.configure(s);
        let n = buf.len().min(sc.len());
        for i in 0..n {
            let (a, _) = self.process_sample(buf[i], buf[i], sc[i], s.mix, s.out_gain);
            buf[i] = a;
        }
    }
}
