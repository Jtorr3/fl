//! `suite_core::fdn` — an 8-line feedback delay network (FDN) reverb core, shared across
//! the suite. MURMUR (stochastic reverb) is the first consumer; UNDERTOW, SEANCE, and
//! CHAMBER reuse it (PRD §2 API rule — this module is public suite-core surface).
//!
//! ```text
//!         ┌─────────────────── 8×8 Householder feedback (lossless) ──────────────┐
//!         │                                                                       │
//! in ─ input diffusion (allpass chain) ─┬─▶ delay₀ ─ damp₀ ─ ×g₀ ─┐              │
//!        (L→even lines, R→odd lines)     ├─▶ delay₁ ─ damp₁ ─ ×g₁ ─┤─ H ─(feedback)┘
//!                                        │      ⋮                  │
//!                                        └─▶ delay₇ ─ damp₇ ─ ×g₇ ─┘
//!  out: even line taps → L, odd line taps → R
//! ```
//!
//! Design points:
//! - **8 delay lines**, each a ring buffer preallocated at `max_delay_samples` so
//!   [`set_delays`](Fdn8::set_delays) changes the active length at runtime with **no
//!   allocation** (safe under nih-plug's `assert_process_allocs`).
//! - **8×8 Householder feedback** `H = I − (2/N)·vvᵀ/(vᵀv)` with `v = ones`. For `v = ones`
//!   this reduces to the O(N) "sum trick": `(Hx)_i = x_i − (2/N)·Σx`. `H` is a real
//!   orthogonal reflection (`‖Hx‖ = ‖x‖`), so the loop is **lossless** apart from the
//!   per-line decay gains and damping — decay is controlled, energy is bounded.
//! - **Per-line one-pole damping** (color tilt): a one-pole lowpass in each feedback path,
//!   with a per-line spread so the tail has color. [`set_damping`](Fdn8::set_damping).
//! - **Per-line decay gain** `g_i = 10^(−3·Lᵢ/(RT60·sr))` where `Lᵢ` is line `i`'s length:
//!   every line hits −60 dB after exactly `RT60` seconds regardless of its length.
//! - **Input diffusion**: a 4-allpass chain per channel, coefficient settable at runtime
//!   ([`set_diffusion`](Fdn8::set_diffusion)).
//! - **Stereo in/out taps**: input drives even lines from L and odd lines from R; output
//!   sums even line taps to L and odd line taps to R (with fixed sign patterns for
//!   decorrelation).
//!
//! Pure Rust, API-agnostic; shared verbatim between the real-time `process` path and the
//! offline harness tests.

/// Number of delay lines / feedback matrix dimension.
pub const N: usize = 8;

/// Fixed input-injection sign pattern (decorrelates the lines).
const IN_SIGN: [f32; N] = [1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];
/// Fixed output-tap sign pattern.
const OUT_SIGN: [f32; N] = [1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0];

/// Fixed allpass lengths (samples) for the two input-diffusion chains. Small, mutually
/// prime-ish values chosen for smooth diffusion without obvious flutter.
const DIFF_LEN_L: [usize; 4] = [142, 107, 379, 277];
const DIFF_LEN_R: [usize; 4] = [165, 131, 397, 251];

/// Maximum per-line decay gain. Capped just below 1.0 so that even a huge RT60 (freeze)
/// can never make the lossless loop grow without bound — the orthogonal matrix already
/// guarantees `‖Hx‖ = ‖x‖`, this makes the product strictly contractive.
const MAX_GAIN: f32 = 0.99995;

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

/// A variable-length ring delay preallocated to a maximum length. `read` returns the sample
/// `len` samples old; `write` stores the new input and advances. Allocation-free.
#[derive(Clone)]
struct VarDelay {
    buf: Vec<f32>,
    pos: usize,
    len: usize,
}

impl VarDelay {
    fn new(max_len: usize) -> Self {
        let cap = max_len.max(1);
        Self {
            buf: vec![0.0; cap],
            pos: 0,
            len: cap,
        }
    }

