//! ASCEND tension-generator DSP core (pure Rust, shared by the plugin and the offline harness).
//!
//! Signal flow (SPECS "ASCEND"):
//! ```text
//! transport (tempo, bar pos) ─▶ COUNTDOWN to the next N-bar boundary ─▶ tension env (curve-morphed)
//!                                                                          │ drives:
//!   filtered noise (white↔pink) ─┐                                        ├─ SVF cutoff  start→end (up)
//!   tonal stack root+fifth (saw↔ │─▶ mid/side ─ SVF sweep ─ vol swell ────┤─ pitch rise  0→24 st
//!   sine)  × 2^(env·rise/12) ─────┘                    │                   ├─ width bloom narrow→wide
//!                                                       │                   └─ volume      quiet→full
//!   boundary ─▶ embedded impact (synth_kick, low) + auto-cut gate ─▶ + ─▶ soft-clip ─▶ out (stereo)
//! ```
//! One master tension envelope. In **riser** mode it climbs 0→1 across the countdown window and the
//! sources gate to silence at the boundary (auto-cut), re-arming for the next boundary. In
//! **downlifter** mode it starts full at the boundary and falls away over the window. With the
//! transport stopped a manual TRIGGER (or a MIDI note) runs the same envelope over a time-based
//! length so the instrument works standalone. Alloc-free `process_sample`; the impact one-shot is
//! synthesized once at construction and played back by index.

use std::f32::consts::TAU;
use suite_core::db_to_lin;
use suite_core::dsp::Svf;
use suite_core::testsig::{synth_kick, KickSpec, Rng};

/// Lowest key reference: C0 = 16.35 Hz. `root_hz(key, octave)` builds up from here.
pub const C0_HZ: f32 = 16.351_6;
/// Just-tempered fifth ratio (7 semitones).
pub const FIFTH_RATIO: f32 = 1.498_307; // 2^(7/12)
/// Maximum pitch rise (semitones) the tonal stack can reach at env = 1 (SPECS: 0–24 st).
pub const MAX_RISE_ST: f32 = 24.0;
/// Length (samples @ engine SR) of the embedded impact one-shot.
const IMPACT_SECS: f32 = 0.9;
/// Auto-cut fade + silence hold after a boundary (riser mode).
const AUTOCUT_FADE_MS: f32 = 4.0;
const AUTOCUT_HOLD_MS: f32 = 45.0;
/// Volume-swell floor: at env = 0 the sources sit "quiet" (this fraction of full) rather than
/// fully silent, so the riser is audible from the start and swells to full at the boundary. The
/// auto-cut gate is what produces true silence at the drop, not the volume curve.
const VOL_FLOOR: f32 = 0.05;

/// Sync target: how the countdown window length (in bars) is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncTarget {
    Bars8,
    Bars16,
    Bars32,
    Custom,
}

impl SyncTarget {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => SyncTarget::Bars8,
            1 => SyncTarget::Bars16,
            2 => SyncTarget::Bars32,
            _ => SyncTarget::Custom,
        }
    }
    /// Resolve the window length in bars, given the custom-bars override.
    pub fn window_bars(self, custom_bars: f32) -> f32 {
        match self {
            SyncTarget::Bars8 => 8.0,
            SyncTarget::Bars16 => 16.0,
            SyncTarget::Bars32 => 32.0,
            SyncTarget::Custom => custom_bars.max(1.0),
        }
    }
}

/// All ASCEND DSP parameters, snapshotted from the nih-plug params once per block.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub key: usize,          // 0..11 root pitch class (C..B)
    pub octave: i32,         // root octave (C0 = octave 0)
    pub sync: SyncTarget,    // window target
    pub custom_bars: f32,    // window length when sync = Custom / free-run reference
    pub curve: f32,          // 0 exp .. 0.5 linear .. 1 log envelope morph
    pub balance: f32,        // 0 = all noise .. 1 = all tonal
    pub color: f32,          // 0 = white .. 1 = pink noise
    pub wave: f32,           // 0 = saw .. 1 = sine tonal blend
    pub filter_start_hz: f32,// SVF cutoff at env = 0
    pub filter_end_hz: f32,  // SVF cutoff at env = 1
    pub rise_st: f32,        // pitch rise at env = 1 (0..24)
    pub width: f32,          // width-bloom maximum (0..1)
    pub impact_on: bool,     // fire the embedded impact at the boundary
    pub impact_level: f32,   // 0..1
    pub auto_cut: bool,      // gate sources to silence at the boundary (riser)
    pub downlifter: bool,    // reversed envelope after the boundary
    pub free_len_s: f32,     // free-run (transport stopped) envelope length in seconds
    pub level_db: f32,       // output trim
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            key: 0,          // C
            octave: 2,       // C2 ≈ 65 Hz
            sync: SyncTarget::Bars8,
            custom_bars: 8.0,
            curve: 0.5,
            balance: 0.5,
            color: 0.7,
            wave: 0.35,
            filter_start_hz: 180.0,
            filter_end_hz: 9000.0,
            rise_st: 12.0,
            width: 0.6,
            impact_on: true,
            impact_level: 0.8,
            auto_cut: true,
            downlifter: false,
            free_len_s: 4.0,
            level_db: -3.0,
        }
    }
}

