//! WIRE — pure-DSP core for the codec-degradation effect (SPECS "WIRE", Codec clone).
//!
//! Signal flow (per PRD §5 / SPECS):
//! ```text
//!  in(host) ─ SRC→48k ─ [+regen fb] ─ bandwidth LP ─ crunch(bit/SR reduce) ─ Opus enc
//!            → packet-loss drop (zero-fill + crossfade PLC) → Opus dec ─┬─ SRC→host ─ width ─ mix ─ out
//!                                                                        └─ regen delay ─ soft-limit ─ DC-block ─┘
//! ```
//!
//! ## Codec plan (PRD §5, Plan A LANDED)
//! Plan A = the pure-Rust `opus-rs` crate (published name is hyphenated; PRD wrote
//! `opus_rs`). Zero C / no CMake. The link-test (see STATUS.md) showed this crate's
//! internal SILK-resampler paths at 12 k/24 k are buggy (decode decorrelates), while the
//! **48 k** path is reliable and its fidelity rises monotonically with bitrate in both
//! Music(Audio) and Voice(VoIP) modes. So WIRE **always runs Opus at 48 k internal** and
//! realises the "Bandwidth NB→FB" control as a *pre-codec low-pass* — exactly the
//! "approximate with bandwidth limiting and note it" fallback PRD §5 sanctions. This
//! dodges every buggy resampler path while keeping bandwidth an audible, on-brand control.
//!
//! ## Threading
//! One 20 ms frame encode+decode costs ~62 µs in release = ~0.3 % of the real-time budget
//! (benched), so the codec runs **in the audio thread**. The nih-plug wrapper enables
//! `assert_process_allocs`; `opus-rs` allocates internally, so the plugin's `process()`
//! wraps the frame work in `nih_plug::util::permit_alloc`. The offline harness calls the
//! core directly (no alloc guard active), so tests need no such wrapper.
//!
//! ## Stereo
//! Two **independent mono** Opus instances (one per channel). Mono is the reliable path
//! across the whole 6–128 kbps range, and independent quantisation of L/R adds a genuine
//! (on-brand) codec-width artifact. Width is then an explicit M/S control.
//!
//! API-agnostic pure Rust; shared verbatim between the nih-plug `process` path and the
//! offline render/done-bar tests.

use opus_rs::{Application, OpusDecoder, OpusEncoder};
use suite_core::dsp::OnePole;

/// The one Opus sampling rate WIRE runs internally (see module docs).
pub const INTERNAL_RATE: f32 = 48_000.0;
/// 20 ms frame at 48 k.
pub const FRAME: usize = 960;
/// Empirically-pinned residual (samples @48 k) beyond the frame + guard buffering. The
/// codec's own algorithmic delay (~0.3–0.5 frame, mode-dependent) is *absorbed inside* the
/// 20 ms frame buffering: a clean (high-bitrate) impulse exits at exactly FRAME + WET_GUARD + 1.
/// The done-bar latency test measures the full pipeline at 128 kbps and asserts it equals the
/// reported value; at heavily-degraded settings the codec smears the click so its peak-lag
/// drifts up to ~one frame later, but the wet there is mush and any mis-alignment is inaudible.
pub const CODEC_CONTENT_DELAY: usize = 1;
/// Small fixed guard on the wet output FIFO so the down-sampler never underflows at
/// non-48 k host rates. Counted into the reported latency so dry stays aligned.
pub const WET_GUARD: usize = 64;
/// Max regen feedback delay (samples @48 k) = 0.5 s.
const MAX_REGEN_DELAY: usize = 24_000;

// ---------------------------------------------------------------------------
// Param-facing enums
// ---------------------------------------------------------------------------

/// Encoder application profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// SILK/hybrid-leaning speech profile (`Application::Voip`).
    Voice,
    /// CELT/music profile (`Application::Audio`).
    Music,
}

