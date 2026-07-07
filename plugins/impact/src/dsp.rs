//! IMPACT kick-synth DSP core (pure Rust, shared by the plugin and the offline harness).
//!
//! Signal flow (SPECS "IMPACT"):
//! ```text
//! note-on ─ pitch env(f_start→f_end, curve) ─ sine/tri body osc ─┐
//!         ─ click: white noise → SVF band-pass + embedded PCM     ├─ mix ─ drive ─ amp env ─ clip ─ out
//!         ─ sub osc (f_end × ratio)                               ┘
//! ```
//! Mono, last-note priority, phase-continuous retrigger with a 1.5 ms declick ramp: the amp
//! envelope multiplies the *entire* voice and always ramps from its current value to the new
//! velocity over 1.5 ms, so a mid-decay retrigger never steps (done-bar #2). The pitch env is
//! exponential `f(t) = f_end + (f_start−f_end)·e^(−t/τ_p)` with a curve param warping the shape;
//! the LENGTH macro scales the amp decay and pitch τ together.

use std::f32::consts::TAU;
use suite_core::db_to_lin;
use suite_core::dsp::{tape_soft, Shaper, Svf};
use suite_core::testsig::Rng;

/// Embedded PCM transients synthesized offline by `build.rs` (`TRANSIENTS: [&[f32]; 3]`,
/// `TRANSIENT_SR`). Windowed to start/end at zero.
#[allow(dead_code)] // TRANSIENT_SR / individual arrays are part of the generated API surface.
mod transients {
    include!(concat!(env!("OUT_DIR"), "/transients.rs"));
}

/// A1 = MIDI note 33 = 55 Hz — the key-track reference (SPECS).
pub const KEYTRACK_REF_HZ: f32 = 55.0;

/// 1.5 ms declick / attack ramp (SPECS).
pub const DECLICK_MS: f32 = 1.5;

/// One drive waveshaper choice from the suite bank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveShape {
    Tube,
    Tape,
    Fold,
    Hard,
}

impl DriveShape {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => DriveShape::Tube,
            1 => DriveShape::Tape,
            2 => DriveShape::Fold,
            _ => DriveShape::Hard,
        }
    }
    fn shaper(self) -> Shaper {
        match self {
            DriveShape::Tube => Shaper::TubeTanh,
            DriveShape::Tape => Shaper::TapeSoft,
            DriveShape::Fold => Shaper::SineFold,
            DriveShape::Hard => Shaper::HardClip,
        }
    }
}

/// All IMPACT DSP parameters, snapshotted from the nih-plug params once per block.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub f_start: f32,        // Hz — pitch env start
    pub f_end: f32,          // Hz — pitch env end / body target
    pub pitch_decay_ms: f32, // τ_p base (scaled by length)
    pub pitch_curve: f32,    // 0..1 morphs τ shape
    pub length: f32,         // macro multiplier on amp decay + pitch τ
    pub amp_decay_ms: f32,   // τ_a base (scaled by length)
    pub amp_curve: f32,      // 0..1 morphs amp env shape
    pub tone: f32,           // 0 = sine .. 1 = triangle body
    pub drive: f32,          // 0..1 pre-shaper drive
    pub shape: DriveShape,   // waveshaper bank selection
    pub clip_soft: bool,     // output stage: soft (tanh) vs hard clip
    pub click_level: f32,    // 0..1 noise-click layer
    pub click_decay_ms: f32, // 5..50 ms click decay
    pub click_freq: f32,     // 1000..8000 Hz band-pass center
    pub transient: usize,    // 0 = off, 1..3 = embedded PCM variant
    pub transient_level: f32,
    pub sub_level: f32,      // 0..1 sub osc
    pub sub_ratio: f32,      // sub freq = f_end × ratio
    pub out_gain_db: f32,    // output trim
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            f_start: 220.0,
            f_end: 55.0,
            pitch_decay_ms: 45.0,
            pitch_curve: 0.5,
            length: 1.0,
            amp_decay_ms: 400.0,
            amp_curve: 0.5,
            tone: 0.0,
            drive: 0.0,
            shape: DriveShape::Tube,
            clip_soft: true,
            click_level: 0.25,
            click_decay_ms: 12.0,
            click_freq: 3500.0,
            transient: 0,
            transient_level: 0.5,
            sub_level: 0.0,
            sub_ratio: 0.5,
            out_gain_db: 0.0,
        }
    }
}

