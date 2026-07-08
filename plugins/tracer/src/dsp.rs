//! TRACER — pure-DSP core for pitch-tracking multiband saturation (SPECS "TRACER").
//!
//! Signal flow (per SPECS):
//! ```text
//! in ─┬─ mono sum → PitchTracker (MPM, decimated, median/hysteresis/slew) → f0, conf
//!     │
//!     └─ LR4 crossover tree (cutoffs = harmonic multiples of f0, recomputed per
//!          32-sample control block; confidence < 0.6 freezes them)
//!            band0..3: [drive → shaper(bank) → 2x OS → level] → sum → mix → out
//! ```
//! The crossovers are Linkwitz-Riley 4th-order splits built from cascaded 2nd-order
//! Butterworth (Q = 1/√2) sections. They are implemented with the suite's **TPT SVF**,
//! which is topology-preserving and unconditionally stable under per-block cutoff
//! modulation — the property that makes the time-varying, pitch-locked crossover safe
//! (SPECS calls this the hard part). Cutoffs are clamped to `[20, 0.45·Fs]` and a
//! per-channel NaN/blow-up guard resets the filter tree and crossfades back in if
//! automation fuzzing ever pushes a section unstable.
//!
//! API-agnostic pure Rust, shared verbatim between the nih-plug `process` path and the
//! offline harness tests.

use suite_core::dsp::{DelayLine, Oversampler2x, Shaper, Svf};
use suite_core::pitch::PitchTracker;

const MAX_BANDS: usize = 4;
const CTRL_BLOCK: usize = 32;

/// Which waveshaper from the suite bank a band uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Tube,
    Tape,
    Fold,
    Hard,
}

impl ShapeKind {
    pub fn from_index(i: usize) -> ShapeKind {
        match i {
            1 => ShapeKind::Tape,
            2 => ShapeKind::Fold,
            3 => ShapeKind::Hard,
            _ => ShapeKind::Tube,
        }
    }
    pub fn shaper(self) -> Shaper {
        match self {
            ShapeKind::Tube => Shaper::TubeTanh,
            ShapeKind::Tape => Shaper::TapeSoft,
            ShapeKind::Fold => Shaper::SineFold,
            ShapeKind::Hard => Shaper::HardClip,
        }
    }
}

/// Per-crossover cutoff source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XoMode {
    /// Cutoff = base harmonic multiple × f0 × 2^smart_freq (locks to detected pitch).
    Track,
    /// Cutoff = fixed Hz (ignores pitch).
    Fixed,
}

impl XoMode {
    pub fn from_index(i: usize) -> XoMode {
        match i {
            1 => XoMode::Fixed,
            _ => XoMode::Track,
        }
    }
}

/// Pitch source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchMode {
    Detect,
    Midi,
}

impl PitchMode {
    pub fn from_index(i: usize) -> PitchMode {
        match i {
            1 => PitchMode::Midi,
            _ => PitchMode::Detect,
        }
    }
}

/// Base harmonic multipliers for the three crossovers relative to f0 (detents at
/// fundamental·1.5 / body / presence). Band 0 = LP below `f0·1.5`, so it is dominated by
/// the fundamental — the property the done-bar band-1 centroid test relies on.
pub const BASE_MULT: [f32; 3] = [1.5, 4.0, 8.0];

