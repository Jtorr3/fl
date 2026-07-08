//! VOXFIT — pure-DSP core for the vocal character conformer (SPECS "VOXFIT"). API-agnostic
//! Rust, shared verbatim between the nih-plug `process` path and the offline harness tests.
//!
//! ```text
//! in ─┬─ formant shift (±5 st, pitch-INDEPENDENT: ShiftEngine pitch_ratio=1,
//!     │   formant_ratio=2^(st/12), envelope-preserve ON)
//!     │      ↓
//!     │   de-esser  (5–9 kHz band via SVF HP→LP split, EnvFollower-keyed downward
//!     │              gain on the band only; threshold / amount / listen)
//!     │      ↓
//!     │   harshness tamer (dynamic bell cut 2–5 kHz: subtract k·bandpass, k follows
//!     │              band energy over threshold × amount)
//!     │      ↓
//!     │   tilt EQ   (complementary low+high shelves pivoting at 1 kHz, ±6 dB)
//!     │      ↓
//!     │   proximity (low-mid shelf ~300 Hz, ±)
//!     │      ↓
//!     │   air       (high shelf 12 kHz, ±)  ─────────────────────── wet ──┐
//!     └─ delay(latency=2048) ───────────────────────────────── dry ───────┤ mix
//!                                                                          └── × out ── clip ── out
//! ```
//!
//! The formant [`ShiftEngine`] introduces the only real latency (its FFT size, 2048); the dry
//! path is delayed to match so `mix=0` nulls exactly against the latency-matched dry. All EQ /
//! de-ess / harsh stages are minimum-phase (biquad / SVF) so they add no reported latency.
//! Everything is preallocated in [`VoxFitCore::new`]; the per-sample path is allocation-free.
//!
//! The **SIT** macro is folded into the effective [`Settings`] in [`Controls::resolve`] (a curated
//! blend: slight formant character, mild de-ess, a 2–5 kHz presence dip, tilt toward dark, a touch
//! of proximity), so the DSP core stays macro-agnostic and `sit=0` leaves every value untouched.

use suite_core::dsp::{DelayLine, EnvFollower, Detector, OnePole, Svf};
use suite_core::shift::{ShiftEngine, DEFAULT_FFT, DEFAULT_HOP};

mod biquad;
pub use biquad::Biquad;

/// Main analysis FFT for the formant-preserving shift (== reported latency).
pub const MAIN_FFT: usize = DEFAULT_FFT;
const MAIN_HOP: usize = DEFAULT_HOP;

/// Audible-scalar smoothing time (ms).
const SMOOTH_MS: f32 = 12.0;

// De-esser crossover (sibilant content = everything above 5 kHz), harshness band (2–5 kHz),
// EQ pivots. The de-ess split is complementary (low + high = x) so the sibilant band can be
// reduced *fully*; a single band-pass would leave an un-removable residual (its passband gain
// is < 1 across a sub-octave 5–9 kHz band).
const DEESS_XOVER: f32 = 5000.0;
/// Upper edge of the sibilant band. The de-esser keys and ducks only the band **between**
/// `DEESS_XOVER` and this — true air (> `DEESS_AIR`, the 12 kHz shelf region and up) is split
/// off and passed at unity, so ducking an ess never dulls the vocal's sparkle. (Previously the
/// sibilant band was `x − low` = everything above 5 kHz → a de-ess pulled the air down as hard
/// as the sibilance, measured −20 to −33 dB of 11–18 kHz air on the de-ess-heavy presets.)
const DEESS_AIR: f32 = 10000.0;
/// Gain-reduction smoothing (ms) on the de-ess band. Rounds the infinite-ratio `gr` step so it
/// no longer clicks on an ess onset (measured 3 click outliers/render before, ratio up to 19.5),
/// while staying short enough to catch the sibilant onset (a longer smoother lets the un-ducked
/// onset transient through).
const DEESS_GR_MS: f32 = 0.5;
const HARSH_CENTER: f32 = 3162.0; // geo-mean of 2000..5000
const HARSH_Q: f32 = 1.05; // fc / bandwidth ≈ 3162 / 3000
const TILT_PIVOT: f32 = 1000.0;
const PROX_FREQ: f32 = 300.0;
const AIR_FREQ: f32 = 12000.0;
const SHELF_SLOPE: f32 = 0.7;