impl Settings {
    /// Root fundamental (Hz) from key + octave.
    pub fn root_hz(&self) -> f32 {
        let semis = self.octave as f32 * 12.0 + self.key as f32;
        C0_HZ * 2.0f32.powf(semis / 12.0)
    }
    /// Resolved countdown window length in bars.
    pub fn window_bars(&self) -> f32 {
        self.sync.window_bars(self.custom_bars)
    }
}

/// Map the 0..1 curve control to a shaping exponent γ so `p^γ` morphs the envelope:
/// c=0 → γ≈3.2 (exponential: slow start, fast finish), c=0.5 → 1 (linear),
/// c=1 → γ≈0.31 (logarithmic: fast start, slow finish). Always > 0.
#[inline]
fn curve_gamma(curve: f32) -> f32 {
    2.0f32.powf((0.5 - curve.clamp(0.0, 1.0)) * 3.32)
}

/// Paul Kellet economical pink-noise filter (per-sample, stateful).
#[derive(Clone, Copy, Default)]
struct Pink {
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
    b6: f32,
}

impl Pink {
    #[inline]
    fn process(&mut self, white: f32) -> f32 {
        self.b0 = 0.99886 * self.b0 + white * 0.0555179;
        self.b1 = 0.99332 * self.b1 + white * 0.0750759;
        self.b2 = 0.96900 * self.b2 + white * 0.1538520;
        self.b3 = 0.86650 * self.b3 + white * 0.3104856;
        self.b4 = 0.55000 * self.b4 + white * 0.5329522;
        self.b5 = -0.7616 * self.b5 - white * 0.0168980;
        let pink = self.b0 + self.b1 + self.b2 + self.b3 + self.b4 + self.b5 + self.b6 + white * 0.5362;
        self.b6 = white * 0.115926;
        pink * 0.11
    }
}

/// A per-block transport snapshot handed to the engine (kept plain so the DSP core stays
/// free of any host type — the plugin fills it from `nih_plug`, the tests from a fake).
#[derive(Clone, Copy, Debug)]
pub struct TransportFrame {
    pub playing: bool,
    /// Song position in bars (fractional, 0-based) at the start of the block.
    pub bar_pos: f64,
    /// Bars advanced per output sample (tempo·timesig derived).
    pub bars_per_sample: f64,
}

/// The tension-generator voice (stereo).
pub struct AscendEngine {
    sr: f32,

    // config (resolved from Settings each block)
    root_hz: f32,
    window_bars: f32,
    gamma: f32,
    balance: f32,
    color: f32,
    wave: f32,
    f_start: f32,
    f_end: f32,
    rise_st: f32,
    width: f32,
    impact_on: bool,
    impact_level: f32,
    auto_cut: bool,
    downlifter: bool,
    free_len_s: f32,
    out_gain: f32,
    root_override: Option<f32>,

    // transport (per block)
    playing: bool,
    bar_pos: f64,
    bars_per_sample: f64,
    last_boundary: i64,
    transport_primed: bool,

    // envelope state
    env: f32,

    // free-run one-shot
    free_active: bool,
    free_t: f32, // seconds since trigger

    // oscillators
    phase_root: f32,
    phase_fifth: f32,

    // noise
    rng_a: Rng,
    rng_b: Rng,
    pink_a: Pink,
    pink_b: Pink,
    svf_mid: Svf,
    svf_side: Svf,

    // impact one-shot (pre-synthesized at construction; played back by index)
    impact_buf: Vec<f32>,
    impact_pos: usize,
    impact_playing: bool,

    // auto-cut gate
    gate: f32,        // current source gate (0..1)
    gate_target: f32, // 0 during a cut, 1 otherwise
    gate_coef: f32,   // one-pole fade coefficient
    gate_hold: u32,   // samples of held silence remaining
    gate_hold_len: u32,
}

