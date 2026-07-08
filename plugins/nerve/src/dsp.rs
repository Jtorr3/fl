//! NERVE DSP — the modulation sources that feed the 8 tier-2 bus streams.
//!
//! Fixed 8-stream layout (documented in docs/NERVE.md), chosen so the whole modulation
//! desk is a bounded, fuzz-safe param set:
//!
//! | stream | source            |
//! |--------|-------------------|
//! | S1..S4 | LFO A..D (+ paired Macro offset) |
//! | S5..S6 | Env follower A..B |
//! | S7..S8 | Random S&H A..B   |
//!
//! The 4 Macro knobs are bipolar hand controllers summed into streams S1..S4 (so a "Macro
//! Desk" preset with all LFO depths at 0 turns NERVE into four hand-ridden DC modulators).
//! LFO A..D swing bipolar `[-1,1]`; env followers are unipolar `[0,1]`; S&H are bipolar.
//! Everything is generated in code (no host state beyond params); NERVE passes audio
//! through **bit-exact** (it is a modulation tap, transparent, zero latency).

use suite_core::bus::NUM_MOD_SIGNALS;

pub const NUM_LFO: usize = 4;
pub const NUM_ENV: usize = 2;
pub const NUM_SH: usize = 2;
pub const NUM_MACRO: usize = 4;

/// LFO waveforms (8, incl. two random variants).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    StepRandom,
    SmoothRandom,
    ExpPulse,
}
impl Shape {
    pub const ALL: [Shape; 8] = [
        Shape::Sine,
        Shape::Triangle,
        Shape::SawUp,
        Shape::SawDown,
        Shape::Square,
        Shape::StepRandom,
        Shape::SmoothRandom,
        Shape::ExpPulse,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Shape::Sine => "Sine",
            Shape::Triangle => "Triangle",
            Shape::SawUp => "Saw Up",
            Shape::SawDown => "Saw Down",
            Shape::Square => "Square",
            Shape::StepRandom => "S&H",
            Shape::SmoothRandom => "Smooth Rnd",
            Shape::ExpPulse => "Exp Pulse",
        }
    }
    pub fn from_index(i: usize) -> Shape {
        Shape::ALL[i.min(7)]
    }
}

/// Musical divisions for tempo-synced sources (beats per LFO cycle, assuming 4/4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Division {
    Bars4,
    Bars2,
    Bar1,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}
impl Division {
    pub const ALL: [Division; 7] = [
        Division::Bars4,
        Division::Bars2,
        Division::Bar1,
        Division::Half,
        Division::Quarter,
        Division::Eighth,
        Division::Sixteenth,
    ];
    /// Beats per full cycle (4/4).
    pub fn beats_per_cycle(self) -> f32 {
        match self {
            Division::Bars4 => 16.0,
            Division::Bars2 => 8.0,
            Division::Bar1 => 4.0,
            Division::Half => 2.0,
            Division::Quarter => 1.0,
            Division::Eighth => 0.5,
            Division::Sixteenth => 0.25,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Division::Bars4 => "4 Bars",
            Division::Bars2 => "2 Bars",
            Division::Bar1 => "1 Bar",
            Division::Half => "1/2",
            Division::Quarter => "1/4",
            Division::Eighth => "1/8",
            Division::Sixteenth => "1/16",
        }
    }
    pub fn from_index(i: usize) -> Division {
        Division::ALL[i.min(6)]
    }
}

#[derive(Clone, Copy)]
pub struct LfoSet {
    pub rate_hz: f32,
    pub synced: bool,
    pub div: Division,
    pub shape: Shape,
    pub depth: f32,
}
#[derive(Clone, Copy)]
pub struct EnvSet {
    pub attack_ms: f32,
    pub release_ms: f32,
    pub depth: f32,
}
#[derive(Clone, Copy)]
pub struct ShSet {
    pub rate_hz: f32,
    pub slew: f32,
    pub depth: f32,
}

