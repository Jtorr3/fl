//! SNAP snare/clap-synth DSP core (pure Rust, shared by the plugin and the offline harness).
//!
//! Signal flow (SPECS "SNAP"):
//! ```text
//! note-on ─ BODY:   sine/tri @ tune (140–260 Hz) w/ fast pitch env (shell knock) ─┐
//!         ─ RATTLE: white noise → 3 parallel BP formants (~800/1.5k/3k) w/ env    ├─ mode
//!         ─ CLAP:   N humanized noise bursts (spread) + 1 longer tail → BP/LP tone ┘  blend
//!         → transient click layer (snap) → drive (suite bank, 2× OS) → amp env
//!         → width (decorrelated per-channel noise) → soft clip → out (stereo)
//! ```
//! Mono-ish last-note-priority voice (IMPACT's architecture is the template): the body osc is
//! phase-continuous across retriggers and its amplitude is governed only by the master amp
//! envelope, which always ramps from its current value to the new velocity over a 1.5 ms
//! declick window — so a mid-decay retrigger never steps (done-bar #3). The fast noise layers
//! (rattle / clap / click) are faded in over the same 1.5 ms via a trigger ramp; being decayed
//! by mid-note they re-onset without a step. The DECAY macro scales every envelope together
//! (Length-style), so it sets the tail RT (done-bar #4). Width decorrelates the *noise* layers
//! per channel while keeping the tonal body mono, so the output stays mono-compatible
//! (correlation > 0.5) at any width. Stereo out, no audio in, MidiConfig::Basic.

use std::f32::consts::TAU;
use suite_core::db_to_lin;
use suite_core::dsp::{tape_soft, Oversampler2x, Shaper, Svf};
use suite_core::testsig::Rng;

/// A1 = MIDI note 33 = 55 Hz would be too low for a snare; SNAP's key-track reference is the
/// body fundamental at MIDI note 45 (A2 = 110 Hz) — playing that note reproduces the knob `tune`.
pub const KEYTRACK_REF_NOTE: u8 = 45;

/// 1.5 ms declick / attack ramp (SPECS — IMPACT's recipe).
pub const DECLICK_MS: f32 = 1.5;

/// Maximum clap taps (the humanized bursts). One extra slot holds the longer tail burst.
pub const MAX_TAPS: usize = 5;
const MAX_BURSTS: usize = MAX_TAPS + 1;

/// Reference decay (ms) that maps the DECAY macro to a length scale of 1.0.
pub const DECAY_REF_MS: f32 = 220.0;

/// Full-scale per-burst humanize jitter (± ms) at humanize = 1.
const JITTER_MAX_MS: f32 = 5.0;

/// Max decorrelation gain of the width side-noise. Chosen so that at width = 1 the L/R
/// noise correlation is 1/(1+k²) = 1/1.64 ≈ 0.61 > 0.5 (mono-compatible, SPECS).
const WIDTH_K_MAX: f32 = 0.8;

/// Body pitch-env start = `tune × SHELL_RATIO`, decaying to `tune` (the shell "knock").
const SHELL_RATIO: f32 = 1.9;

/// All SNAP DSP parameters, snapshotted from the nih-plug params once per block.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub mode: f32,      // 0 = Snare .. 0.5 = Hybrid .. 1 = Clap (engine crossfade)
    pub tune: f32,      // body fundamental Hz (140..260 typical)
    pub balance: f32,   // body/noise balance within the snare engine (0 = body .. 1 = rattle)
    pub snap: f32,      // 0..1 — scales click level + rattle env speed
    pub decay_ms: f32,  // master amp decay; Length-style macro scales ALL envelopes
    pub taps: usize,    // clap bursts (3..5)
    pub spread_ms: f32, // total clap spread window (8..30 ms)
    pub humanize: f32,  // 0..1 per-burst pre-delay jitter
    pub tone: f32,      // 0..1 clap/noise BP center (low..high)
    pub drive: f32,     // 0..1 pre-shaper drive
    pub width: f32,     // 0..1 decorrelated-noise stereo width
    pub level_db: f32,  // output trim
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: 0.35,
            tune: 190.0,
            balance: 0.55,
            snap: 0.5,
            decay_ms: 220.0,
            taps: 4,
            spread_ms: 22.0,
            humanize: 0.4,
            tone: 0.5,
            drive: 0.15,
            width: 0.4,
            level_db: 0.0,
        }
    }
}