impl Mode {
    pub fn from_index(i: usize) -> Mode {
        match i {
            0 => Mode::Voice,
            _ => Mode::Music,
        }
    }
    fn application(self) -> Application {
        match self {
            Mode::Voice => Application::Voip,
            Mode::Music => Application::Audio,
        }
    }
}

/// Bandwidth selector — realised as a pre-codec low-pass cutoff (module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandwidthSel {
    Narrow,
    Medium,
    Wide,
    Superwide,
    Full,
}

impl BandwidthSel {
    pub fn from_index(i: usize) -> BandwidthSel {
        match i {
            0 => BandwidthSel::Narrow,
            1 => BandwidthSel::Medium,
            2 => BandwidthSel::Wide,
            3 => BandwidthSel::Superwide,
            _ => BandwidthSel::Full,
        }
    }
    /// Pre-codec low-pass cutoff (Hz), roughly the standard Opus band edges.
    pub fn cutoff_hz(self) -> f32 {
        match self {
            BandwidthSel::Narrow => 3_500.0,
            BandwidthSel::Medium => 5_000.0,
            BandwidthSel::Wide => 7_500.0,
            BandwidthSel::Superwide => 12_000.0,
            BandwidthSel::Full => 20_000.0,
        }
    }
}

/// A full snapshot of WIRE's controls (plain, un-normalized values).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Target Opus bitrate, 6..128 kbps.
    pub bitrate_kbps: f32,
    pub mode: Mode,
    pub bandwidth: BandwidthSel,
    /// In-band FEC toggle (encoder hint; also latches encoder packet_loss_perc).
    pub fec: bool,
    /// Simulated packet-loss probability, 0..100 %.
    pub loss_pct: f32,
    /// Crunch macro 0..1 (bit-depth + sample-rate reduction, pre-codec).
    pub crunch: f32,
    /// Regen feedback delay (ms), 0..500.
    pub regen_delay_ms: f32,
    /// Regen feedback amount, 0..0.95 (generation-loss re-encode loop).
    pub regen_amount: f32,
    /// Stereo width, 0..2 (M/S side gain).
    pub width: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            bitrate_kbps: 32.0,
            mode: Mode::Music,
            bandwidth: BandwidthSel::Full,
            fec: false,
            loss_pct: 0.0,
            crunch: 0.0,
            regen_delay_ms: 120.0,
            regen_amount: 0.0,
            width: 1.0,
            mix: 1.0,
            out_db: 0.0,
        }
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Small building blocks
// ---------------------------------------------------------------------------

/// One-pole DC blocker (high-pass) for the feedback path (PRD §3 feedback convention).
#[derive(Clone, Copy, Default)]
struct DcBlocker {
    x1: f32,
    y1: f32,
}
impl DcBlocker {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        // y[n] = x[n] - x[n-1] + R*y[n-1], R≈0.9975 → ~20 Hz corner at 48 k.
        let y = x - self.x1 + 0.9975 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// Bit-depth + sample-rate reducer ("crunch"), pre-codec. `amount` 0..1 macro:
/// bits 16→~5, sample-hold decimation 1→~24.
#[derive(Clone, Copy, Default)]
struct Crunch {
    hold: f32,
    counter: f32,
}
impl Crunch {
    fn reset(&mut self) {
        self.hold = 0.0;
        self.counter = 0.0;
    }
    #[inline]
    fn process(&mut self, x: f32, amount: f32) -> f32 {
        let a = amount.clamp(0.0, 1.0);
        if a <= 1.0e-4 {
            return x;
        }
        // Sample-rate reduction: hold the last value for `step` samples.
        let step = 1.0 + a * 23.0; // 1..24
        self.counter += 1.0;
        if self.counter >= step {
            self.counter -= step;
            self.hold = x;
        }
        let held = self.hold;
        // Bit-depth reduction: quantise to `levels` steps.
        let bits = 16.0 - a * 11.0; // 16..5 bits
        let levels = 2.0f32.powf(bits);
        (held * levels).round() / levels
    }
}

/// A fixed-capacity sample FIFO (ring buffer). Never reallocates after construction, so it
/// is safe under `assert_process_allocs`. Panics (debug) on overflow — capacities are sized
/// so that can't happen in normal operation.
#[derive(Clone)]
struct SampleFifo {
    buf: Vec<f32>,
    head: usize,
    len: usize,
}
impl SampleFifo {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity.max(1)],
            head: 0,
            len: 0,
        }
    }
    fn clear_to_zeros(&mut self, count: usize) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.head = 0;
        self.len = count.min(self.buf.len());
    }
    #[inline]
    fn len(&self) -> usize {
        self.len
    }
    #[inline]
    fn push(&mut self, x: f32) {
        debug_assert!(self.len < self.buf.len(), "SampleFifo overflow");
        let tail = (self.head + self.len) % self.buf.len();
        self.buf[tail] = x;
        if self.len < self.buf.len() {
            self.len += 1;
        } else {
            // Overwrite oldest (shouldn't happen with correct sizing).
            self.head = (self.head + 1) % self.buf.len();
        }
    }
    #[inline]
    fn pop(&mut self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        let x = self.buf[self.head];
        self.head = (self.head + 1) % self.buf.len();
        self.len -= 1;
        x
    }
}