/// A full snapshot of TRACER's controls (plain, un-normalized values).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub pitch_mode: PitchMode,
    /// MIDI note frequency (Hz) when `pitch_mode == Midi`; ignored otherwise.
    pub midi_note_hz: Option<f32>,
    /// Number of active bands (2..=4).
    pub band_count: usize,
    /// Global octave offset applied to every tracked crossover (Smart Frequency knob).
    pub smart_freq_oct: f32,
    pub xo_mode: [XoMode; 3],
    pub xo_fixed_hz: [f32; 3],
    /// Constant-color drive: scale each band's drive by an inverse equal-loudness weight.
    pub const_color: bool,
    /// Input trim (dB, wet path only).
    pub trim_db: f32,
    pub band_drive_db: [f32; MAX_BANDS],
    pub band_shape: [ShapeKind; MAX_BANDS],
    pub band_level_db: [f32; MAX_BANDS],
    /// Pitch slew limit, Hz/ms.
    pub slew_hz_per_ms: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim, dB.
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            pitch_mode: PitchMode::Detect,
            midi_note_hz: None,
            band_count: 3,
            smart_freq_oct: 0.0,
            xo_mode: [XoMode::Track, XoMode::Track, XoMode::Track],
            xo_fixed_hz: [200.0, 1000.0, 4000.0],
            const_color: true,
            trim_db: 0.0,
            band_drive_db: [10.0, 8.0, 6.0, 4.0],
            band_shape: [
                ShapeKind::Tube,
                ShapeKind::Tube,
                ShapeKind::Tape,
                ShapeKind::Tape,
            ],
            band_level_db: [0.0, 0.0, 0.0, 0.0],
            slew_hz_per_ms: 200.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// Merge a per-sample smoothed parameter value with a block-rate NERVE modulation delta,
/// clamped to the param's plain `[min, max]` range. `delta` is `modulated_plain − base_plain`,
/// computed once per block from the listen layer; when no route is live `delta == 0` and the
/// result is exactly `smoothed` (bit-identical to the unmodulated path). Alloc-free.
#[inline]
pub fn apply_mod_delta(smoothed: f32, delta: f32, min: f32, max: f32) -> f32 {
    (smoothed + delta).clamp(min, max)
}

// --- Coarse ISO-226-shaped equal-loudness table (rel. 1 kHz, ~60 phon) --------------
// (freq Hz, extra dB the ear needs to match 1 kHz loudness). Log-freq interpolated.
const ELC_F: [f32; 11] = [
    20.0, 40.0, 63.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
const ELC_DB: [f32; 11] = [
    45.0, 33.0, 24.0, 17.0, 9.0, 2.5, 0.0, -1.5, -3.0, 3.0, 12.0,
];

/// Inverse equal-loudness drive weight at `f_hz`: bands where the ear is less sensitive
/// get proportionally more drive so the added color reads evenly (SPECS constant-color).
/// A coarse color compensation, not a measurement; result clamped to a sane range.
fn color_weight(f_hz: f32) -> f32 {
    let f = f_hz.clamp(ELC_F[0], ELC_F[ELC_F.len() - 1]);
    let mut db = ELC_DB[0];
    for i in 1..ELC_F.len() {
        if f <= ELC_F[i] {
            let t = (f.ln() - ELC_F[i - 1].ln()) / (ELC_F[i].ln() - ELC_F[i - 1].ln());
            db = ELC_DB[i - 1] + t * (ELC_DB[i] - ELC_DB[i - 1]);
            break;
        }
    }
    // Gentle color compensation (0.35 scaling), clamped.
    db_to_lin(0.35 * db).clamp(0.4, 3.0)
}

/// First-order DC blocker (~5 Hz corner at 48 kHz). Odd-symmetric shapers should not
/// create DC from a zero-mean input, but heavy saturation of a near-square/asymmetric
/// bass envelope (plus filter-startup bias) leaks a measurable offset — up to ~-0.06 on
/// hot Reese presets — which eats asymmetric headroom on a bass. This keeps the summed
/// wet path bias-free without touching the harmonic character (−0.1 dB at 40 Hz).
#[derive(Clone, Copy, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
    r: f32,
}