/// Map the 0..1 `tone` control to a band-pass center (Hz), log-spaced 500 Hz .. 5 kHz.
#[inline]
pub fn tone_center_hz(tone: f32) -> f32 {
    let t = tone.clamp(0.0, 1.0);
    500.0 * (10.0f32).powf(t) // 500 .. 5000
}

/// One scheduled clap burst: a windowed noise slap. `start` in samples since note-on.
#[derive(Clone, Copy, Debug, Default)]
struct Burst {
    start: u32,
    attack: u32,   // linear attack length (samples)
    inv_tau: f32,  // 1 / decay-tau (samples) for the exponential release
    amp: f32,
}

impl Burst {
    /// Envelope value at sample `n` (n = samples since note-on).
    #[inline]
    fn env(&self, n: u32) -> f32 {
        if n < self.start {
            return 0.0;
        }
        let e = n - self.start;
        let v = if e < self.attack {
            e as f32 / self.attack.max(1) as f32
        } else {
            (-(((e - self.attack) as f32) * self.inv_tau)).exp()
        };
        v * self.amp
    }
}

/// A per-channel noise filter bank: 3 rattle formants + clap BP→LP + click BP.
struct NoiseChannel {
    rattle: [Svf; 3],
    clap_bp: Svf,
    clap_lp: Svf,
    click_bp: Svf,
    os: Oversampler2x,
}

impl NoiseChannel {
    fn new() -> Self {
        Self {
            rattle: [Svf::new(), Svf::new(), Svf::new()],
            clap_bp: Svf::new(),
            clap_lp: Svf::new(),
            click_bp: Svf::new(),
            os: Oversampler2x::new(),
        }
    }
    fn reset(&mut self) {
        for r in self.rattle.iter_mut() {
            r.reset();
        }
        self.clap_bp.reset();
        self.clap_lp.reset();
        self.click_bp.reset();
        self.os.reset();
    }
}

/// Mono-ish last-note-priority snare/clap voice with a decorrelated stereo noise field.
pub struct SnapVoice {
    sr: f32,

    // resolved config (from Settings each block)
    f_start: f32,
    f_end: f32,
    // Last Tune snapshot value `configure` saw (design (b) key-track guard, mirrors the
    // existing `tone` change-detect below). `note_on` may override the body fundamental from
    // the played key; `configure` must not clobber that override every block unless the user
    // actually moved the Tune knob. NAN = no snapshot yet.
    last_tune_snap: f32,
    inv_tau_p: f32, // 1 / pitch-env tau (samples)
    snare_w: f32,   // engine crossfade weights (equal-power)
    clap_w: f32,
    body_gain: f32,
    rattle_gain: f32,
    rattle_inv_tau: f32, // 1 / rattle-env tau (samples)
    click_level: f32,
    click_inv_tau: f32,
    amp_inv_tau: f32, // 1 / amp-env tau (samples)
    drive_pregain: f32,
    width_k: f32,
    width_norm: f32,
    level: f32,

    // clap schedule (resolved at note-on; MAX_BURSTS slots, first `num_bursts` active)
    taps: usize,
    spread_ms: f32,
    humanize: f32,
    tone: f32,
    bursts: [Burst; MAX_BURSTS],
    num_bursts: usize,

    // body oscillator (phase-continuous across retriggers)
    phase: f32,

    // envelopes / timers
    pitch_n: u32, // samples since note-on (also drives clap schedule)
    amp_t: f32,   // seconds since attack completed
    amp_env: f32,
    peak: f32,
    attack_remaining: u32,
    attack_len: u32,
    attack_start: f32,
    trig_remaining: u32, // fade-in for the fast noise layers
    rattle_env: f32,
    click_env: f32,

    // per-channel noise field
    chan: [NoiseChannel; 2],
    rng_shared: Rng,
    rng_diff_l: Rng,
    rng_diff_r: Rng,
    rng_clap: Rng, // humanize jitter (reseeded per note-on for determinism)
    hit_counter: u32, // increments each note_on; XORed into the humanize seed so consecutive
                      // hits differ while renders stay deterministic run-to-run (reset by reset()).

    active: bool,
}