impl AscendEngine {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        // Embedded impact: IMPACT's own kick math at a low pitch, synthesized once.
        let impact_buf = synth_kick(
            &KickSpec {
                f_start: 120.0,
                f_end: 42.0,
                pitch_decay_s: 0.06,
                amp_decay_s: 0.42,
                click: 0.12,
                sub_level: 0.6,
                sub_ratio: 0.5,
                drive: 0.3,
            },
            (IMPACT_SECS * sr) as usize,
            sr,
        );
        let fade = ((AUTOCUT_FADE_MS * 0.001) * sr).max(1.0);
        let mut e = Self {
            sr,
            root_hz: 65.41,
            window_bars: 8.0,
            gamma: 1.0,
            balance: 0.5,
            color: 0.7,
            wave: 0.35,
            f_start: 180.0,
            f_end: 9000.0,
            rise_st: 12.0,
            width: 0.6,
            impact_on: true,
            impact_level: 0.8,
            auto_cut: true,
            downlifter: false,
            free_len_s: 4.0,
            out_gain: db_to_lin(-3.0),
            root_override: None,
            playing: false,
            bar_pos: 0.0,
            bars_per_sample: 0.0,
            last_boundary: 0,
            transport_primed: false,
            env: 0.0,
            free_active: false,
            free_t: 0.0,
            phase_root: 0.0,
            phase_fifth: 0.0,
            rng_a: Rng::new(0x0A5C_3D11),
            rng_b: Rng::new(0x7E11_2B93),
            pink_a: Pink::default(),
            pink_b: Pink::default(),
            svf_mid: Svf::new(),
            svf_side: Svf::new(),
            impact_buf,
            impact_pos: 0,
            impact_playing: false,
            gate: 1.0,
            gate_target: 1.0,
            gate_coef: (-1.0 / fade).exp(),
            gate_hold: 0,
            gate_hold_len: ((AUTOCUT_HOLD_MS * 0.001) * sr) as u32,
        };
        e.svf_mid.set(180.0, 0.8, sr);
        e.svf_side.set(180.0, 0.8, sr);
        e
    }

    pub fn reset(&mut self) {
        self.env = 0.0;
        self.free_active = false;
        self.free_t = 0.0;
        self.phase_root = 0.0;
        self.phase_fifth = 0.0;
        self.pink_a = Pink::default();
        self.pink_b = Pink::default();
        self.svf_mid.reset();
        self.svf_side.reset();
        self.impact_playing = false;
        self.impact_pos = 0;
        self.gate = 1.0;
        self.gate_target = 1.0;
        self.gate_hold = 0;
        self.transport_primed = false;
        self.last_boundary = 0;
    }

    /// Apply a parameter snapshot (cheap; called once per block — no allocation).
    pub fn configure(&mut self, s: &Settings) {
        self.root_hz = self.root_override.unwrap_or_else(|| s.root_hz()).max(1.0);
        self.window_bars = s.window_bars().max(1.0);
        self.gamma = curve_gamma(s.curve);
        self.balance = s.balance.clamp(0.0, 1.0);
        self.color = s.color.clamp(0.0, 1.0);
        self.wave = s.wave.clamp(0.0, 1.0);
        self.f_start = s.filter_start_hz.clamp(20.0, self.sr * 0.45);
        self.f_end = s.filter_end_hz.clamp(20.0, self.sr * 0.45);
        self.rise_st = s.rise_st.clamp(0.0, MAX_RISE_ST);
        self.width = s.width.clamp(0.0, 1.0);
        self.impact_on = s.impact_on;
        self.impact_level = s.impact_level.clamp(0.0, 1.0);
        self.auto_cut = s.auto_cut;
        self.downlifter = s.downlifter;
        self.free_len_s = s.free_len_s.max(0.05);
        self.out_gain = db_to_lin(s.level_db);
    }

    /// Set the key-track root override (Some when a note is held and keytrack is on).
    pub fn set_root_override(&mut self, root: Option<f32>) {
        self.root_override = root;
    }

    /// Update the per-block transport frame. Resyncs the internal bar position to the host;
    /// a large discontinuity (seek/loop) re-primes the boundary detector without firing.
    pub fn set_transport(&mut self, t: TransportFrame) {
        let was_playing = self.playing;
        self.playing = t.playing;
        self.bars_per_sample = t.bars_per_sample.max(0.0);
        let w = if self.window_bars > 0.0 {
            t.bar_pos / self.window_bars as f64
        } else {
            0.0
        };
        let boundary = w.floor() as i64;
        let discontinuous = (t.bar_pos - self.bar_pos).abs() > 0.5 || !self.transport_primed || !was_playing;
        if discontinuous {
            self.last_boundary = boundary;
        }
        self.bar_pos = t.bar_pos;
        self.transport_primed = true;
    }

    /// Trigger the free-run one-shot envelope (manual TRIGGER button or a MIDI note while the
    /// transport is stopped). Ignored while the transport is playing (transport drives the env).
    pub fn trigger_free(&mut self) {
        if self.playing {
            return;
        }
        self.free_active = true;
        self.free_t = 0.0;
        self.phase_root = 0.0;
        self.phase_fifth = 0.0;
        self.gate = 1.0;
        self.gate_target = 1.0;
        self.gate_hold = 0;
        if self.downlifter && self.impact_on {
            // Downlifter: the impact is the drop at the *start* of the fall.
            self.fire_impact();
        }
    }

    #[inline]
    fn fire_impact(&mut self) {
        self.impact_playing = true;
        self.impact_pos = 0;
    }

    #[inline]
    fn start_autocut(&mut self) {
        if self.auto_cut {
            self.gate_target = 0.0;
            self.gate_hold = self.gate_hold_len;
        }
    }

    /// Called when a countdown boundary is crossed (riser: the drop; downlifter: the reset).
    #[inline]
    fn on_boundary(&mut self) {
        if self.impact_on {
            self.fire_impact();
        }
        if self.downlifter {
            // Restart the fall from full; clean phase for the new segment.
            self.phase_root = 0.0;
            self.phase_fifth = 0.0;
        } else {
            self.start_autocut();
        }
    }

    /// Envelope phase in [0,1] for the current segment, plus the segment-completion flag used
    /// by the free-run path. Returns the raw (unshaped) linear phase.
    #[inline]
    fn shaped(&self, p: f32) -> f32 {
        let pp = if self.downlifter { 1.0 - p.clamp(0.0, 1.0) } else { p.clamp(0.0, 1.0) };
        pp.powf(self.gamma)
    }

    /// Bars remaining until the next boundary (for the GUI countdown). Free-run reports the
    /// remaining fraction scaled to the window length so the display stays meaningful.
    pub fn bars_remaining(&self) -> f32 {
        if self.playing {
            let w = self.bar_pos / self.window_bars.max(1.0) as f64;
            let frac = (w.ceil() - w) as f32;
            let frac = if frac <= 1.0e-6 { self.window_bars } else { frac * self.window_bars };
            frac.max(0.0)
        } else if self.free_active {
            let remaining_s = (self.free_len_s - self.free_t).max(0.0);
            // Convert to a "bars" figure via the window/free-len ratio for a stable display.
            remaining_s / self.free_len_s.max(1.0e-3) * self.window_bars
        } else {
            0.0
        }
    }

    /// Produce one stereo output sample.
    #[inline]
    pub fn process_sample(&mut self) -> (f32, f32) {
        // --- Advance the tension phase / envelope ---
        if self.playing {
            // Advance the internal bar position and detect boundary crossings sample-accurately.
            self.bar_pos += self.bars_per_sample;
            let w = self.bar_pos / self.window_bars.max(1.0) as f64;
            let boundary = w.floor() as i64;
            if boundary > self.last_boundary {
                self.last_boundary = boundary;
                self.on_boundary();
            }
            let p = (w - w.floor()) as f32;
            self.env = self.shaped(p);
        } else if self.free_active {
            self.free_t += 1.0 / self.sr;
            let p = (self.free_t / self.free_len_s).clamp(0.0, 1.0);
            self.env = self.shaped(p);
            if self.free_t >= self.free_len_s {
                // Segment complete.
                if !self.downlifter && self.impact_on {
                    self.fire_impact();
                }
                if !self.downlifter {
                    self.start_autocut();
                }
                self.free_active = false;
                self.env = 0.0;
            }
        } else {
            self.env = 0.0;
        }
        let env = self.env.clamp(0.0, 1.0);

        // --- Auto-cut gate (one-pole fade toward target, with a silence hold) ---
        if self.gate_hold > 0 {
            self.gate_hold -= 1;
            if self.gate_hold == 0 {
                self.gate_target = 1.0;
            }
        }
        self.gate = self.gate_target + self.gate_coef * (self.gate - self.gate_target);

        // --- Sources: tonal stack (root + fifth, saw↔sine) with env-driven pitch rise ---
        let rise = 2.0f32.powf(env * self.rise_st / 12.0);
        let f_root = (self.root_hz * rise).min(self.sr * 0.45);
        let f_fifth = (self.root_hz * FIFTH_RATIO * rise).min(self.sr * 0.45);
        self.phase_root += f_root / self.sr;
        if self.phase_root >= 1.0 {
            self.phase_root -= self.phase_root.floor();
        }
        self.phase_fifth += f_fifth / self.sr;
        if self.phase_fifth >= 1.0 {
            self.phase_fifth -= self.phase_fifth.floor();
        }
        let osc = |ph: f32, wave: f32| {
            let saw = 2.0 * ph - 1.0;
            let sine = (TAU * ph).sin();
            saw * (1.0 - wave) + sine * wave
        };
        let tonal = 0.6 * osc(self.phase_root, self.wave) + 0.4 * osc(self.phase_fifth, self.wave);

        // --- Sources: filtered noise (white↔pink), decorrelated per channel ---
        let wa = self.rng_a.next_bipolar();
        let wb = self.rng_b.next_bipolar();
        let pa = self.pink_a.process(wa);
        let pb = self.pink_b.process(wb);
        let noise_a = wa * (1.0 - self.color) + pa * self.color;
        let noise_b = wb * (1.0 - self.color) + pb * self.color;

        // Balance: 0 = all noise, 1 = all tonal.
        let tone_g = self.balance;
        let noise_g = 1.0 - self.balance;
        let mid_pre = 0.42 * tone_g * tonal + 0.55 * noise_g * noise_a;
        let side_pre = 0.55 * noise_g * noise_b;

        // --- SVF sweep: cutoff climbs start→end with the envelope (exp interpolation) ---
        let fc = (self.f_start * (self.f_end / self.f_start).powf(env)).clamp(20.0, self.sr * 0.45);
        self.svf_mid.set(fc, 0.8, self.sr);
        self.svf_side.set(fc, 0.8, self.sr);
        let mid = self.svf_mid.process(mid_pre).lp;
        let side = self.svf_side.process(side_pre).lp;

        // --- Width bloom (narrow→wide): mid/side, mono-compatible ---
        let w_amt = self.width * env;
        let mut l = mid + w_amt * side;
        let mut r = mid - w_amt * side;

        // --- Volume swell (quiet→full) and the auto-cut gate on the SOURCE only ---
        let vol = (VOL_FLOOR + (1.0 - VOL_FLOOR) * env) * self.gate;
        l *= vol;
        r *= vol;

        // --- Embedded impact one-shot (mono), added post-gate so it survives the cut ---
        if self.impact_playing {
            if self.impact_pos < self.impact_buf.len() {
                let s = self.impact_buf[self.impact_pos] * self.impact_level;
                self.impact_pos += 1;
                l += s;
                r += s;
            } else {
                self.impact_playing = false;
            }
        }

        // --- Output trim + soft-clip safety ---
        l = suite_core::dsp::tape_soft(l * self.out_gain).clamp(-0.999, 0.999);
        r = suite_core::dsp::tape_soft(r * self.out_gain).clamp(-0.999, 0.999);
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_gamma_is_monotonic_around_unity() {
        assert!((curve_gamma(0.5) - 1.0).abs() < 1e-4);
        assert!(curve_gamma(0.0) > curve_gamma(0.5)); // exp
        assert!(curve_gamma(1.0) < curve_gamma(0.5)); // log
    }

    #[test]
    fn root_hz_tracks_octave_and_key() {
        let mut s = Settings::default();
        s.key = 0;
        s.octave = 2;
        assert!((s.root_hz() - 65.41).abs() < 0.5, "C2 should be ~65.41: {}", s.root_hz());
        s.octave = 3;
        assert!((s.root_hz() - 130.81).abs() < 1.0, "C3 should be ~130.8: {}", s.root_hz());
    }

    #[test]
    fn engine_is_finite_and_bounded() {
        let sr = 48_000.0;
        let mut e = AscendEngine::new(sr);
        e.configure(&Settings::default());
        e.set_transport(TransportFrame { playing: true, bar_pos: 0.0, bars_per_sample: 0.001 });
        for _ in 0..sr as usize {
            let (l, r) = e.process_sample();
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0);
        }
    }

    #[test]
    fn free_run_one_shot_runs_and_stops() {
        let sr = 48_000.0;
        let mut e = AscendEngine::new(sr);
        let mut s = Settings::default();
        s.free_len_s = 0.25;
        s.impact_on = false;
        e.configure(&s);
        e.set_transport(TransportFrame { playing: false, bar_pos: 0.0, bars_per_sample: 0.0 });
        e.trigger_free();
        assert!(e.free_active);
        let n = (sr * 0.5) as usize;
        let mut peak = 0.0f32;
        for _ in 0..n {
            let (l, _r) = e.process_sample();
            peak = peak.max(l.abs());
        }
        assert!(peak > 0.001, "free-run produced no level");
        assert!(!e.free_active, "free-run one-shot should have completed");
    }
}