/// Streaming linear resampler. Converts a push stream at `in_rate` to `out_rate`, emitting
/// zero or more output samples per pushed input sample. When the ratio is ~1 it is an exact
/// pass-through (no interpolation, no added delay) — the property that keeps latency exact at
/// a 48 k host rate. Linear interpolation is a deliberate, documented quality choice: cheap,
/// ~1-sample group delay, and its mild aliasing is on-brand for a degradation effect.
#[derive(Clone)]
struct PushResampler {
    ratio: f32, // out_rate / in_rate
    passthrough: bool,
    prev: f32,
    cur: f32,
    have: bool,
    // Fractional output position measured in input samples since `prev`.
    t: f32,
}
impl PushResampler {
    fn new(in_rate: f32, out_rate: f32) -> Self {
        let ratio = out_rate / in_rate;
        Self {
            ratio,
            passthrough: (ratio - 1.0).abs() < 1.0e-6,
            prev: 0.0,
            cur: 0.0,
            have: false,
            t: 0.0,
        }
    }
    fn reset(&mut self) {
        self.prev = 0.0;
        self.cur = 0.0;
        self.have = false;
        self.t = 0.0;
    }
    /// Push one input sample; call `emit` for each produced output sample.
    #[inline]
    fn push<F: FnMut(f32)>(&mut self, x: f32, mut emit: F) {
        if self.passthrough {
            emit(x);
            return;
        }
        // Shift the two-sample interpolation window.
        self.prev = self.cur;
        self.cur = x;
        if !self.have {
            self.have = true;
            self.t = 0.0;
            return;
        }
        // Output samples fall at input-time increments of 1/ratio. `t` is the position of the
        // next output within [prev, cur) in units of input samples (0..1).
        let dt = 1.0 / self.ratio;
        while self.t < 1.0 {
            let frac = self.t;
            emit(self.prev + (self.cur - self.prev) * frac);
            self.t += dt;
        }
        self.t -= 1.0;
    }
}

/// Streaming **pull**-based linear resampler (48 k→host). Per host output sample it advances a
/// fractional read phase by `step = 48000/host` and consumes that many 48 k samples from a
/// FIFO, linearly interpolating. Pass-through when the ratio is ~1 (exact, no added delay).
#[derive(Clone)]
struct PullResampler {
    step: f32, // 48 k input samples consumed per host output sample
    passthrough: bool,
    prev: f32,
    cur: f32,
    phase: f32,
    primed: bool,
}
impl PullResampler {
    fn new(out_rate: f32) -> Self {
        let step = INTERNAL_RATE / out_rate;
        Self {
            step,
            passthrough: (step - 1.0).abs() < 1.0e-6,
            prev: 0.0,
            cur: 0.0,
            phase: 0.0,
            primed: false,
        }
    }
    fn reset(&mut self) {
        self.prev = 0.0;
        self.cur = 0.0;
        self.phase = 0.0;
        self.primed = false;
    }
    /// Produce one host output sample, consuming 48 k samples from `wet` as needed.
    #[inline]
    fn pull(&mut self, wet: &mut SampleFifo) -> f32 {
        if self.passthrough {
            return wet.pop();
        }
        if !self.primed {
            self.prev = wet.pop();
            self.cur = wet.pop();
            self.primed = true;
        }
        while self.phase >= 1.0 {
            self.prev = self.cur;
            self.cur = if wet.len() > 0 { wet.pop() } else { self.cur };
            self.phase -= 1.0;
        }
        let y = self.prev + (self.cur - self.prev) * self.phase;
        self.phase += self.step;
        y
    }
}