    fn set_len(&mut self, len: usize) {
        self.len = len.clamp(1, self.buf.len());
    }

    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }

    /// Read the sample written `len` samples ago (before this step's write).
    #[inline]
    fn read(&self) -> f32 {
        let cap = self.buf.len();
        let r = (self.pos + cap - self.len) % cap;
        self.buf[r]
    }

    /// Read at a **fractional** delay `d` (samples ago), linearly interpolated. Used by the
    /// optional delay-modulation path so the FDN's comb nulls sweep and its discrete modes
    /// smear into a dense band (anti-metallic). Clamped to `[1, cap-2]`. At an integer `d`
    /// (fraction 0) this returns **exactly** [`read`](Self::read) with `len == d`, so the
    /// zero-depth modulation path is bit-identical to the unmodulated core.
    #[inline]
    fn read_frac(&self, d: f32) -> f32 {
        let cap = self.buf.len();
        let d = d.clamp(1.0, (cap.saturating_sub(2)).max(1) as f32);
        let d0 = d.floor();
        let frac = d - d0;
        let d0 = d0 as usize;
        let a = self.buf[(self.pos + cap - d0) % cap]; // d0 samples ago
        let b = self.buf[(self.pos + cap - d0 - 1) % cap]; // d0+1 samples ago (older)
        a + frac * (b - a)
    }

    /// Write `x` at the head and advance.
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }
}

/// One-pole lowpass damping filter: `y = (1−a)·x + a·y⁻¹`. `a → 0` is bright (no damping),
/// larger `a` rolls off highs (darker tail). DC gain is exactly 1, so low frequencies decay
/// at the rate set by the line gain and only highs are extra-damped.
#[derive(Clone, Copy, Default)]
struct Damp {
    z: f32,
    a: f32,
}

impl Damp {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.z = (1.0 - self.a) * x + self.a * self.z;
        self.z
    }
    fn reset(&mut self) {
        self.z = 0.0;
    }
}

/// Schroeder allpass (fixed length, settable coefficient) for input diffusion.
#[derive(Clone)]
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
    g: f32,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            g: 0.5,
        }
    }

    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let d = self.buf[self.pos];
        let out = -self.g * x + d;
        self.buf[self.pos] = x + self.g * out;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Fdn8
// ---------------------------------------------------------------------------

/// An 8-line Householder FDN reverb. See the module docs for the topology.
///
/// Typical use:
/// ```no_run
/// # use suite_core::fdn::Fdn8;
/// let sr = 48_000.0;
/// let mut fdn = Fdn8::new((0.2 * sr) as usize, sr);
/// fdn.set_delays(&[1557, 1617, 1491, 1422, 1277, 1356, 1188, 1116]);
/// fdn.set_rt60(2.5);
/// fdn.set_damping(0.2);
/// fdn.set_diffusion(0.5);
/// let (l, r) = fdn.process(0.0, 0.0);
/// # let _ = (l, r);
/// ```
#[derive(Clone)]
pub struct Fdn8 {
    sr: f32,
    max_delay: usize,
    lines: Vec<VarDelay>,
    damp: [Damp; N],
    gain: [f32; N],
    delay_len: [usize; N],
    rt60: f32,
    diff_l: Vec<Allpass>,
    diff_r: Vec<Allpass>,
    /// Final output scaling so a dense late field stays well below 0 dBFS.
    out_gain: f32,
    /// Optional per-line delay modulation (anti-metallic). `mod_depth` samples of slow
    /// sinusoidal delay-length wobble, decorrelated per line, sweeps the comb nulls so the
    /// tail's discrete modes smear into a dense band. `0` (default) = classic static FDN,
    /// bit-identical to the pre-modulation core (see [`read_frac`](VarDelay::read_frac)).
    mod_depth: f32,
    mod_phase: [f32; N],
    mod_inc: [f32; N],
}

impl Fdn8 {
    /// Create an FDN whose lines can each be up to `max_delay_samples` long, at sample rate
    /// `sr`. All lines start at the maximum length (call [`set_delays`](Self::set_delays)).
    pub fn new(max_delay_samples: usize, sr: f32) -> Self {
        let max_delay = max_delay_samples.max(1);
        let sr = sr.max(1.0);
        let lines = (0..N).map(|_| VarDelay::new(max_delay)).collect();
        let diff_l = DIFF_LEN_L.iter().map(|&l| Allpass::new(l)).collect();
        let diff_r = DIFF_LEN_R.iter().map(|&l| Allpass::new(l)).collect();
        let mut me = Self {
            sr,
            max_delay,
            lines,
            damp: [Damp::default(); N],
            gain: [0.0; N],
            delay_len: [max_delay; N],
            rt60: 2.0,
            diff_l,
            diff_r,
            out_gain: 0.4,
            mod_depth: 0.0,
            // Staggered start phases so the eight lines never move in lockstep.
            mod_phase: {
                let mut p = [0.0f32; N];
                for (i, ph) in p.iter_mut().enumerate() {
                    *ph = core::f32::consts::TAU * (i as f32) / (N as f32);
                }
                p
            },
            mod_inc: [0.0; N],
        };
        me.set_damping(0.2);
        me.set_diffusion(0.5);
        me.recompute_gains();
        me
    }

