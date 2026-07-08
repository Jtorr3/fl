//! VOXKEY — pure-DSP core for the vocal retuner (SPECS "VOXKEY"). API-agnostic Rust,
//! shared verbatim between the nih-plug `process` path and the offline harness tests.
//!
//! ```text
//! in ─┬─ mono sum → PitchTracker ── detected f0 + confidence
//!     │                                     │
//!     │        target note = nearest scale tone (Root+Scale) OR held MIDI note
//!     │                                     │
//!     │        correction cents = 1200·log2(target/detected), ±1 octave
//!     │        + humanize drift, one-pole GLIDE (retune 0–400 ms), × amount
//!     │                                     ↓  pitch ratio
//!     ├─ delay(latency=2048) ─────────────────────────────── dry ──┐
//!     └ TWO ShiftEngines (stereo, envelope-preserve ON,             │ mix
//!       formant-offset via set_formant_ratio) ──────────── wet ─────┴── + ── out
//! ```
//!
//! Below the confidence gate (unpitched / silence / breaths) the correction glides to 1.0 so
//! nothing is retuned — no artifacts on noise. Latency = the ShiftEngine FFT size (2048); the
//! dry path is delayed to match so `mix=0` nulls exactly against the latency-matched dry.
//! Everything is preallocated in [`VoxCore::new`]; the per-sample path is allocation-free.

use suite_core::dsp::{DelayLine, OnePole, Svf};
use suite_core::pitch::Mpm;
use suite_core::shift::{ShiftEngine, DEFAULT_FFT, DEFAULT_HOP};
use suite_core::testsig::Rng;

/// Main analysis FFT for the formant-preserving shift (== reported latency).
pub const MAIN_FFT: usize = DEFAULT_FFT;
const MAIN_HOP: usize = DEFAULT_HOP;

/// Audible-scalar smoothing time (ms).
const SMOOTH_MS: f32 = 12.0;
/// Humanize drift: redraw the random target every this many ms, smoothed over `HUM_SMOOTH_MS`.
const HUM_REDRAW_MS: f32 = 45.0;
const HUM_SMOOTH_MS: f32 = 130.0;
/// Default frozen pitch before the first confident detect (A3).
const DEFAULT_F0: f32 = 220.0;
/// Wet-path safety-clip knee (identity below, tanh above → |y| < 1).
const CLIP_KNEE: f32 = 0.9;

// ---------------------------------------------------------------------------
// Scales
// ---------------------------------------------------------------------------

/// Scale names, menu order (index into [`SCALES`]).
pub const SCALE_NAMES: [&str; 7] = [
    "Chromatic",
    "Major",
    "Natural Minor",
    "Harmonic Minor",
    "Phrygian",
    "Dorian",
    "Minor Pentatonic",
];

/// Allowed semitone classes (relative to the root) for each scale in [`SCALE_NAMES`] order.
pub const SCALES: [&[i32]; 7] = [
    &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], // Chromatic
    &[0, 2, 4, 5, 7, 9, 11],                 // Major (Ionian)
    &[0, 2, 3, 5, 7, 8, 10],                 // Natural Minor (Aeolian)
    &[0, 2, 3, 5, 7, 8, 11],                 // Harmonic Minor
    &[0, 1, 3, 5, 7, 8, 10],                 // Phrygian
    &[0, 2, 3, 5, 7, 9, 10],                 // Dorian
    &[0, 3, 5, 7, 10],                       // Minor Pentatonic
];

/// Root-note names (index 0 = C).
pub const ROOT_NAMES: [&str; 12] =
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Continuous MIDI note number for a frequency (69 + 12·log2(f/440)).
#[inline]
pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz.max(1.0e-6) / 440.0).log2()
}

/// Frequency (Hz) for a (possibly fractional) MIDI note number.
#[inline]
pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2.0f32.powf((midi - 69.0) / 12.0)
}

/// Nearest in-scale note frequency to `f0`, given `root` (0..11) and `scale` index.
/// Chromatic returns the nearest equal-tempered semitone. A ±3-semitone search window covers
/// the widest scale gap (minor pentatonic's 3-semitone step).
pub fn nearest_scale_hz(f0: f32, root: usize, scale: usize) -> f32 {
    if f0 <= 0.0 {
        return f0;
    }
    let mask = SCALES[scale.min(SCALES.len() - 1)];
    let root = (root % 12) as i32;
    let m = hz_to_midi(f0);
    let base = m.round() as i32;
    let mut best_n = base;
    let mut best_d = f32::INFINITY;
    for n in (base - 3)..=(base + 3) {
        let pc = (((n - root) % 12) + 12) % 12;
        if mask.contains(&pc) {
            let d = (m - n as f32).abs();
            if d < best_d {
                best_d = d;
                best_n = n;
            }
        }
    }
    midi_to_hz(best_n as f32)
}

