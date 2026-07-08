//! Generated-in-code test signals (PRD §4). No external audio files, ever.
//!
//! All generators return `Vec<f32>` mono at [`crate::TEST_SR`] unless a sample rate
//! is passed explicitly.

use std::f32::consts::PI;

/// Tiny deterministic PRNG (xorshift32) so noise renders are reproducible.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform float in [-1, 1).
    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Unit impulse: a single 1.0 at sample 0, silence after.
pub fn impulse(len: usize) -> Vec<f32> {
    let mut v = vec![0.0; len.max(1)];
    v[0] = 1.0;
    v
}

/// Sine at `freq` Hz, `amp` linear, `len` samples.
pub fn sine(freq: f32, amp: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    (0..len)
        .map(|n| amp * (2.0 * PI * freq * n as f32 / sample_rate).sin())
        .collect()
}

/// Logarithmic (exponential) sine sweep from `f0` to `f1` Hz.
pub fn log_chirp(f0: f32, f1: f32, amp: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    let n = len.max(1) as f32;
    let t_total = n / sample_rate;
    let k = (f1 / f0).ln();
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate;
            // Instantaneous-phase integral of an exponential frequency sweep.
            let phase = 2.0 * PI * f0 * t_total / k * ((k * t / t_total).exp() - 1.0);
            amp * phase.sin()
        })
        .collect()
}

/// White-noise burst (uniform), deterministic for a given seed.
pub fn white_noise(amp: f32, len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..len).map(|_| amp * rng.next_bipolar()).collect()
}

