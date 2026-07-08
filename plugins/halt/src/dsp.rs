//! HALT — pure-DSP core for the performance buffer FX (SPECS "HALT", Phase 3). Shared
//! verbatim between the nih-plug `process` path and the offline done-bar tests.
//!
//! ```text
//!   in ─┬──────────────────────────────────────────────── dry ──────────────────┐
//!       │                                                                          ├─(1-mix)/mix─► out
//!       └─► 4-bar circular capture (ALWAYS recording) ──► read head(s) ──► wet ───┘
//!               modes (momentary, last-pressed wins, 5 ms equal-power crossfades):
//!                 TAPE STOP  rate 1→0 over a synced/free duration with a curve
//!                 STUTTER    loop the last 1/4..1/64 (decay + pitch-step per repeat,
//!                            retrigger-quantized anchor)
//!                 REVERSE    read backward from the trigger point
//!                 HALF-SPEED read forward at rate 0.5
//! ```
//!
//! **Null / latency.** The core reports ZERO latency. While no mode is engaged (and no
//! crossfade is in flight) the core is *idle* and the plugin returns the input verbatim,
//! so an inactive HALT is a **bit-exact passthrough**. When a mode is active the plugin
//! blends `out = (1-mix)·dry + mix·wet`, so `mix = 0` is also an exact passthrough (the
//! CLEAVE null contract). The buffer keeps recording regardless, so a mode always has the
//! recent past to play with.
//!
//! **Transport.** The core is handed a [`TransportFrame`] each block (the plugin fills it
//! from the host, tests from a `FakeTransport`). Tempo drives the stutter division and the
//! synced tape-stop duration; the playhead drives retrigger quantize. When the host is
//! stopped the stutter/tape still run at the host tempo (free-run, CLEAVE precedent) and
//! quantize falls back to "anchor at the present" (no grid to snap to).
//!
//! **Crossfades (click-free).** Every engage / disengage / mode-change / stutter loop-wrap
//! snapshots the currently-sounding reader into `prev` (it keeps advancing, coherently) and
//! swaps in a fresh `cur`; the two are mixed with a 5 ms equal-power fade. A dry/idle state
//! is itself a reader (it returns the live input), so engage and disengage use the same path.

use suite_core::testsig::TransportFrame;

/// Longest buffer we allocate: 4 bars of 4/4 down to 30 BPM = 32 s. Slower tempos clamp the
/// effective read window to this length (documented; benign for a momentary performance FX).
pub const MAX_BUFFER_S: f32 = 32.0;
/// Equal-power crossfade length for every transition (SPECS: 5 ms).
pub const FADE_MS: f32 = 5.0;
/// Number of momentary modes (tape-stop / stutter / reverse / half-speed).
pub const NUM_MODES: usize = 4;

// ---------------------------------------------------------------------------
// Mode + param enums
// ---------------------------------------------------------------------------

/// The active read behaviour. `Dry` = idle (return the live input; bit-exact passthrough).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Dry,
    TapeStop,
    Stutter,
    Reverse,
    HalfSpeed,
}

impl Mode {
    /// Button/MIDI index → mode (0 tape-stop, 1 stutter, 2 reverse, 3 half-speed). The MIDI
    /// map is C1..D#1 (base note + 0..3) within the C1..E1 region the SPEC calls out.
    pub fn from_index(i: usize) -> Mode {
        match i {
            0 => Mode::TapeStop,
            1 => Mode::Stutter,
            2 => Mode::Reverse,
            _ => Mode::HalfSpeed,
        }
    }
}

/// Stutter loop division (fraction of a bar's worth of beats).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StutterDiv {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl StutterDiv {
    pub fn from_index(i: usize) -> StutterDiv {
        match i {
            0 => StutterDiv::Quarter,
            1 => StutterDiv::Eighth,
            2 => StutterDiv::Sixteenth,
            3 => StutterDiv::ThirtySecond,
            _ => StutterDiv::SixtyFourth,
        }
    }
    /// Length in quarter-note beats (1/4 = 1.0 beat).
    pub fn beats(self) -> f64 {
        match self {
            StutterDiv::Quarter => 1.0,
            StutterDiv::Eighth => 0.5,
            StutterDiv::Sixteenth => 0.25,
            StutterDiv::ThirtySecond => 0.125,
            StutterDiv::SixtyFourth => 0.0625,
        }
    }
}