/// Max dynamic bell cut the harshness tamer will apply (dB).
const MAX_HARSH_CUT_DB: f32 = 18.0;
/// Wet-path safety-clip knee (identity below, tanh above → |y| < 1).
const CLIP_KNEE: f32 = 0.9;

/// Effective formant ratio within this of unity → bypass the phase vocoder (it is an audible-
/// but-pointless identity there, nulling only ~−15 dB and smearing transients).
const FORMANT_BYPASS_EPS: f32 = 1.0e-4;
/// Crossfade time (ms) between the PV output and the latency-matched dry when the bypass toggles.
const PV_XFADE_MS: f32 = 15.0;

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Wet-path safety clip: exact identity for |x| ≤ 0.9, tanh-compressed above so |y| < 1.
/// The identity region keeps the `mix=0` (dry) null exact and only touches EQ-boosted peaks.
#[inline]
fn safety_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= CLIP_KNEE {
        x
    } else {
        let over = (a - CLIP_KNEE) / (1.0 - CLIP_KNEE);
        x.signum() * (CLIP_KNEE + (1.0 - CLIP_KNEE) * over.tanh())
    }
}

/// Per-sample one-pole scalar smoother.
#[derive(Clone, Copy)]
struct Smooth {
    op: OnePole,
    target: f32,
}
impl Smooth {
    fn new(sr: f32, init: f32) -> Self {
        let mut op = OnePole::new();
        op.set_time(SMOOTH_MS, sr);
        op.reset(init);
        Self { op, target: init }
    }
    #[inline]
    fn set(&mut self, t: f32) {
        self.target = t;
    }
    #[inline]
    fn next(&mut self) -> f32 {
        self.op.process(self.target)
    }
    fn reset(&mut self, v: f32) {
        self.op.reset(v);
        self.target = v;
    }
}

// ---------------------------------------------------------------------------
// Settings + Controls
// ---------------------------------------------------------------------------

/// A full snapshot of VOXFIT's *effective* controls (SIT macro already folded in). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Formant shift ratio (2^(st/12)); pitch stays put (preserve ON, pitch_ratio = 1).
    pub formant_ratio: f32,
    /// De-esser threshold (linear amplitude of the 5–9 kHz band envelope).
    pub deess_thresh: f32,
    /// De-esser amount 0..1 (0 = off, 1 = pull the band down to threshold on sibilants).
    pub deess_amount: f32,
    /// De-esser listen: output only the removed sibilant content for tuning.
    pub deess_listen: bool,
    /// Harshness-tamer threshold (linear amplitude of the 2–5 kHz band envelope).
    pub harsh_thresh: f32,
    /// Harshness-tamer amount 0..1 (scales the dynamic bell-cut depth).
    pub harsh_amount: f32,
    /// Tilt EQ (dB): >0 bright (high-shelf up / low-shelf down), <0 dark. Pivot 1 kHz.
    pub tilt_db: f32,
    /// Proximity low-mid shelf gain (dB) at ~300 Hz.
    pub prox_db: f32,
    /// Air high shelf gain (dB) at 12 kHz.
    pub air_db: f32,
    /// Dry/wet mix 0..1 (0 = pure dry, latency-matched).
    pub mix: f32,
    /// Output trim (linear).
    pub out_gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            formant_ratio: 1.0,
            deess_thresh: db_to_gain(-24.0),
            deess_amount: 0.0,
            deess_listen: false,
            harsh_thresh: db_to_gain(-24.0),
            harsh_amount: 0.0,
            tilt_db: 0.0,
            prox_db: 0.0,
            air_db: 0.0,
            mix: 1.0,
            out_gain: 1.0,
        }
    }
}

/// Raw control values in natural param units — built by both the plugin's param snapshot and the
/// preset loader, then resolved (folding the SIT macro) into effective [`Settings`].
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub formant_st: f32,
    pub deess_thresh_db: f32,
    pub deess_amount: f32,
    pub deess_listen: bool,
    pub harsh_thresh_db: f32,
    pub harsh_amount: f32,
    pub tilt_db: f32,
    pub prox_db: f32,
    pub air_db: f32,
    pub sit: f32,
    pub mix: f32,
    pub out_db: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            formant_st: 0.0,
            deess_thresh_db: -24.0,
            deess_amount: 0.0,
            deess_listen: false,
            harsh_thresh_db: -24.0,
            harsh_amount: 0.0,
            tilt_db: 0.0,
            prox_db: 0.0,
            air_db: 0.0,
            sit: 0.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