/// Human-readable note name for a frequency, e.g. "A3" / "F#4" (nearest equal-tempered note).
pub fn hz_to_note_name(hz: f32) -> String {
    if !(hz.is_finite()) || hz <= 0.0 {
        return "—".to_string();
    }
    let midi = hz_to_midi(hz).round() as i32;
    let pc = (((midi % 12) + 12) % 12) as usize;
    let octave = midi / 12 - 1;
    format!("{}{}", ROOT_NAMES[pc], octave)
}

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Settings snapshot (already in plain units; ratios linear)
// ---------------------------------------------------------------------------

/// A full snapshot of VOXKEY's effective controls. Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Root note (0 = C .. 11 = B).
    pub root: usize,
    /// Scale index into [`SCALES`].
    pub scale: usize,
    /// Retune glide time (ms); 0 = hard snap (autotune artifact).
    pub retune_ms: f32,
    /// Correction amount 0..1 (scales the cents deviation applied).
    pub amount: f32,
    /// Humanize: peak random cents drift on the target (0 = off).
    pub humanize_cents: f32,
    /// Formant offset shift ratio (2^(st/12)); independent of pitch, preserve always on.
    pub formant_ratio: f32,
    /// Confidence gate 0..1 — below this the correction glides to 1.0 (no retune).
    pub conf_gate: f32,
    /// MIDI override mode: when a note is held it becomes the target (scale ignored).
    pub midi_mode: bool,
    /// Held MIDI note frequency (Hz) when in MIDI mode; `None` = no note held.
    pub held_midi_hz: Option<f32>,
    /// Dry/wet mix 0..1 (0 = pure dry, latency-matched).
    pub mix: f32,
    /// Output trim (linear).
    pub out_gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: 0,
            scale: 1, // Major
            retune_ms: 40.0,
            amount: 1.0,
            humanize_cents: 0.0,
            formant_ratio: 1.0,
            conf_gate: 0.6,
            midi_mode: false,
            held_midi_hz: None,
            mix: 1.0,
            out_gain: 1.0,
        }
    }
}

/// Raw control values in natural param units — built by both the plugin's param snapshot and
/// the preset loader, then resolved (with any live MIDI note) into [`Settings`].
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub root: usize,
    pub scale: usize,
    pub retune_ms: f32,
    pub amount: f32,
    pub humanize_cents: f32,
    pub formant_st: f32,
    pub conf_gate: f32,
    pub midi_mode: bool,
    pub mix: f32,
    pub out_db: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            root: 0,
            scale: 1,
            retune_ms: 40.0,
            amount: 1.0,
            humanize_cents: 0.0,
            formant_st: 0.0,
            conf_gate: 0.6,
            midi_mode: false,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

