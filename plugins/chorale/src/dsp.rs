//! CHORALE DSP core — a bank of 12–24 waveguide resonators.
//!
//! Each resonator is an extended-Karplus-Strong loop (the PLUCK recipe): a Catmull-Rom
//! **fractional delay line** + a **one-pole (2-tap) damping low-pass** + a first-order
//! **all-pass fine-tune**, all in the loop, with the loop delay solved so the fundamental
//! stays in tune to within ±10 cents (`frac_read + p_damp + p_ap == sr / f0`). A gentle
//! `tanh` soft-clip in the feedback path makes each loop self-limiting at high feedback so
//! the continuously-excited bank never runs away, and a DC blocker cleans the sum.
//!
//! Unlike PLUCK (onset-triggered strum), CHORALE is **continuously excited**: the audio
//! input feeds every resonator all the time, so the tuned loops ring sympathetically. Each
//! resonator's input gain is optionally weighted by the input's band energy at that
//! resonator's pitch (via `suite_core::spectrum::SpectrumTap`), so the bank "sings" the
//! notes present in the source.
//!
//! Tuning sources: (a) held MIDI notes (voice-assigned, octave-stacked), (b) a selected
//! scale/chord on a root, spread across octaves, (c) a chromagram key-detect (confidence
//! gated, falling back to the scale/chord).
//!
//! Everything is preallocated (24 loops at max delay); `process_sample` never allocates.
//! Denormals are handled by the caller's `ScopedFtz`.

use suite_core::spectrum::{SpectrumTap, F_HIGH, F_LOW, NUM_BANDS};
use suite_core::stft::{Complex, Stft};

/// Maximum resonators in the bank (params expose 12..=24).
pub const MAX_RESONATORS: usize = 24;
/// Longest loop we ever need: ~30 Hz at 96 kHz ≈ 3200 samples; round up.
pub const MAX_DELAY: usize = 4096;
/// Recompute the sympathetic weights (and republish the input band energies) this often.
/// Block-size-independent so the core behaves identically under the plugin and the tests.
const WEIGHT_UPDATE: usize = 2048;

const PI: f32 = std::f32::consts::PI;

/// Glide time (ms) for a live retune of the loop length. When the target chord/scale/root (or
/// a held MIDI chord) changes while a string is ringing, the delay-length target moves and the
/// read length slews toward it over this time instead of jumping — a jump zips/clicks the tail.
/// The FIRST tuning of each resonator snaps (no ringing to protect); only live retunes glide.
const RETUNE_GLIDE_MS: f32 = 25.0;

/// Lowest resonator's anchor MIDI note before the root offset / octave spread (C2 = 36).
pub const BASE_MIDI: i32 = 36;
/// The bank spreads across at most this many octaves; past it the pitches wrap back down and
/// stack (duplicates then detune under `spread`, thickening the sound) rather than climbing
/// toward Nyquist. Keeps every scale/chord musical regardless of resonator count.
const SPAN_OCT: i32 = 5;

// ---------------------------------------------------------------------------
// Enums (tuning source / scale-chord type).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TuningSource {
    Scale,
    Midi,
    KeyDetect,
}
impl TuningSource {
    pub fn from_index(i: usize) -> TuningSource {
        match i {
            1 => TuningSource::Midi,
            2 => TuningSource::KeyDetect,
            _ => TuningSource::Scale,
        }
    }
}

/// The scale / chord the resonators are tuned to. Each returns a set of semitone offsets
/// within one octave; the bank fills `count` resonators by walking the offsets and stacking
/// octaves (`offset[i % L] + 12 * (i / L)`), spreading the pitches across the range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    MinorTriad,
    MajorTriad,
    Minor7,
    Major7,
    Sus2,
    Sus4,
    Power5,
    MinorPentatonic,
    MajorPentatonic,
    Phrygian,
    Dorian,
    Octaves,
}
impl Scale {
    pub fn from_index(i: usize) -> Scale {
        match i {
            1 => Scale::MajorTriad,
            2 => Scale::Minor7,
            3 => Scale::Major7,
            4 => Scale::Sus2,
            5 => Scale::Sus4,
            6 => Scale::Power5,
            7 => Scale::MinorPentatonic,
            8 => Scale::MajorPentatonic,
            9 => Scale::Phrygian,
            10 => Scale::Dorian,
            11 => Scale::Octaves,
            _ => Scale::MinorTriad,
        }
    }
    /// Semitone offsets within an octave (low→high), root-relative.
    pub fn offsets(self) -> &'static [i32] {
        match self {
            Scale::MinorTriad => &[0, 3, 7],
            Scale::MajorTriad => &[0, 4, 7],
            Scale::Minor7 => &[0, 3, 7, 10],
            Scale::Major7 => &[0, 4, 7, 11],
            Scale::Sus2 => &[0, 2, 7],
            Scale::Sus4 => &[0, 5, 7],
            Scale::Power5 => &[0, 7],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Octaves => &[0],
        }
    }
}