/// Feedback delay line with independent read/peek and write (needed because the regen loop
/// reads a *past* decoded sample before this sample's decoded value exists).
#[derive(Clone)]
struct FbDelay {
    buf: Vec<f32>,
    pos: usize,
    delay: usize,
}
impl FbDelay {
    fn new(max_delay: usize) -> Self {
        Self {
            buf: vec![0.0; max_delay + 1],
            pos: 0,
            delay: 1,
        }
    }
    fn set_delay(&mut self, d: usize) {
        self.delay = d.clamp(1, self.buf.len() - 1);
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }
    #[inline]
    fn read(&self) -> f32 {
        let len = self.buf.len();
        self.buf[(self.pos + len - self.delay) % len]
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Per-channel framed Opus codec engine (48 k).
// ---------------------------------------------------------------------------

/// One channel's worth of: bandwidth LP → crunch → 20 ms framing → Opus enc → packet-loss
/// drop (zero-fill + crossfade PLC) → Opus dec → regen feedback wrap. Streams one 48 k
/// sample in / one 48 k sample out.
pub struct ChannelCodec {
    enc: OpusEncoder,
    dec: OpusDecoder,
    mode: Mode,

    // Pre-codec.
    lp: suite_core::dsp::Svf,
    crunch: Crunch,

    // Framing FIFOs.
    in_fifo: SampleFifo,   // accumulates pre-codec samples until a FRAME is ready
    out_fifo: SampleFifo,  // decoded samples awaiting the streaming reader (seeded w/ latency)

    // Scratch (preallocated; codec calls happen inside permit_alloc in the plugin).
    frame_in: Vec<f32>,
    frame_out: Vec<f32>,
    packet: Vec<u8>,

    // Packet-loss + PLC.
    rng: u32,
    last_tail: f32, // last emitted decoded sample, for the drop crossfade

    // Regen feedback.
    fb_delay: FbDelay,
    dc: DcBlocker,

    // Cached to detect changes.
    cur_bitrate: i32,
    cur_fec: bool,
    cur_loss: i32,
}

impl ChannelCodec {
    pub fn new() -> Self {
        let mode = Mode::Music;
        let enc = OpusEncoder::new(INTERNAL_RATE as i32, 1, mode.application())
            .expect("opus encoder init");
        let dec = OpusDecoder::new(INTERNAL_RATE as i32, 1).expect("opus decoder init");
        let mut lp = suite_core::dsp::Svf::new();
        lp.set(20_000.0, 0.707, INTERNAL_RATE);

        let mut out_fifo = SampleFifo::new(FRAME * 3 + WET_GUARD + 16);
        // Seeded with one frame so the streaming reader never underflows while the first frame
        // accumulates. The codec's own content delay adds on top of this (measured by test).
        out_fifo.clear_to_zeros(FRAME);

        Self {
            enc,
            dec,
            mode,
            lp,
            crunch: Crunch::default(),
            in_fifo: SampleFifo::new(FRAME * 2 + 16),
            out_fifo,
            frame_in: vec![0.0; FRAME],
            frame_out: vec![0.0; FRAME],
            packet: vec![0u8; 4096],
            rng: 0x9E37_79B9,
            last_tail: 0.0,
            fb_delay: FbDelay::new(MAX_REGEN_DELAY),
            dc: DcBlocker::default(),
            cur_bitrate: -1,
            cur_fec: false,
            cur_loss: -1,
        }
    }

    /// The channel's 48 k input→output latency (samples): one frame of buffering + the codec's
    /// algorithmic content delay + the output-FIFO guard.
    pub fn latency_48k() -> usize {
        FRAME + CODEC_CONTENT_DELAY + WET_GUARD
    }

    pub fn reset(&mut self) {
        // Recreate the codec state (there is no public reset on opus-rs; a fresh pair is the
        // clean equivalent and only happens on host reset, never per block).
        self.enc = OpusEncoder::new(INTERNAL_RATE as i32, 1, self.mode.application())
            .expect("opus encoder init");
        self.dec = OpusDecoder::new(INTERNAL_RATE as i32, 1).expect("opus decoder init");
        self.cur_bitrate = -1;
        self.cur_fec = false;
        self.cur_loss = -1;
        self.crunch.reset();
        self.dc.reset();
        self.fb_delay.reset();
        self.in_fifo.clear_to_zeros(0);
        self.out_fifo.clear_to_zeros(FRAME);
        self.last_tail = 0.0;
    }

    fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            self.mode = mode;
            // Application is fixed at construction in opus-rs → rebuild the codec pair.
            self.enc = OpusEncoder::new(INTERNAL_RATE as i32, 1, mode.application())
                .expect("opus encoder init");
            self.dec = OpusDecoder::new(INTERNAL_RATE as i32, 1).expect("opus decoder init");
            self.cur_bitrate = -1;
            self.cur_fec = false;
            self.cur_loss = -1;
        }
    }