impl Controls {
    /// Resolve to effective [`Settings`], folding in the live held MIDI note (if any).
    pub fn resolve(&self, held_midi_hz: Option<f32>) -> Settings {
        Settings {
            root: self.root.min(11),
            scale: self.scale.min(SCALES.len() - 1),
            retune_ms: self.retune_ms.max(0.0),
            amount: self.amount.clamp(0.0, 1.0),
            humanize_cents: self.humanize_cents.max(0.0),
            formant_ratio: 2.0f32.powf(self.formant_st / 12.0),
            conf_gate: self.conf_gate.clamp(0.0, 1.0),
            midi_mode: self.midi_mode,
            held_midi_hz: if self.midi_mode { held_midi_hz } else { None },
            mix: self.mix.clamp(0.0, 1.0),
            out_gain: db_to_gain(self.out_db),
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Wet-path safety clip: exact identity for |x| ≤ 0.9, tanh-compressed above so |y| < 1.
/// The identity region keeps the `mix=0` (dry) null exact.
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

/// Per-sample glide coefficient for a one-pole toward a target reaching ~63% in `ms`.
/// `ms <= ~0.1` collapses to 1.0 (hard snap — the autotune artifact).
#[inline]
fn glide_coef(ms: f32, sr: f32) -> f32 {
    if ms <= 0.1 {
        return 1.0;
    }
    let tau = (ms * 0.001 * sr).max(1.0);
    1.0 - (-1.0 / tau).exp()
}

// ---------------------------------------------------------------------------
// RetunePitch — accurate, hysteresis-free streaming pitch (built on suite_core::pitch::Mpm)
// ---------------------------------------------------------------------------

/// Streaming fundamental estimator for the retune ratio. It mirrors the front end of
/// `suite_core::pitch::PitchTracker` (anti-alias → decimate to ~12 kHz → windowed
/// `suite_core::pitch::Mpm`) but takes the raw per-hop detection with only a light median-3
/// for jitter — **no ±35-cent re-lock hysteresis**. PitchTracker's hysteresis is tuned for
/// TRACER's crossover *stability*; on a retuner it biases the correction ratio by up to 35
/// cents (the detector reads that far below the true pitch right after a note change, then
/// sticks), which lands the output that far off the scale tone. A pitch corrector needs an
/// *accurate* f0, so VOXKEY reads `Mpm` directly (same `suite_core::pitch` module).
struct RetunePitch {
    decim: usize,
    aa: Svf,
    decim_count: usize,
    ring: Vec<f32>,
    frame: Vec<f32>,
    write: usize,
    fill: usize,
    hop: usize,
    since_hop: usize,
    mpm: Mpm,
    med: [f32; 3],
    med_pos: usize,
    med_n: usize,
    f0: f32,
    conf: f32,
    default_f0: f32,
}

impl RetunePitch {
    fn new(sr: f32, default_f0: f32) -> Self {
        let input_sr = sr.max(1.0);
        let decim = ((input_sr / 12_000.0).round() as usize).max(1);
        let analysis_sr = input_sr / decim as f32;
        let window = 1024usize;
        let hop = window / 4;
        let mut aa = Svf::new();
        aa.set((analysis_sr * 0.45).min(input_sr * 0.45), 0.707, input_sr);
        let mpm = Mpm::new(window, analysis_sr, 60.0, 1000.0);
        let default_f0 = default_f0.clamp(20.0, 5000.0);
        Self {
            decim,
            aa,
            decim_count: 0,
            ring: vec![0.0; window],
            frame: vec![0.0; window],
            write: 0,
            fill: 0,
            hop,
            since_hop: 0,
            mpm,
            med: [default_f0; 3],
            med_pos: 0,
            med_n: 0,
            f0: default_f0,
            conf: 0.0,
            default_f0,
        }
    }

    fn reset(&mut self) {
        self.aa.reset();
        self.decim_count = 0;
        for v in self.ring.iter_mut() {
            *v = 0.0;
        }
        self.write = 0;
        self.fill = 0;
        self.since_hop = 0;
        self.med = [self.default_f0; 3];
        self.med_pos = 0;
        self.med_n = 0;
        self.f0 = self.default_f0;
        self.conf = 0.0;
    }

    #[inline]
    fn f0(&self) -> f32 {
        self.f0
    }
    #[inline]
    fn confidence(&self) -> f32 {
        self.conf
    }

    #[inline]
    fn push(&mut self, x: f32) {
        let lp = self.aa.process(x).lp;
        self.decim_count += 1;
        if self.decim_count < self.decim {
            return;
        }
        self.decim_count = 0;
        let n = self.ring.len();
        self.ring[self.write] = lp;
        self.write = (self.write + 1) % n;
        if self.fill < n {
            self.fill += 1;
        }
        self.since_hop += 1;
        if self.since_hop >= self.hop && self.fill >= n {
            self.since_hop = 0;
            for i in 0..n {
                self.frame[i] = self.ring[(self.write + i) % n];
            }
            let r = self.mpm.analyze(&self.frame);
            self.conf = r.confidence;
            if r.f0_hz > 0.0 {
                self.med[self.med_pos] = r.f0_hz;
                self.med_pos = (self.med_pos + 1) % 3;
                if self.med_n < 3 {
                    self.med_n += 1;
                }
                self.f0 = median3(&self.med);
            }
        }
    }
}

/// Median of three values.
#[inline]
fn median3(a: &[f32; 3]) -> f32 {
    let (x, y, z) = (a[0], a[1], a[2]);
    x.max(y).min(x.max(z)).max(y.min(z))
}

// ---------------------------------------------------------------------------
// VoxCore
// ---------------------------------------------------------------------------

/// VOXKEY's full stereo DSP core.
pub struct VoxCore {
    sr: f32,
    settings: Settings,

    tracker: RetunePitch,
    shift_l: ShiftEngine,
    shift_r: ShiftEngine,
    dry_l: DelayLine,
    dry_r: DelayLine,

    // Retune glide (in cents, log domain).
    corr_cents: f32,
    glide_c: f32,

    // Humanize drift.
    hum_rng: Rng,
    hum_target: f32,
    hum_smooth: OnePole,
    hum_counter: usize,
    hum_redraw: usize,

    // Smoothed audible scalars.
    sm_amount: Smooth,
    sm_formant: Smooth,
    sm_mix: Smooth,
    sm_out: Smooth,

    // Live read-outs (for the GUI meter + tests). Written every sample.
    last_detected: f32,
    last_target: f32,
    last_conf: f32,
    last_ratio: f32,
    last_active: bool,
}

impl VoxCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let lat = MAIN_FFT;
        let d = Settings::default();

        let tracker = RetunePitch::new(sr, DEFAULT_F0);

        let mut shift_l = ShiftEngine::new(MAIN_FFT, MAIN_HOP, sr);
        let mut shift_r = ShiftEngine::new(MAIN_FFT, MAIN_HOP, sr);
        shift_l.set_envelope_preserve(true);
        shift_r.set_envelope_preserve(true);

        let mut dry_l = DelayLine::new(lat + 1);
        let mut dry_r = DelayLine::new(lat + 1);
        dry_l.set_delay(lat);
        dry_r.set_delay(lat);

        let mut hum_smooth = OnePole::new();
        hum_smooth.set_time(HUM_SMOOTH_MS, sr);
        hum_smooth.reset(0.0);

        Self {
            sr,
            settings: d,
            tracker,
            shift_l,
            shift_r,
            dry_l,
            dry_r,
            corr_cents: 0.0,
            glide_c: glide_coef(d.retune_ms, sr),
            hum_rng: Rng::new(0x00F0_0D17),
            hum_target: 0.0,
            hum_smooth,
            hum_counter: 0,
            hum_redraw: ((HUM_REDRAW_MS * 0.001 * sr) as usize).max(1),
            sm_amount: Smooth::new(sr, d.amount),
            sm_formant: Smooth::new(sr, d.formant_ratio),
            sm_mix: Smooth::new(sr, d.mix),
            sm_out: Smooth::new(sr, d.out_gain),
            last_detected: DEFAULT_F0,
            last_target: DEFAULT_F0,
            last_conf: 0.0,
            last_ratio: 1.0,
            last_active: false,
        }
    }

    /// Reported constant latency in samples (the ShiftEngine FFT size).
    pub fn latency_samples(&self) -> u32 {
        MAIN_FFT as u32
    }

    pub fn reset(&mut self) {
        self.tracker.reset();
        self.shift_l.reset();
        self.shift_r.reset();
        self.dry_l.reset();
        self.dry_r.reset();
        self.corr_cents = 0.0;
        self.hum_target = 0.0;
        self.hum_smooth.reset(0.0);
        self.hum_counter = 0;
        self.sm_amount.reset(self.settings.amount);
        self.sm_formant.reset(self.settings.formant_ratio);
        self.sm_mix.reset(self.settings.mix);
        self.sm_out.reset(self.settings.out_gain);
        self.last_detected = DEFAULT_F0;
        self.last_target = DEFAULT_F0;
        self.last_conf = 0.0;
        self.last_ratio = 1.0;
        self.last_active = false;
    }

    /// Latch a settings snapshot (block rate).
    pub fn configure(&mut self, s: &Settings) {
        self.settings = *s;
        self.shift_l.set_envelope_preserve(true);
        self.shift_r.set_envelope_preserve(true);
        self.glide_c = glide_coef(s.retune_ms, self.sr);
        self.sm_amount.set(s.amount);
        self.sm_formant.set(s.formant_ratio);
        self.sm_mix.set(s.mix);
        self.sm_out.set(s.out_gain);
    }

    // --- Live read-outs for the GUI / tests ---
    pub fn detected_hz(&self) -> f32 {
        self.last_detected
    }
    pub fn target_hz(&self) -> f32 {
        self.last_target
    }
    pub fn confidence(&self) -> f32 {
        self.last_conf
    }
    pub fn ratio(&self) -> f32 {
        self.last_ratio
    }
    pub fn active(&self) -> bool {
        self.last_active
    }

    /// Process one stereo sample pair.
    #[inline]
    pub fn process_sample(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let amount = self.sm_amount.next();
        let formant = self.sm_formant.next();
        let mix = self.sm_mix.next();
        let out_g = self.sm_out.next();

        // Detect pitch on the mono sum.
        let mono = 0.5 * (in_l + in_r);
        self.tracker.push(mono);
        let detected = self.tracker.f0().max(1.0e-3);
        let conf = self.tracker.confidence();
        let s = self.settings;
        let pitched = conf >= s.conf_gate;

        // Choose the target and the raw correction (cents).
        let (corr_base, target_hz, active) = if s.midi_mode {
            match s.held_midi_hz {
                Some(h) => {
                    let c = (1200.0 * (h / detected).log2()).clamp(-1200.0, 1200.0);
                    (c, h, true)
                }
                None => (0.0, detected, false),
            }
        } else if pitched {
            let t = nearest_scale_hz(detected, s.root, s.scale);
            let c = (1200.0 * (t / detected).log2()).clamp(-1200.0, 1200.0);
            (c, t, true)
        } else {
            // Below gate: no target → correction glides to 1.0 (no retune on breaths/noise).
            (0.0, detected, false)
        };

        // Humanize: slow random-walk cents drift on the target (only while correcting).
        self.hum_counter += 1;
        if self.hum_counter >= self.hum_redraw {
            self.hum_counter = 0;
            self.hum_target = self.hum_rng.next_bipolar() * s.humanize_cents;
        }
        let hum = self.hum_smooth.process(self.hum_target);
        let corr_target = corr_base + if active { hum } else { 0.0 };

        // Glide the correction in the log (cents) domain; retune=0 → hard snap.
        self.corr_cents += self.glide_c * (corr_target - self.corr_cents);

        // Amount scales the applied deviation; clamp the ratio to ±1 octave.
        let applied = amount * self.corr_cents;
        let ratio = 2.0f32.powf(applied / 1200.0).clamp(0.5, 2.0);

        // Drive both shift engines (formant offset moves formants independently).
        self.shift_l.set_pitch_ratio(ratio);
        self.shift_r.set_pitch_ratio(ratio);
        self.shift_l.set_formant_ratio(formant);
        self.shift_r.set_formant_ratio(formant);

        let wl = self.shift_l.process(in_l);
        let wr = self.shift_r.process(in_r);
        let dl = self.dry_l.process(in_l);
        let dr = self.dry_r.process(in_r);

        // Read-outs.
        self.last_detected = detected;
        self.last_target = target_hz;
        self.last_conf = conf;
        self.last_ratio = ratio;
        self.last_active = active;

        // Mix (mix=0 → latency-matched dry, exact) + out trim, safety-clipped.
        let out_l = safety_clip(out_g * ((1.0 - mix) * dl + mix * wl));
        let out_r = safety_clip(out_g * ((1.0 - mix) * dr + mix * wr));
        (out_l, out_r)
    }

    /// Offline convenience: process stereo slices in place with fixed settings.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], s: &Settings) {
        self.configure(s);
        self.reset();
        let n = left.len().min(right.len());
        for i in 0..n {
            let (l, r) = self.process_sample(left[i], right[i]);
            left[i] = l;
            right[i] = r;
        }
    }