/// Tape-stop duration selection. `Free` uses the seconds knob; the rest are transport-synced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TapeSync {
    Free,
    Beat,   // 1 beat
    Half,   // 1/2 bar (2 beats)
    Bar,    // 1 bar (4 beats)
    TwoBar, // 2 bars (8 beats)
}

impl TapeSync {
    pub fn from_index(i: usize) -> TapeSync {
        match i {
            0 => TapeSync::Free,
            1 => TapeSync::Beat,
            2 => TapeSync::Half,
            3 => TapeSync::Bar,
            _ => TapeSync::TwoBar,
        }
    }
    /// Beats for the synced options; `None` for `Free` (use the seconds knob).
    pub fn beats(self) -> Option<f64> {
        match self {
            TapeSync::Free => None,
            TapeSync::Beat => Some(1.0),
            TapeSync::Half => Some(2.0),
            TapeSync::Bar => Some(4.0),
            TapeSync::TwoBar => Some(8.0),
        }
    }
}

/// Retrigger-quantize grid for stutter (snaps the loop anchor to the beat grid).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuantDiv {
    Off,
    Sixteenth,
    Eighth,
    Quarter,
}

impl QuantDiv {
    pub fn from_index(i: usize) -> QuantDiv {
        match i {
            0 => QuantDiv::Off,
            1 => QuantDiv::Sixteenth,
            2 => QuantDiv::Eighth,
            _ => QuantDiv::Quarter,
        }
    }
    /// Grid spacing in beats, or `None` for `Off`.
    pub fn beats(self) -> Option<f64> {
        match self {
            QuantDiv::Off => None,
            QuantDiv::Sixteenth => Some(0.25),
            QuantDiv::Eighth => Some(0.5),
            QuantDiv::Quarter => Some(1.0),
        }
    }
}

/// Tape-stop release behaviour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TapeRelease {
    /// Spin the tape back up to speed, then rejoin the dry (a "reverse tape-stop").
    Ramp,
    /// Immediately crossfade back to the dry.
    Instant,
}

impl TapeRelease {
    pub fn from_index(i: usize) -> TapeRelease {
        match i {
            0 => TapeRelease::Ramp,
            _ => TapeRelease::Instant,
        }
    }
}

// ---------------------------------------------------------------------------
// Settings (configured each block)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub stutter_div: StutterDiv,
    /// Per-repeat amplitude loss, 0 (no decay) .. 1 (fast decay).
    pub stutter_decay: f32,
    /// Per-repeat pitch step in semitones (−12..12; 0 keeps the loop period exact).
    pub stutter_pitch: i32,
    pub tape_sync: TapeSync,
    /// Free tape-stop duration in seconds (used when `tape_sync == Free`).
    pub tape_free_s: f32,
    /// Curve morph 0 (fast-out, exp) .. 0.5 (linear) .. 1 (slow-out, log).
    pub tape_curve: f32,
    pub tape_release: TapeRelease,
    pub quantize: QuantDiv,
    pub mix: f32,
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            stutter_div: StutterDiv::Eighth,
            stutter_decay: 0.0,
            stutter_pitch: 0,
            tape_sync: TapeSync::Bar,
            tape_free_s: 1.0,
            tape_curve: 0.5,
            tape_release: TapeRelease::Instant,
            quantize: QuantDiv::Off,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Reader: one read head with enough state to produce the next sample of its mode.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Reader {
    mode: Mode,
    /// Absolute fractional sample index into the capture (same coordinate as `write_pos`).
    read_pos: f64,
    /// Advance per output sample (negative for reverse, <1 for slow-downs).
    rate: f64,
    /// Output amplitude (stutter per-repeat decay lives here).
    amp: f32,
    // --- tape-stop ---
    tape_phase: f64,
    tape_dur: f64,
    tape_gamma: f64,
    tape_releasing: bool,
    // --- stutter ---
    loop_start: f64,
    loop_len: f64,
    /// Output-sample phase since the last retrigger (0..loop_len). Decoupled from the read
    /// rate so the loop PERIOD stays exactly `loop_len` regardless of the pitch step.
    loop_pos: f64,
    stutter_repeat_gain: f32,
    stutter_pitch_ratio: f64,
}

impl Reader {
    fn dry() -> Self {
        Self {
            mode: Mode::Dry,
            read_pos: 0.0,
            rate: 0.0,
            amp: 1.0,
            tape_phase: 0.0,
            tape_dur: 1.0,
            tape_gamma: 1.0,
            tape_releasing: false,
            loop_start: 0.0,
            loop_len: 1.0,
            loop_pos: 0.0,
            stutter_repeat_gain: 1.0,
            stutter_pitch_ratio: 1.0,
        }
    }

