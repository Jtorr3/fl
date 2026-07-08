//! PLUCK DSP core — a 6-string Karplus-Strong strummer.
//!
//! Each string is an extended-Karplus-Strong resonator: a Catmull-Rom **fractional
//! delay line** (copy-pasteable from FLYBY's `dsp::FracDelay`), a **one-pole (2-tap)
//! damping low-pass** in the loop, and a first-order **all-pass fine-tune** in the loop.
//! The loop feedback sets the decay; the damping sets the brightness / HF decay.
//!
//! The six strings are tuned from one of three sources (a `Settings` param):
//!  - **Chord select**: a dark-taste chord table (m, m7, sus2, m9, 5th-stack, sus4)
//!    voiced across six strings on a selectable root.
//!  - **MIDI-held notes**: up to six held notes, voice-assigned low→high (extra strings
//!    octave-double the held notes).
//!  - **Key-detect**: a coarse chromagram over the audio input (`suite_core::stft`),
//!    confidence-gated, falling back to chord-select when unsure.
//!
//! Excitation is the **audio input**: an onset detector fires a STRUM — a staggered
//! excitation across the strings (strum time 5–80 ms, up/down/alternate), where the
//! exciter is a windowed ~500-sample grab of the input at the onset (its timbre colors
//! the pluck). A **continuous-drive** mode feeds the input into the strings at low gain.
//!
//! A small embedded **body IR** (a sum of decaying modal resonances, generated at init)
//! is convolved into the wet path (direct FIR at the SPECS-mandated 2048 taps — benched in the
//! tests and comfortably under real-time at this length).
//!
//! Everything is preallocated; `process_sample` never allocates. Denormals are handled
//! by the caller's `ScopedFtz`.

use suite_core::stft::{Complex, Stft};

pub const MAX_STRINGS: usize = 6;
/// Longest KS loop we ever need: lowest note ~55 Hz at 96 kHz ≈ 1745 samples; round up.
pub const MAX_DELAY: usize = 4096;
/// Windowed exciter-burst length grabbed at each onset (~500 samples ≈ 10 ms @ 48 k).
pub const BURST: usize = 500;
/// Embedded body impulse response length (taps). SPECS "PLUCK" mandates a **2048-tap body IR**;
/// the direct-FIR body convolution is benched in the tests (`body_conv_within_rt_budget`) and
/// stays well under real-time at this length on the build machine.
pub const BODY_LEN: usize = 2048;

const PI: f32 = std::f32::consts::PI;