impl Controls {
    /// Resolve to effective [`Settings`], folding in the SIT macro (a curated blend tuned for
    /// dropping a bright pop vocal into a dark mix). `sit = 0` leaves every value untouched.
    pub fn resolve(&self) -> Settings {
        let sit = self.sit.clamp(0.0, 1.0);
        // Curated SIT offsets (added on top of the user's base values, then clamped to range).
        let formant_st = (self.formant_st + sit * -1.0).clamp(-5.0, 5.0);
        let deess_amount = (self.deess_amount + sit * 0.45).clamp(0.0, 1.0);
        let deess_thresh_db = (self.deess_thresh_db + sit * -8.0).clamp(-60.0, 0.0);
        let harsh_amount = (self.harsh_amount + sit * 0.45).clamp(0.0, 1.0);
        let harsh_thresh_db = (self.harsh_thresh_db + sit * -6.0).clamp(-60.0, 0.0);
        let tilt_db = (self.tilt_db + sit * -4.0).clamp(-6.0, 6.0);
        let prox_db = (self.prox_db + sit * 2.0).clamp(-6.0, 6.0);
        let air_db = self.air_db.clamp(-6.0, 6.0);

        Settings {
            formant_ratio: 2.0f32.powf(formant_st / 12.0),
            deess_thresh: db_to_gain(deess_thresh_db),
            deess_amount,
            deess_listen: self.deess_listen,
            harsh_thresh: db_to_gain(harsh_thresh_db),
            harsh_amount,
            tilt_db,
            prox_db,
            air_db,
            mix: self.mix.clamp(0.0, 1.0),
            out_gain: db_to_gain(self.out_db),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-channel character strip (de-ess → harsh → tilt → proximity → air)
// ---------------------------------------------------------------------------

/// The minimum-phase character chain for one channel. Biquad EQ coefficients are shared between
/// channels (recomputed at block rate in [`VoxFitCore::configure`]); each channel owns its own
/// filter *state*. The de-ess / harsh detectors run per channel (independent stereo de-essing).
struct ChannelStrip {
    // De-ess 3-way complementary split (low + sib + air = x exactly, so the sibilant band can
    // still be reduced *fully*): two cascaded SVF low-passes @5k give LOW (<5k), two cascaded
    // SVF high-passes @10k give AIR (>10k), and SIB = x − low − air is the 5–10 kHz sibilant
    // band. Only SIB is keyed and ducked; AIR always passes at unity.
    deess_lp_a: Svf,
    deess_lp_b: Svf,
    deess_air_a: Svf,
    deess_air_b: Svf,
    deess_env: EnvFollower,
    // Smoother on the de-ess gain reduction — declicks the infinite-ratio `gr` step at ess onset.
    deess_gr: OnePole,
    // Harsh band: unity-peak (0 dB) band-pass; its output feeds both the detector and the subtract.
    harsh_bp: Biquad,
    harsh_env: EnvFollower,
    // EQ: complementary tilt shelves, proximity low shelf, air high shelf.
    tilt_low: Biquad,
    tilt_high: Biquad,
    prox: Biquad,
    air: Biquad,
}

impl ChannelStrip {
    fn new(sr: f32) -> Self {
        let mut deess_lp_a = Svf::new();
        deess_lp_a.set(DEESS_XOVER, 0.707, sr);
        let mut deess_lp_b = Svf::new();
        deess_lp_b.set(DEESS_XOVER, 0.707, sr);
        let mut deess_air_a = Svf::new();
        deess_air_a.set(DEESS_AIR, 0.707, sr);
        let mut deess_air_b = Svf::new();
        deess_air_b.set(DEESS_AIR, 0.707, sr);
        let mut deess_env = EnvFollower::new(Detector::Peak);
        deess_env.set_times(1.0, 60.0, sr);
        let mut deess_gr = OnePole::new();
        deess_gr.set_time(DEESS_GR_MS, sr);
        deess_gr.reset(1.0);
        let mut harsh_env = EnvFollower::new(Detector::Peak);
        harsh_env.set_times(3.0, 80.0, sr);
        let mut harsh_bp = Biquad::default();
        harsh_bp.bandpass_0db(HARSH_CENTER, HARSH_Q, sr);
        Self {
            deess_lp_a,
            deess_lp_b,
            deess_air_a,
            deess_air_b,
            deess_env,
            deess_gr,
            harsh_bp,
            harsh_env,
            tilt_low: Biquad::default(),
            tilt_high: Biquad::default(),
            prox: Biquad::default(),
            air: Biquad::default(),
        }
    }

    fn reset(&mut self) {
        self.deess_lp_a.reset();
        self.deess_lp_b.reset();
        self.deess_air_a.reset();
        self.deess_air_b.reset();
        self.deess_env.reset();
        self.deess_gr.reset(1.0);
        self.harsh_bp.reset();
        self.harsh_env.reset();
        self.tilt_low.reset();
        self.tilt_high.reset();
        self.prox.reset();
        self.air.reset();
    }

    /// Process one sample through the character chain. `x` is the formant-shifted input.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        x: f32,
        deess_thresh: f32,
        deess_amount: f32,
        deess_listen: bool,
        harsh_thresh: f32,
        harsh_amount: f32,
    ) -> f32 {
        // --- De-esser: 3-way complementary split (low + sib + air = x). Key & duck only the
        // 5–10 kHz sibilant band; AIR (>10 kHz) always passes at unity, so ducking an ess never
        // pulls the vocal's sparkle down with it. ---
        let low = self.deess_lp_b.process(self.deess_lp_a.process(x).lp).lp;
        let air = self.deess_air_b.process(self.deess_air_a.process(x).hp).hp;
        let sib = x - low - air;
        let env = self.deess_env.process(sib);
        // gr ∈ (0,1]: infinite-ratio downward gain, scaled by amount. amount=0 → gr=1 (bypass).
        let gr_target = if env > deess_thresh && deess_amount > 0.0 {
            (deess_thresh / env.max(1.0e-9)).powf(deess_amount)
        } else {
            1.0
        };
        // Smooth the gain reduction (declick the onset step; still ms-fast).
        let gr = self.deess_gr.process(gr_target);
        if deess_listen {
            // Monitor the removed sibilant content (silent at rest, lights up on esses).
            return (1.0 - gr) * sib;
        }
        // out = low + gr·sib + air ⇒ exact identity when gr = 1, full sibilant removal as gr → 0,
        // air preserved throughout.
        let deessed = low + gr * sib + air;

        // --- Harshness tamer: dynamic bell cut at 2–5 kHz (subtract k·bandpass, no coeff clicks). ---
        let hb = self.harsh_bp.process(deessed);
        let henv = self.harsh_env.process(hb);
        let k = if henv > harsh_thresh && harsh_amount > 0.0 {
            let over_db = 20.0 * (henv / harsh_thresh.max(1.0e-9)).log10();
            let cut_db = -(over_db * harsh_amount).clamp(0.0, MAX_HARSH_CUT_DB);
            (1.0 - db_to_gain(cut_db)).clamp(0.0, 0.95)
        } else {
            0.0
        };
        let tamed = deessed - k * hb;

        // --- Static EQ: tilt shelves → proximity shelf → air shelf. ---
        let y = self.tilt_low.process(tamed);
        let y = self.tilt_high.process(y);
        let y = self.prox.process(y);
        self.air.process(y)
    }

    /// Recompute the EQ biquad coefficients (block rate) from smoothed dB targets.
    fn configure_eq(&mut self, tilt_db: f32, prox_db: f32, air_db: f32, sr: f32) {
        // Complementary tilt shelves pivoting at 1 kHz: dark (tilt<0) boosts lows, cuts highs.
        self.tilt_low.low_shelf(TILT_PIVOT, -tilt_db, SHELF_SLOPE, sr);
        self.tilt_high.high_shelf(TILT_PIVOT, tilt_db, SHELF_SLOPE, sr);
        self.prox.low_shelf(PROX_FREQ, prox_db, SHELF_SLOPE, sr);
        self.air.high_shelf(AIR_FREQ, air_db, SHELF_SLOPE, sr);
    }
}

// ---------------------------------------------------------------------------
// VoxFitCore
// ---------------------------------------------------------------------------

/// VOXFIT's full stereo DSP core.
pub struct VoxFitCore {
    sr: f32,
    settings: Settings,

    shift_l: ShiftEngine,
    shift_r: ShiftEngine,
    dry_l: DelayLine,
    dry_r: DelayLine,
    strip_l: ChannelStrip,
    strip_r: ChannelStrip,

    // Per-sample smoothed scalars.
    sm_formant: Smooth,
    sm_deess_thresh: Smooth,
    sm_deess_amount: Smooth,
    sm_harsh_thresh: Smooth,
    sm_harsh_amount: Smooth,
    sm_mix: Smooth,
    sm_out: Smooth,

    // Block-rate smoothed EQ dB (stepped once per configure), reconfigure biquads from these.
    eq_tilt: OnePole,
    eq_prox: OnePole,
    eq_air: OnePole,

    // PV-bypass crossfade: 0 = full phase-vocoder output into the character chain, 1 = the
    // latency-matched dry (bypass). Ramps by `pv_xf_step` per sample when the target flips.
    pv_bypass: f32,
    pv_xf_step: f32,
}

impl VoxFitCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let lat = MAIN_FFT;
        let d = Settings::default();

        let mut shift_l = ShiftEngine::new(MAIN_FFT, MAIN_HOP, sr);
        let mut shift_r = ShiftEngine::new(MAIN_FFT, MAIN_HOP, sr);
        shift_l.set_envelope_preserve(true);
        shift_r.set_envelope_preserve(true);
        shift_l.set_pitch_ratio(1.0);
        shift_r.set_pitch_ratio(1.0);

        let mut dry_l = DelayLine::new(lat + 1);
        let mut dry_r = DelayLine::new(lat + 1);
        dry_l.set_delay(lat);
        dry_r.set_delay(lat);

        let mk_eq = |init: f32| {
            let mut op = OnePole::new();
            op.set_time(SMOOTH_MS, sr);
            op.reset(init);
            op
        };

        let mut core = Self {
            sr,
            settings: d,
            shift_l,
            shift_r,
            dry_l,
            dry_r,
            strip_l: ChannelStrip::new(sr),
            strip_r: ChannelStrip::new(sr),
            sm_formant: Smooth::new(sr, d.formant_ratio),
            sm_deess_thresh: Smooth::new(sr, d.deess_thresh),
            sm_deess_amount: Smooth::new(sr, d.deess_amount),
            sm_harsh_thresh: Smooth::new(sr, d.harsh_thresh),
            sm_harsh_amount: Smooth::new(sr, d.harsh_amount),
            sm_mix: Smooth::new(sr, d.mix),
            sm_out: Smooth::new(sr, d.out_gain),
            eq_tilt: mk_eq(d.tilt_db),
            eq_prox: mk_eq(d.prox_db),
            eq_air: mk_eq(d.air_db),
            pv_bypass: if (d.formant_ratio - 1.0).abs() < FORMANT_BYPASS_EPS { 1.0 } else { 0.0 },
            pv_xf_step: 1.0 / (PV_XFADE_MS * 0.001 * sr).max(1.0),
        };
        core.strip_l.configure_eq(d.tilt_db, d.prox_db, d.air_db, sr);
        core.strip_r.configure_eq(d.tilt_db, d.prox_db, d.air_db, sr);
        core
    }