    /// The maximum per-line delay (samples) this instance was allocated for.
    pub fn max_delay(&self) -> usize {
        self.max_delay
    }

    /// Set the eight line lengths (samples), each clamped to `[1, max_delay]`. Recomputes the
    /// per-line decay gains (they depend on length). Does not clear the delay buffers, so the
    /// change is click-masked by a crossfade at the call site (MURMUR's ping-pong).
    pub fn set_delays(&mut self, delays: &[usize; N]) {
        for i in 0..N {
            let d = delays[i].clamp(1, self.max_delay);
            self.delay_len[i] = d;
            self.lines[i].set_len(d);
        }
        // Re-zero the modulation LFOs so the first sample read at the new lengths carries no
        // modulation offset (eff == delay_len). A delay-length change is already a
        // discontinuity the call site masks (crossfade / silence); this keeps the modulation
        // from *adding* to that step. The LFOs re-diverge over the following cycle to resume
        // smearing during the static hold. No-op when modulation is disabled.
        self.mod_phase = [0.0; N];
        self.recompute_gains();
    }

    /// Set the reverb time (seconds). Recomputes per-line gains so every line reaches −60 dB
    /// after `rt60` seconds. Very large values approach a lossless (freeze) tail.
    pub fn set_rt60(&mut self, rt60_s: f32) {
        self.rt60 = rt60_s.max(1.0e-3);
        self.recompute_gains();
    }

    /// Set the damping tilt in `[−1, 1]`: −1 = bright (no HF damping), 0 = mild, +1 = dark.
    /// Applied per line with a small spread so the tail has color.
    pub fn set_damping(&mut self, tilt: f32) {
        let t = tilt.clamp(-1.0, 1.0);
        // base_a: −1 → 0.0 (bright), 0 → 0.30 (mild), +1 → 0.60 (dark).
        let base_a = (0.30 + 0.30 * t).clamp(0.0, 0.9);
        for i in 0..N {
            // Per-line color spread of ±0.08 across the lines.
            let spread = (i as f32 / (N as f32 - 1.0) - 0.5) * 0.16;
            self.damp[i].a = (base_a + spread).clamp(0.0, 0.95);
        }
    }

    /// Set the input-diffusion allpass coefficient (both channels), clamped to `[0, 0.9]`.
    /// Higher = denser, more smeared onset.
    pub fn set_diffusion(&mut self, coeff: f32) {
        let g = coeff.clamp(0.0, 0.9);
        for ap in self.diff_l.iter_mut().chain(self.diff_r.iter_mut()) {
            ap.g = g;
        }
    }

    /// Overall output gain applied to the summed taps (default 0.4).
    pub fn set_output_gain(&mut self, g: f32) {
        self.out_gain = g;
    }

    /// Enable **delay-length modulation** to break up the metallic ringing of a static FDN.
    /// `depth_samples` of slow sinusoidal wobble is applied to each line's read pointer with
    /// a per-line rate spread around `rate_hz`, so the comb-null frequencies sweep and the
    /// tail's discrete modes smear into a dense, diffuse band (the SOUND-PASS anti-metallic
    /// fix for MURMUR / CHAMBER / SEANCE). `depth_samples == 0` (the default) restores the
    /// exact static core — consumers that never call this (e.g. UNDERTOW) are unchanged.
    /// Small depths (~0.1–0.3 ms) are inaudible as vibrato yet enough to diffuse the modes.
    pub fn set_modulation(&mut self, depth_samples: f32, rate_hz: f32) {
        self.mod_depth = depth_samples.max(0.0);
        let base = (rate_hz.max(0.0) * core::f32::consts::TAU) / self.sr;
        for i in 0..N {
            // Per-line rate spread of ±~30 % so lines decorrelate rather than detune as one.
            let mult = 1.0 + 0.3 * ((i as f32 / (N as f32 - 1.0)) - 0.5);
            self.mod_inc[i] = base * mult;
        }
    }