// ---------------------------------------------------------------------------
// Enums (tuning source / chord / strum direction).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TuningSource {
    Chord,
    Midi,
    KeyDetect,
}
impl TuningSource {
    pub fn from_index(i: usize) -> TuningSource {
        match i {
            1 => TuningSource::Midi,
            2 => TuningSource::KeyDetect,
            _ => TuningSource::Chord,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chord {
    Minor,
    Minor7,
    Sus2,
    Minor9,
    Power5,
    Sus4,
}
impl Chord {
    pub fn from_index(i: usize) -> Chord {
        match i {
            1 => Chord::Minor7,
            2 => Chord::Sus2,
            3 => Chord::Minor9,
            4 => Chord::Power5,
            5 => Chord::Sus4,
            _ => Chord::Minor,
        }
    }
    /// Six semitone offsets (low→high) from the root note. Dark-taste voicings.
    pub fn voicing(self) -> [i32; MAX_STRINGS] {
        match self {
            Chord::Minor => [0, 7, 12, 15, 19, 24],
            Chord::Minor7 => [0, 7, 10, 15, 19, 22],
            Chord::Sus2 => [0, 7, 12, 14, 19, 24],
            Chord::Minor9 => [0, 7, 10, 14, 15, 19],
            Chord::Power5 => [0, 7, 12, 19, 24, 31],
            Chord::Sus4 => [0, 7, 12, 17, 19, 24],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrumDir {
    Up,
    Down,
    Alternate,
}
impl StrumDir {
    pub fn from_index(i: usize) -> StrumDir {
        match i {
            1 => StrumDir::Down,
            2 => StrumDir::Alternate,
            _ => StrumDir::Up,
        }
    }
}

/// MIDI note number → frequency (Hz).
#[inline]
pub fn midi_to_freq(m: f32) -> f32 {
    440.0 * 2.0f32.powf((m - 69.0) / 12.0)
}

/// Lowest string's root MIDI note for chord/key-detect voicings (C2 = 36).
pub const BASE_MIDI: i32 = 36;

// ---------------------------------------------------------------------------
// Per-render settings (sampled once per block from the params).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub source: TuningSource,
    pub root_pc: i32, // 0..11
    pub chord: Chord,
    pub decay: f32,     // 0..1 → sustain time
    pub damp: f32,      // 0..1 → brightness (0 dark? see map)
    pub strum_ms: f32,  // 5..80
    pub dir: StrumDir,
    pub exciter_gain: f32, // 0..2 linear
    pub continuous: bool,
    pub vel_bright: f32, // 0..1
    pub body: f32,       // 0..1
    pub spread_cents: f32, // 0..50
    pub stereo_alt: f32, // 0..1
    pub wet_solo: bool,
    pub mix: f32,   // 0..1
    pub out_db: f32,
    /// Up to six held MIDI notes (Hz), NaN = empty. Voice-assign source for Midi mode.
    pub held: [f32; MAX_STRINGS],
    pub held_count: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            source: TuningSource::Chord,
            root_pc: 0,
            chord: Chord::Minor,
            decay: 0.6,
            damp: 0.4,
            strum_ms: 25.0,
            dir: StrumDir::Up,
            exciter_gain: 1.0,
            continuous: false,
            vel_bright: 0.4,
            body: 0.4,
            spread_cents: 6.0,
            stereo_alt: 0.5,
            wet_solo: false,
            mix: 1.0,
            out_db: 0.0,
            held: [f32::NAN; MAX_STRINGS],
            held_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// FracDelay — Catmull-Rom fractional delay (copy of FLYBY's dsp::FracDelay).
// ---------------------------------------------------------------------------

struct FracDelay {
    buf: Vec<f32>,
    wpos: usize,
}
impl FracDelay {
    fn new(max_delay: usize) -> Self {
        Self {
            buf: vec![0.0; max_delay.max(8) + 4],
            wpos: 0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
    }
    /// Read `delay` samples in the past (Catmull-Rom over the 4 straddling samples).
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let len = self.buf.len() as isize;
        let d = delay.clamp(1.0, (self.buf.len() - 3) as f32);
        let rpos = self.wpos as f32 - d;
        let base = rpos.floor();
        let frac = rpos - base;
        let i1 = base as isize;
        let get = |k: isize| -> f32 {
            let idx = ((i1 + k).rem_euclid(len)) as usize;
            self.buf[idx]
        };
        catmull_rom(get(-1), get(0), get(1), get(2), frac)
    }
}

#[inline]
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let a0 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
    let a1 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let a2 = -0.5 * p0 + 0.5 * p2;
    ((a0 * t + a1) * t + a2) * t + p1
}

// ---------------------------------------------------------------------------
// KsString — one extended-Karplus-Strong resonator.
// ---------------------------------------------------------------------------

struct KsString {
    frac: FracDelay,
    damp_z: f32, // one-pole (2-tap) LP state: previous delay-line output
    ap_x1: f32,  // all-pass input history
    ap_y1: f32,  // all-pass output history
    // Loop parameters, set by `tune`:
    delay_read: f32, // continuous read length so total loop delay == period
    s_damp: f32,     // 2-tap damping coefficient (0..0.49)
    c_ap: f32,       // all-pass coefficient
    feedback: f32,   // loop gain (decay)
    freq: f32,
    // Excitation schedule (samples from the last strum trigger):
    exc_start: usize,
    exc_pos: usize,
    exc_active: bool,
    exc_gain: f32,
}

impl KsString {
    fn new() -> Self {
        Self {
            frac: FracDelay::new(MAX_DELAY),
            damp_z: 0.0,
            ap_x1: 0.0,
            ap_y1: 0.0,
            delay_read: 100.0,
            s_damp: 0.25,
            c_ap: 0.0,
            feedback: 0.99,
            freq: 110.0,
            exc_start: 0,
            exc_pos: 0,
            exc_active: false,
            exc_gain: 0.0,
        }
    }

    fn reset(&mut self) {
        self.frac.reset();
        self.damp_z = 0.0;
        self.ap_x1 = 0.0;
        self.ap_y1 = 0.0;
        self.exc_active = false;
        self.exc_pos = 0;
    }

    /// Set the loop for a target frequency, damping (0..0.49) and feedback.
    fn tune(&mut self, freq: f32, sr: f32, s_damp: f32, feedback: f32) {
        self.freq = freq;
        self.s_damp = s_damp.clamp(0.0, 0.49);
        // Mild fixed all-pass dispersion; its low-freq phase delay is subtracted so the
        // fundamental stays in tune (the all-pass stretches only the upper partials).
        self.c_ap = 0.0;
        let p_ap = (1.0 - self.c_ap) / (1.0 + self.c_ap); // low-freq phase delay
        let period = sr / freq.max(20.0);
        // total loop delay = frac_read + p_damp + p_ap == period.
        let p_damp = self.s_damp; // ≈ 2-tap phase delay at low freq
        self.delay_read = (period - p_damp - p_ap).clamp(2.0, (MAX_DELAY - 4) as f32);
        self.feedback = feedback.clamp(0.0, 0.99995);
    }

    /// Schedule this string's excitation to begin `start` samples after the strum trigger.
    fn schedule(&mut self, start: usize, gain: f32) {
        self.exc_start = start;
        self.exc_pos = 0;
        self.exc_active = false;
        self.exc_gain = gain;
    }

    #[inline]
    fn process(&mut self, strum_clock: usize, burst: &[f32], cont: f32) -> f32 {
        // Excitation from the strum burst (staggered start) + continuous drive.
        let mut exc = cont;
        if !self.exc_active && strum_clock >= self.exc_start {
            self.exc_active = true;
            self.exc_pos = 0;
        }
        if self.exc_active && self.exc_pos < burst.len() {
            exc += burst[self.exc_pos] * self.exc_gain;
            self.exc_pos += 1;
            if self.exc_pos >= burst.len() {
                self.exc_active = false;
            }
        }

        let delayed = self.frac.read(self.delay_read);
        // One-pole (2-tap) damping low-pass in the loop.
        let lp = (1.0 - self.s_damp) * delayed + self.s_damp * self.damp_z;
        self.damp_z = delayed;
        // First-order all-pass fine-tune in the loop: y = c*x + x1 - c*y1.
        let ap = self.c_ap * lp + self.ap_x1 - self.c_ap * self.ap_y1;
        self.ap_x1 = lp;
        self.ap_y1 = ap;
        // Feedback + excitation into the loop.
        let inp = self.feedback * ap + exc;
        self.frac.write(inp);
        delayed
    }
}

// ---------------------------------------------------------------------------
// Body IR — sum of decaying modal resonances, direct-FIR convolution.
// ---------------------------------------------------------------------------

struct Body {
    ir: [f32; BODY_LEN],
    ring_l: [f32; BODY_LEN],
    ring_r: [f32; BODY_LEN],
    pos: usize,
}
impl Body {
    fn new(sr: f32) -> Self {
        let mut b = Self {
            ir: [0.0; BODY_LEN],
            ring_l: [0.0; BODY_LEN],
            ring_r: [0.0; BODY_LEN],
            pos: 0,
        };
        b.generate(sr);
        b
    }
    /// Generate a plausible small-instrument body IR: a direct impulse plus a handful of
    /// decaying modal resonances (like a guitar/harp body's low modes).
    fn generate(&mut self, sr: f32) {
        for v in self.ir.iter_mut() {
            *v = 0.0;
        }
        // (freq Hz, decay time s, amplitude) — a few body modes.
        let modes: [(f32, f32, f32); 6] = [
            (98.0, 0.10, 1.0),
            (196.0, 0.08, 0.7),
            (392.0, 0.06, 0.5),
            (740.0, 0.05, 0.4),
            (1300.0, 0.035, 0.28),
            (2600.0, 0.02, 0.18),
        ];
        for n in 0..BODY_LEN {
            let t = n as f32 / sr;
            let mut acc = 0.0;
            for &(f, dec, amp) in modes.iter() {
                acc += amp * (-t / dec).exp() * (2.0 * PI * f * t).sin();
            }
            self.ir[n] = acc;
        }
        // Prepend a strong direct impulse so body=full keeps some attack transient.
        self.ir[0] += 1.0;
        // Normalize to unit L2 energy so the wet level stays sane.
        let energy: f32 = self.ir.iter().map(|v| v * v).sum::<f32>().sqrt();
        if energy > 1e-9 {
            let g = 1.0 / energy;
            for v in self.ir.iter_mut() {
                *v *= g;
            }
        }
    }
    fn reset(&mut self) {
        for v in self.ring_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.ring_r.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }
    /// Direct-FIR convolution of the stereo wet pair through the body IR.
    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.ring_l[self.pos] = l;
        self.ring_r[self.pos] = r;
        let mut yl = 0.0;
        let mut yr = 0.0;
        // ring[pos] is the newest sample; walk the IR backwards over history.
        let mut idx = self.pos;
        for k in 0..BODY_LEN {
            let h = self.ir[k];
            yl += h * self.ring_l[idx];
            yr += h * self.ring_r[idx];
            idx = if idx == 0 { BODY_LEN - 1 } else { idx - 1 };
        }
        self.pos += 1;
        if self.pos >= BODY_LEN {
            self.pos = 0;
        }
        (yl, yr)
    }
}

// ---------------------------------------------------------------------------
// Onset detector — fast/slow envelope with refractory.
// ---------------------------------------------------------------------------

struct Onset {
    fast: f32,
    slow: f32,
    a_fast: f32,
    r_fast: f32,
    r_slow: f32,
    since: usize,
    refractory: usize,
}
impl Onset {
    fn new(sr: f32) -> Self {
        let a_fast = 1.0 - (-1.0 / (0.001 * sr)).exp(); // 1 ms attack
        let r_fast = 1.0 - (-1.0 / (0.030 * sr)).exp(); // 30 ms release
        let r_slow = 1.0 - (-1.0 / (0.150 * sr)).exp(); // 150 ms floor
        Self {
            fast: 0.0,
            slow: 0.0,
            a_fast,
            r_fast,
            r_slow,
            since: 999_999,
            refractory: (0.040 * sr) as usize, // 40 ms
        }
    }
    fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
        self.since = 999_999;
    }
    /// Returns Some(velocity 0..1) on an onset.
    #[inline]
    fn process(&mut self, x: f32) -> Option<f32> {
        let a = x.abs();
        // Fast envelope: quick attack, medium release.
        if a > self.fast {
            self.fast += self.a_fast * (a - self.fast);
        } else {
            self.fast += self.r_fast * (a - self.fast);
        }
        // Slow reference floor.
        self.slow += self.r_slow * (a - self.slow);
        self.since = self.since.saturating_add(1);

        let onset = self.fast > 0.02
            && self.fast > self.slow * 1.8 + 0.01
            && self.since >= self.refractory;
        if onset {
            self.since = 0;
            Some(self.fast.clamp(0.0, 1.0))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// KeyDetect — coarse chromagram over the input (suite_core::stft).
// ---------------------------------------------------------------------------

struct KeyDetect {
    stft: Stft,
    chroma: [f32; 12],
    sr: f32,
    frames: u32,
}
impl KeyDetect {
    fn new(sr: f32) -> Self {
        Self {
            stft: Stft::new(4096, 1024),
            chroma: [0.0; 12],
            sr,
            frames: 0,
        }
    }
    fn reset(&mut self) {
        self.stft.reset();
        self.chroma = [0.0; 12];
        self.frames = 0;
    }
    #[inline]
    fn push(&mut self, x: f32) {
        // Split-borrow self so the STFT callback can touch chroma/frames independently.
        let Self {
            stft,
            chroma,
            sr,
            frames,
        } = self;
        let sr = *sr;
        stft.process(x, &mut |spec: &mut [Complex<f32>]| {
            // Decay the running chroma, then fold this frame's magnitude into 12 classes.
            for c in chroma.iter_mut() {
                *c *= 0.90;
            }
            let bins = spec.len();
            for k in 1..bins {
                let f = k as f32 * sr / 4096.0;
                if f < 55.0 || f > 5000.0 {
                    continue;
                }
                let mag = spec[k].norm();
                // pitch class = round(12*log2(f/C0)) mod 12; C0 = 16.3516 Hz.
                let pc = (12.0 * (f / 16.35160).log2()).round() as i32;
                let pc = pc.rem_euclid(12) as usize;
                chroma[pc] += mag;
            }
            *frames = frames.saturating_add(1);
        });
    }
    /// Best (root pitch-class, is_minor, confidence 0..1) if we have enough evidence.
    fn analyze(&self) -> Option<(i32, bool, f32)> {
        if self.frames < 3 {
            return None;
        }
        let sum: f32 = self.chroma.iter().sum();
        if sum < 1e-6 {
            return None;
        }
        let mut root = 0usize;
        let mut max = self.chroma[0];
        for (i, &c) in self.chroma.iter().enumerate() {
            if c > max {
                max = c;
                root = i;
            }
        }
        let mean = sum / 12.0;
        let conf = ((max - mean) / max).clamp(0.0, 1.0);
        let minor3 = self.chroma[(root + 3) % 12];
        let major3 = self.chroma[(root + 4) % 12];
        Some((root as i32, minor3 >= major3, conf))
    }
}

// ---------------------------------------------------------------------------
// PluckCore — the whole instrument (stereo out, mono/stereo in as exciter).
// ---------------------------------------------------------------------------

pub struct PluckCore {
    sr: f32,
    strings: Vec<KsString>,
    body: Body,
    onset: Onset,
    key: KeyDetect,
    // The current windowed exciter burst (captured forward from an onset).
    burst: [f32; BURST],
    // Forward capture of the input burst following an onset (colors the pluck).
    capturing: bool,
    cap_count: usize,
    pending_vel: f32,
    strum_clock: usize,
    strumming: bool,
    alt_flip: bool,
    // Smoothers for the null-preserving output stage.
    mix_s: f32,
    out_s: f32,
    primed: bool,
    // Per-string tuned freqs (published for tests/GUI).
    freqs: [f32; MAX_STRINGS],
    // Last strum's per-string onset offsets (samples) + order — for the done-bar test.
    last_onsets: [usize; MAX_STRINGS],
    // Pan gains per string (equal-power), set from stereo_alt.
    pan_l: [f32; MAX_STRINGS],
    pan_r: [f32; MAX_STRINGS],
    // Per-string smoothed |output| envelope for the GUI activity display.
    str_env: [f32; MAX_STRINGS],
}

impl PluckCore {
    pub fn new(sr: f32) -> Self {
        let mut strings = Vec::with_capacity(MAX_STRINGS);
        for _ in 0..MAX_STRINGS {
            strings.push(KsString::new());
        }
        Self {
            sr,
            strings,
            body: Body::new(sr),
            onset: Onset::new(sr),
            key: KeyDetect::new(sr),
            burst: [0.0; BURST],
            capturing: false,
            cap_count: 0,
            pending_vel: 0.0,
            strum_clock: 0,
            strumming: false,
            alt_flip: false,
            mix_s: 1.0,
            out_s: 1.0,
            primed: false,
            freqs: [110.0; MAX_STRINGS],
            last_onsets: [0; MAX_STRINGS],
            pan_l: [0.70710677; MAX_STRINGS],
            pan_r: [0.70710677; MAX_STRINGS],
            str_env: [0.0; MAX_STRINGS],
        }
    }

    pub fn string_env(&self) -> [f32; MAX_STRINGS] {
        self.str_env
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.body = Body::new(sr);
        self.onset = Onset::new(sr);
        self.key = KeyDetect::new(sr);
    }

    pub fn reset(&mut self) {
        for s in self.strings.iter_mut() {
            s.reset();
        }
        self.body.reset();
        self.onset.reset();
        self.key.reset();
        self.burst = [0.0; BURST];
        self.capturing = false;
        self.cap_count = 0;
        self.strum_clock = 0;
        self.strumming = false;
    }

    pub fn tuned_freqs(&self) -> [f32; MAX_STRINGS] {
        self.freqs
    }
    pub fn last_onsets(&self) -> [usize; MAX_STRINGS] {
        self.last_onsets
    }

    /// Map decay param (0..1) → per-string feedback gain for a target sustain time.
    fn feedback_for(&self, decay: f32, freq: f32) -> f32 {
        // Sustain time T (seconds) for a ~60 dB fundamental decay.
        let t = 0.30 * (40.0f32).powf(decay.clamp(0.0, 1.0)); // 0.3 .. 12 s
        let period = self.sr / freq.max(20.0);
        // amplitude ^ (t*sr/period) = 1e-3  ⇒  g = 10^(-3*period/(t*sr)).
        let g = 10.0f32.powf(-3.0 * period / (t * self.sr));
        g.clamp(0.80, 0.99995)
    }

    /// Damping coefficient (2-tap LP) from the damp param. damp=0 → bright (little LP),
    /// damp=1 → dark (max LP). Optionally brightened by velocity.
    fn damp_coeff(damp: f32, vel_bright: f32, vel: f32) -> f32 {
        let base = 0.48 * damp.clamp(0.0, 1.0);
        (base * (1.0 - vel_bright.clamp(0.0, 1.0) * vel)).clamp(0.0, 0.49)
    }

    /// Compute the six target frequencies from the active tuning source.
    fn compute_freqs(&self, s: &Settings) -> [f32; MAX_STRINGS] {
        let mut f = [110.0; MAX_STRINGS];
        // Detune spread across strings, symmetric ±spread_cents.
        let spread = |i: usize| -> f32 {
            let c = ((i as f32) - (MAX_STRINGS as f32 - 1.0) * 0.5)
                / ((MAX_STRINGS as f32 - 1.0) * 0.5);
            s.spread_cents * c
        };
        match s.source {
            TuningSource::Midi if s.held_count > 0 => {
                // Voice-assign held notes low→high; extra strings octave-double. `configure()`
                // runs once PER BLOCK on the RT thread, so this must not allocate: a fixed
                // `[f32; MAX_STRINGS]` scratch + in-place `sort_unstable_by` (non-allocating;
                // stable `sort_by` heap-allocates a scratch buffer) replaces the old
                // `Vec` collect+sort. Held notes are bounded at MAX_STRINGS.
                let mut notes = [0.0f32; MAX_STRINGS];
                let mut nf = 0usize;
                for &v in s.held.iter() {
                    if v.is_finite() && nf < MAX_STRINGS {
                        notes[nf] = v;
                        nf += 1;
                    }
                }
                notes[..nf].sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
                let n = nf.max(1);
                for i in 0..MAX_STRINGS {
                    let base = notes[i % n];
                    let oct = (i / n) as f32; // stack octaves for extra strings
                    let hz = base * 2.0f32.powf(oct);
                    f[i] = hz * 2.0f32.powf(spread(i) / 1200.0);
                }
            }
            TuningSource::KeyDetect => {
                // Chromagram → root + minor/major; fall back to chord-select when unsure.
                let (root_pc, is_minor) = match self.key.analyze() {
                    Some((pc, minor, conf)) if conf > 0.12 => (pc, minor),
                    _ => (s.root_pc, true),
                };
                let voicing = if is_minor {
                    Chord::Minor.voicing()
                } else {
                    Chord::Sus2.voicing()
                };
                for i in 0..MAX_STRINGS {
                    let m = BASE_MIDI + root_pc + voicing[i];
                    f[i] = midi_to_freq(m as f32) * 2.0f32.powf(spread(i) / 1200.0);
                }
            }
            _ => {
                // Chord select (also the Midi fallback when no notes are held).
                let voicing = s.chord.voicing();
                for i in 0..MAX_STRINGS {
                    let m = BASE_MIDI + s.root_pc + voicing[i];
                    f[i] = midi_to_freq(m as f32) * 2.0f32.powf(spread(i) / 1200.0);
                }
            }
        }
        f
    }

    /// Recompute string tuning + pans + feedback from the current settings (per block).
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.mix_s = s.mix.clamp(0.0, 1.0);
            self.out_s = db_to_lin(s.out_db);
            self.primed = true;
        }
        let freqs = self.compute_freqs(s);
        self.freqs = freqs;
        let s_damp = Self::damp_coeff(s.damp, 0.0, 0.0);
        for i in 0..MAX_STRINGS {
            let fb = self.feedback_for(s.decay, freqs[i]);
            self.strings[i].tune(freqs[i], self.sr, s_damp, fb);
        }
        // Stereo-alternate pans: even strings left, odd strings right, by amount.
        let amt = s.stereo_alt.clamp(0.0, 1.0);
        for i in 0..MAX_STRINGS {
            // pan position -amt..+amt
            let pos = if i % 2 == 0 { -amt } else { amt };
            let theta = (pos * 0.5 + 0.5) * std::f32::consts::FRAC_PI_2;
            self.pan_l[i] = theta.cos();
            self.pan_r[i] = theta.sin();
        }
    }

    /// Fire a strum from the already-captured burst: schedule staggered excitation.
    fn fire_strum(&mut self, vel: f32, s: &Settings) {
        // Re-brighten damping by velocity for this strum.
        let s_damp = Self::damp_coeff(s.damp, s.vel_bright, vel);
        for i in 0..MAX_STRINGS {
            let freq = self.freqs[i];
            let fb = self.feedback_for(s.decay, freq);
            self.strings[i].tune(freq, self.sr, s_damp, fb);
        }
        // Staggered onset offsets: stride = strum_time / 5 (6 strings, 5 gaps).
        let strum_samples = (s.strum_ms.clamp(1.0, 200.0) * 0.001 * self.sr) as usize;
        let stride = strum_samples / (MAX_STRINGS - 1);
        let dir = match s.dir {
            StrumDir::Alternate => {
                self.alt_flip = !self.alt_flip;
                if self.alt_flip {
                    StrumDir::Up
                } else {
                    StrumDir::Down
                }
            }
            d => d,
        };
        let gain = s.exciter_gain.clamp(0.0, 4.0) * (0.3 + 0.7 * vel);
        for i in 0..MAX_STRINGS {
            let order = match dir {
                StrumDir::Down => MAX_STRINGS - 1 - i,
                _ => i, // Up
            };
            let start = order * stride;
            self.strings[i].schedule(start, gain);
            self.last_onsets[i] = start;
        }
        self.strum_clock = 0;
        self.strumming = true;
    }

    /// Process one stereo input frame → one stereo output frame.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        let mono = 0.5 * (l_in + r_in);

        // Key detection (only when that source is active).
        if s.source == TuningSource::KeyDetect {
            self.key.push(mono);
        }

        // Onset → begin a forward capture of the input burst (the pick attack/timbre).
        if !self.capturing {
            if let Some(vel) = self.onset.process(mono) {
                self.capturing = true;
                self.cap_count = 0;
                self.pending_vel = vel;
            }
        } else {
            // Keep the onset envelope running (but ignore new onsets while capturing).
            let _ = self.onset.process(mono);
        }
        // While capturing, window the live input into the burst; fire the strum when full.
        if self.capturing {
            let j = self.cap_count;
            let w = 0.5 - 0.5 * (2.0 * PI * j as f32 / (BURST as f32 - 1.0)).cos();
            self.burst[j] = mono * w;
            self.cap_count += 1;
            if self.cap_count >= BURST {
                self.capturing = false;
                let vel = self.pending_vel;
                self.fire_strum(vel, s);
            }
        }

        // Continuous drive: constant low-gain input into every string.
        let cont = if s.continuous {
            mono * s.exciter_gain.clamp(0.0, 4.0) * 0.05
        } else {
            0.0
        };

        // Run the six strings, panned.
        let clk = if self.strumming { self.strum_clock } else { usize::MAX };
        let mut wl = 0.0;
        let mut wr = 0.0;
        for i in 0..MAX_STRINGS {
            let y = self.strings[i].process(clk, &self.burst, cont);
            let a = y.abs();
            self.str_env[i] = if a > self.str_env[i] {
                a
            } else {
                self.str_env[i] * 0.9995
            };
            wl += y * self.pan_l[i];
            wr += y * self.pan_r[i];
        }
        // Normalize the string sum a touch (6 strings) to avoid clipping headroom loss.
        wl *= 0.5;
        wr *= 0.5;
        if self.strumming {
            self.strum_clock = self.strum_clock.saturating_add(1);
        }

        // Body: blend dry wet with its convolution.
        let (bl, br) = self.body.process(wl, wr);
        let body = s.body.clamp(0.0, 1.0);
        let wet_l = (1.0 - body) * wl + body * bl;
        let wet_r = (1.0 - body) * wr + body * br;

        // Null-preserving dry/wet mix + output trim (mix smoothed per sample).
        let mix_t = if s.wet_solo { 1.0 } else { s.mix.clamp(0.0, 1.0) };
        self.mix_s += 0.001 * (mix_t - self.mix_s);
        let out_lin = db_to_lin(s.out_db);
        self.out_s += 0.001 * (out_lin - self.out_s);
        let ol = (l_in * (1.0 - self.mix_s) + wet_l * self.mix_s) * self.out_s;
        let or = (r_in * (1.0 - self.mix_s) + wet_r * self.mix_s) * self.out_s;

        (ol.clamp(-0.999, 0.999), or.clamp(-0.999, 0.999))
    }

    /// Stereo render from a mono input (input fed to both channels as the exciter).
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

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// Test-only microbench: run the stereo body convolution over `seconds` of audio at `sr` and
/// return its cost as a percentage of real time (compute_time / audio_duration × 100). Used by
/// the BODY_LEN spec-compliance test to confirm the 2048-tap direct FIR stays within budget.
#[cfg(test)]
pub fn bench_body_rt_percent(sr: f32, seconds: f32) -> f32 {
    use std::time::Instant;
    let mut body = Body::new(sr);
    let n = (sr * seconds) as usize;
    let mut acc = 0.0f32;
    let t0 = Instant::now();
    for i in 0..n {
        let x = (i as f32 * 0.001).sin();
        let (l, r) = body.process(x, x * 0.5);
        acc += l + r;
    }
    let elapsed = t0.elapsed().as_secs_f32();
    std::hint::black_box(acc);
    100.0 * elapsed / seconds
}