    /// Reported constant latency in samples (the ShiftEngine FFT size).
    pub fn latency_samples(&self) -> u32 {
        MAIN_FFT as u32
    }

    pub fn reset(&mut self) {
        self.shift_l.reset();
        self.shift_r.reset();
        self.dry_l.reset();
        self.dry_r.reset();
        self.strip_l.reset();
        self.strip_r.reset();
        self.sm_formant.reset(self.settings.formant_ratio);
        self.sm_deess_thresh.reset(self.settings.deess_thresh);
        self.sm_deess_amount.reset(self.settings.deess_amount);
        self.sm_harsh_thresh.reset(self.settings.harsh_thresh);
        self.sm_harsh_amount.reset(self.settings.harsh_amount);
        self.sm_mix.reset(self.settings.mix);
        self.sm_out.reset(self.settings.out_gain);
        self.eq_tilt.reset(self.settings.tilt_db);
        self.eq_prox.reset(self.settings.prox_db);
        self.eq_air.reset(self.settings.air_db);
        self.pv_bypass =
            if (self.settings.formant_ratio - 1.0).abs() < FORMANT_BYPASS_EPS { 1.0 } else { 0.0 };
        self.strip_l
            .configure_eq(self.settings.tilt_db, self.settings.prox_db, self.settings.air_db, self.sr);
        self.strip_r
            .configure_eq(self.settings.tilt_db, self.settings.prox_db, self.settings.air_db, self.sr);
    }