    #[inline]
    fn next_rand(&mut self) -> f32 {
        // xorshift32 → [0,1)
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f32 / u32::MAX as f32
    }

    /// Apply per-frame encoder config that changed.
    fn configure(&mut self, s: &Settings) {
        self.set_mode(s.mode);
        let br = (s.bitrate_kbps.clamp(6.0, 128.0) * 1000.0) as i32;
        if br != self.cur_bitrate {
            self.enc.bitrate_bps = br;
            self.enc.use_cbr = true;
            self.enc.complexity = 5;
            self.cur_bitrate = br;
        }
        if s.fec != self.cur_fec {
            self.enc.use_inband_fec = s.fec;
            self.cur_fec = s.fec;
        }
        let loss = s.loss_pct.clamp(0.0, 100.0) as i32;
        if loss != self.cur_loss {
            // Let the encoder adapt its redundancy to the expected loss.
            self.enc.packet_loss_perc = loss;
            self.cur_loss = loss;
        }
        self.lp.set(
            s.bandwidth.cutoff_hz().min(INTERNAL_RATE * 0.49),
            0.707,
            INTERNAL_RATE,
        );
        self.fb_delay
            .set_delay(((s.regen_delay_ms.clamp(0.0, 500.0) * 0.001 * INTERNAL_RATE) as usize).max(1));
    }

    /// Encode+decode one accumulated frame in `frame_in` into `frame_out`. Allocates
    /// internally (opus-rs) — the plugin wraps `process_sample` in `permit_alloc`; tests call
    /// directly. `loss_pct` may drop the packet (PLC zero-fill handled by the caller).
    fn run_frame(&mut self, loss_pct: f32) {
        let drop = self.next_rand() * 100.0 < loss_pct;
        let n = self.enc.encode(&self.frame_in, FRAME, &mut self.packet).unwrap_or(0);
        if drop || n < 2 {
            // Packet loss / encode failure: zero-fill with a short fade from the last tail so
            // the dropout is click-free at entry. (opus-rs decode() rejects empty input, so we
            // synthesise the concealment rather than calling a decoder PLC path.)
            let fade = 128.min(FRAME); // ~2.7 ms
            for i in 0..FRAME {
                let g = if i < fade {
                    1.0 - i as f32 / fade as f32
                } else {
                    0.0
                };
                self.frame_out[i] = self.last_tail * g;
            }
        } else {
            let got = self
                .dec
                .decode(&self.packet[..n], FRAME, &mut self.frame_out)
                .unwrap_or(0);
            // Short crossfade at re-entry from a previous drop is implicit: the decoder output
            // starts near zero after concealment. Fill any short decode with zeros.
            for v in self.frame_out.iter_mut().skip(got) {
                *v = 0.0;
            }
        }
    }