/// A full block-rate configuration snapshot built from the plugin params + transport.
#[derive(Clone, Copy)]
pub struct Settings {
    pub lfo: [LfoSet; NUM_LFO],
    pub macros: [f32; NUM_MACRO],
    pub env: [EnvSet; NUM_ENV],
    pub sh: [ShSet; NUM_SH],
    /// Host tempo (BPM). 0 or negative → treated as free-run only.
    pub tempo: f32,
    /// Transport beat position at the start of the block (quarter notes).
    pub beats: f64,
    pub playing: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let lfo = LfoSet {
            rate_hz: 1.0,
            synced: false,
            div: Division::Bar1,
            shape: Shape::Sine,
            depth: 1.0,
        };
        Settings {
            lfo: [lfo; NUM_LFO],
            macros: [0.0; NUM_MACRO],
            env: [EnvSet {
                attack_ms: 10.0,
                release_ms: 150.0,
                depth: 1.0,
            }; NUM_ENV],
            sh: [ShSet {
                rate_hz: 4.0,
                slew: 0.0,
                depth: 1.0,
            }; NUM_SH],
            tempo: 120.0,
            beats: 0.0,
            playing: false,
        }
    }
}

#[inline]
fn xorshift(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    // Map to [-1, 1].
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Evaluate a periodic waveform at phase in `[0,1)`. Random shapes use the supplied
/// prev/next endpoints (regenerated once per cycle by the caller).
#[inline]
fn eval_shape(shape: Shape, phase: f32, prev: f32, next: f32) -> f32 {
    let p = phase - phase.floor();
    match shape {
        Shape::Sine => (std::f32::consts::TAU * p).sin(),
        Shape::Triangle => {
            let t = if p < 0.5 { p * 2.0 } else { 2.0 - p * 2.0 };
            t * 2.0 - 1.0
        }
        Shape::SawUp => p * 2.0 - 1.0,
        Shape::SawDown => 1.0 - p * 2.0,
        Shape::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Shape::StepRandom => next, // held for the whole cycle
        Shape::SmoothRandom => prev + (next - prev) * (p * p * (3.0 - 2.0 * p)), // smoothstep lerp
        Shape::ExpPulse => 2.0 * (-p * 5.0).exp() - 1.0,
    }
}

/// The modulation engine. Alloc-free after construction; advances at block rate for the
/// LFO / S&H streams and per sample for the env followers.
pub struct NerveCore {
    sr: f32,
    // LFOs: fractional cumulative cycles (free-run accumulator) + per-cycle random endpoints.
    lfo_accum: [f64; NUM_LFO],
    lfo_cycle: [i64; NUM_LFO],
    lfo_prev: [f32; NUM_LFO],
    lfo_next: [f32; NUM_LFO],
    lfo_rng: [u32; NUM_LFO],
    // Env followers (state = smoothed |x|).
    env_state: [f32; NUM_ENV],
    // S&H engines.
    sh_phase: [f32; NUM_SH],
    sh_cur: [f32; NUM_SH],
    sh_tgt: [f32; NUM_SH],
    sh_rng: [u32; NUM_SH],
    out: [f32; NUM_MOD_SIGNALS],
}

impl NerveCore {
    pub fn new(sr: f32) -> Self {
        let mut c = NerveCore {
            sr: sr.max(1.0),
            lfo_accum: [0.0; NUM_LFO],
            lfo_cycle: [i64::MIN; NUM_LFO],
            lfo_prev: [0.0; NUM_LFO],
            lfo_next: [0.0; NUM_LFO],
            lfo_rng: [0x1234_5678, 0x9E37_79B9, 0x2545_F491, 0xC2B2_AE35],
            env_state: [0.0; NUM_ENV],
            sh_phase: [0.0; NUM_SH],
            sh_cur: [0.0; NUM_SH],
            sh_tgt: [0.0; NUM_SH],
            sh_rng: [0xDEAD_BEEF, 0x1357_9BDF],
            out: [0.0; NUM_MOD_SIGNALS],
        };
        // Seed the first random targets.
        for k in 0..NUM_LFO {
            c.lfo_next[k] = xorshift(&mut c.lfo_rng[k]);
            c.lfo_prev[k] = c.lfo_next[k];
        }
        for k in 0..NUM_SH {
            c.sh_tgt[k] = xorshift(&mut c.sh_rng[k]);
            c.sh_cur[k] = c.sh_tgt[k];
        }
        c
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr.max(1.0);
    }

    pub fn reset(&mut self) {
        self.lfo_accum = [0.0; NUM_LFO];
        self.lfo_cycle = [i64::MIN; NUM_LFO];
        self.env_state = [0.0; NUM_ENV];
        self.sh_phase = [0.0; NUM_SH];
    }

    /// Feed one mono input sample to the env followers (per sample).
    #[inline]
    pub fn feed_input(&mut self, x: f32, set: &Settings) {
        let ax = x.abs();
        for k in 0..NUM_ENV {
            let atk = env_coeff(set.env[k].attack_ms, self.sr);
            let rel = env_coeff(set.env[k].release_ms, self.sr);
            let s = self.env_state[k];
            let coeff = if ax > s { atk } else { rel };
            self.env_state[k] = s + (ax - s) * coeff;
        }
    }

    /// Advance the LFO / S&H streams by `n` samples and recompute all 8 outputs. Returns the
    /// published stream vector. Alloc-free.
    pub fn advance(&mut self, n: usize, set: &Settings) -> [f32; NUM_MOD_SIGNALS] {
        let dt_cycles_per_sample = |rate_hz: f32| (rate_hz as f64) / (self.sr as f64);
        let block = n.max(1) as f64;

        // LFOs (streams 0..4) + paired macro offset.
        for k in 0..NUM_LFO {
            let l = set.lfo[k];
            let accum = if l.synced && set.playing && set.tempo > 0.0 {
                // Lock phase to the transport: cycles = beats / beats_per_cycle.
                set.beats / l.div.beats_per_cycle() as f64
            } else {
                self.lfo_accum[k] + dt_cycles_per_sample(l.rate_hz) * block
            };
            self.lfo_accum[k] = accum;
            let cyc = accum.floor() as i64;
            if cyc != self.lfo_cycle[k] {
                // New cycle(s): roll fresh random endpoints for the random shapes.
                self.lfo_cycle[k] = cyc;
                self.lfo_prev[k] = self.lfo_next[k];
                self.lfo_next[k] = xorshift(&mut self.lfo_rng[k]);
            }
            let phase = (accum - accum.floor()) as f32;
            let raw = eval_shape(l.shape, phase, self.lfo_prev[k], self.lfo_next[k]);
            let macro_off = set.macros[k];
            self.out[k] = (raw * l.depth + macro_off).clamp(-1.0, 1.0);
        }

        // Env followers (streams 4..6), unipolar.
        for k in 0..NUM_ENV {
            self.out[NUM_LFO + k] = (self.env_state[k] * set.env[k].depth).clamp(0.0, 1.0);
        }

        // S&H (streams 6..8), bipolar, block-advanced with a slew glide.
        for k in 0..NUM_SH {
            let s = set.sh[k];
            self.sh_phase[k] += (dt_cycles_per_sample(s.rate_hz) * block) as f32;
            if self.sh_phase[k] >= 1.0 {
                self.sh_phase[k] -= self.sh_phase[k].floor();
                self.sh_tgt[k] = xorshift(&mut self.sh_rng[k]);
            }
            // Slew: 0 = instant step, →1 = slow glide toward the target.
            self.sh_cur[k] += (self.sh_tgt[k] - self.sh_cur[k]) * (1.0 - s.slew.clamp(0.0, 0.999));
            self.out[NUM_LFO + NUM_ENV + k] = (self.sh_cur[k] * s.depth).clamp(-1.0, 1.0);
        }

        self.out
    }

    /// The most recently computed outputs (for the GUI scopes).
    pub fn outputs(&self) -> [f32; NUM_MOD_SIGNALS] {
        self.out
    }
}

#[inline]
fn env_coeff(time_ms: f32, sr: f32) -> f32 {
    let t = (time_ms.max(0.01) * 0.001 * sr).max(1.0);
    1.0 - (-1.0 / t).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_lfo(shape: Shape, rate: f32) -> Settings {
        let mut s = Settings::default();
        for l in s.lfo.iter_mut() {
            l.shape = shape;
            l.rate_hz = rate;
            l.depth = 1.0;
            l.synced = false;
        }
        s
    }

    #[test]
    fn sine_lfo_oscillates_in_range() {
        let sr = 48_000.0;
        let mut core = NerveCore::new(sr);
        let set = set_lfo(Shape::Sine, 4.0); // 4 Hz
        let block = 64;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        // Run 1 second and collect stream 0.
        for _ in 0..(sr as usize / block) {
            let o = core.advance(block, &set);
            min = min.min(o[0]);
            max = max.max(o[0]);
            assert!(o[0].is_finite() && o[0].abs() <= 1.0001);
        }
        assert!(max > 0.9 && min < -0.9, "sine should span ±1: {min}..{max}");
    }

    #[test]
    fn env_follower_tracks_level() {
        let sr = 48_000.0;
        let mut core = NerveCore::new(sr);
        let set = Settings::default();
        // Feed loud then silence; env (stream 4) should rise then fall.
        for _ in 0..4800 {
            core.feed_input(0.8, &set);
        }
        let hot = core.advance(1, &set)[NUM_LFO];
        for _ in 0..48000 {
            core.feed_input(0.0, &set);
        }
        let cold = core.advance(1, &set)[NUM_LFO];
        assert!(hot > 0.3, "env should rise under signal: {hot}");
        assert!(cold < hot * 0.2, "env should fall in silence: {cold} vs {hot}");
    }

    #[test]
    fn macro_offsets_paired_stream() {
        let mut core = NerveCore::new(48_000.0);
        let mut set = Settings::default();
        for l in set.lfo.iter_mut() {
            l.depth = 0.0; // LFOs off → stream is pure macro
        }
        set.macros[2] = 0.7;
        let o = core.advance(64, &set);
        assert!((o[2] - 0.7).abs() < 1e-4, "stream 2 should equal macro 2: {}", o[2]);
    }

    #[test]
    fn sh_streams_change_and_bounded() {
        let mut core = NerveCore::new(48_000.0);
        let mut set = Settings::default();
        set.sh[0].rate_hz = 100.0;
        set.sh[0].slew = 0.0;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let o = core.advance(64, &set);
            let v = o[NUM_LFO + NUM_ENV]; // stream 6
            assert!(v.is_finite() && v.abs() <= 1.0001);
            seen.insert((v * 1000.0) as i64);
        }
        assert!(seen.len() > 3, "S&H should produce varied values");
    }

    #[test]
    fn synced_lfo_locks_to_transport() {
        let sr = 48_000.0;
        let mut core = NerveCore::new(sr);
        let mut set = Settings::default();
        set.playing = true;
        set.tempo = 120.0;
        set.lfo[0].synced = true;
        set.lfo[0].div = Division::Bar1; // 4 beats/cycle
        set.lfo[0].shape = Shape::SawUp;
        set.lfo[0].depth = 1.0;
        // At beat 0 → phase 0 → sawup = -1.
        set.beats = 0.0;
        let a = core.advance(64, &set)[0];
        // At beat 2 (half a bar) → phase 0.5 → sawup = 0.
        set.beats = 2.0;
        let b = core.advance(64, &set)[0];
        assert!((a + 1.0).abs() < 0.05, "phase0 sawup ~ -1: {a}");
        assert!(b.abs() < 0.05, "half-bar sawup ~ 0: {b}");
    }
}