/// Pink-noise burst via Paul Kellet's economical filter, normalized to `amp`.
pub fn pink_noise(amp: f32, len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) =
        (0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    (0..len)
        .map(|_| {
            let white = rng.next_bipolar();
            b0 = 0.99886 * b0 + white * 0.0555179;
            b1 = 0.99332 * b1 + white * 0.0750759;
            b2 = 0.96900 * b2 + white * 0.1538520;
            b3 = 0.86650 * b3 + white * 0.3104856;
            b4 = 0.55000 * b4 + white * 0.5329522;
            b5 = -0.7616 * b5 - white * 0.0168980;
            let pink = b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362;
            b6 = white * 0.115926;
            amp * pink * 0.11
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Stubs required by later plugins (IMPACT, SEANCE, etc.). Real synthesis lands
// with the owning plugin; these keep the harness API stable and are intentionally
// simple placeholders (PRD §4).
// ---------------------------------------------------------------------------

/// Parameters for [`synth_kick`] — IMPACT's own kick math, exposed as a reusable synthetic
/// signal for the rest of the suite (UNDERTOW's kick-duck test, SEANCE, etc.).
#[derive(Clone, Copy, Debug)]
pub struct KickSpec {
    /// Pitch-envelope start frequency (Hz).
    pub f_start: f32,
    /// Pitch-envelope end / body frequency (Hz).
    pub f_end: f32,
    /// Pitch-envelope time constant τ_p (seconds).
    pub pitch_decay_s: f32,
    /// Amp-envelope time constant τ_a (seconds).
    pub amp_decay_s: f32,
    /// Band-passed noise click amount, 0..1.
    pub click: f32,
    /// Sub-oscillator level, 0..1 (sine at `f_end × sub_ratio`).
    pub sub_level: f32,
    /// Sub-oscillator frequency ratio of `f_end`.
    pub sub_ratio: f32,
    /// Pre-envelope drive into a `tanh` saturator, 0..1.
    pub drive: f32,
}

impl Default for KickSpec {
    /// A general-purpose kick: 180→55 Hz sweep, ~0.5 s tail, light click.
    fn default() -> Self {
        Self {
            f_start: 180.0,
            f_end: 55.0,
            pitch_decay_s: 0.03,
            amp_decay_s: 0.5,
            click: 0.2,
            sub_level: 0.0,
            sub_ratio: 0.5,
            drive: 0.0,
        }
    }
}

/// Synthetic kick using IMPACT's own signal path (PRD §4): exponential pitch envelope into a
/// phase-continuous sine body, a band-passed white-noise click, a sub oscillator, `tanh` drive
/// pre-envelope, an exponential amp envelope with a 1.5 ms attack (declick), and a soft clip.
/// Deterministic. Peak-bounded below 0 dBFS.
pub fn synth_kick(spec: &KickSpec, len: usize, sample_rate: f32) -> Vec<f32> {
    let sr = sample_rate.max(1.0);
    let dt = 1.0 / sr;
    let mut phase = 0.0f32;
    let mut sub_phase = 0.0f32;
    let mut click_svf = crate::dsp::Svf::new();
    click_svf.set(3500.0, 2.0, sr);
    let mut click_env = 1.0f32;
    let click_coef = (-1.0 / (0.012 * sr)).exp(); // ~12 ms click decay
    let mut rng = Rng::new(0x51AC_2E17);
    let attack_len = ((0.0015 * sr).round() as usize).max(1);
    let tau_p = spec.pitch_decay_s.max(1.0e-5);
    let tau_a = spec.amp_decay_s.max(1.0e-4);
    let pregain = 1.0 + spec.drive.clamp(0.0, 1.0) * 11.0;

    (0..len)
        .map(|n| {
            let t = n as f32 / sr;
            // Pitch envelope → phase-continuous body sine.
            let f = spec.f_end + (spec.f_start - spec.f_end) * (-t / tau_p).exp();
            phase += f / sr;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            let body = (2.0 * PI * phase).sin();
            // Sub oscillator.
            let sub = if spec.sub_level > 0.0 {
                sub_phase += (spec.f_end * spec.sub_ratio) / sr;
                if sub_phase >= 1.0 {
                    sub_phase -= sub_phase.floor();
                }
                (2.0 * PI * sub_phase).sin() * spec.sub_level
            } else {
                0.0
            };
            // Band-passed noise click.
            let click = if spec.click > 0.0 {
                let bp = click_svf.process(rng.next_bipolar()).bp;
                let c = bp * click_env * spec.click;
                click_env *= click_coef;
                c
            } else {
                0.0
            };
            // Mix → drive (pre-envelope) → amp env → soft clip.
            let driven = (pregain * (body + sub + click)).tanh();
            let amp = if n < attack_len {
                n as f32 / attack_len as f32
            } else {
                let ta = (n - attack_len) as f32 * dt;
                (-ta / tau_a).exp()
            };
            crate::dsp::tape_soft(driven * amp).clamp(-0.999, 0.999)
        })
        .collect()
}

/// Synthetic kick with the default [`KickSpec`]. Kept for callers that want a one-shot kick
/// without configuring a spec (formerly a decaying-sine stub; now IMPACT's real math).
pub fn synth_kick_stub(len: usize, sample_rate: f32) -> Vec<f32> {
    synth_kick(&KickSpec::default(), len, sample_rate)
}

/// Synthetic vocal (PRD §4; SEANCE reuses this): a sawtooth glottal source with a 5 Hz
/// vibrato passed through three formant band-passes (an `/a/`-like vowel: F1≈700,
/// F2≈1220, F3≈2600 Hz). Deterministic, peak-normalized below 0 dBFS.
pub fn synth_vocal(freq: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    let sr = sample_rate.max(1.0);
    let f = freq.clamp(20.0, sr * 0.25);
    // Three formant band-passes (center Hz, Q, linear gain).
    let formants = [(700.0f32, 6.0f32, 1.0f32), (1220.0, 8.0, 0.55), (2600.0, 10.0, 0.28)];
    let mut bp = [crate::dsp::Svf::new(), crate::dsp::Svf::new(), crate::dsp::Svf::new()];
    for (i, &(fc, q, _)) in formants.iter().enumerate() {
        bp[i].set(fc.min(sr * 0.45), q, sr);
    }
    let mut phase = 0.0f32;
    let mut out: Vec<f32> = Vec::with_capacity(len);
    let mut peak = 1.0e-6f32;
    for n in 0..len {
        let t = n as f32 / sr;
        // 5 Hz vibrato, ±1% depth, on the fundamental.
        let vib = 1.0 + 0.01 * (2.0 * PI * 5.0 * t).sin();
        phase += f * vib / sr;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
        let saw = 2.0 * phase - 1.0;
        let mut y = 0.0f32;
        for (i, &(_, _, g)) in formants.iter().enumerate() {
            y += bp[i].process(saw).bp * g;
        }
        peak = peak.max(y.abs());
        out.push(y);
    }
    // Peak-normalize to 0.7 so downstream stages have headroom.
    let norm = 0.7 / peak;
    for v in out.iter_mut() {
        *v *= norm;
    }
    out
}

/// STUB shim kept for API stability: delegates to [`synth_vocal`].
pub fn synth_vocal_stub(freq: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    synth_vocal(freq, len, sample_rate)
}

/// Sliding-pitch sawtooth (808 / glide stand-in, PRD §4): an exponential glissando from
/// `f_start` to `f_end` over the whole buffer. Use [`sliding_saw_f0`] to get the exact
/// instantaneous fundamental at any sample for measurement.
pub fn sliding_saw(f_start: f32, f_end: f32, amp: f32, len: usize, sample_rate: f32) -> Vec<f32> {
    let sr = sample_rate.max(1.0);
    let mut phase = 0.0f32;
    (0..len)
        .map(|n| {
            let f = sliding_saw_f0(f_start, f_end, n, len);
            phase += f / sr;
            if phase >= 1.0 {
                phase -= phase.floor();
            }
            amp * (2.0 * phase - 1.0)
        })
        .collect()
}

/// Instantaneous fundamental (Hz) of [`sliding_saw`] at sample `n` of `len` (exponential
/// glide: `f(t) = f_start · (f_end/f_start)^(n/len)`).
#[inline]
pub fn sliding_saw_f0(f_start: f32, f_end: f32, n: usize, len: usize) -> f32 {
    let denom = (len.max(1)) as f32;
    let frac = (n as f32 / denom).clamp(0.0, 1.0);
    f_start * (f_end / f_start).powf(frac)
}

// ---------------------------------------------------------------------------
// Musical audition sources (SOUND-PASS infra). Deterministic, seeded, 48 kHz by
// default. Alloc-into-Vec at test time is fine — these run offline only, never on
// the audio thread. They exist so per-plugin sound-quality agents can render every
// factory preset over genre-appropriate material (dark techno + atmospheric dnb)
// and judge "would a producer keep this in a real song?" with tools/audition.py.
// ---------------------------------------------------------------------------

/// Naive (aliasing) sawtooth helper — one detuned voice. Phase in/out by reference so
/// callers can run several partials in lockstep. Returns the sample at the current
/// phase and advances it.
#[inline]
fn saw_tick(phase: &mut f32, freq: f32, sr: f32) -> f32 {
    *phase += freq / sr;
    if *phase >= 1.0 {
        *phase -= phase.floor();
    }
    2.0 * *phase - 1.0
}

/// Peak-normalize a buffer in place to `target` linear (≤ 0 dBFS). No-op if silent.
fn peak_normalize(buf: &mut [f32], target: f32) {
    let peak = buf.iter().fold(1.0e-9f32, |a, &v| a.max(v.abs()));
    let g = target / peak;
    for v in buf.iter_mut() {
        *v *= g;
    }
}

/// Four-on-the-floor techno kick loop: the suite's default dark-techno driver.
/// A kick on every quarter note, 55 Hz fundamental, with a slight deterministic
/// per-hit level variation so it reads as a performance, not a copy-paste. Peak
/// bounded below 0 dBFS. `bars` bars of 4/4 at `bpm`.
pub fn synth_kick_loop(bpm: f32, bars: usize, sr: f32) -> Vec<f32> {
    let sr = sr.max(1.0);
    let bpm = bpm.clamp(40.0, 300.0);
    let beat_samples = (60.0 / bpm * sr).round() as usize;
    let beats = 4 * bars.max(1);
    let len = beat_samples * beats;
    let mut out = vec![0.0f32; len.max(1)];
    // One kick one-shot, reused per hit. Tight body so hits stay separated.
    let spec = KickSpec {
        f_start: 140.0,
        f_end: 55.0,
        pitch_decay_s: 0.03,
        amp_decay_s: 0.34,
        click: 0.25,
        sub_level: 0.3,
        sub_ratio: 0.5,
        drive: 0.1,
    };
    let kick_len = (0.45 * sr) as usize;
    let kick = synth_kick(&spec, kick_len, sr);
    let mut rng = Rng::new(0x4F00_2B17);
    for b in 0..beats {
        // slight level variation 0.82..1.0, accent the downbeat of each bar
        let mut lvl = 0.82 + 0.18 * (rng.next_u32() as f32 / u32::MAX as f32);
        if b % 4 == 0 {
            lvl = 1.0;
        }
        let start = b * beat_samples;
        for (i, &k) in kick.iter().enumerate() {
            let idx = start + i;
            if idx >= out.len() {
                break;
            }
            out[idx] += k * lvl;
        }
    }
    peak_normalize(&mut out, 0.9);
    out
}

/// Detuned dual-saw "reese" bass bed (atmospheric dnb). Two saws a fixed beat apart
/// (`±0.4 Hz`, ~0.8 Hz beating) summed and run through a gentle 2-pole low-pass at
/// ~1.2 kHz so the energy sits low and thick. Deterministic; peak below 0 dBFS.
pub fn synth_reese(f0: f32, seconds: f32, sr: f32) -> Vec<f32> {
    let sr = sr.max(1.0);
    let f0 = f0.clamp(20.0, sr * 0.2);
    let len = (seconds.max(0.0) * sr) as usize;
    let mut lp1 = crate::dsp::Svf::new();
    let mut lp2 = crate::dsp::Svf::new();
    lp1.set(1200.0, 0.707, sr);
    lp2.set(1200.0, 0.707, sr);
    let (mut p1, mut p2) = (0.0f32, 0.37f32);
    let mut out: Vec<f32> = Vec::with_capacity(len.max(1));
    for _ in 0..len {
        let a = saw_tick(&mut p1, f0 - 0.4, sr);
        let b = saw_tick(&mut p2, f0 + 0.4, sr);
        // gentle low-pass (cascaded 2-pole -> ~ -24 dB/oct, still musical) keeps the
        // reese energy concentrated well below 1.5 kHz.
        let y = lp2.process(lp1.process(0.5 * (a + b)).lp).lp;
        out.push(y);
    }
    peak_normalize(&mut out, 0.85);
    out
}

/// Synthesized amen-ish breakbeat (atmospheric dnb / breakcore). A 16-step-per-bar
/// pattern of the kick synth + a snare burst (200 Hz body + band-passed noise) +
/// closed-hat ticks (short high-passed noise). Doesn't need to sound human — it needs
/// transients and gaps for transient/gate/chop plugins. Deterministic; ≤ 0 dBFS.
pub fn synth_break(bpm: f32, bars: usize, sr: f32) -> Vec<f32> {
    let sr = sr.max(1.0);
    let bpm = bpm.clamp(40.0, 300.0);
    let beat_samples = (60.0 / bpm * sr).round() as usize;
    let step = beat_samples / 4; // 16th note
    let bars = bars.max(1);
    let len = beat_samples * 4 * bars;
    let mut out = vec![0.0f32; len.max(1)];

    // --- one-shots ---
    let kick = synth_kick(
        &KickSpec { f_start: 150.0, f_end: 60.0, amp_decay_s: 0.18, click: 0.3, drive: 0.15, ..KickSpec::default() },
        (0.25 * sr) as usize,
        sr,
    );
    let snare = synth_snare(sr);
    let hat = synth_hat(sr);

    // --- 16-step pattern (per bar), amen-ish placement ---
    let kick_steps = [0usize, 6, 10];
    let snare_steps = [4usize, 12]; // backbeats
    let ghost_steps = [7usize, 14]; // ghost snares
    let hat_steps: [usize; 8] = [0, 2, 4, 6, 8, 10, 12, 14]; // 8th-note hats

    let mut rng = Rng::new(0x00B7_EA71);
    let place = |out: &mut Vec<f32>, oneshot: &[f32], step_idx: usize, lvl: f32| {
        let start = step_idx * step;
        for (i, &s) in oneshot.iter().enumerate() {
            let idx = start + i;
            if idx >= out.len() {
                break;
            }
            out[idx] += s * lvl;
        }
    };
    for bar in 0..bars {
        let base = bar * 16;
        for &s in &kick_steps {
            place(&mut out, &kick, base + s, 1.0);
        }
        for &s in &snare_steps {
            place(&mut out, &snare, base + s, 0.9);
        }
        for &s in &ghost_steps {
            place(&mut out, &snare, base + s, 0.35);
        }
        for &s in &hat_steps {
            let lvl = 0.4 + 0.25 * (rng.next_u32() as f32 / u32::MAX as f32);
            place(&mut out, &hat, base + s, lvl);
        }
    }
    peak_normalize(&mut out, 0.9);
    out
}

/// Snare one-shot: 200 Hz decaying-sine body + a band-passed white-noise burst.
fn synth_snare(sr: f32) -> Vec<f32> {
    let len = (0.14 * sr) as usize;
    let mut bp = crate::dsp::Svf::new();
    bp.set(1800.0, 1.2, sr);
    let mut rng = Rng::new(0x5A17_9E33);
    let body_tau = 0.06 * sr;
    let noise_tau = 0.09 * sr;
    let mut phase = 0.0f32;
    let mut out: Vec<f32> = Vec::with_capacity(len);
    for n in 0..len {
        phase += 200.0 / sr;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
        let body = (2.0 * PI * phase).sin() * (-(n as f32) / body_tau).exp() * 0.6;
        let noise = bp.process(rng.next_bipolar()).bp * (-(n as f32) / noise_tau).exp();
        out.push(body + noise);
    }
    peak_normalize(&mut out, 0.9);
    out
}

/// Closed-hat one-shot: short high-passed white noise, fast decay.
fn synth_hat(sr: f32) -> Vec<f32> {
    let len = (0.05 * sr) as usize;
    let mut hp = crate::dsp::Svf::new();
    hp.set(7000.0, 0.9, sr);
    let mut rng = Rng::new(0x11CE_77A5);
    let tau = 0.012 * sr;
    let mut out: Vec<f32> = Vec::with_capacity(len);
    for n in 0..len {
        let h = hp.process(rng.next_bipolar()).hp * (-(n as f32) / tau).exp();
        out.push(h);
    }
    peak_normalize(&mut out, 0.8);
    out
}

/// Sustained detuned-saw minor-triad pad through a slow low-pass sweep (reverb /
/// texture fodder). Root + minor third + fifth, each a detuned saw pair; the summed
/// bank runs through one slow LP sweep (~300 Hz→~2.5 kHz and back over the buffer).
/// Normalized to roughly -18 dBFS RMS with a low crest factor (dense, sustained).
/// Deterministic; peak below 0 dBFS.
pub fn synth_pad(root_hz: f32, seconds: f32, sr: f32) -> Vec<f32> {
    let sr = sr.max(1.0);
    let root = root_hz.clamp(30.0, sr * 0.1);
    let len = (seconds.max(0.0) * sr) as usize;
    // minor triad: root, +3 semitones, +7 semitones
    let notes = [root, root * 2.0f32.powf(3.0 / 12.0), root * 2.0f32.powf(7.0 / 12.0)];
    // detuned saw pair per note (±0.5 Hz), staggered start phases for density
    let mut phases = [0.0f32, 0.11, 0.23, 0.41, 0.57, 0.79];
    let freqs = [
        notes[0] - 0.5, notes[0] + 0.5,
        notes[1] - 0.5, notes[1] + 0.5,
        notes[2] - 0.5, notes[2] + 0.5,
    ];
    let mut lp = crate::dsp::Svf::new();
    let mut out: Vec<f32> = Vec::with_capacity(len.max(1));
    let sweep_hz = if seconds > 0.0 { 0.5 / seconds.max(0.1) } else { 0.1 }; // ~half a cycle over the buffer
    for n in 0..len {
        let t = n as f32 / sr;
        // slow LP sweep 300 Hz .. 2500 Hz
        let lfo = 0.5 * (1.0 - (2.0 * PI * sweep_hz * t).cos());
        let cutoff = 300.0 + 2200.0 * lfo;
        lp.set(cutoff, 0.8, sr);
        let mut s = 0.0f32;
        for (i, &f) in freqs.iter().enumerate() {
            s += saw_tick(&mut phases[i], f, sr);
        }
        s /= freqs.len() as f32;
        out.push(lp.process(s).lp);
    }
    // normalize to ~ -18 dBFS RMS, then guard the peak below 0 dBFS.
    let rms = (out.iter().map(|v| v * v).sum::<f32>() / out.len().max(1) as f32).sqrt();
    if rms > 1.0e-9 {
        let g = 0.1259 / rms; // -18 dBFS
        for v in out.iter_mut() {
            *v *= g;
        }
    }
    let peak = out.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
    if peak > 0.9 {
        let g = 0.9 / peak;
        for v in out.iter_mut() {
            *v *= g;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Fake transport (PRD §4: "fake-transport struct (tempo, playhead, bar pos) for
// ASCEND/CLEAVE/HALT"). Promoted from ASCEND's crate-local pattern into the shared
// harness so any transport-locked plugin (CLEAVE now, HALT later) can be driven
// against a synthetic, sample-accurate 4/4 playhead offline. ASCEND keeps its own
// crate-local copy (it could migrate at PEDAL-UI time; do not touch its crate here).
// ---------------------------------------------------------------------------

/// A synthetic per-block transport snapshot — the shared analogue of `nih_plug`'s
/// `Transport`, kept as a plain data struct so pure-DSP cores stay free of any host
/// type. The plugin fills one from `nih_plug` each block; the offline harness fills
/// one from a [`FakeTransport`]. All positions are 0-based and fractional.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportFrame {
    /// Whether the host transport is rolling.
    pub playing: bool,
    /// Tempo in beats per minute.
    pub tempo: f64,
    /// Song position in quarter-note beats (== nih_plug `pos_beats`).
    pub ppq_pos: f64,
    /// Song position in bars (fractional). For 4/4 this is `ppq_pos / 4`.
    pub bar_pos: f64,
    /// Bars advanced per output sample (tempo · time-signature derived). Multiply by a
    /// sample count to advance the playhead within a block.
    pub bars_per_sample: f64,
    /// Beats (quarter notes) per bar — 4.0 for 4/4.
    pub beats_per_bar: f64,
}

impl TransportFrame {
    /// Beats (quarter notes) advanced per output sample.
    #[inline]
    pub fn beats_per_sample(&self) -> f64 {
        self.bars_per_sample * self.beats_per_bar
    }
    /// A stopped transport at bar 0 (useful as a default / free-run marker).
    pub fn stopped(tempo: f64, sample_rate: f64) -> Self {
        FakeTransport::new(sample_rate, tempo).frame_stopped()
    }
}

/// A sample-accurate synthetic transport driver for offline tests. Models a steady
/// 4/4 (configurable time signature) playhead at a fixed BPM; advance it by whole
/// sample counts and snapshot a [`TransportFrame`] at each block boundary, exactly as
/// a real host would present one to `process`.
#[derive(Clone, Copy, Debug)]
pub struct FakeTransport {
    sample_rate: f64,
    tempo: f64,
    beats_per_bar: f64,
    /// Samples elapsed since the transport started (the playhead).
    pos_samples: f64,
    playing: bool,
}

impl FakeTransport {
    /// A rolling 4/4 transport at `tempo` BPM from bar 0.
    pub fn new(sample_rate: f64, tempo: f64) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            tempo: tempo.max(1.0),
            beats_per_bar: 4.0,
            pos_samples: 0.0,
            playing: true,
        }
    }

    /// Set a non-4/4 time signature (`num`/`den`, e.g. 3/4, 6/8). Beats-per-bar is
    /// `num · 4 / den` quarter notes. Returns `self` for chaining.
    pub fn with_time_sig(mut self, num: u32, den: u32) -> Self {
        let n = num.max(1) as f64;
        let d = den.max(1) as f64;
        self.beats_per_bar = (n * 4.0 / d).max(1.0e-3);
        self
    }

    /// Set the playing flag (default rolling). Returns `self` for chaining.
    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    /// Bars advanced per sample at the current tempo/time-signature.
    #[inline]
    pub fn bars_per_sample(&self) -> f64 {
        (self.tempo / 60.0 / self.sample_rate) / self.beats_per_bar
    }

    /// Advance the playhead by `n` samples.
    #[inline]
    pub fn advance(&mut self, n: usize) {
        self.pos_samples += n as f64;
    }

    /// Seek the playhead to an absolute sample position.
    pub fn seek_samples(&mut self, pos: f64) {
        self.pos_samples = pos.max(0.0);
    }

    /// Current playhead in samples.
    #[inline]
    pub fn pos_samples(&self) -> f64 {
        self.pos_samples
    }

    /// Snapshot a [`TransportFrame`] at the current playhead (playing per the flag).
    pub fn frame(&self) -> TransportFrame {
        let beats = self.pos_samples / self.sample_rate * (self.tempo / 60.0);
        TransportFrame {
            playing: self.playing,
            tempo: self.tempo,
            ppq_pos: beats,
            bar_pos: beats / self.beats_per_bar,
            bars_per_sample: self.bars_per_sample(),
            beats_per_bar: self.beats_per_bar,
        }
    }

    /// Snapshot a frame with `playing = false` (free-run marker), position unchanged.
    pub fn frame_stopped(&self) -> TransportFrame {
        let mut f = self.frame();
        f.playing = false;
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_kick_is_finite_bounded_and_non_silent() {
        let sr = 48_000.0f32;
        let x = synth_kick(&KickSpec::default(), (sr * 0.5) as usize, sr);
        assert!(x.iter().all(|v| v.is_finite()));
        let peak = x.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        assert!(peak <= 1.0, "kick peak exceeds 0 dBFS: {peak}");
        assert!(peak > 0.2, "kick too quiet: {peak}");
        // Starts loud, decays: early RMS >> late RMS.
        let e: f32 = x[..2000].iter().map(|v| v * v).sum();
        let l: f32 = x[x.len() - 2000..].iter().map(|v| v * v).sum();
        assert!(e > l, "kick did not decay (early {e} late {l})");
    }

    #[test]
    fn fake_transport_advances_sample_accurately_in_4_4() {
        let sr = 48_000.0;
        let bpm = 120.0;
        let mut t = FakeTransport::new(sr, bpm);
        // At 120 BPM a beat is 0.5 s = 24000 samples; a 4/4 bar = 4 beats = 96000 samples.
        let bar_samples = 96_000usize;
        // Drive block-by-block; a frame taken at the block boundary must report the exact
        // playhead the sample count implies.
        let block = 512usize;
        let mut n = 0usize;
        while n < bar_samples {
            let f = t.frame();
            let expect_bar = n as f64 / bar_samples as f64;
            assert!((f.bar_pos - expect_bar).abs() < 1e-9, "bar_pos {} vs {}", f.bar_pos, expect_bar);
            assert!((f.ppq_pos - expect_bar * 4.0).abs() < 1e-9);
            assert!(f.playing);
            let step = block.min(bar_samples - n);
            t.advance(step);
            n += step;
        }
        // After exactly one bar the playhead is at bar 1.0.
        assert!((t.frame().bar_pos - 1.0).abs() < 1e-9);
        // bars_per_sample × bar_samples == 1 bar.
        assert!((t.bars_per_sample() * bar_samples as f64 - 1.0).abs() < 1e-9);
    }

    // --- shared helpers for the musical-source tests -----------------------

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0f32, |a, &v| a.max(v.abs()))
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// Count onsets via a simple spectral-flux-free energy-flux detector: per-frame
    /// RMS, positive difference (flux), then peaks above a fraction of the max flux
    /// separated by a refractory gap.
    fn count_onsets(x: &[f32], sr: f32) -> usize {
        let hop = (0.005 * sr) as usize; // 5 ms
        let win = hop * 2;
        if x.len() < win {
            return 0;
        }
        let nframes = (x.len() - win) / hop + 1;
        let mut env = Vec::with_capacity(nframes);
        for i in 0..nframes {
            let seg = &x[i * hop..i * hop + win];
            env.push(rms(seg));
        }
        let mut flux: Vec<f32> = vec![0.0; nframes];
        for i in 1..nframes {
            flux[i] = (env[i] - env[i - 1]).max(0.0);
        }
        let maxf = flux.iter().cloned().fold(0.0f32, f32::max);
        if maxf <= 1.0e-9 {
            return 0;
        }
        let thresh = 0.18 * maxf;
        let refractory = (0.10 * sr / hop as f32) as usize; // 100 ms in frames
        let mut count = 0usize;
        let mut last = 0usize;
        let mut armed = true;
        for i in 1..nframes - 1 {
            let is_peak = flux[i] > thresh && flux[i] >= flux[i - 1] && flux[i] >= flux[i + 1];
            if is_peak && armed && (count == 0 || i - last >= refractory) {
                count += 1;
                last = i;
                armed = false;
            }
            if flux[i] < 0.5 * thresh {
                armed = true;
            }
        }
        count
    }

    /// Fraction of energy above `cut` Hz (via a 2-pole high-pass), for the reese test.
    fn hf_energy_fraction(x: &[f32], cut: f32, sr: f32) -> f32 {
        let mut hp = crate::dsp::Svf::new();
        hp.set(cut, 0.707, sr);
        let mut hi = 0.0f64;
        let mut tot = 0.0f64;
        for &s in x {
            let h = hp.process(s).hp;
            hi += (h * h) as f64;
            tot += (s * s) as f64;
        }
        (hi / (tot + 1.0e-12)) as f32
    }

    #[test]
    fn kick_loop_has_four_onsets_per_bar() {
        let sr = 48_000.0;
        let bars = 2;
        let x = synth_kick_loop(130.0, bars, sr);
        assert!(x.iter().all(|v| v.is_finite()));
        assert!(peak(&x) <= 1.0, "peak {} over 0 dBFS", peak(&x));
        assert!(rms(&x) > 1.0e-3, "silent");
        // deterministic
        let y = synth_kick_loop(130.0, bars, sr);
        assert_eq!(x, y, "kick loop not deterministic");
        let onsets = count_onsets(&x, sr);
        assert_eq!(onsets, 4 * bars, "expected {} onsets, got {onsets}", 4 * bars);
    }

    #[test]
    fn reese_energy_is_low_concentrated() {
        let sr = 48_000.0;
        let x = synth_reese(55.0, 1.0, sr);
        assert!(x.iter().all(|v| v.is_finite()));
        assert!(peak(&x) <= 1.0);
        assert!(rms(&x) > 1.0e-3, "silent");
        assert_eq!(x, synth_reese(55.0, 1.0, sr), "reese not deterministic");
        let hf = hf_energy_fraction(&x, 1500.0, sr);
        assert!(hf < 0.10, "too much energy above 1.5 kHz: {hf}");
    }

    #[test]
    fn break_has_many_onsets() {
        let sr = 48_000.0;
        let bars = 2;
        let x = synth_break(174.0, bars, sr);
        assert!(x.iter().all(|v| v.is_finite()));
        assert!(peak(&x) <= 1.0);
        assert!(rms(&x) > 1.0e-3, "silent");
        assert_eq!(x, synth_break(174.0, bars, sr), "break not deterministic");
        let onsets = count_onsets(&x, sr);
        assert!(onsets > 8, "break should have >8 onsets/2 bars, got {onsets}");
    }

    #[test]
    fn pad_is_sustained_low_crest() {
        let sr = 48_000.0;
        let x = synth_pad(110.0, 2.0, sr);
        assert!(x.iter().all(|v| v.is_finite()));
        assert!(peak(&x) <= 1.0);
        let r = rms(&x);
        assert!(r > 1.0e-3, "silent");
        assert_eq!(x, synth_pad(110.0, 2.0, sr), "pad not deterministic");
        let crest_db = 20.0 * (peak(&x) / r).log10();
        assert!(crest_db < 12.0, "pad crest too high (not sustained): {crest_db} dB");
    }

    #[test]
    fn fake_transport_beats_per_sample_and_stopped() {
        let sr = 44_100.0;
        let t = FakeTransport::new(sr, 90.0).playing(false);
        // beats/sample = tempo/60/sr.
        assert!((t.frame().beats_per_sample() - 90.0 / 60.0 / sr).abs() < 1e-12);
        assert!(!t.frame().playing);
        // 3/4 time-signature: 3 beats per bar.
        let t34 = FakeTransport::new(sr, 120.0).with_time_sig(3, 4);
        assert!((t34.frame().beats_per_bar - 3.0).abs() < 1e-9);
    }
}