/// MIDI note number → frequency (Hz).
#[inline]
pub fn midi_to_freq(m: f32) -> f32 {
    440.0 * 2.0f32.powf((m - 69.0) / 12.0)
}

// ---------------------------------------------------------------------------
// Per-render settings (sampled once per block from the params).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub source: TuningSource,
    pub root_pc: i32, // 0..11
    pub scale: Scale,
    pub count: usize,    // 12..=24 active resonators
    pub decay: f32,      // 0..1 → sustain time
    pub damp: f32,       // 0..1 → darkness (loop LP)
    pub spread_cents: f32, // 0..50, alternating ±
    pub sympathetic: f32, // 0..1 weighting amount
    pub excite: f32,      // 0..2 input drive into the bank
    pub stereo: f32,      // 0..1 alternate/width amount
    pub wet_solo: bool,
    pub mix: f32,  // 0..1
    pub out_db: f32,
    /// Up to MAX_RESONATORS held MIDI notes (Hz), NaN = empty (Midi source).
    pub held: [f32; MAX_RESONATORS],
    pub held_count: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            source: TuningSource::Scale,
            root_pc: 9, // A
            scale: Scale::MinorTriad,
            count: 16,
            decay: 0.85,
            damp: 0.4,
            spread_cents: 6.0,
            sympathetic: 0.5,
            excite: 1.0,
            stereo: 0.6,
            wet_solo: false,
            mix: 0.5,
            out_db: 0.0,
            held: [f32::NAN; MAX_RESONATORS],
            held_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// FracDelay — Catmull-Rom fractional delay (copy of FLYBY/PLUCK's dsp::FracDelay).
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
// Resonator — one extended-Karplus-Strong loop with a self-limiting feedback path.
// ---------------------------------------------------------------------------

struct Resonator {
    frac: FracDelay,
    damp_z: f32,
    ap_x1: f32,
    ap_y1: f32,
    delay_read: f32,   // the length actually read (glides toward `delay_target`)
    delay_target: f32, // the solved loop length for the current tuning (−1 = never tuned)
    glide_coef: f32,   // one-pole slew coefficient for delay_read → delay_target
    s_damp: f32,
    c_ap: f32,
    feedback: f32,
    freq: f32,
    in_gain: f32,  // per-resonator continuous input gain (base × sympathetic weight)
    env: f32,      // smoothed |output| for the GUI activity display
}
impl Resonator {
    fn new() -> Self {
        Self {
            frac: FracDelay::new(MAX_DELAY),
            damp_z: 0.0,
            ap_x1: 0.0,
            ap_y1: 0.0,
            delay_read: 200.0,
            delay_target: -1.0,
            glide_coef: 1.0,
            s_damp: 0.2,
            c_ap: 0.0,
            feedback: 0.99,
            freq: 220.0,
            in_gain: 0.0,
            env: 0.0,
        }
    }
    fn reset(&mut self) {
        self.frac.reset();
        self.damp_z = 0.0;
        self.ap_x1 = 0.0;
        self.ap_y1 = 0.0;
        self.env = 0.0;
        // The buffer is now silent (no ringing to protect) → let the next tuning snap.
        self.delay_target = -1.0;
    }
    /// Solve the loop for `freq` (PLUCK's cent-accurate tuning: total loop delay == period).
    fn tune(&mut self, freq: f32, sr: f32, s_damp: f32, feedback: f32) {
        self.freq = freq;
        self.s_damp = s_damp.clamp(0.0, 0.49);
        self.c_ap = 0.0;
        let p_ap = (1.0 - self.c_ap) / (1.0 + self.c_ap);
        let period = sr / freq.max(20.0);
        let p_damp = self.s_damp;
        let target = (period - p_damp - p_ap).clamp(2.0, (MAX_DELAY - 4) as f32);
        // First tuning snaps (nothing ringing yet); later live retunes glide via `process`.
        if self.delay_target < 0.0 {
            self.delay_read = target;
        }
        self.delay_target = target;
        // One-pole slew coefficient for a ~RETUNE_GLIDE_MS glide, recomputed from sr.
        self.glide_coef = 1.0 - (-1.0 / (RETUNE_GLIDE_MS * 0.001 * sr).max(1.0)).exp();
        self.feedback = feedback.clamp(0.0, 0.99995);
    }
    #[inline]
    fn process(&mut self, exc: f32) -> f32 {
        // Glide the loop length toward its target (portamento) so a live retune of a ringing
        // string does not snap the delay tap and zip/click the sustaining tail.
        let dt = self.delay_target - self.delay_read;
        if dt != 0.0 {
            self.delay_read += self.glide_coef * dt;
            if (self.delay_target - self.delay_read).abs() < 1.0e-3 {
                self.delay_read = self.delay_target;
            }
        }
        let delayed = self.frac.read(self.delay_read);
        // One-pole (2-tap) damping low-pass in the loop.
        let lp = (1.0 - self.s_damp) * delayed + self.s_damp * self.damp_z;
        self.damp_z = delayed;
        // First-order all-pass fine-tune in the loop.
        let ap = self.c_ap * lp + self.ap_x1 - self.c_ap * self.ap_y1;
        self.ap_x1 = lp;
        self.ap_y1 = ap;
        // Self-limiting feedback (tanh soft-clip keeps the high-feedback loop bounded; a
        // memoryless nonlinearity does not shift the resonant pitch) + continuous excitation.
        let inp = (self.feedback * ap).tanh() + exc;
        self.frac.write(inp);
        let a = delayed.abs();
        self.env = if a > self.env { a } else { self.env * 0.9995 };
        delayed
    }
}