    /// Produce this reader's next stereo sample and advance it. `l_in`/`r_in` are the live
    /// dry samples (returned verbatim by the `Dry` reader — the passthrough path).
    #[inline]
    fn next_sample(
        &mut self,
        buf_l: &[f32],
        buf_r: &[f32],
        len: usize,
        l_in: f32,
        r_in: f32,
    ) -> (f32, f32) {
        if self.mode == Mode::Dry {
            return (l_in, r_in);
        }
        let (l, r) = read_interp(buf_l, buf_r, len, self.read_pos);
        let out = (l * self.amp, r * self.amp);
        self.advance();
        out
    }

    /// Advance the read head (mode-specific rate law). Stutter loop-wrap is handled by the
    /// core so it can crossfade the wrap.
    #[inline]
    fn advance(&mut self) {
        self.read_pos += self.rate;
        if self.mode == Mode::Stutter {
            // Keep the read inside the captured slice (a higher pitch just loops the slice
            // faster); the retrigger PERIOD is tracked separately in the core.
            if self.read_pos >= self.loop_start + self.loop_len {
                self.read_pos -= self.loop_len;
            } else if self.read_pos < self.loop_start {
                self.read_pos += self.loop_len;
            }
        }
        if self.mode == Mode::TapeStop {
            let step = 1.0 / self.tape_dur.max(1.0);
            if self.tape_releasing {
                // Spin back up: phase 1→0, rate climbs toward 1.
                self.tape_phase = (self.tape_phase - step).max(0.0);
            } else {
                // Slow down: phase 0→1, rate falls toward 0.
                self.tape_phase = (self.tape_phase + step).min(1.0);
            }
            self.rate = (1.0 - self.tape_phase).max(0.0).powf(self.tape_gamma);
        }
    }
}

/// Linear-interpolated stereo read of the circular capture at absolute fractional index `pos`.
#[inline]
fn read_interp(buf_l: &[f32], buf_r: &[f32], len: usize, pos: f64) -> (f32, f32) {
    let i0 = pos.floor();
    let frac = (pos - i0) as f32;
    let a = wrap_index(i0 as i64, len);
    let b = wrap_index(i0 as i64 + 1, len);
    let l = buf_l[a] + (buf_l[b] - buf_l[a]) * frac;
    let r = buf_r[a] + (buf_r[b] - buf_r[a]) * frac;
    (l, r)
}

/// Euclidean modulo of an absolute (possibly negative) index into `[0, len)`.
#[inline]
fn wrap_index(i: i64, len: usize) -> usize {
    let l = len as i64;
    (((i % l) + l) % l) as usize
}