    /// Latch a settings snapshot and step the block-rate EQ smoothers / reconfigure the shelves.
    /// `block_size` is the number of samples this configure covers (the host buffer length): the
    /// EQ smoothers are stepped **once per block**, so their one-pole coefficient is derived
    /// against the *block* rate (`sr / block_size`) — giving a ~`SMOOTH_MS` settle in real time
    /// regardless of buffer size. (Previously the coefficient was cut for the *sample* rate but
    /// only advanced once per block, making tone knobs / preset loads creep in over seconds.)
    pub fn configure(&mut self, s: &Settings, block_size: usize) {
        self.settings = *s;
        self.shift_l.set_envelope_preserve(true);
        self.shift_r.set_envelope_preserve(true);
        self.shift_l.set_pitch_ratio(1.0);
        self.shift_r.set_pitch_ratio(1.0);

        self.sm_formant.set(s.formant_ratio);
        self.sm_deess_thresh.set(s.deess_thresh);
        self.sm_deess_amount.set(s.deess_amount);
        self.sm_harsh_thresh.set(s.harsh_thresh);
        self.sm_harsh_amount.set(s.harsh_amount);
        self.sm_mix.set(s.mix);
        self.sm_out.set(s.out_gain);

        // Re-derive the EQ smoother coefficients for the block cadence, then take one step.
        let block_rate = (self.sr / block_size.max(1) as f32).max(1.0);
        self.eq_tilt.set_time(SMOOTH_MS, block_rate);
        self.eq_prox.set_time(SMOOTH_MS, block_rate);
        self.eq_air.set_time(SMOOTH_MS, block_rate);
        let tilt = self.eq_tilt.process(s.tilt_db);
        let prox = self.eq_prox.process(s.prox_db);
        let air = self.eq_air.process(s.air_db);
        self.strip_l.configure_eq(tilt, prox, air, self.sr);
        self.strip_r.configure_eq(tilt, prox, air, self.sr);
    }