impl DcBlock {
    fn set(&mut self, sample_rate: f32) {
        // ~10 Hz corner: strips saturation DC and sub-audio wander (e.g. the ~0.8 Hz
        // detune beat of a Reese) while costing only ~-0.25 dB at 41 Hz, so the lowest
        // 808/Reese fundamentals stay powerful.
        self.r = 1.0 - (std::f32::consts::TAU * 10.0 / sample_rate.max(1.0));
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// Linkwitz-Riley 4th-order crossover: two cascaded 2nd-order Butterworth LP and HP
/// sections (TPT SVF, Q = 1/√2). `process` returns `(low, high)`.
#[derive(Clone, Copy, Default)]
struct Lr4Crossover {
    lp1: Svf,
    lp2: Svf,
    hp1: Svf,
    hp2: Svf,
}

impl Lr4Crossover {
    fn set(&mut self, fc: f32, sr: f32) {
        let q = std::f32::consts::FRAC_1_SQRT_2; // Butterworth
        self.lp1.set(fc, q, sr);
        self.lp2.set(fc, q, sr);
        self.hp1.set(fc, q, sr);
        self.hp2.set(fc, q, sr);
    }
    fn reset(&mut self) {
        self.lp1.reset();
        self.lp2.reset();
        self.hp1.reset();
        self.hp2.reset();
    }
    #[inline]
    fn process(&mut self, x: f32) -> (f32, f32) {
        let lo = self.lp2.process(self.lp1.process(x).lp).lp;
        let hi = self.hp2.process(self.hp1.process(x).hp).hp;
        (lo, hi)
    }
}

/// Per-channel filter tree + per-band oversamplers.
#[derive(Clone)]
struct Channel {
    xover: [Lr4Crossover; 3],
    band_os: [Oversampler2x; MAX_BANDS],
    /// DC blocker on the summed wet path (removes saturation-injected bias).
    dc: DcBlock,
    /// Instability heal ramp (samples remaining of a fade-in after a reset).
    heal: u32,
}

impl Channel {
    fn new() -> Self {
        Channel {
            xover: [Lr4Crossover::default(); 3],
            band_os: [
                Oversampler2x::new(),
                Oversampler2x::new(),
                Oversampler2x::new(),
                Oversampler2x::new(),
            ],
            dc: DcBlock::default(),
            heal: 0,
        }
    }
    fn reset(&mut self) {
        for x in self.xover.iter_mut() {
            x.reset();
        }
        for o in self.band_os.iter_mut() {
            o.reset();
        }
        // Preserve the configured corner; only clear the filter memory.
        self.dc.reset();
        self.heal = 0;
    }
}

/// Stereo TRACER core (usable mono by passing R = L). Owns the shared pitch tracker, the
/// per-channel LR4 tree, per-band oversamplers, and the control-block cutoff state.
pub struct TracerCore {
    sr: f32,
    ch: [Channel; 2],
    tracker: PitchTracker,
    ctrl_count: usize,
    cutoffs: [f32; 3],
    color: [f32; MAX_BANDS],
    band_centers: [f32; MAX_BANDS],
    configured: bool,
    // Dry-path delay compensation: every band's saturation runs through a 2x oversampler
    // with the same fixed halfband group delay, so the band sum is coherent but delayed
    // relative to the input. The dry path is delayed by that integer amount so partial mix
    // does not comb-filter; the delay is reported to the host as latency.
    dry_delay: [DelayLine; 2],
    latency: usize,
}

impl TracerCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        // All bands share one 2x-oversampler group delay (SR-independent — fixed FIR taps).
        // Use the empirically-measured integer peak lag for exact dry/wet alignment.
        let latency = Oversampler2x::measure_group_delay();
        let mut core = TracerCore {
            sr,
            ch: [Channel::new(), Channel::new()],
            tracker: PitchTracker::new(sr, 110.0),
            ctrl_count: 0,
            cutoffs: [165.0, 440.0, 880.0],
            color: [1.0; MAX_BANDS],
            band_centers: [110.0, 400.0, 1500.0, 6000.0],
            configured: false,
            dry_delay: [DelayLine::new(latency), DelayLine::new(latency)],
            latency,
        };
        core.set_sample_rate(sr);
        core
    }

    /// Reported plugin latency (samples) = the per-band oversampler group delay the dry
    /// path is compensated by. Constant across sample rates.
    pub fn latency_samples(&self) -> u32 {
        self.latency as u32
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        self.tracker = PitchTracker::new(self.sr, 110.0);
        for c in self.ch.iter_mut() {
            c.dc.set(self.sr);
            c.dc.reset();
        }
        self.configured = false;
    }

