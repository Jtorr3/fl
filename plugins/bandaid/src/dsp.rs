//! BANDAID — pure-DSP core for the multiband transient designer (SPECS "BANDAID").
//!
//! Signal flow (per channel, one [`BandaidCore`] per channel — the plugin owns two):
//! ```text
//!  x ─ LR4 split → low / mid / high
//!         per band: transient detector = fast env (1 ms) − slow env (50 ms)
//!                   diff > 0 → ATTACK region, diff < 0 → SUSTAIN/tail region
//!                   gain_dB = attack_dB·att_w + sustain_dB·sus_w   (±12 dB each)
//!                   g = db_to_lin(gain_dB), 5 ms-smoothed (no zipper)
//!  out = x + mix · Σ_b (g_b − 1)·band_b          (parallel-delta reconstruction)
//! ```
//!
//! **Why parallel-delta (`x + Σ (g−1)·band`) instead of `Σ g·band`?** An LR4 split-sum is
//! *allpass-flat* (unity magnitude, but 360° phase lag), so `Σ band ≠ x` and a naive
//! `Σ g·band` recombination would NOT null against the dry input. Adding only the per-band
//! *difference* the shaping makes means every band contributes `(g−1)·band`, which is
//! **exactly 0** when a band's gain is 0 dB (`g == 1`). So all-attack/sustain-0 ⇒
//! `out == x` bit-for-bit ⇒ the "neutral nulls to input" done-bar (PRD §4) holds to float
//! precision, regardless of crossover accuracy. Zero latency (minimum-phase LR4). Solo
//! auditions a single processed band (`g_b·band_b`), bypassing the dry.

use suite_core::db_to_lin;
use suite_core::dsp::{Detector, EnvFollower, OnePole, Svf};

/// Number of bands (fixed 3-way LR4).
pub const NUM_BANDS: usize = 3;

/// Final safety clamp — a pure runaway/NaN guard well above full scale (±8.0 ≈ +18 dBFS),
/// NOT a level ceiling. The old ±0.999 clamp digitally clipped boosted transients and any
/// legitimate >0 dBFS float headroom (routine on FL buses); identity for any real signal.
const CEILING: f32 = 8.0;

/// Envelope floor used as the transient-difference denominator (~−80 dBFS). Keeps the
/// attack/sustain weights finite near silence; irrelevant to the null (band ≈ 0 there).
const ENV_FLOOR: f32 = 1.0e-4;

/// Block-rate settings snapshot (filled from the params / NERVE listen layer each block).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Low↔mid crossover (Hz).
    pub xover_low: f32,
    /// Mid↔high crossover (Hz).
    pub xover_high: f32,
    /// Per-band attack-region gain (dB, ±12), index 0=low 1=mid 2=high.
    pub attack_db: [f32; NUM_BANDS],
    /// Per-band sustain-region gain (dB, ±12).
    pub sustain_db: [f32; NUM_BANDS],
    /// Per-band solo/listen toggle.
    pub solo: [bool; NUM_BANDS],
    /// Detector time scale (1.0 = 1 ms fast / 50 ms slow; <1 faster, >1 slower).
    pub det_scale: f32,
    /// Dry↔shaped blend (1.0 = fully shaped). `0` returns the dry input exactly.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            xover_low: 150.0,
            xover_high: 2500.0,
            attack_db: [0.0; NUM_BANDS],
            sustain_db: [0.0; NUM_BANDS],
            solo: [false; NUM_BANDS],
            det_scale: 1.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

/// One LR4 (4th-order Linkwitz-Riley) crossover: two cascaded Butterworth TPT-SVF stages.
/// `split` returns `(low, high)` summing to a flat magnitude (allpass phase). Copied from the
/// TRACER / OVERSEER split precedent — the TPT SVF is unconditionally stable.
#[derive(Clone, Copy, Default)]
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
    #[inline]
    fn split(&mut self, x: f32) -> (f32, f32) {
        let low = self.lp2.process(self.lp1.process(x).lp).lp;
        let high = self.hp2.process(self.hp1.process(x).hp).hp;
        (low, high)
    }
}

/// Per-band transient detector + smoothed applied gain.
#[derive(Clone, Copy)]
struct Band {
    fast: EnvFollower,
    slow: EnvFollower,
    gain: OnePole, // 5 ms smoother on the applied linear gain (primed to 1.0)
}

impl Band {
    fn new(sr: f32) -> Self {
        let mut gain = OnePole::new();
        gain.set_time(5.0, sr);
        gain.reset(1.0);
        Self {
            fast: EnvFollower::new(Detector::Peak),
            slow: EnvFollower::new(Detector::Peak),
            gain,
        }
    }
    fn reset(&mut self) {
        self.fast.reset();
        self.slow.reset();
        self.gain.reset(1.0);
    }
}

/// Mono BANDAID core (the plugin runs one per channel). Shared verbatim with the harness
/// tests, so the tested math is the shipped math.
pub struct BandaidCore {
    sr: f32,
    xo_low: Lr4,
    xo_high: Lr4,
    bands: [Band; NUM_BANDS],

    // cached block-rate state
    attack_db: [f32; NUM_BANDS],
    sustain_db: [f32; NUM_BANDS],
    solo: [bool; NUM_BANDS],
    any_solo: bool,
    det_scale: f32,

    // per-sample smoothed globals
    mix: OnePole,
    out: OnePole,
    mix_target: f32,
    out_target: f32,