/// Map a 0..1 curve control to a shaping exponent. 0.5 → 1.0 (pure exponential); lower →
/// front-loaded (fast initial drop), higher → back-loaded (holds then drops).
#[inline]
fn curve_exp(curve: f32) -> f32 {
    // 2^((c-0.5)*4): c=0 → 0.0625, c=0.5 → 1, c=1 → 16.
    2.0f32.powf((curve.clamp(0.0, 1.0) - 0.5) * 4.0)
}

/// Mono last-note-priority kick voice.
pub struct KickVoice {
    sr: f32,

    // config (resolved from Settings each block)
    f_start: f32,
    f_end: f32,
    tau_p: f32,   // seconds
    curve_p: f32, // pitch shaping exponent
    tau_a: f32,   // seconds
    curve_a: f32, // amp shaping exponent
    tone: f32,
    drive_pregain: f32,
    shaper: Shaper,
    clip_soft: bool,
    click_level: f32,
    click_decay_coef: f32,
    transient: usize,
    transient_level: f32,
    sub_level: f32,
    sub_ratio: f32,
    out_gain: f32,

    // body oscillator (phase-continuous across retriggers)
    phase: f32,
    sub_phase: f32,

    // envelopes
    pitch_t: f32, // seconds since note-on
    amp_t: f32,   // seconds since attack completed
    amp_env: f32, // current linear amp gain (always the master voice gain)
    peak: f32,    // note velocity → decay target
    attack_remaining: u32,
    attack_len: u32,
    attack_start: f32,

    // trigger declick ramp (0→1 over 1.5 ms) applied to the click + transient layers so
    // their onsets never step, independent of the underlying amp-envelope level.
    trig_remaining: u32,

    // click layer
    click_svf: Svf,
    click_env: f32,
    rng: Rng,

    // embedded transient playback
    trans_pos: usize,
    trans_playing: bool,

    active: bool,
}

impl KickVoice {
    pub fn new(sample_rate: f32) -> Self {
        let mut v = Self {
            sr: sample_rate.max(1.0),
            f_start: 220.0,
            f_end: 55.0,
            tau_p: 0.045,
            curve_p: 1.0,
            tau_a: 0.4,
            curve_a: 1.0,
            tone: 0.0,
            drive_pregain: 1.0,
            shaper: Shaper::TubeTanh,
            clip_soft: true,
            click_level: 0.25,
            click_decay_coef: 0.999,
            transient: 0,
            transient_level: 0.5,
            sub_level: 0.0,
            sub_ratio: 0.5,
            out_gain: 1.0,
            phase: 0.0,
            sub_phase: 0.0,
            pitch_t: 0.0,
            amp_t: 0.0,
            amp_env: 0.0,
            peak: 0.0,
            attack_remaining: 0,
            attack_len: 1,
            trig_remaining: 0,
            attack_start: 0.0,
            click_svf: Svf::new(),
            click_env: 0.0,
            rng: Rng::new(0x51AC_2E17),
            trans_pos: 0,
            trans_playing: false,
            active: false,
        };
        v.attack_len = ((DECLICK_MS * 0.001) * v.sr).round().max(1.0) as u32;
        v.click_svf.set(3500.0, 2.0, v.sr);
        v
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.sub_phase = 0.0;
        self.pitch_t = 0.0;
        self.amp_t = 0.0;
        self.amp_env = 0.0;
        self.peak = 0.0;
        self.attack_remaining = 0;
        self.trig_remaining = 0;
        self.click_env = 0.0;
        self.click_svf.reset();
        self.trans_playing = false;
        self.trans_pos = 0;
        self.active = false;
    }