    /// Stream one 48 k input sample through the whole channel; returns one 48 k output sample.
    /// `permit`-wrapped by the caller in the real-time path.
    #[inline]
    pub fn process_sample(&mut self, x_in: f32, s: &Settings) -> f32 {
        // Regen feedback: a past decoded sample, DC-blocked and soft-limited (PRD §3).
        let fb = if s.regen_amount > 1.0e-4 {
            let d = self.dc.process(self.fb_delay.read());
            s.regen_amount.clamp(0.0, 0.95) * d.tanh()
        } else {
            0.0
        };
        // Bandwidth low-pass → crunch → into the frame accumulator.
        let pre = self.lp.process(x_in + fb).lp;
        let pre = self.crunch.process(pre, s.crunch);
        self.in_fifo.push(pre);

        // Drain a full frame if one is ready.
        if self.in_fifo.len() >= FRAME {
            for i in 0..FRAME {
                self.frame_in[i] = self.in_fifo.pop();
            }
            self.run_frame(s.loss_pct);
            for i in 0..FRAME {
                self.out_fifo.push(self.frame_out[i]);
            }
        }

        let d_out = self.out_fifo.pop();
        self.last_tail = d_out;
        // Feed decoded output into the feedback delay for future regeneration.
        self.fb_delay.write(d_out);
        d_out
    }
}

impl Default for ChannelCodec {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// WIRE stereo core
// ---------------------------------------------------------------------------

/// The full WIRE processor: host-rate SRC ⇄ 48 k, two channel codecs, width, latency-aligned
/// dry/wet mix, output trim.
pub struct WireCore {
    // SRC: host→48 k (per channel) and 48 k→host (per channel).
    up_l: PushResampler,
    up_r: PushResampler,
    down_l: PullResampler,
    down_r: PullResampler,
    // 48 k wet FIFOs feeding the down-samplers (seeded with the guard so pull never starves).
    wet_l: SampleFifo,
    wet_r: SampleFifo,

    ch: [ChannelCodec; 2],

    // Dry-path delay (host rate) to align with the reported wet latency (PDC).
    dry_l: suite_core::dsp::DelayLine,
    dry_r: suite_core::dsp::DelayLine,

    // Smoothed host-domain controls.
    width_s: OnePole,
    mix_s: OnePole,
    out_s: OnePole,
    primed: bool,

    reported_latency: usize,
}

impl WireCore {
    pub fn new(host_rate: f32) -> Self {
        let sr = if host_rate > 0.0 { host_rate } else { 48_000.0 };
        let reported = Self::compute_latency(sr);
        let mut wet_l = SampleFifo::new(FRAME * 3 + WET_GUARD + 16);
        let mut wet_r = SampleFifo::new(FRAME * 3 + WET_GUARD + 16);
        wet_l.clear_to_zeros(WET_GUARD);
        wet_r.clear_to_zeros(WET_GUARD);

        let mut core = WireCore {
            up_l: PushResampler::new(sr, INTERNAL_RATE),
            up_r: PushResampler::new(sr, INTERNAL_RATE),
            down_l: PullResampler::new(sr),
            down_r: PullResampler::new(sr),
            wet_l,
            wet_r,
            ch: [ChannelCodec::new(), ChannelCodec::new()],
            dry_l: suite_core::dsp::DelayLine::new(reported.max(1)),
            dry_r: suite_core::dsp::DelayLine::new(reported.max(1)),
            width_s: OnePole::new(),
            mix_s: OnePole::new(),
            out_s: OnePole::new(),
            primed: false,
            reported_latency: reported,
        };
        core.dry_l.set_delay(reported);
        core.dry_r.set_delay(reported);
        let t = 15.0;
        core.width_s.set_time(t, sr);
        core.mix_s.set_time(t, sr);
        core.out_s.set_time(t, sr);
        core
    }