    primed: bool,
}

impl BandaidCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let mut mix = OnePole::new();
        mix.set_time(5.0, sr);
        mix.reset(1.0);
        let mut out = OnePole::new();
        out.set_time(5.0, sr);
        out.reset(1.0);
        let mut core = Self {
            sr,
            xo_low: Lr4::default(),
            xo_high: Lr4::default(),
            bands: [Band::new(sr); NUM_BANDS],
            attack_db: [0.0; NUM_BANDS],
            sustain_db: [0.0; NUM_BANDS],
            solo: [false; NUM_BANDS],
            any_solo: false,
            det_scale: 1.0,
            mix,
            out,
            mix_target: 1.0,
            out_target: 1.0,
            primed: false,
        };
        core.set_det_times(1.0);
        core.xo_low.set(150.0, sr);
        core.xo_high.set(2500.0, sr);
        core
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Zero latency — the dry path is never delayed (minimum-phase LR4).
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn reset(&mut self) {
        self.xo_low.reset();
        self.xo_high.reset();
        for b in self.bands.iter_mut() {
            b.reset();
        }
        self.mix.reset(self.mix_target);
        self.out.reset(self.out_target);
    }

    fn set_det_times(&mut self, scale: f32) {
        let s = scale.clamp(0.25, 4.0);
        for b in self.bands.iter_mut() {
            // Fast env: 1 ms attack catches onsets; a longer (60 ms) release keeps the peak
            // envelope from *rippling* on low-frequency content (which would spuriously fire
            // the attack detector each half-cycle). Slow env lags with a 40 ms attack, so the
            // fast-minus-slow overshoot marks the rising onset; through the decaying tail the
            // fast env sits below the slower-releasing slow env ⇒ no false attack.
            b.fast.set_times(1.0 * s, 60.0 * s, self.sr);
            b.slow.set_times(40.0 * s, 150.0 * s, self.sr);
        }
    }

    /// Apply a block-rate settings snapshot. Crossovers + env times recompute here; the
    /// per-sample smoothers chase `mix`/`out`.
    pub fn configure(&mut self, s: &Settings) {
        // Crossover clamp (pluginval fuzzes params to range edges; keep min<max ordered so
        // `clamp` never panics, and keep low < high).
        let max_hz = (self.sr * 0.45).max(100.0);
        let lo = s.xover_low.clamp(20.0, max_hz - 20.0);
        let hi = s.xover_high.clamp(lo + 10.0, max_hz);
        self.xo_low.set(lo, self.sr);
        self.xo_high.set(hi, self.sr);

        self.attack_db = s.attack_db;
        self.sustain_db = s.sustain_db;
        self.solo = s.solo;
        self.any_solo = s.solo.iter().any(|&v| v);

        let scale = s.det_scale.clamp(0.25, 4.0);
        if (scale - self.det_scale).abs() > 1.0e-6 {
            self.det_scale = scale;
            self.set_det_times(scale);
        }

        self.mix_target = s.mix.clamp(0.0, 1.0);
        self.out_target = db_to_lin(s.out_db);
        if !self.primed {
            self.mix.reset(self.mix_target);
            self.out.reset(self.out_target);
            self.primed = true;
        }
    }

    /// Process one sample. Returns the shaped output for this channel.
    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let (low, rest) = self.xo_low.split(x);
        let (mid, high) = self.xo_high.split(rest);
        let bands = [low, mid, high];

        let mut delta_sum = 0.0f32;
        let mut solo_sum = 0.0f32;
        for b in 0..NUM_BANDS {
            let band = bands[b];
            let ef = self.bands[b].fast.process(band);
            let es = self.bands[b].slow.process(band);
            let diff = ef - es;
            let denom = es.max(ENV_FLOOR);
            // SPL-style transient split: the ATTACK region is where the fast envelope
            // overshoots the slow one (a rising onset); the SUSTAIN region is everything
            // else (steady body + decaying tail), so `sus_w = 1 − att_w`. A steady tone is
            // fully "sustain"; a transient onset is fully "attack".
            let att_w = (diff / denom).clamp(0.0, 1.0);
            let sus_w = 1.0 - att_w;
            let gain_db = self.attack_db[b] * att_w + self.sustain_db[b] * sus_w;
            // gain_db == 0 exactly when both band gains are 0 dB ⇒ target == 1.0 ⇒ delta == 0.
            let target = db_to_lin(gain_db);
            let g = self.bands[b].gain.process(target);
            delta_sum += (g - 1.0) * band;
            if self.solo[b] {
                solo_sum += g * band;
            }
        }

        let mix = self.mix.process(self.mix_target);
        let out_g = self.out.process(self.out_target);

        let y = if self.any_solo {
            solo_sum
        } else {
            // Parallel-delta reconstruction: neutral (all deltas 0) ⇒ y == x exactly.
            x + mix * delta_sum
        };
        (y * out_g).clamp(-CEILING, CEILING)
    }
}

impl Clone for BandaidCore {
    fn clone(&self) -> Self {
        // Fresh core at the same sample rate (used to build the stereo pair); state is reset.
        BandaidCore::new(self.sr)
    }
}

// The harness `Processor` runs the mono core over a block in place (tests the exact shipped
// path, including mix/out). The plugin configures the core before rendering.
impl suite_core::harness::Processor for BandaidCore {
    #[inline]
    fn process(&mut self, block: &mut [f32]) {
        for s in block.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}