    pub fn reset(&mut self) {
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.tracker.reset();
        self.ctrl_count = 0;
        self.configured = false;
        for d in self.dry_delay.iter_mut() {
            d.reset();
        }
    }

    /// Latest crossover cutoffs (Hz). Exposed for the freeze done-bar test.
    pub fn cutoffs(&self) -> [f32; 3] {
        self.cutoffs
    }

    /// Current smoothed detected/MIDI pitch (Hz).
    pub fn f0(&self) -> f32 {
        self.tracker.f0()
    }

    /// Current gated pitch confidence (0..1).
    pub fn confidence(&self) -> f32 {
        self.tracker.confidence()
    }

    /// Apply per-block control settings to the tracker (slew, MIDI note).
    pub fn configure(&mut self, s: &Settings) {
        self.tracker.set_slew(s.slew_hz_per_ms);
        let midi = if s.pitch_mode == PitchMode::Midi {
            s.midi_note_hz
        } else {
            None
        };
        self.tracker.set_midi_note(midi);
    }

    /// Recompute the crossover cutoffs, colors and band centers from the current pitch.
    fn recompute(&mut self, s: &Settings) {
        let f0 = self.tracker.f0();
        let shift = 2.0f32.powf(s.smart_freq_oct);
        let nyq_lim = self.sr * 0.45;
        let n_active = s.band_count.clamp(2, MAX_BANDS);
        let n_xover = n_active - 1;

        let mut prev = 20.0f32;
        for i in 0..3 {
            let raw = match s.xo_mode[i] {
                XoMode::Track => BASE_MULT[i] * f0 * shift,
                XoMode::Fixed => s.xo_fixed_hz[i],
            };
            // Clamp + enforce a monotonic, non-degenerate ordering.
            let mut fc = raw.clamp(20.0, nyq_lim);
            if i > 0 {
                fc = fc.max(prev * 1.05).min(nyq_lim);
            }
            self.cutoffs[i] = fc;
            prev = fc;
        }

        // Band centers (geometric) for the color weights.
        for b in 0..MAX_BANDS {
            let center = if b == 0 {
                self.cutoffs[0] * 0.6
            } else if b >= n_active - 1 {
                (self.cutoffs[n_xover - 1] * 1.6).min(nyq_lim)
            } else {
                (self.cutoffs[b - 1] * self.cutoffs[b]).sqrt()
            };
            self.band_centers[b] = center.clamp(20.0, self.sr * 0.5);
            self.color[b] = if s.const_color {
                color_weight(self.band_centers[b])
            } else {
                1.0
            };
        }

        // Push cutoffs into every crossover (state preserved → smooth pitch glide).
        for c in self.ch.iter_mut() {
            for i in 0..n_xover {
                c.xover[i].set(self.cutoffs[i], self.sr);
            }
        }
        self.configured = true;
    }

    /// Process one stereo sample. Call once per sample; cutoffs recompute internally every
    /// [`CTRL_BLOCK`] samples from the pitch tracker.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        // Pitch tracking on the mono sum (updates the per-sample slew).
        let mono = 0.5 * (l_in + r_in);
        self.tracker.push(mono);

        // Control-block cutoff recompute.
        if !self.configured || self.ctrl_count == 0 {
            self.recompute(s);
        }
        self.ctrl_count += 1;
        if self.ctrl_count >= CTRL_BLOCK {
            self.ctrl_count = 0;
        }

        let n_active = s.band_count.clamp(2, MAX_BANDS);
        let trim = db_to_lin(s.trim_db);
        let mix = s.mix.clamp(0.0, 1.0);
        let out_lin = db_to_lin(s.out_db);
        let wet_in = [l_in, r_in];
        // Latency-compensated dry for the mix (kept aligned with the oversampled wet sum).
        let dry = [
            self.dry_delay[0].process(l_in),
            self.dry_delay[1].process(r_in),
        ];
        let mut out = [0.0f32; 2];