impl SnapVoice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let mut v = Self {
            sr,
            f_start: 360.0,
            f_end: 190.0,
            last_tune_snap: f32::NAN,
            inv_tau_p: 1.0 / (0.008 * sr),
            snare_w: 1.0,
            clap_w: 0.0,
            body_gain: 0.6,
            rattle_gain: 0.6,
            rattle_inv_tau: 1.0 / (0.13 * sr),
            click_level: 0.4,
            click_inv_tau: 1.0 / (0.008 * sr),
            amp_inv_tau: 1.0 / (0.22 * sr),
            drive_pregain: 1.0,
            width_k: 0.0,
            width_norm: 1.0,
            level: 1.0,
            taps: 4,
            spread_ms: 22.0,
            humanize: 0.4,
            tone: 0.5,
            bursts: [Burst::default(); MAX_BURSTS],
            num_bursts: 0,
            phase: 0.0,
            pitch_n: 0,
            amp_t: 0.0,
            amp_env: 0.0,
            peak: 0.0,
            attack_remaining: 0,
            attack_len: 1,
            attack_start: 0.0,
            trig_remaining: 0,
            rattle_env: 0.0,
            click_env: 0.0,
            chan: [NoiseChannel::new(), NoiseChannel::new()],
            rng_shared: Rng::new(0x5EED_5A11),
            rng_diff_l: Rng::new(0x11AA_3C57),
            rng_diff_r: Rng::new(0x77BB_91DF),
            rng_clap: Rng::new(0x0C1A_9B33),
            hit_counter: 0,
            active: false,
        };
        v.attack_len = ((DECLICK_MS * 0.001) * sr).round().max(1.0) as u32;
        v.set_filters();
        v
    }

    fn set_filters(&mut self) {
        let sr = self.sr;
        let center = tone_center_hz(self.tone);
        let lp = (center * 3.0).min(0.45 * sr);
        for ch in self.chan.iter_mut() {
            ch.rattle[0].set(800.0, 2.5, sr);
            ch.rattle[1].set(1500.0, 2.5, sr);
            ch.rattle[2].set(3000.0, 2.0, sr);
            ch.clap_bp.set(center, 1.1, sr);
            ch.clap_lp.set(lp, 0.7, sr);
            ch.click_bp.set(4200.0, 2.0, sr);
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.pitch_n = 0;
        self.amp_t = 0.0;
        self.amp_env = 0.0;
        self.peak = 0.0;
        self.attack_remaining = 0;
        self.trig_remaining = 0;
        self.rattle_env = 0.0;
        self.click_env = 0.0;
        self.num_bursts = 0;
        self.hit_counter = 0;
        for ch in self.chan.iter_mut() {
            ch.reset();
        }
        self.active = false;
    }

    /// Apply a parameter snapshot (called once per block; cheap).
    pub fn configure(&mut self, s: &Settings) {
        let sr = self.sr;
        let len = (s.decay_ms.max(1.0) / DECAY_REF_MS).clamp(0.05, 20.0);

        // Design (b) — reapply-on-change: only write the body fundamental from the Tune knob
        // when the snapshot value genuinely moved. A note-on key-track override
        // (`note_on(_, Some(key_hz), _)`) then survives block-rate reconfigure with unchanged
        // settings, while a live Tune tweak still applies immediately. (Same pattern the `tone`
        // filter update already uses below.)
        if self.last_tune_snap.is_nan() || (s.tune - self.last_tune_snap).abs() > 1.0e-4 {
            self.f_end = s.tune.clamp(30.0, 2000.0);
            self.f_start = (self.f_end * SHELL_RATIO).min(0.45 * sr);
        }
        self.last_tune_snap = s.tune;
        let tau_p = (0.008 * len * sr).max(1.0);
        self.inv_tau_p = 1.0 / tau_p;

        // Equal-power engine crossfade: Snare (mode 0) → Clap (mode 1).
        let m = s.mode.clamp(0.0, 1.0);
        self.snare_w = (m * std::f32::consts::FRAC_PI_2).cos();
        self.clap_w = (m * std::f32::consts::FRAC_PI_2).sin();

        let bal = s.balance.clamp(0.0, 1.0);
        self.body_gain = (1.0 - bal) * 0.9;
        self.rattle_gain = bal * 0.8;

        let snap = s.snap.clamp(0.0, 1.0);
        // Rattle env: base ~130 ms, sped up (shorter) as snap rises.
        let rattle_tau = (0.13 * len / (1.0 + snap * 2.0) * sr).max(1.0);
        self.rattle_inv_tau = 1.0 / rattle_tau;
        self.click_level = 0.12 + snap * 0.55;
        let click_tau = (0.008 * len * sr).max(1.0);
        self.click_inv_tau = 1.0 / click_tau;

        let amp_tau = (s.decay_ms.max(1.0) * 0.001 * sr).max(1.0);
        self.amp_inv_tau = 1.0 / amp_tau;

        self.drive_pregain = 1.0 + s.drive.clamp(0.0, 1.0) * 11.0;

        let w = s.width.clamp(0.0, 1.0);
        self.width_k = WIDTH_K_MAX * w;
        self.width_norm = 1.0 / (1.0 + self.width_k * self.width_k).sqrt();

        self.level = db_to_lin(s.level_db);

        self.taps = s.taps.clamp(1, MAX_TAPS);
        self.spread_ms = s.spread_ms.clamp(1.0, 200.0);
        self.humanize = s.humanize.clamp(0.0, 1.0);

        if (self.tone - s.tone).abs() > 1.0e-4 {
            self.tone = s.tone.clamp(0.0, 1.0);
            self.set_filters();
        }

        self.attack_len = ((DECLICK_MS * 0.001) * sr).round().max(1.0) as u32;
    }

    /// Build the clap burst schedule at note-on. `len` scales the burst envelopes with the
    /// DECAY macro. Humanize jitter uses a per-note reseeded RNG so a given (seed, humanize)
    /// is deterministic (SPECS — deterministic per seed for tests); humanize = 0 ⇒ no jitter ⇒
    /// exactly `taps` evenly-spaced bursts + 1 tail = `taps + 1` onsets.
    fn schedule_clap(&mut self, len: f32) {
        // Derive the humanize seed from a per-hit counter so consecutive claps get DIFFERENT
        // jitter (fixing the "every hit identical" dead-control bug) while a given sequence from
        // a fresh/reset core still reproduces the same jitter run-to-run (determinism).
        self.rng_clap = Rng::new(0x0C1A_9B33 ^ self.hit_counter);
        self.hit_counter = self.hit_counter.wrapping_add(1);
        let sr = self.sr;
        let spread = (self.spread_ms * 0.001 * sr).max(1.0);
        let jitter_max = JITTER_MAX_MS * 0.001 * sr;
        let atk_tap = (0.0003 * sr).max(1.0) as u32; // 0.3 ms
        let atk_tail = (0.001 * sr).max(1.0) as u32; // 1 ms
        let tap_tau = (0.0025 * len * sr).max(1.0); // ~2.5 ms slap (snappy, separable)
        let tail_tau = (0.09 * len * sr).max(1.0); // ~90 ms room tail

        let n = self.taps.min(MAX_TAPS);
        for j in 0..n {
            let base = (j as f32 / n as f32) * spread;
            let jitter = self.humanize * self.rng_clap.next_bipolar() * jitter_max;
            let start = (base + jitter).max(0.0).round() as u32;
            self.bursts[j] = Burst {
                start,
                attack: atk_tap,
                inv_tau: 1.0 / tap_tau,
                amp: 1.0,
            };
        }
        // One longer tail burst just after the spread window.
        let tail_start = (spread + 0.002 * sr).round() as u32;
        self.bursts[n] = Burst {
            start: tail_start,
            attack: atk_tail,
            inv_tau: 1.0 / tail_tau,
            amp: 0.85,
        };
        self.num_bursts = n + 1;
    }

    /// Trigger a note. `velocity` in [0, 1]. `key_hz`, if `Some`, sets the body fundamental
    /// (key-track). Phase-continuous: the body oscillator phase is intentionally *not* reset.
    pub fn note_on(&mut self, velocity: f32, key_hz: Option<f32>, len: f32) {
        if let Some(f) = key_hz {
            self.f_end = f.clamp(30.0, 2000.0);
            self.f_start = (self.f_end * SHELL_RATIO).min(0.45 * self.sr);
        }
        self.pitch_n = 0;
        self.amp_t = 0.0;
        self.peak = velocity.clamp(0.0, 1.0).max(1.0e-4);
        // Declick: ramp the master amp env from its CURRENT value to the new peak over 1.5 ms.
        self.attack_start = self.amp_env;
        self.attack_remaining = self.attack_len;
        // Fade the fast noise layers in from zero over the same window (they are decayed by
        // mid-note, so this re-onsets them without a step — IMPACT's declick recipe).
        self.trig_remaining = self.attack_len;
        self.rattle_env = 1.0;
        self.click_env = 1.0;
        self.schedule_clap(len);
        self.active = true;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    #[inline]
    fn body_wave(&self, phase: f32) -> f32 {
        let sine = (TAU * phase).sin();
        let tri = 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0;
        0.8 * sine + 0.2 * tri
    }

    /// Produce one stereo output sample.
    #[inline]
    pub fn process_sample(&mut self) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }
        let n = self.pitch_n;

        // --- BODY: exponential pitch env → phase-continuous sine/tri (mono) ---
        let e_p = (-(n as f32) * self.inv_tau_p).exp();
        let f = self.f_end + (self.f_start - self.f_end) * e_p;
        self.phase += f / self.sr;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }
        let body = self.body_wave(self.phase) * self.body_gain;

        // --- Clap burst envelope sum (shared across channels; onsets from the attacks) ---
        let mut clap_env = 0.0;
        for b in self.bursts[..self.num_bursts].iter() {
            clap_env += b.env(n);
        }

        // --- Trigger fade-in for the fast noise layers (0→1 over 1.5 ms) ---
        let trig = if self.trig_remaining > 0 {
            let g = (self.attack_len - self.trig_remaining) as f32 / self.attack_len as f32;
            self.trig_remaining -= 1;
            g
        } else {
            1.0
        };

        // --- Per-channel decorrelated white noise (shared + independent difference) ---
        let s = self.rng_shared.next_bipolar();
        let dl = self.rng_diff_l.next_bipolar();
        let dr = self.rng_diff_r.next_bipolar();
        let noise_l = (s + self.width_k * dl) * self.width_norm;
        let noise_r = (s + self.width_k * dr) * self.width_norm;

        // Advance the shared fast envelopes (rattle/click) once per sample.
        let rattle_env = self.rattle_env;
        let click_env = self.click_env;
        self.rattle_env -= self.rattle_env * self.rattle_inv_tau;
        self.click_env -= self.click_env * self.click_inv_tau;

        let noises = [noise_l, noise_r];
        let mut out = [0.0f32; 2];
        for c in 0..2 {
            let ch = &mut self.chan[c];
            let noise = noises[c];

            // RATTLE: noise → 3 parallel BP formants, own env.
            let rattle = (ch.rattle[0].process(noise).bp
                + ch.rattle[1].process(noise).bp
                + ch.rattle[2].process(noise).bp)
                * 0.5
                * rattle_env
                * self.rattle_gain;

            // CLAP: noise → BP → LP tone shaping, × burst-env sum.
            let clap = ch.clap_lp.process(ch.clap_bp.process(noise).bp).lp * clap_env;

            // CLICK transient layer: noise → high BP, fast env, snap-scaled.
            let click = ch.click_bp.process(noise).bp * click_env * self.click_level;

            // Snare engine (body + rattle) vs clap engine, blended by mode. The rattle is a fast
            // noise layer: note_on resets its env to 1.0, so it too must fade in through the
            // trigger ramp (else a mid-decay retrigger steps it to full within one sample). The
            // body is deliberately left un-ramped — it is phase-continuous and its level is
            // governed by the master amp declick envelope.
            let snare_engine = body + rattle * trig;
            let voice_pre =
                snare_engine * self.snare_w + (clap * self.clap_w + click) * trig;

            // Drive (suite bank, 2× oversampled nonlinearity) → amp env → soft clip.
            let pregain = self.drive_pregain;
            let shaper = Shaper::TubeTanh;
            let driven = ch.os.process(voice_pre, |x| shaper.apply(x, pregain));
            out[c] = driven;
        }

        // --- Master amp envelope (also the declick master gain) ---
        if self.attack_remaining > 0 {
            let done = (self.attack_len - self.attack_remaining) as f32 / self.attack_len as f32;
            self.amp_env = self.attack_start + (self.peak - self.attack_start) * done;
            self.attack_remaining -= 1;
            if self.attack_remaining == 0 {
                self.amp_t = 0.0;
            }
        } else {
            self.amp_env = self.peak * (-self.amp_t * self.sr * self.amp_inv_tau).exp();
            self.amp_t += 1.0 / self.sr;
            if self.amp_env < 1.0e-4 && n > self.attack_len {
                self.active = false;
            }
        }
        self.pitch_n = self.pitch_n.saturating_add(1);

        let amp = self.amp_env;
        let clip = |x: f32| {
            let y = if x.abs() > 0.9 { tape_soft(x) } else { x };
            (y * self.level).clamp(-0.999, 0.999)
        };
        (clip(out[0] * amp), clip(out[1] * amp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(s: &Settings, len_samps: usize) -> (Vec<f32>, Vec<f32>) {
        let sr = 48_000.0;
        let mut v = SnapVoice::new(sr);
        v.configure(s);
        let macro_len = s.decay_ms.max(1.0) / DECAY_REF_MS;
        v.note_on(1.0, None, macro_len);
        let mut l = vec![0.0f32; len_samps];
        let mut r = vec![0.0f32; len_samps];
        for i in 0..len_samps {
            let (a, b) = v.process_sample();
            l[i] = a;
            r[i] = b;
        }
        (l, r)
    }

    #[test]
    fn voice_is_finite_and_decays() {
        let (l, _r) = render(&Settings::default(), 48_000);
        for &x in &l {
            assert!(x.is_finite());
        }
        let peak = l.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        assert!(peak > 0.05, "voice never produced level ({peak})");
        let win = 2400usize;
        let early: f32 = l[..win].iter().map(|v| v * v).sum::<f32>() / win as f32;
        let late: f32 = l[l.len() - win..].iter().map(|v| v * v).sum::<f32>() / win as f32;
        assert!(late < early * 0.25, "voice did not decay (early {early} late {late})");
    }

    #[test]
    fn tone_center_is_monotonic() {
        assert!(tone_center_hz(0.0) < tone_center_hz(0.5));
        assert!(tone_center_hz(0.5) < tone_center_hz(1.0));
    }

    #[test]
    fn width_zero_is_mono() {
        let mut s = Settings::default();
        s.width = 0.0;
        let (l, r) = render(&s, 24_000);
        for i in 0..l.len() {
            assert!((l[i] - r[i]).abs() < 1.0e-6, "width=0 not mono at {i}");
        }
    }

    fn clap_starts(v: &SnapVoice) -> Vec<u32> {
        v.bursts[..v.num_bursts].iter().map(|b| b.start).collect()
    }

    /// Design (b) regression: a key-track note_on override survives block-rate reconfigure
    /// with the SAME settings. Without the guard, `configure` snaps the body fundamental back
    /// to the Tune knob (190 Hz) on the very next block.
    #[test]
    fn keytrack_survives_reconfigure_same_settings() {
        let sr = 48_000.0;
        let s = Settings::default(); // Tune knob = 190 Hz
        let mut v = SnapVoice::new(sr);
        v.configure(&s);
        v.note_on(1.0, Some(110.0), 1.0); // key-track to 110 Hz
        assert!((v.f_end - 110.0).abs() < 1e-3, "note_on did not set override");
        for _ in 0..8 {
            v.configure(&s); // reconfigure every block with unchanged knobs
            for _ in 0..256 {
                let _ = v.process_sample();
            }
        }
        assert!(
            (v.f_end - 110.0).abs() < 1e-3,
            "key-track override clobbered by configure: f_end = {} (expected 110)",
            v.f_end
        );
    }

    /// Design (b) regression: turning the Tune knob mid-note DOES take effect (override released
    /// when the user genuinely moves the knob).
    #[test]
    fn knob_change_overrides_keytrack_midnote() {
        let sr = 48_000.0;
        let mut s = Settings::default();
        let mut v = SnapVoice::new(sr);
        v.configure(&s);
        v.note_on(1.0, Some(110.0), 1.0);
        assert!((v.f_end - 110.0).abs() < 1e-3);
        s.tune = 250.0; // user drags Tune up
        v.configure(&s);
        assert!(
            (v.f_end - 250.0).abs() < 1e-3,
            "Tune knob change ignored mid-note: f_end = {} (expected 250)",
            v.f_end
        );
    }

    /// P1 regression: humanize now varies hit-to-hit (per-hit counter seed) yet stays
    /// deterministic run-to-run from a fresh core.
    #[test]
    fn humanize_varies_hit_to_hit_but_is_deterministic() {
        let sr = 48_000.0;
        let mut s = Settings::default();
        s.humanize = 0.8;
        let render_pair = || {
            let mut v = SnapVoice::new(sr);
            v.configure(&s);
            v.note_on(1.0, None, 1.0);
            let a = clap_starts(&v);
            v.note_on(1.0, None, 1.0);
            let b = clap_starts(&v);
            (a, b)
        };
        let (a1, b1) = render_pair();
        assert_ne!(a1, b1, "consecutive hits have identical jitter (humanize is a dead control)");
        let (a2, b2) = render_pair();
        assert_eq!(a1, a2, "hit 1 jitter not reproducible run-to-run (nondeterministic)");
        assert_eq!(b1, b2, "hit 2 jitter not reproducible run-to-run (nondeterministic)");
    }
}