// ---------------------------------------------------------------------------
// Dc blocker (one-pole high-pass at ~5 Hz) for the summed wet output.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct DcBlock {
    x1: f32,
    y1: f32,
    r: f32,
}
impl DcBlock {
    fn new(sr: f32) -> Self {
        // R ≈ 1 - 2π·fc/sr for fc ≈ 5 Hz.
        let r = 1.0 - (2.0 * PI * 5.0 / sr);
        Self { x1: 0.0, y1: 0.0, r: r.clamp(0.9, 0.99999) }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

// ---------------------------------------------------------------------------
// KeyDetect — coarse chromagram over the input (copied from PLUCK).
// ---------------------------------------------------------------------------

struct KeyDetect {
    stft: Stft,
    chroma: [f32; 12],
    sr: f32,
    frames: u32,
}
impl KeyDetect {
    fn new(sr: f32) -> Self {
        Self { stft: Stft::new(4096, 1024), chroma: [0.0; 12], sr, frames: 0 }
    }
    fn reset(&mut self) {
        self.stft.reset();
        self.chroma = [0.0; 12];
        self.frames = 0;
    }
    #[inline]
    fn push(&mut self, x: f32) {
        let Self { stft, chroma, sr, frames } = self;
        let sr = *sr;
        stft.process(x, &mut |spec: &mut [Complex<f32>]| {
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
                let pc = (12.0 * (f / 16.35160).log2()).round() as i32;
                let pc = pc.rem_euclid(12) as usize;
                chroma[pc] += mag;
            }
            *frames = frames.saturating_add(1);
        });
    }
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

/// Nearest [`SpectrumTap`] band index for a frequency (inverse of `band_center_hz`).
#[inline]
fn band_index_for(freq: f32) -> usize {
    let f = freq.clamp(F_LOW, F_HIGH);
    let t = (f / F_LOW).ln() / (F_HIGH / F_LOW).ln();
    ((t * (NUM_BANDS - 1) as f32).round() as i32).clamp(0, NUM_BANDS as i32 - 1) as usize
}

// ---------------------------------------------------------------------------
// ChoraleCore — the whole resonator bank (stereo out, mono/stereo in as exciter).
// ---------------------------------------------------------------------------

pub struct ChoraleCore {
    sr: f32,
    res: Vec<Resonator>,
    key: KeyDetect,
    in_tap: SpectrumTap,   // input band energies for the sympathetic weighting
    bands: [f32; NUM_BANDS],
    weight_clock: usize,
    active: usize,
    dc_l: DcBlock,
    dc_r: DcBlock,
    // Null-preserving output stage smoothers.
    mix_s: f32,
    out_s: f32,
    primed: bool,
    // Published for tests / GUI.
    freqs: [f32; MAX_RESONATORS],
    pan_l: [f32; MAX_RESONATORS],
    pan_r: [f32; MAX_RESONATORS],
}

impl ChoraleCore {
    pub fn new(sr: f32) -> Self {
        let mut res = Vec::with_capacity(MAX_RESONATORS);
        for _ in 0..MAX_RESONATORS {
            res.push(Resonator::new());
        }
        Self {
            sr,
            res,
            key: KeyDetect::new(sr),
            in_tap: SpectrumTap::new(sr),
            bands: [0.0; NUM_BANDS],
            weight_clock: 0,
            active: 16,
            dc_l: DcBlock::new(sr),
            dc_r: DcBlock::new(sr),
            mix_s: 0.5,
            out_s: 1.0,
            primed: false,
            freqs: [220.0; MAX_RESONATORS],
            pan_l: [0.70710677; MAX_RESONATORS],
            pan_r: [0.70710677; MAX_RESONATORS],
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.key = KeyDetect::new(sr);
        self.in_tap.set_sample_rate(sr);
        self.dc_l = DcBlock::new(sr);
        self.dc_r = DcBlock::new(sr);
    }

    pub fn reset(&mut self) {
        for r in self.res.iter_mut() {
            r.reset();
        }
        self.key.reset();
        self.in_tap.reset();
        self.bands = [0.0; NUM_BANDS];
        self.weight_clock = 0;
    }

    pub fn tuned_freqs(&self) -> [f32; MAX_RESONATORS] {
        self.freqs
    }
    pub fn active(&self) -> usize {
        self.active
    }
    /// Per-resonator smoothed |output| envelope (for the GUI activity display).
    pub fn res_env(&self) -> [f32; MAX_RESONATORS] {
        let mut e = [0.0f32; MAX_RESONATORS];
        for i in 0..MAX_RESONATORS {
            e[i] = self.res[i].env;
        }
        e
    }

    /// Map decay (0..1) → per-resonator feedback gain for a target sustain time (PLUCK's map,
    /// with a slightly higher ceiling so long drones sustain).
    fn feedback_for(&self, decay: f32, freq: f32) -> f32 {
        let t = 0.30 * (60.0f32).powf(decay.clamp(0.0, 1.0)); // 0.3 .. 18 s
        let period = self.sr / freq.max(20.0);
        let g = 10.0f32.powf(-3.0 * period / (t * self.sr));
        g.clamp(0.80, 0.99993)
    }

    #[inline]
    fn damp_coeff(damp: f32) -> f32 {
        (0.48 * damp.clamp(0.0, 1.0)).clamp(0.0, 0.49)
    }

    /// Compute the target frequency of resonator `i` from the active tuning source.
    fn compute_freqs(&self, s: &Settings) -> [f32; MAX_RESONATORS] {
        let mut f = [220.0; MAX_RESONATORS];
        // Alternating ± detune spread.
        let spread = |i: usize| -> f32 {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            s.spread_cents * sign
        };
        match s.source {
            TuningSource::Midi if s.held_count > 0 => {
                // Voice-assign held notes low→high; extra resonators octave-stack.
                let mut notes = [0.0f32; MAX_RESONATORS];
                let mut nf = 0usize;
                for &v in s.held.iter() {
                    if v.is_finite() && nf < MAX_RESONATORS {
                        notes[nf] = v;
                        nf += 1;
                    }
                }
                notes[..nf].sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
                let n = nf.max(1);
                for i in 0..MAX_RESONATORS {
                    let base = notes[i % n];
                    let oct = ((i / n) as i32 % SPAN_OCT) as f32;
                    let hz = base * 2.0f32.powf(oct);
                    f[i] = hz * 2.0f32.powf(spread(i) / 1200.0);
                }
            }
            TuningSource::KeyDetect => {
                let (root_pc, is_minor) = match self.key.analyze() {
                    Some((pc, minor, conf)) if conf > 0.12 => (pc, minor),
                    _ => (s.root_pc, matches!(s.scale, Scale::MinorTriad | Scale::Minor7 | Scale::MinorPentatonic | Scale::Phrygian | Scale::Dorian)),
                };
                let scale = if is_minor { Scale::MinorTriad } else { Scale::MajorTriad };
                self.fill_scale(&mut f, root_pc, scale, &spread);
            }
            _ => {
                self.fill_scale(&mut f, s.root_pc, s.scale, &spread);
            }
        }
        f
    }

    fn fill_scale(&self, f: &mut [f32; MAX_RESONATORS], root_pc: i32, scale: Scale, spread: &dyn Fn(usize) -> f32) {
        let offs = scale.offsets();
        let l = offs.len().max(1);
        for i in 0..MAX_RESONATORS {
            let oct = (i / l) as i32 % SPAN_OCT;
            let m = BASE_MIDI + root_pc + offs[i % l] + 12 * oct;
            f[i] = midi_to_freq(m as f32) * 2.0f32.powf(spread(i) / 1200.0);
        }
    }

    /// Recompute tuning + feedback + pans from the current settings (per block).
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.mix_s = s.mix.clamp(0.0, 1.0);
            self.out_s = db_to_lin(s.out_db);
            self.primed = true;
        }
        self.active = s.count.clamp(12, MAX_RESONATORS);
        let freqs = self.compute_freqs(s);
        self.freqs = freqs;
        let s_damp = Self::damp_coeff(s.damp);
        for i in 0..MAX_RESONATORS {
            let fb = self.feedback_for(s.decay, freqs[i]);
            self.res[i].tune(freqs[i], self.sr, s_damp, fb);
        }
        // Stereo alternate pans: even resonators left, odd right, by amount.
        let amt = s.stereo.clamp(0.0, 1.0);
        for i in 0..MAX_RESONATORS {
            let pos = if i % 2 == 0 { -amt } else { amt };
            let theta = (pos * 0.5 + 0.5) * std::f32::consts::FRAC_PI_2;
            self.pan_l[i] = theta.cos();
            self.pan_r[i] = theta.sin();
        }
        self.update_weights(s);
    }

    /// Recompute per-resonator continuous input gains from the last input band energies,
    /// blended with a flat gain by the sympathetic amount.
    fn update_weights(&mut self, s: &Settings) {
        // Normalize the band energies to their max so the weighting is level-independent.
        let mut bmax = 1e-9f32;
        for &b in self.bands.iter() {
            if b > bmax {
                bmax = b;
            }
        }
        let amt = s.sympathetic.clamp(0.0, 1.0);
        // Base excitation gain: modest, since the high-feedback loops accumulate energy.
        let base = 0.006 * s.excite.clamp(0.0, 2.0);
        for i in 0..MAX_RESONATORS {
            let bi = band_index_for(self.freqs[i]);
            let norm = (self.bands[bi] / bmax).clamp(0.0, 1.0);
            let w = (1.0 - amt) + amt * norm;
            self.res[i].in_gain = base * w;
        }
    }

    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        let mono = 0.5 * (l_in + r_in);

        // Input analysis for weighting + key detection.
        self.in_tap.feed(mono);
        if s.source == TuningSource::KeyDetect {
            self.key.push(mono);
        }
        self.weight_clock += 1;
        if self.weight_clock >= WEIGHT_UPDATE {
            self.weight_clock = 0;
            let mut out = [0.0f32; NUM_BANDS];
            let _ = self.in_tap.finish(&mut out);
            self.bands = out;
            self.update_weights(s);
        }

        // Run the active resonators, continuously excited & panned.
        let active = self.active.clamp(1, MAX_RESONATORS);
        let mut wl = 0.0;
        let mut wr = 0.0;
        for i in 0..active {
            let exc = mono * self.res[i].in_gain;
            let y = self.res[i].process(exc);
            wl += y * self.pan_l[i];
            wr += y * self.pan_r[i];
        }
        // Energy-normalize the sum by the active count, with a little headroom.
        let norm = 1.6 / (active as f32).sqrt();
        let wet_l = self.dc_l.process(wl * norm);
        let wet_r = self.dc_r.process(wr * norm);

        // Null-preserving dry/wet mix + output trim (smoothed per sample).
        let mix_t = if s.wet_solo { 1.0 } else { s.mix.clamp(0.0, 1.0) };
        self.mix_s += 0.001 * (mix_t - self.mix_s);
        let out_lin = db_to_lin(s.out_db);
        self.out_s += 0.001 * (out_lin - self.out_s);
        let ol = (l_in * (1.0 - self.mix_s) + wet_l * self.mix_s) * self.out_s;
        let or = (r_in * (1.0 - self.mix_s) + wet_r * self.mix_s) * self.out_s;

        (ol.clamp(-8.0, 8.0), or.clamp(-8.0, 8.0))
    }

    /// Stereo render from a mono input (fed to both channels as the exciter).
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