    /// Apply a parameter snapshot (called once per block; cheap).
    pub fn configure(&mut self, s: &Settings) {
        let len = s.length.max(0.01);
        self.f_start = s.f_start.max(1.0);
        self.f_end = s.f_end.max(1.0);
        self.tau_p = (s.pitch_decay_ms * len * 0.001).max(1.0e-5);
        self.curve_p = curve_exp(s.pitch_curve);
        self.tau_a = (s.amp_decay_ms * len * 0.001).max(1.0e-4);
        self.curve_a = curve_exp(s.amp_curve);
        self.tone = s.tone.clamp(0.0, 1.0);
        self.drive_pregain = 1.0 + s.drive.clamp(0.0, 1.0) * 11.0;
        self.shaper = s.shape.shaper();
        self.clip_soft = s.clip_soft;
        self.click_level = s.click_level.max(0.0);
        let click_samps = (s.click_decay_ms.clamp(1.0, 200.0) * 0.001) * self.sr;
        self.click_decay_coef = (-1.0 / click_samps.max(1.0)).exp();
        self.click_svf.set(s.click_freq.clamp(200.0, 16_000.0), 2.0, self.sr);
        self.transient = s.transient.min(3);
        self.transient_level = s.transient_level.max(0.0);
        self.sub_level = s.sub_level.max(0.0);
        self.sub_ratio = s.sub_ratio.clamp(0.05, 2.0);
        self.out_gain = db_to_lin(s.out_gain_db);
        self.attack_len = ((DECLICK_MS * 0.001) * self.sr).round().max(1.0) as u32;
    }

    /// Trigger a note. `velocity` in [0, 1]. `key_hz`, if `Some`, sets `f_end` (key-track).
    /// Phase-continuous: the body oscillator phase is intentionally *not* reset.
    pub fn note_on(&mut self, velocity: f32, key_hz: Option<f32>) {
        if let Some(f) = key_hz {
            self.f_end = f.max(1.0);
        }
        self.pitch_t = 0.0;
        self.amp_t = 0.0;
        self.peak = velocity.clamp(0.0, 1.0).max(1.0e-4);
        // Declick: ramp the master amp env from its CURRENT value to the new peak over
        // 1.5 ms instead of jumping. Guarantees continuity on both fresh notes and retriggers.
        self.attack_start = self.amp_env;
        self.attack_remaining = self.attack_len;
        // Restart the transient layers. The noise-click SVF state is intentionally kept
        // (continuous); the click envelope restarts and the PCM transient plays from index 0.
        // A 1.5 ms trigger ramp fades the whole click+transient layer in from zero so its
        // onset never steps, even when retriggered at a high mid-decay envelope level.
        self.trig_remaining = self.attack_len;
        self.click_env = 1.0;
        self.trans_playing = self.transient > 0;
        self.trans_pos = 0;
        self.active = true;
    }

    /// True while the voice is producing sound (used to report KeepAlive vs allow sleep).
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[inline]
    fn body_wave(&self, phase: f32) -> f32 {
        let sine = (TAU * phase).sin();
        if self.tone <= 1.0e-4 {
            return sine;
        }
        // Naive triangle from the same phase accumulator.
        let tri = 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0;
        sine * (1.0 - self.tone) + tri * self.tone
    }

    /// Produce one output sample.
    #[inline]
    pub fn process_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let dt = 1.0 / self.sr;

        // --- Pitch envelope (exponential, curve-morphed) ---
        let e_p = (-self.pitch_t / self.tau_p).exp().powf(self.curve_p);
        let f = self.f_end + (self.f_start - self.f_end) * e_p;
        self.pitch_t += dt;