        for ci in 0..2 {
            let x = wet_in[ci] * trim;

            // Serial LR4 split into `n_active` bands (band 0 = lowest).
            let mut bands = [0.0f32; MAX_BANDS];
            let mut cur = x;
            for i in 0..(n_active - 1) {
                let (lo, hi) = self.ch[ci].xover[i].process(cur);
                bands[i] = lo;
                cur = hi;
            }
            bands[n_active - 1] = cur;

            // Per-band saturation (2x oversampled) → level → sum.
            let mut wet = 0.0f32;
            for b in 0..n_active {
                let shaper = s.band_shape[b].shaper();
                let drive = (db_to_lin(s.band_drive_db[b]) * self.color[b]).clamp(0.0, 64.0);
                let level = db_to_lin(s.band_level_db[b]);
                let y = self.ch[ci].band_os[b].process(bands[b], |v| shaper.apply(v, drive));
                wet += y * level;
            }

            // Instability guard: reset the tree and crossfade back in on any blow-up. The
            // threshold is a runaway/NaN catch, not a musical ceiling — 16.0 was hair-trigger
            // (four hard-clipped bands with levels toward +12 dB legitimately sum past it,
            // causing spurious resets + 256-sample fade-ins = rhythmic dropout). 64.0 still
            // catches genuine divergence well before the ±8.0 output clamp cannot contain it.
            if !wet.is_finite() || wet.abs() > 64.0 {
                self.ch[ci].reset();
                for i in 0..(n_active - 1) {
                    self.ch[ci].xover[i].set(self.cutoffs[i], self.sr);
                }
                self.ch[ci].heal = 256;
                wet = 0.0;
            }
            if self.ch[ci].heal > 0 {
                // Linear fade-in of the wet path after a reset (no click).
                let g = 1.0 - self.ch[ci].heal as f32 / 256.0;
                wet *= g;
                self.ch[ci].heal -= 1;
            }

            // Strip saturation-injected DC from the summed wet before the blend. Wet-path
            // only, so the mix=0 null against the dry path is untouched.
            wet = self.ch[ci].dc.process(wet);

            let mixed = dry[ci] * (1.0 - mix) + wet * mix;
            out[ci] = (mixed * out_lin).clamp(-8.0, 8.0);
        }