    fn recompute_gains(&mut self) {
        for i in 0..N {
            let len = self.delay_len[i] as f32;
            let g = 10.0_f32.powf(-3.0 * len / (self.rt60 * self.sr));
            self.gain[i] = g.min(MAX_GAIN);
        }
    }

    /// Clear all delay lines, damping states, and diffusion buffers.
    pub fn reset(&mut self) {
        for l in self.lines.iter_mut() {
            l.reset();
        }
        for d in self.damp.iter_mut() {
            d.reset();
        }
        for ap in self.diff_l.iter_mut().chain(self.diff_r.iter_mut()) {
            ap.reset();
        }
    }

    /// Process one stereo sample pair, returning the reverberated stereo pair.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        // Input diffusion (per channel).
        let mut dl = l;
        for ap in self.diff_l.iter_mut() {
            dl = ap.process(dl);
        }
        let mut dr = r;
        for ap in self.diff_r.iter_mut() {
            dr = ap.process(dr);
        }

        // Read current line outputs (the output taps). With modulation enabled each line is
        // read at a slowly-swept fractional delay so the comb nulls move and the modes smear;
        // at `mod_depth == 0` this is bit-identical to the plain integer `read()`.
        let mut s = [0.0f32; N];
        if self.mod_depth > 0.0 {
            for i in 0..N {
                let lfo = self.mod_phase[i].sin();
                let eff = self.delay_len[i] as f32 + self.mod_depth * lfo;
                s[i] = self.lines[i].read_frac(eff);
                self.mod_phase[i] += self.mod_inc[i];
                if self.mod_phase[i] >= core::f32::consts::TAU {
                    self.mod_phase[i] -= core::f32::consts::TAU;
                }
            }
        } else {
            for i in 0..N {
                s[i] = self.lines[i].read();
            }
        }

        // Damping + decay → feedback pre-vector v.
        let mut v = [0.0f32; N];
        for i in 0..N {
            let damped = self.damp[i].process(s[i]);
            v[i] = self.gain[i] * damped;
        }

        // Householder feedback via the O(N) sum trick: (Hv)_i = v_i − (2/N)·Σv.
        let sum: f32 = v.iter().sum();
        let c = (2.0 / N as f32) * sum;

        // Write input injection + feedback into each line.
        for i in 0..N {
            let inj = if i % 2 == 0 { dl } else { dr } * IN_SIGN[i];
            self.lines[i].write(inj + (v[i] - c));
        }

        // Output taps: even lines → L, odd lines → R.
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;
        for i in 0..N {
            if i % 2 == 0 {
                out_l += s[i] * OUT_SIGN[i];
            } else {
                out_r += s[i] * OUT_SIGN[i];
            }
        }
        (out_l * self.out_gain, out_r * self.out_gain)
    }
}

// ---------------------------------------------------------------------------
// RT60 measurement (test/harness helper — reused by MURMUR's done-bar test)
// ---------------------------------------------------------------------------