    /// Process one stereo sample pair.
    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let formant = self.sm_formant.next();
        let deess_thresh = self.sm_deess_thresh.next();
        let deess_amount = self.sm_deess_amount.next();
        let harsh_thresh = self.sm_harsh_thresh.next();
        let harsh_amount = self.sm_harsh_amount.next();
        let mix = self.sm_mix.next();
        let out_g = self.sm_out.next();
        let listen = self.settings.deess_listen;

        // Pitch-independent formant shift. The engine is fed **every** sample even while the
        // bypass is engaged, so its phase-vocoder state stays warm and re-engaging is click-free.
        self.shift_l.set_formant_ratio(formant);
        self.shift_r.set_formant_ratio(formant);
        let sl = self.shift_l.process(in_l);
        let sr = self.shift_r.process(in_r);

        // Latency-matched dry (also the character-chain source when the PV is bypassed — the dry
        // line is already delayed by exactly the ShiftEngine latency, so no extra delay needed).
        let dl = self.dry_l.process(in_l);
        let dr = self.dry_r.process(in_r);

        // PV bypass: at unity effective formant the phase vocoder is a lossy identity (nulls only
        // ~−15 dB, smears transients), so a de-ess/tilt/air-only use still pays for PV coloration.
        // Crossfade the character-chain input between the PV output and the latency-matched dry;
        // PDC and the dry mix path are unchanged.
        let bypass_target =
            if (self.settings.formant_ratio - 1.0).abs() < FORMANT_BYPASS_EPS { 1.0 } else { 0.0 };
        if self.pv_bypass < bypass_target {
            self.pv_bypass = (self.pv_bypass + self.pv_xf_step).min(bypass_target);
        } else if self.pv_bypass > bypass_target {
            self.pv_bypass = (self.pv_bypass - self.pv_xf_step).max(bypass_target);
        }
        let src_l = sl + self.pv_bypass * (dl - sl);
        let src_r = sr + self.pv_bypass * (dr - sr);