    /// Reported host-rate latency: the channel's 48 k latency scaled to the host rate (SRC is
    /// linear, ≤1 sample each way and folded into the guard). Exact at a 48 k host rate.
    fn compute_latency(host_rate: f32) -> usize {
        let l48 = ChannelCodec::latency_48k() as f32;
        ((l48 * host_rate / INTERNAL_RATE).round() as usize).max(1)
    }

    pub fn latency_samples(&self) -> u32 {
        self.reported_latency as u32
    }

    pub fn reset(&mut self) {
        self.up_l.reset();
        self.up_r.reset();
        self.down_l.reset();
        self.down_r.reset();
        self.wet_l.clear_to_zeros(WET_GUARD);
        self.wet_r.clear_to_zeros(WET_GUARD);
        for c in self.ch.iter_mut() {
            c.reset();
        }
        self.dry_l.reset();
        self.dry_r.reset();
        self.dry_l.set_delay(self.reported_latency);
        self.dry_r.set_delay(self.reported_latency);
        self.primed = false;
    }

    fn prime(&mut self, s: &Settings) {
        self.width_s.reset(s.width);
        self.mix_s.reset(s.mix.clamp(0.0, 1.0));
        self.out_s.reset(s.out_db);
        self.primed = true;
    }

    /// Latch per-block config on both channel codecs. Call once per block before the loop.
    pub fn configure(&mut self, s: &Settings) {
        if !self.primed {
            self.prime(s);
        }
        self.ch[0].configure(s);
        self.ch[1].configure(s);
    }

    /// Process one stereo host-rate sample. In the plugin this whole call is inside
    /// `permit_alloc`; the offline harness calls it directly.
    #[inline]
    pub fn process_sample(&mut self, l_in: f32, r_in: f32, s: &Settings) -> (f32, f32) {
        // Up-sample host→48 k, run each 48 k sample through the codec, buffer the wet result.
        let ch = &mut self.ch;
        let wet_l = &mut self.wet_l;
        let wet_r = &mut self.wet_r;
        self.up_l.push(l_in, |x48| wet_l.push(ch[0].process_sample(x48, s)));
        self.up_r.push(r_in, |x48| wet_r.push(ch[1].process_sample(x48, s)));

        // Down-sample 48 k→host: pull exactly one host sample from each wet FIFO.
        let out_l = self.down_l.pull(wet_l);
        let out_r = self.down_r.pull(wet_r);

        // Stereo width (M/S).
        let width = self.width_s.process(s.width.clamp(0.0, 2.0));
        let mid = 0.5 * (out_l + out_r);
        let side = 0.5 * (out_l - out_r) * width;
        let wet_l_out = mid + side;
        let wet_r_out = mid - side;

        // Latency-aligned dry/wet mix + output trim.
        let mix = self.mix_s.process(s.mix.clamp(0.0, 1.0));
        let out_lin = db_to_lin(self.out_s.process(s.out_db));
        let dry_l = self.dry_l.process(l_in);
        let dry_r = self.dry_r.process(r_in);
        let o_l = ((dry_l * (1.0 - mix) + wet_l_out * mix) * out_lin).clamp(-0.999, 0.999);
        let o_r = ((dry_r * (1.0 - mix) + wet_r_out * mix) * out_lin).clamp(-0.999, 0.999);
        (o_l, o_r)
    }

    /// Convenience mono renderer for the offline harness (feeds `main` as both channels,
    /// returns the left output in place).
    pub fn process_mono(&mut self, main: &mut [f32], s: &Settings) {
        self.configure(s);
        for m in main.iter_mut() {
            let (l, _r) = self.process_sample(*m, *m, s);
            *m = l;
        }
    }

    /// Stereo renderer from a mono input.
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