        (out[0], out[1])
    }

    /// Convenience for the mono offline harness: process `main` in place.
    pub fn process_mono(&mut self, main: &mut [f32], s: &Settings) {
        self.configure(s);
        for m in main.iter_mut() {
            let (l, _r) = self.process_sample(*m, *m, s);
            *m = l;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use realfft::RealFftPlanner;
    use suite_core::testsig;

    /// Power-weighted spectral centroid of `buf` restricted to `[f_lo, f_hi]`.
    fn band_centroid(buf: &[f32], sr: f32, f_lo: f32, f_hi: f32) -> f32 {
        let n = buf.len();
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let mut input = buf.to_vec();
        // Hann window to tame leakage.
        for (i, v) in input.iter_mut().enumerate() {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos();
            *v *= w;
        }
        let mut spectrum = fft.make_output_vec();
        fft.process(&mut input, &mut spectrum).unwrap();
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        for (k, c) in spectrum.iter().enumerate() {
            let f = k as f32 * sr / n as f32;
            if f < f_lo || f > f_hi {
                continue;
            }
            let p = c.norm_sqr();
            num += f * p;
            den += p;
        }
        if den > 0.0 {
            num / den
        } else {
            0.0
        }
    }

    #[test]
    fn color_weight_is_unity_at_1k_and_boosts_lows() {
        assert!((color_weight(1000.0) - 1.0).abs() < 0.05);
        assert!(color_weight(50.0) > 1.5, "lows should get more drive");
        assert!(color_weight(4000.0) < 1.0, "ear-sensitive band drives less");
    }

    /// DONE-BAR (1): sliding-saw input, pitch-locked band 1 → band-1 output energy
    /// centroid tracks f0 within ±1 semitone across the slide.
    #[test]
    fn band1_centroid_tracks_sliding_pitch() {
        let sr = 48_000.0f32;
        let f_start = 100.0f32;
        let f_end = 180.0f32;
        let len = (sr * 3.0) as usize;
        let saw = testsig::sliding_saw(f_start, f_end, 0.7, len, sr);

        // Solo band 0 (lowest): mute the others; keep drive low so the band-1 output is
        // essentially the filtered fundamental region.
        let mut s = Settings::default();
        s.band_count = 3;
        s.const_color = false;
        s.band_drive_db = [0.0, 0.0, 0.0, 0.0];
        s.band_level_db = [0.0, -120.0, -120.0, -120.0];
        s.mix = 1.0;
        s.slew_hz_per_ms = 300.0;

        let mut core = TracerCore::new(sr);
        let mut out = saw.clone();
        core.process_mono(&mut out, &s);

        // Measure the band-1 output centroid in windows along the slide (skip the warmup
        // while the tracker locks on) and compare to the known instantaneous f0.
        let win = 8192usize;
        let mut checked = 0;
        let mut n = (sr * 0.6) as usize;
        while n + win < len {
            let seg = &out[n..n + win];
            let centroid = band_centroid(seg, sr, 40.0, 400.0);
            let center = n + win / 2;
            let f0 = testsig::sliding_saw_f0(f_start, f_end, center, len);
            let semis = 12.0 * (centroid / f0).log2();
            assert!(
                semis.abs() <= 1.0,
                "band-1 centroid {centroid:.1} Hz vs f0 {f0:.1} Hz = {semis:.2} semitones"
            );
            checked += 1;
            n += win;
        }
        assert!(checked >= 3, "not enough windows checked ({checked})");
    }

    /// DONE-BAR (2): white-noise input (confidence collapses) → crossover frequencies
    /// frozen (unchanged over 1 s).
    #[test]
    fn crossovers_freeze_on_white_noise() {
        let sr = 48_000.0f32;
        let len = sr as usize; // 1 s
        let noise = testsig::white_noise(0.8, len, 4242);

        let s = Settings::default();
        let mut core = TracerCore::new(sr);
        core.configure(&s);

        let mut first: Option<[f32; 3]> = None;
        let mut max_dev = 0.0f32;
        for &x in &noise {
            let _ = core.process_sample(x, x, &s);
            let c = core.cutoffs();
            match first {
                None => first = Some(c),
                Some(f) => {
                    for i in 0..3 {
                        max_dev = max_dev.max((c[i] - f[i]).abs());
                    }
                }
            }
        }
        assert!(core.confidence() < 0.6, "noise should not be confident");
        assert!(
            max_dev < 0.5,
            "crossovers drifted {max_dev:.3} Hz on noise (freeze failed)"
        );
    }

    #[test]
    fn mix_zero_nulls_against_latency_matched_dry() {
        let sr = 48_000.0f32;
        let n = 24_000usize;
        let main: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 120.0 * i as f32 / sr).sin())
            .collect();
        let mut s = Settings::default();
        s.mix = 0.0;
        s.out_db = 0.0;
        let mut core = TracerCore::new(sr);
        let lat = core.latency_samples() as usize;
        let mut out = main.clone();
        core.process_mono(&mut out, &s);
        // At mix=0 the output is the dry path delayed by the reported latency.
        let m = n - lat;
        let mse = (0..m)
            .map(|i| {
                let d = main[i] - out[i + lat];
                d * d
            })
            .sum::<f32>()
            / m as f32;
        let resid = 20.0 * mse.sqrt().max(1.0e-12).log10();
        assert!(resid < -80.0, "mix=0 did not null: residual {resid:.1} dB");
    }

    /// Regression (HARD CHECKPOINT 1): at mix=0.5 the delayed dry and the oversampled wet
    /// sum must form a SINGLE coherent impulse peak — all bands share one group delay, so
    /// with dry-path compensation there is exactly one cluster, not two peaks.
    #[test]
    fn partial_mix_impulse_is_single_coherent_peak() {
        use suite_core::harness::assert_single_coherent_peak;
        let sr = 48_000.0f32;
        let n = 256usize;
        let mut s = Settings::default();
        s.mix = 0.5;
        s.const_color = false;
        s.band_drive_db = [0.0, 0.0, 0.0, 0.0]; // ~unity wet
        s.band_level_db = [0.0, 0.0, 0.0, 0.0];
        s.xo_mode = [XoMode::Fixed, XoMode::Fixed, XoMode::Fixed];
        s.xo_fixed_hz = [200.0, 1000.0, 4000.0];
        let mut main = vec![0.0f32; n];
        main[0] = 1.0;
        let mut core = TracerCore::new(sr);
        core.process_mono(&mut main, &s);
        assert_single_coherent_peak(&main, 2, 0.5);
    }

    #[test]
    fn extreme_params_stay_finite_and_bounded() {
        // Fuzz-like: max drive on all bands, hard shaper, tiny/huge fixed cutoffs.
        let sr = 48_000.0f32;
        let x = testsig::white_noise(0.95, 20_000, 1);
        let mut s = Settings::default();
        s.band_count = 4;
        s.xo_mode = [XoMode::Fixed, XoMode::Fixed, XoMode::Fixed];
        s.xo_fixed_hz = [19.0, 21.0, 30_000.0];
        s.band_drive_db = [48.0, 48.0, 48.0, 48.0];
        s.band_shape = [ShapeKind::Hard; 4];
        s.trim_db = 24.0;
        let mut core = TracerCore::new(sr);
        let mut out = x.clone();
        core.process_mono(&mut out, &s);
        assert!(out.iter().all(|v| v.is_finite()));
        let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        // Clamp policy (TRIAGE 2026-07-08): final clamp is a ±8.0 runaway/NaN guard
        // (≈ +18 dBFS), not a 0 dBFS ceiling — extreme fuzz asserts finite && ≤ the guard.
        assert!(peak <= 8.001, "peak {peak} exceeded the +18 dBFS safety guard");
    }

    /// Mean (DC) of a buffer.
    fn dc_mean(x: &[f32]) -> f32 {
        x.iter().copied().sum::<f32>() / x.len().max(1) as f32
    }

    /// First-difference outlier "click" count vs a local (50 ms) RMS floor — a port of
    /// tools/audition.py::detect_clicks so the zipper regression matches the sound-pass metric.
    fn click_count(x: &[f32], sr: f32, thresh_ratio: f32) -> usize {
        if x.len() < 8 {
            return 0;
        }
        let d: Vec<f32> = x.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        let win = ((50.0e-3 * sr) as usize).max(8);
        // Boxcar RMS of the difference.
        let mut count = 0;
        let edge = (20.0e-3 * sr) as usize;
        for i in edge..d.len().saturating_sub(edge) {
            let lo = i.saturating_sub(win / 2);
            let hi = (i + win / 2).min(d.len());
            let ms = d[lo..hi].iter().map(|&v| v * v).sum::<f32>() / (hi - lo) as f32;
            let local_rms = ms.sqrt() + 1e-9;
            if d[i] / local_rms > thresh_ratio {
                count += 1;
            }
        }
        count
    }

    /// REGRESSION (DC-blocker): heavy odd-saturation of a bass leaks DC without the wet-path
    /// blocker — hot Reese presets measured up to ~-0.06. Assert the summed wet stays
    /// bias-free (well under the audition DC flag threshold of 1e-3) on a hot config.
    #[test]
    fn hot_saturation_does_not_inject_dc() {
        let sr = 48_000.0f32;
        let reese = testsig::synth_reese(55.0, 3.0, sr);
        // A deliberately hot, asymmetric-leaning config (mirrors "Reese Tracker": high mid
        // drive, Fold band) that produced the worst DC before the blocker.
        let mut s = Settings::default();
        s.band_count = 3;
        s.band_drive_db = [12.0, 15.0, 8.0, 4.0];
        s.band_shape = [ShapeKind::Tube, ShapeKind::Fold, ShapeKind::Tape, ShapeKind::Tape];
        s.mix = 1.0;
        let mut core = TracerCore::new(sr);
        let mut out = reese.clone();
        core.process_mono(&mut out, &s);
        // Skip the DC blocker's own settle (its ~5 Hz corner needs a few hundred ms).
        let tail = &out[sr as usize..];
        let dc = dc_mean(tail).abs();
        assert!(dc < 1.0e-3, "wet path leaked DC {dc:.5} (blocker not working)");
    }

    /// REGRESSION (no zipper on a glide): the crossover cutoffs step every 32-sample control
    /// block as the tracked pitch glides. On a SMOOTH gliding sine (no source reset edges to
    /// confound the detector) the moving LR4 tree must not emit discontinuities — assert the
    /// click count stays negligible. A raw saw is unusable here (its per-cycle reset is itself
    /// a discontinuity); the sine isolates crossover-modulation artifacts.
    #[test]
    fn gliding_pitch_no_crossover_zipper() {
        let sr = 48_000.0f32;
        let len = (sr * 3.0) as usize;
        // Smooth sine gliding 60 → 150 Hz (exponential), amp 0.7.
        let mut phase = 0.0f32;
        let sine: Vec<f32> = (0..len)
            .map(|n| {
                let f = testsig::sliding_saw_f0(60.0, 150.0, n, len);
                phase += f / sr;
                if phase >= 1.0 {
                    phase -= phase.floor();
                }
                0.7 * (std::f32::consts::TAU * phase).sin()
            })
            .collect();

        // Tracking crossovers, fast slew so the cutoffs actually move each block; moderate
        // drive so any coefficient-step zipper would surface as an output discontinuity.
        let mut s = Settings::default();
        s.band_count = 4;
        s.slew_hz_per_ms = 800.0;
        s.band_drive_db = [10.0, 10.0, 8.0, 6.0];
        s.mix = 1.0;
        let mut core = TracerCore::new(sr);
        let mut out = sine.clone();
        core.process_mono(&mut out, &s);

        assert!(out.iter().all(|v| v.is_finite()));
        // The dry sine glide itself yields 0 clicks; allow a tiny margin for the harmonic
        // richening from saturation. Real zipper would spike this into the hundreds (one per
        // control block ≈ thousands over 3 s).
        let clicks = click_count(&out, sr, 8.0);
        assert!(clicks <= 4, "gliding-pitch crossover zipper: {clicks} click(s) detected");
    }

    #[test]
    fn hot_multiband_does_not_trip_instability_guard() {
        // Four hard-clipped bands with levels toward +12 dB legitimately sum past the OLD guard
        // threshold (16.0) on broadband input; that spuriously reset the filter tree and faded
        // it back in (rhythmic dropout). With the guard raised to 64.0 the signal passes
        // (bounded by the ±8 output clamp) instead of being repeatedly nuked toward silence.
        let sr = 48_000.0f32;
        let x = testsig::white_noise(0.95, 60_000, 7);
        let mut s = Settings::default();
        s.band_count = 4;
        s.xo_mode = [XoMode::Fixed, XoMode::Fixed, XoMode::Fixed];
        s.xo_fixed_hz = [200.0, 1000.0, 4000.0];
        s.const_color = false;
        s.band_drive_db = [48.0, 48.0, 48.0, 48.0];
        s.band_shape = [ShapeKind::Hard; 4];
        s.band_level_db = [12.0, 12.0, 12.0, 12.0];
        s.mix = 1.0;

        let mut core = TracerCore::new(sr);
        let mut out = x.clone();
        core.process_mono(&mut out, &s);

        let seg = &out[8_000..];
        assert!(seg.iter().all(|v| v.is_finite()), "produced NaN/inf");
        let peak = seg.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak <= 8.001, "peak {peak} exceeded the ±8 safety clamp");
        // If the guard were still tripping on this reachable config the output would be
        // repeatedly reset + faded, dragging its RMS far down. A healthy hot signal keeps it up.
        let rms = (seg.iter().map(|&v| v * v).sum::<f32>() / seg.len() as f32).sqrt();
        assert!(
            rms > 1.0,
            "hot multiband RMS collapsed to {rms:.3} — instability guard likely still tripping"
        );
    }
}