        // Character chain.
        let wl = self.strip_l.process(src_l, deess_thresh, deess_amount, listen, harsh_thresh, harsh_amount);
        let wr = self.strip_r.process(src_r, deess_thresh, deess_amount, listen, harsh_thresh, harsh_amount);

        // Mix (mix=0 → latency-matched dry, exact) + out trim, safety-clipped.
        let out_l = safety_clip(out_g * ((1.0 - mix) * dl + mix * wl));
        let out_r = safety_clip(out_g * ((1.0 - mix) * dr + mix * wr));
        (out_l, out_r)
    }

    /// Offline convenience: process stereo slices in place with fixed settings.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], s: &Settings) {
        let n = left.len().min(right.len());
        self.configure(s, n.max(1));
        self.reset();
        self.configure(s, n.max(1));
        for i in 0..n {
            let (l, r) = self.process_sample(left[i], right[i]);
            left[i] = l;
            right[i] = r;
        }
    }

    /// Offline mono convenience: duplicate to stereo, process, return the L channel.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        let n = buf.len().max(1);
        self.configure(s, n);
        self.reset();
        self.configure(s, n);
        for x in buf.iter_mut() {
            let (l, _r) = self.process_sample(*x, *x);
            *x = l;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_neutral_is_identity_settings() {
        let s = Controls::default().resolve();
        assert!((s.formant_ratio - 1.0).abs() < 1e-6);
        assert_eq!(s.deess_amount, 0.0);
        assert_eq!(s.harsh_amount, 0.0);
        assert_eq!(s.tilt_db, 0.0);
        assert_eq!(s.mix, 1.0);
    }

    #[test]
    fn sit_macro_pushes_curated_direction() {
        let mut c = Controls::default();
        c.sit = 1.0;
        let s = c.resolve();
        assert!(s.formant_ratio < 1.0, "sit should lower formants slightly");
        assert!(s.deess_amount > 0.0, "sit should engage mild de-ess");
        assert!(s.harsh_amount > 0.0, "sit should engage the presence dip");
        assert!(s.tilt_db < 0.0, "sit should tilt dark");
        assert!(s.prox_db > 0.0, "sit should add proximity");
    }

    #[test]
    fn mix_zero_equals_latency_delayed_dry() {
        let sr = 48_000.0f32;
        let mut core = VoxFitCore::new(sr);
        // Non-trivial character so the wet path is clearly different from dry.
        let mut c = Controls::default();
        c.formant_st = 4.0;
        c.deess_amount = 1.0;
        c.deess_thresh_db = -40.0;
        c.harsh_amount = 1.0;
        c.tilt_db = -6.0;
        c.prox_db = 4.0;
        c.air_db = 4.0;
        c.mix = 0.0; // dry
        let s = c.resolve();
        core.configure(&s, 512);
        core.reset();
        core.configure(&s, 512);
        let input = suite_core::testsig::synth_vocal(150.0, (sr * 0.5) as usize, sr);
        let lat = MAIN_FFT;
        let mut max_err = 0.0f32;
        for (i, &x) in input.iter().enumerate() {
            let (l, _r) = core.process_sample(x, x);
            if i >= lat {
                max_err = max_err.max((l - input[i - lat]).abs());
            }
        }
        assert!(max_err < 1.0e-5, "mix=0 not equal to latency-delayed dry: {max_err}");
    }

    #[test]
    fn tone_smoother_settles_in_block_time() {
        // Drive configure() at a realistic 512-sample / 48 kHz block cadence with a step change in
        // tilt and assert the smoothed EQ target reaches >90% of the step within ~30 ms of block-
        // time (a handful of blocks) — not the seconds the mis-scaled per-sample coefficient gave.
        let sr = 48_000.0f32;
        let block = 512usize;
        let block_ms = 1000.0 * block as f32 / sr;
        let mut core = VoxFitCore::new(sr);
        core.reset(); // EQ smoothers jump to the default (tilt 0)
        let mut c = Controls::default();
        c.tilt_db = 6.0;
        let s = c.resolve();
        let target = s.tilt_db;
        let mut settle_ms = f32::INFINITY;
        for b in 1..=64 {
            core.configure(&s, block);
            if core.eq_tilt.value() >= 0.9 * target {
                settle_ms = b as f32 * block_ms;
                break;
            }
        }
        assert!(
            settle_ms <= 35.0,
            "tilt smoother reached 90% in {settle_ms} ms of block-time (want ~30 ms / a few blocks, not seconds)"
        );
    }

    #[test]
    fn unity_formant_bypasses_pv_and_nulls_dry() {
        // At formant = 0 st (unity) + all character neutral + mix = 1, the wet path should bypass
        // the phase vocoder and null against the latency-delayed dry far better than the PV
        // identity (~−15 dB). Assert the residual is well below −40 dB.
        let sr = 48_000.0f32;
        let mut core = VoxFitCore::new(sr);
        let s = Controls::default().resolve(); // formant 0, neutral, mix 1
        core.configure(&s, 512);
        core.reset();
        core.configure(&s, 512);
        let input = suite_core::testsig::synth_vocal(150.0, (sr * 0.7) as usize, sr);
        let lat = MAIN_FFT;
        let start = lat + 4096;
        let mut num = 0.0f64;
        let mut den = 0.0f64;
        for (i, &x) in input.iter().enumerate() {
            let (l, _r) = core.process_sample(x, x);
            if i >= start {
                let d = input[i - lat];
                let e = (l - d) as f64;
                num += e * e;
                den += (d as f64) * (d as f64);
            }
        }
        let residual_db = 10.0 * (num / den.max(1e-20)).log10();
        assert!(
            residual_db < -40.0,
            "unity-formant wet path residual {residual_db:.1} dB not below −40 dB (PV bypass failed)"
        );
    }

    #[test]
    fn pv_bypass_toggle_is_click_free() {
        // Toggling the formant between unity (bypass) and a shift (PV engaged) must crossfade —
        // no full-scale single-sample discontinuity. Feed a steady tone and switch mid-stream.
        let sr = 48_000.0f32;
        let mut core = VoxFitCore::new(sr);
        let neutral = Controls::default().resolve(); // unity → bypass
        let shifted = {
            let mut c = Controls::default();
            c.formant_st = 4.0;
            c.resolve()
        };
        let n = 12_288usize;
        core.configure(&neutral, 512);
        core.reset();
        let mut prev = 0.0f32;
        let mut max_jump = 0.0f32;
        for i in 0..n {
            let s = if i < n / 2 {
                &neutral
            } else if i < 3 * n / 4 {
                &shifted
            } else {
                &neutral
            };
            if i % 512 == 0 {
                core.configure(s, 512);
            }
            let x = 0.2 * (std::f32::consts::TAU * 220.0 * i as f32 / sr).sin();
            let (l, _r) = core.process_sample(x, x);
            if i > MAIN_FFT + 1024 {
                max_jump = max_jump.max((l - prev).abs());
            }
            prev = l;
        }
        // A 220 Hz, 0.2-amp sine steps ≲0.007/sample; a missing crossfade would swap PV↔dry
        // instantly (a jump of order 0.1–0.4). Catch that without false-tripping on the tone.
        assert!(
            max_jump < 0.1,
            "PV bypass toggle produced a {max_jump:.3} sample jump (click — crossfade missing)"
        );
    }

    #[test]
    fn safety_clip_is_identity_in_range() {
        assert_eq!(safety_clip(0.5), 0.5);
        assert_eq!(safety_clip(-0.9), -0.9);
        // f32 tanh saturates, so the extreme caps at exactly 1.0 (peak ≤ 0 dBFS is the guarantee).
        assert!(safety_clip(2.0) <= 1.0 && safety_clip(2.0) > 0.9);
        assert!(safety_clip(1.1) < 1.0, "moderate overshoot must compress below full scale");
    }
}