/// Equal-power crossfade gains for the incoming reader at progress `t` in `[0, 1]`.
/// Returns `(gain_new, gain_old)`; `gain_new² + gain_old² == 1`.
#[inline]
fn eq_power(t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    let theta = t * std::f32::consts::FRAC_PI_2;
    (theta.sin(), theta.cos())
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct HaltCore {
    sr: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    len: usize,
    /// Absolute count of samples recorded (the write head; buffer index = `write_pos % len`).
    write_pos: i64,

    settings: Settings,
    frame: TransportFrame,

    // mode management (last-pressed wins)
    held: [bool; NUM_MODES],
    serial: [u64; NUM_MODES],
    serial_ctr: u64,
    active: Mode,

    // readers + crossfade
    cur: Reader,
    prev: Reader,
    xfade_left: u32,
    xfade_len: u32,
}

impl HaltCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let len = ((MAX_BUFFER_S * sr) as usize).max(1024);
        let xfade_len = ((FADE_MS * 0.001 * sr).round() as u32).max(1);
        Self {
            sr,
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            len,
            write_pos: 0,
            settings: Settings::default(),
            frame: TransportFrame {
                playing: false,
                tempo: 120.0,
                ppq_pos: 0.0,
                bar_pos: 0.0,
                bars_per_sample: 0.0,
                beats_per_bar: 4.0,
            },
            held: [false; NUM_MODES],
            serial: [0; NUM_MODES],
            serial_ctr: 0,
            active: Mode::Dry,
            cur: Reader::dry(),
            prev: Reader::dry(),
            xfade_left: 0,
            xfade_len,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Zero — the dry path is never delayed (the wet is a re-timed creative signal).
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn reset(&mut self) {
        for v in self.buf_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.buf_r.iter_mut() {
            *v = 0.0;
        }
        self.write_pos = 0;
        self.held = [false; NUM_MODES];
        self.serial = [0; NUM_MODES];
        self.serial_ctr = 0;
        self.active = Mode::Dry;
        self.cur = Reader::dry();
        self.prev = Reader::dry();
        self.xfade_left = 0;
    }

    pub fn configure(&mut self, s: &Settings) {
        self.settings = *s;
    }

    pub fn set_transport(&mut self, frame: &TransportFrame) {
        self.frame = *frame;
    }

    /// True when nothing is sounding through the buffer — the plugin returns the input
    /// verbatim (bit-exact passthrough).
    #[inline]
    pub fn is_idle(&self) -> bool {
        self.active == Mode::Dry && self.xfade_left == 0
    }

    pub fn active_mode(&self) -> Mode {
        self.active
    }

    // --- mode buttons ------------------------------------------------------

    /// Update the held state of the four momentary modes (buttons OR MIDI notes, already
    /// combined by the plugin). Detects edges, tracks press order, and (re)engages the
    /// last-pressed held mode. Block-rate is fine — every switch is crossfaded.
    pub fn set_held(&mut self, new_held: &[bool; NUM_MODES]) {
        for i in 0..NUM_MODES {
            if new_held[i] && !self.held[i] {
                self.serial_ctr += 1;
                self.serial[i] = self.serial_ctr;
            } else if !new_held[i] {
                self.serial[i] = 0;
            }
            self.held[i] = new_held[i];
        }
        let desired = self.desired_mode();
        self.engage(desired);
    }

    /// The held mode with the most-recent press wins; none held → `Dry`.
    fn desired_mode(&self) -> Mode {
        let mut best_serial = 0u64;
        let mut best = Mode::Dry;
        for i in 0..NUM_MODES {
            if self.held[i] && self.serial[i] > best_serial {
                best_serial = self.serial[i];
                best = Mode::from_index(i);
            }
        }
        best
    }

    fn engage(&mut self, desired: Mode) {
        // A tape-stop Ramp-release already in progress: let it spin the tape back up
        // (`process_sample` auto-disengages at full speed). Only a *new* momentary mode
        // cancels it early.
        if self.active == Mode::TapeStop && self.cur.tape_releasing {
            if desired != Mode::Dry && desired != Mode::TapeStop {
                self.begin_transition();
                self.active = desired;
                self.cur = self.make_reader(desired);
            }
            return;
        }
        // Disengaging a tape-stop with Ramp release spins the tape back up instead of an
        // instant crossfade.
        if desired == Mode::Dry
            && self.active == Mode::TapeStop
            && self.settings.tape_release == TapeRelease::Ramp
        {
            self.cur.tape_releasing = true;
            return;
        }
        // Same mode still held → nothing to do (never re-trigger mid-crossfade).
        if desired == self.active {
            return;
        }
        self.begin_transition();
        self.active = desired;
        self.cur = self.make_reader(desired);
    }

    /// Snapshot the currently-sounding reader into `prev` and arm a fresh crossfade.
    fn begin_transition(&mut self) {
        self.prev = self.cur;
        self.xfade_left = self.xfade_len;
    }

    fn make_reader(&self, mode: Mode) -> Reader {
        let w = self.write_pos as f64;
        match mode {
            Mode::Dry => Reader::dry(),
            Mode::Reverse => Reader {
                mode,
                read_pos: w - 1.0,
                rate: -1.0,
                ..Reader::dry()
            },
            Mode::HalfSpeed => Reader {
                mode,
                read_pos: w - 1.0,
                rate: 0.5,
                ..Reader::dry()
            },
            Mode::TapeStop => Reader {
                mode,
                read_pos: w - 1.0,
                rate: 1.0,
                tape_phase: 0.0,
                tape_dur: self.tape_dur_samps(),
                tape_gamma: self.tape_gamma(),
                tape_releasing: false,
                ..Reader::dry()
            },
            Mode::Stutter => {
                let loop_len = self.stutter_loop_len();
                let anchor = self.stutter_anchor();
                let ls = anchor - loop_len;
                Reader {
                    mode,
                    read_pos: ls,
                    rate: 1.0,
                    loop_start: ls,
                    loop_len,
                    loop_pos: 0.0,
                    stutter_repeat_gain: (1.0 - self.settings.stutter_decay).clamp(0.0, 1.0),
                    stutter_pitch_ratio: 2f64.powf(self.settings.stutter_pitch as f64 / 12.0),
                    ..Reader::dry()
                }
            }
        }
    }

    // --- derived timings ---------------------------------------------------

    fn tempo(&self) -> f64 {
        self.frame.tempo.max(1.0)
    }

    fn samples_per_beat(&self) -> f64 {
        60.0 / self.tempo() * self.sr as f64
    }

    fn tape_dur_samps(&self) -> f64 {
        let s = match self.settings.tape_sync.beats() {
            Some(beats) => beats * self.samples_per_beat(),
            None => self.settings.tape_free_s.max(0.01) as f64 * self.sr as f64,
        };
        // Clamp to something sane and never zero.
        s.clamp(1.0, self.len as f64)
    }

    fn tape_gamma(&self) -> f64 {
        // curve 0 → 1/3 (fast-out), 0.5 → 1 (linear), 1 → 3 (slow-out).
        (1.0 / 3.0) * 9f64.powf(self.settings.tape_curve.clamp(0.0, 1.0) as f64)
    }

    fn stutter_loop_len(&self) -> f64 {
        let l = self.settings.stutter_div.beats() * self.samples_per_beat();
        // Keep it inside the capture with room for the read head.
        l.clamp(4.0, (self.len - 4) as f64)
    }

    /// Where the stutter loop ends (its most-recent edge). With quantize on and the transport
    /// rolling, snap to the most-recent grid line; otherwise anchor at the present.
    fn stutter_anchor(&self) -> f64 {
        let w = self.write_pos as f64;
        if !self.frame.playing {
            return w;
        }
        let grid = match self.settings.quantize.beats() {
            Some(b) => b,
            None => return w,
        };
        let ppq = self.frame.ppq_pos;
        let snapped = (ppq / grid).floor() * grid;
        let delta_beats = (ppq - snapped).max(0.0);
        let delta_samps = delta_beats * self.samples_per_beat();
        (w - delta_samps).max(0.0)
    }

    // --- per-sample --------------------------------------------------------

    /// Record the input, then produce the wet stereo sample (== the input while idle, so the
    /// plugin can null exactly). Never allocates.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32) -> (f32, f32) {
        // 1) Always record into the circular capture.
        let wi = (self.write_pos.rem_euclid(self.len as i64)) as usize;
        self.buf_l[wi] = l_in;
        self.buf_r[wi] = r_in;
        self.write_pos += 1;

        // 2) Produce the current reader (and the outgoing reader during a fade).
        let (cl, cr) = self
            .cur
            .next_sample(&self.buf_l, &self.buf_r, self.len, l_in, r_in);
        let (mut out_l, mut out_r) = (cl, cr);
        if self.xfade_left > 0 {
            let (pl, pr) = self
                .prev
                .next_sample(&self.buf_l, &self.buf_r, self.len, l_in, r_in);
            let t = 1.0 - (self.xfade_left as f32 / self.xfade_len as f32);
            let (gn, go) = eq_power(t);
            out_l = cl * gn + pl * go;
            out_r = cr * gn + pr * go;
            self.xfade_left -= 1;
        }

        // 3) Stutter retrigger every `loop_len` OUTPUT samples (period independent of pitch) →
        //    crossfaded reset with the per-repeat decay + pitch step. The read rate is clamped
        //    to ±2 octaves so a compounding pitch step can never run away.
        if self.cur.mode == Mode::Stutter {
            self.cur.loop_pos += 1.0;
            if self.cur.loop_pos >= self.cur.loop_len {
                // Snapshot the just-advanced reader as the fade-out tail, then reset the live
                // reader's phase to the loop start.
                self.begin_transition();
                self.cur.loop_pos -= self.cur.loop_len;
                self.cur.read_pos = self.cur.loop_start;
                self.cur.amp *= self.cur.stutter_repeat_gain;
                self.cur.rate = (self.cur.rate * self.cur.stutter_pitch_ratio).clamp(0.25, 4.0);
            }
        }

        // 4) Tape-stop Ramp release that has spun back up to speed → auto-disengage to dry.
        if self.cur.mode == Mode::TapeStop
            && self.cur.tape_releasing
            && self.cur.tape_phase <= 0.0
            && self.xfade_left == 0
        {
            let desired = self.desired_mode();
            self.begin_transition();
            self.active = desired;
            self.cur = self.make_reader(desired);
        }

        (out_l, out_r)
    }

    // --- test hooks --------------------------------------------------------

    #[cfg(test)]
    pub fn test_write_pos(&self) -> i64 {
        self.write_pos
    }
}