/// Estimate RT60 (seconds) from an impulse response via Schroeder backward energy
/// integration, fitting the decay slope between −5 dB and −35 dB. Returns `None` if the
/// tail never falls 35 dB (IR too short) or the fit window is degenerate.
pub fn measure_rt60(ir: &[f32], sr: f32) -> Option<f32> {
    let n = ir.len();
    if n < 16 || sr <= 0.0 {
        return None;
    }
    // Backward cumulative energy (Schroeder EDC).
    let mut edc = vec![0.0f64; n];
    let mut acc = 0.0f64;
    for i in (0..n).rev() {
        acc += (ir[i] as f64) * (ir[i] as f64);
        edc[i] = acc;
    }
    let e0 = edc[0];
    if e0 <= 0.0 {
        return None;
    }
    let db = |i: usize| 10.0 * (edc[i] / e0).log10();

    // First index at or below −5 dB and −35 dB.
    let mut i5 = None;
    let mut i35 = None;
    for i in 0..n {
        let d = db(i);
        if i5.is_none() && d <= -5.0 {
            i5 = Some(i);
        }
        if d <= -35.0 {
            i35 = Some(i);
            break;
        }
    }
    let (i5, i35) = (i5?, i35?);
    if i35 <= i5 {
        return None;
    }
    // 30 dB drop over (i35 − i5) samples → RT60 = 60 dB worth of that slope.
    let t = (i35 - i5) as f32 / sr;
    Some(2.0 * t) // 60 / 30 = 2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nominal, mutually-prime-ish delay set (samples @ 48 kHz) for a mid-size room.
    fn nominal_delays() -> [usize; N] {
        [1557, 1617, 1491, 1423, 1277, 1357, 1188, 1117]
    }

    fn impulse_response(rt60: f32, damping: f32, sr: f32, len: usize) -> Vec<f32> {
        let mut fdn = Fdn8::new((0.1 * sr) as usize, sr);
        fdn.set_delays(&nominal_delays());
        fdn.set_rt60(rt60);
        fdn.set_damping(damping);
        fdn.set_diffusion(0.5);
        let mut out = Vec::with_capacity(len);
        for n in 0..len {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let (a, b) = fdn.process(x, x);
            // Sum to mono for envelope/energy analysis.
            out.push(0.5 * (a + b));
        }
        out
    }

    #[test]
    fn impulse_decays_monotonically_in_windows() {
        let sr = 48_000.0;
        let ir = impulse_response(2.0, 0.2, sr, (sr * 4.0) as usize);
        assert!(ir.iter().all(|v| v.is_finite()));
        // Windowed RMS should trend downward over the tail.
        let win = (sr * 0.2) as usize;
        let rms = |s: &[f32]| (s.iter().map(|&v| v * v).sum::<f32>() / s.len() as f32).sqrt();
        let early = rms(&ir[win..2 * win]);
        let mid = rms(&ir[5 * win..6 * win]);
        let late = rms(&ir[9 * win..10 * win]);
        assert!(early > mid, "early {early} not > mid {mid}");
        assert!(mid > late, "mid {mid} not > late {late}");
        assert!(late < early * 0.5, "tail did not decay enough");
    }

    #[test]
    fn measured_rt60_within_25_percent_at_two_settings() {
        let sr = 48_000.0;
        for &target in &[1.0f32, 3.0f32] {
            // Light damping so the broadband decay is dominated by the line gains.
            let ir = impulse_response(target, 0.0, sr, (sr * target * 2.5) as usize);
            let measured = measure_rt60(&ir, sr).expect("RT60 measurable");
            let err = (measured - target).abs() / target;
            assert!(
                err <= 0.25,
                "RT60 target {target}s measured {measured}s (err {:.1}%)",
                err * 100.0
            );
        }
    }

    #[test]
    fn energy_is_bounded() {
        let sr = 48_000.0;
        // Drive with white noise for 1 s, then silence; output must never blow up.
        let mut fdn = Fdn8::new((0.1 * sr) as usize, sr);
        fdn.set_delays(&nominal_delays());
        fdn.set_rt60(5.0);
        fdn.set_damping(0.2);
        let mut seed = 0x1234_5678u32;
        let mut rng = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let mut peak = 0.0f32;
        for n in 0..(sr as usize * 3) {
            let x = if n < sr as usize { rng() } else { 0.0 };
            let (a, b) = fdn.process(x, x);
            assert!(a.is_finite() && b.is_finite());
            peak = peak.max(a.abs()).max(b.abs());
        }
        assert!(peak < 4.0, "FDN output grew too large: peak {peak}");
    }

    #[test]
    fn reset_clears_the_tail() {
        let sr = 48_000.0;
        let mut fdn = Fdn8::new((0.1 * sr) as usize, sr);
        fdn.set_delays(&nominal_delays());
        fdn.set_rt60(3.0);
        // Excite, then reset.
        for _ in 0..1000 {
            fdn.process(1.0, 1.0);
        }
        fdn.reset();
        // After reset, silent input must give exactly silent output.
        for _ in 0..2000 {
            let (a, b) = fdn.process(0.0, 0.0);
            assert_eq!(a, 0.0);
            assert_eq!(b, 0.0);
        }
    }

    #[test]
    fn longer_rt60_decays_slower() {
        let sr = 48_000.0;
        let short = measure_rt60(&impulse_response(0.8, 0.0, sr, (sr * 2.5) as usize), sr).unwrap();
        let long = measure_rt60(&impulse_response(4.0, 0.0, sr, (sr * 10.0) as usize), sr).unwrap();
        assert!(long > short * 2.0, "RT60 ordering wrong: {short} vs {long}");
    }
}