        // --- Body oscillator (phase-continuous) ---
        self.phase += f / self.sr;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }
        let body = self.body_wave(self.phase);

        // --- Sub oscillator (tracks f_end) ---
        let sub = if self.sub_level > 0.0 {
            let sf = self.f_end * self.sub_ratio;
            self.sub_phase += sf / self.sr;
            if self.sub_phase >= 1.0 {
                self.sub_phase -= self.sub_phase.floor();
            }
            (TAU * self.sub_phase).sin() * self.sub_level
        } else {
            0.0
        };

        // --- Click layer: white noise → band-pass, own fast decay ---
        let click = if self.click_level > 0.0 {
            let bp = self.click_svf.process(self.rng.next_bipolar()).bp;
            let c = bp * self.click_env * self.click_level;
            self.click_env *= self.click_decay_coef;
            c
        } else {
            0.0
        };

        // --- Embedded PCM transient ---
        let trans = if self.trans_playing {
            let tbl = transients::TRANSIENTS[self.transient - 1];
            if self.trans_pos < tbl.len() {
                let v = tbl[self.trans_pos] * self.transient_level;
                self.trans_pos += 1;
                v
            } else {
                self.trans_playing = false;
                0.0
            }
        } else {
            0.0
        };

        // --- Trigger declick ramp for the click + transient onset (0→1 over 1.5 ms) ---
        let trig = if self.trig_remaining > 0 {
            let g = (self.attack_len - self.trig_remaining) as f32 / self.attack_len as f32;
            self.trig_remaining -= 1;
            g
        } else {
            1.0
        };

        // --- Mix → drive (waveshaper, pre-amp-env) ---
        let mix = body + sub + (click + trans) * trig;
        let driven = self.shaper.apply(mix, self.drive_pregain);

        // --- Amp envelope (also the declick master gain) ---
        if self.attack_remaining > 0 {
            let done = (self.attack_len - self.attack_remaining) as f32 / self.attack_len as f32;
            self.amp_env = self.attack_start + (self.peak - self.attack_start) * done;
            self.attack_remaining -= 1;
            if self.attack_remaining == 0 {
                self.amp_t = 0.0;
            }
        } else {
            let e_a = (-self.amp_t / self.tau_a).exp().powf(self.curve_a);
            self.amp_env = self.peak * e_a;
            self.amp_t += dt;
            if self.amp_env < 1.0e-4 {
                self.active = false;
            }
        }

        // --- Output: amp env → clip → trim ---
        let mut y = driven * self.amp_env;
        y = if self.clip_soft {
            tape_soft(y)
        } else {
            y.clamp(-1.0, 1.0)
        };
        y *= self.out_gain;
        // Safety ceiling so a hot preset can never break the peak ≤ 0 dBFS universal assertion.
        y.clamp(-0.999, 0.999)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transients_start_and_end_at_zero() {
        for t in transients::TRANSIENTS.iter() {
            assert!(t.len() > 8);
            assert!(t[0].abs() < 1.0e-6, "transient onset not zero: {}", t[0]);
            assert!(t[t.len() - 1].abs() < 1.0e-6, "transient offset not zero");
        }
    }

    #[test]
    fn curve_exp_is_monotonic_around_unity() {
        assert!((curve_exp(0.5) - 1.0).abs() < 1e-4);
        assert!(curve_exp(0.0) < curve_exp(0.5));
        assert!(curve_exp(1.0) > curve_exp(0.5));
    }

    #[test]
    fn voice_is_finite_and_decays() {
        let sr = 48_000.0;
        let mut v = KickVoice::new(sr);
        v.configure(&Settings::default());
        v.note_on(1.0, None);
        let n = (sr * 2.0) as usize;
        let mut out = vec![0.0f32; n];
        for o in out.iter_mut() {
            let y = v.process_sample();
            assert!(y.is_finite());
            *o = y;
        }
        let peak = out.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        assert!(peak > 0.1, "voice never produced level");
        // The exponential amp envelope must decay: late RMS well below early RMS.
        let win = (sr * 0.05) as usize;
        let early: f32 = out[..win].iter().map(|v| v * v).sum::<f32>() / win as f32;
        let late: f32 = out[n - win..].iter().map(|v| v * v).sum::<f32>() / win as f32;
        assert!(late < early * 0.25, "voice did not decay (early {early} late {late})");
    }
}