    /// Offline mono convenience: duplicate to stereo, process, return the L channel.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &Settings) {
        self.configure(s);
        self.reset();
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
    fn nearest_scale_snaps_to_tones() {
        // Root A (9), Natural Minor: A B C D E F G. A slightly-sharp A3 (~223 Hz) → A3.
        let a3 = 220.0f32;
        let snapped = nearest_scale_hz(a3 * 2.0f32.powf(0.3 / 12.0), 9, 2);
        assert!((snapped - a3).abs() < 0.5, "expected snap to A3=220, got {snapped}");
        // A note between C and C# (root C major) rounds to the nearest scale tone.
        let m = hz_to_midi(nearest_scale_hz(261.0, 0, 1));
        assert!((m - m.round()).abs() < 1e-3, "snap should land on an integer MIDI note");
    }

    #[test]
    fn chromatic_snaps_to_nearest_semitone() {
        let f = 260.0f32; // ~C4 (261.63) minus a bit
        let snapped = nearest_scale_hz(f, 0, 0);
        assert!((snapped - 261.63).abs() < 1.0, "chromatic snap to C4, got {snapped}");
    }

    #[test]
    fn glide_coef_snaps_at_zero() {
        assert_eq!(glide_coef(0.0, 48_000.0), 1.0);
        assert!(glide_coef(400.0, 48_000.0) < 0.001);
    }

    #[test]
    fn note_name_roundtrips() {
        assert_eq!(hz_to_note_name(440.0), "A4");
        assert_eq!(hz_to_note_name(261.63), "C4");
    }

    #[test]
    fn mix_zero_equals_latency_delayed_dry() {
        let sr = 48_000.0f32;
        let mut core = VoxCore::new(sr);
        let s = Settings { mix: 0.0, out_gain: 1.0, ..Settings::default() };
        core.configure(&s);
        core.reset();
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
}
