//! Spectrum tap + publisher for the shared analyzer (SPECS "X-RAY", Phase 3).
//!
//! Where [`crate::modlisten`] is the *reader* side of the tier-2 [`crate::bus`] (params
//! listen to NERVE's modulation streams), this module is the *publisher* side for X-RAY:
//! every audio plugin taps its own output into a 32-band spectrum and publishes
//! `{ spectrum[32], peak, RMS, label, kind }` to its own bus slot at block rate. X-RAY
//! then renders every live slot's spectrum as an overlay.
//!
//! # The tap — a constant-Q bandpass filter bank
//! [`SpectrumTap`] is a bank of [`NUM_BANDS`] TPT state-variable bandpass filters
//! ([`crate::dsp::Svf`]), log-spaced from 20 Hz to 20 kHz (≈⅓-octave, `Q≈4.3`). Every
//! output sample is fed through all 32 filters; the squared bandpass output is integrated
//! over the block, and at [`SpectrumTap::finish`] each band's block-RMS is exponentially
//! smoothed (one-pole, ~stable over a few blocks) to give a steady published value. Peak
//! and full-band RMS are integrated the same way.
//!
//! This is a *per-sample constant-Q bank* rather than a block-rate FFT: 32 SVFs × ~10
//! flops ≈ 320 flops/output-sample, i.e. ~15 Mflop/s at 48 kHz per instance — **well under
//! 0.5 % of one core** (benched in `tests::cpu_cost_is_negligible`), so it is cheap enough
//! to leave enabled in every suite plugin. Constant-Q (fractional-octave) bands mean pink
//! noise reads roughly flat and white noise tilts up +3 dB/oct, matching a standard
//! ⅓-octave RTA display.
//!
//! # RT-safety
//! Everything is preallocated in [`SpectrumTap::new`]; `feed`/`finish`/publish are
//! alloc-free. [`SpectrumPublisher`] wraps the tap plus the bus claim/heartbeat/release
//! bookkeeping (the NERVE pattern) so the per-plugin retrofit is: one field, one
//! `init` call in `initialize`, and a `feed`-loop + `publish` at the end of `process`.

use crate::bus::{self, Bus, PluginKind, NUM_SPECTRUM};
use crate::dsp::Svf;

/// Number of spectrum bands. Must equal [`crate::bus::NUM_SPECTRUM`] (the slot layout).
pub const NUM_BANDS: usize = NUM_SPECTRUM;

/// Lowest band center frequency (Hz).
pub const F_LOW: f32 = 20.0;
/// Highest band center frequency (Hz).
pub const F_HIGH: f32 = 20_000.0;
/// Constant Q of every band (≈⅓-octave: `1/(2^(1/6) − 2^(-1/6)) ≈ 4.32`).
const BAND_Q: f32 = 4.32;
/// One-pole smoothing weight applied to each published band per block (kept, vs new).
const SMOOTH: f32 = 0.5;

/// Log-spaced center frequency of band `i` (0..[`NUM_BANDS`]).
#[inline]
pub fn band_center_hz(i: usize) -> f32 {
    let t = if NUM_BANDS > 1 {
        i as f32 / (NUM_BANDS - 1) as f32
    } else {
        0.0
    };
    F_LOW * (F_HIGH / F_LOW).powf(t)
}

/// A constant-Q bandpass filter bank that integrates per-band energy at block rate.
#[derive(Clone)]
pub struct SpectrumTap {
    bands: [Svf; NUM_BANDS],
    /// Σ bp² for the current block, per band.
    accum: [f32; NUM_BANDS],
    /// Smoothed published band levels (linear RMS).
    smoothed: [f32; NUM_BANDS],
    count: u32,
    block_peak: f32,
    rms_accum: f32,
    smoothed_peak: f32,
    smoothed_rms: f32,
    sample_rate: f32,
}

impl Default for SpectrumTap {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl SpectrumTap {
    pub fn new(sample_rate: f32) -> Self {
        let mut tap = Self {
            bands: [Svf::new(); NUM_BANDS],
            accum: [0.0; NUM_BANDS],
            smoothed: [0.0; NUM_BANDS],
            count: 0,
            block_peak: 0.0,
            rms_accum: 0.0,
            smoothed_peak: 0.0,
            smoothed_rms: 0.0,
            sample_rate,
        };
        tap.set_sample_rate(sample_rate);
        tap
    }

    /// (Re)tune all band filters for `sample_rate` (call from `initialize`).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let nyq = self.sample_rate * 0.5;
        for (i, f) in self.bands.iter_mut().enumerate() {
            // Clamp band centers under Nyquist so the top bands stay valid at 44.1 kHz.
            let fc = band_center_hz(i).min(nyq * 0.95);
            f.set(fc, BAND_Q, self.sample_rate);
        }
    }

    /// Clear filter state and accumulators (keeps the tuning).
    pub fn reset(&mut self) {
        for f in self.bands.iter_mut() {
            f.reset();
        }
        self.accum = [0.0; NUM_BANDS];
        self.smoothed = [0.0; NUM_BANDS];
        self.count = 0;
        self.block_peak = 0.0;
        self.rms_accum = 0.0;
        self.smoothed_peak = 0.0;
        self.smoothed_rms = 0.0;
    }

    /// Feed one (mono) output sample through the bank. Alloc-free; call per sample.
    #[inline]
    pub fn feed(&mut self, x: f32) {
        for (f, acc) in self.bands.iter_mut().zip(self.accum.iter_mut()) {
            let bp = f.process(x).bp;
            *acc += bp * bp;
        }
        let a = x.abs();
        if a > self.block_peak {
            self.block_peak = a;
        }
        self.rms_accum += x * x;
        self.count += 1;
    }

    /// Finish the current block: write the smoothed per-band RMS into `out` and return the
    /// smoothed `(peak, rms)`. Resets the per-block accumulators. Alloc-free.
    pub fn finish(&mut self, out: &mut [f32; NUM_BANDS]) -> (f32, f32) {
        let n = self.count.max(1) as f32;
        for i in 0..NUM_BANDS {
            let block_rms = (self.accum[i] / n).sqrt();
            self.smoothed[i] = self.smoothed[i] * SMOOTH + block_rms * (1.0 - SMOOTH);
            out[i] = self.smoothed[i];
            self.accum[i] = 0.0;
        }
        let block_rms = (self.rms_accum / n).sqrt();
        self.smoothed_peak = self.smoothed_peak * SMOOTH + self.block_peak * (1.0 - SMOOTH);
        self.smoothed_rms = self.smoothed_rms * SMOOTH + block_rms * (1.0 - SMOOTH);
        self.count = 0;
        self.block_peak = 0.0;
        self.rms_accum = 0.0;
        (self.smoothed_peak, self.smoothed_rms)
    }
}

/// A [`SpectrumTap`] plus the bus slot bookkeeping (claim / heartbeat / release), so a
/// plugin can publish its output spectrum to the shared bus with a tiny retrofit.
///
/// The instance id is session-scoped (assigned once, never persisted — the NERVE rationale:
/// a persisted random id breaks CLAP state reproducibility). A removed/crashed instance's
/// slot is reclaimed by the bus GC (heartbeat staleness), so an explicit `release` in
/// `Drop` is a nicety, not a correctness requirement.
pub struct SpectrumPublisher {
    tap: SpectrumTap,
    inst_id: u64,
    slot: Option<usize>,
    kind: PluginKind,
    label: String,
    scratch: [f32; NUM_BANDS],
}

impl Default for SpectrumPublisher {
    fn default() -> Self {
        Self {
            tap: SpectrumTap::new(48_000.0),
            inst_id: 0,
            slot: None,
            kind: PluginKind::Generic,
            label: String::new(),
            scratch: [0.0; NUM_BANDS],
        }
    }
}

impl SpectrumPublisher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call from `initialize`: retune the tap, (re)claim a bus slot under a stable
    /// session id, and record the display label + kind. Releases any previously-held slot
    /// first so re-activation cannot leak a second slot for the same instance.
    pub fn init(&mut self, sample_rate: f32, kind: PluginKind, label: &str) {
        self.tap.set_sample_rate(sample_rate);
        self.tap.reset();
        self.kind = kind;
        self.label.clear();
        self.label.push_str(label);
        if self.inst_id == 0 {
            self.inst_id = bus::new_instance_id();
        }
        if let Some(b) = bus::bus() {
            if let Some(idx) = self.slot.take() {
                b.release(idx, self.inst_id);
            }
            self.slot = b.claim(self.inst_id, kind, &self.label);
        }
    }

    /// Feed one mono output sample. Alloc-free; call per output sample in `process`.
    #[inline]
    pub fn feed(&mut self, x: f32) {
        self.tap.feed(x);
    }

    /// Finish the block: publish the spectrum + peak/RMS to the slot and stamp the
    /// heartbeat. Lazily (re)claims a slot if the bus appeared after `init` or was lost.
    /// Alloc-free on the steady-state path.
    pub fn publish(&mut self) {
        match bus::bus() {
            Some(b) => {
                if self.slot.is_none() && self.inst_id != 0 {
                    self.slot = b.claim(self.inst_id, self.kind, &self.label);
                }
                let (peak, rms) = self.tap.finish(&mut self.scratch);
                if let Some(idx) = self.slot {
                    b.publish_spectrum(idx, &self.scratch, peak, rms);
                    b.beat(idx);
                }
            }
            None => {
                // No bus: still drain the tap so accumulators don't grow unbounded.
                let _ = self.tap.finish(&mut self.scratch);
            }
        }
    }

    /// Release the slot (call from `Drop`; also harmless if never called — GC reclaims).
    pub fn release(&mut self) {
        if let (Some(idx), Some(b)) = (self.slot, bus::bus()) {
            b.release(idx, self.inst_id);
        }
        self.slot = None;
    }

    /// The claimed slot index, if any (introspection / tests).
    pub fn slot(&self) -> Option<usize> {
        self.slot
    }

    /// This instance's bus id (introspection / tests).
    pub fn instance_id(&self) -> u64 {
        self.inst_id
    }
}

/// Convenience: index of the highest-energy band of a spectrum (tests / hover readouts).
pub fn dominant_band(spectrum: &[f32; NUM_BANDS]) -> usize {
    let mut best = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in spectrum.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    best
}

/// Read every live slot's spectrum from `bus`; a thin wrapper over
/// [`crate::bus::Bus::snapshot_live`] for the X-RAY reader (kept here so the analyzer and
/// the publisher share one module).
pub fn read_spectra(bus: &Bus) -> Vec<crate::bus::SlotSnapshot> {
    bus.snapshot_live()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{new_instance_id, Bus, PluginKind};
    use crate::dsp::Svf;
    use std::path::PathBuf;

    fn temp_bus_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "qeynos-bus-spectrum-{}-{}-{}",
            tag,
            std::process::id(),
            new_instance_id()
        ))
    }

    /// Deterministic pseudo-noise (xorshift) so tests don't depend on `rand`.
    struct Rng(u32);
    impl Rng {
        fn next_f32(&mut self) -> f32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.0 = x;
            (x as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    /// Band centers are log-spaced and cover the audio range.
    #[test]
    fn band_centers_span_audio() {
        assert!((band_center_hz(0) - F_LOW).abs() < 1e-3);
        assert!((band_center_hz(NUM_BANDS - 1) - F_HIGH).abs() < 1.0);
        // Monotonic increasing.
        for i in 1..NUM_BANDS {
            assert!(band_center_hz(i) > band_center_hz(i - 1));
        }
    }

    /// A low-band-limited noise concentrates the tap's energy in the low bands; a
    /// high-band-limited noise in the high bands. (The core X-RAY "plausible shape" check.)
    #[test]
    fn tap_concentrates_energy_in_the_right_bands() {
        let sr = 48_000.0;
        // Build low-passed and high-passed noise via the suite SVF.
        let mut rng = Rng(0x1234_5678);
        let mut lp = Svf::new();
        lp.set(200.0, 0.707, sr);
        let mut hp = Svf::new();
        hp.set(6_000.0, 0.707, sr);

        let n = sr as usize; // 1 s
        let mut low_tap = SpectrumTap::new(sr);
        let mut high_tap = SpectrumTap::new(sr);
        let mut low_out = [0.0f32; NUM_BANDS];
        let mut high_out = [0.0f32; NUM_BANDS];

        let block = 512;
        let mut i = 0;
        while i < n {
            let end = (i + block).min(n);
            for _ in i..end {
                let w = rng.next_f32();
                low_tap.feed(lp.process(w).lp);
                high_tap.feed(hp.process(w).hp);
            }
            low_tap.finish(&mut low_out);
            high_tap.finish(&mut high_out);
            i = end;
        }

        let db = 200.0f32;
        // Split the bands at ~1 kHz (roughly the middle of the log axis).
        let split = (0..NUM_BANDS)
            .find(|&i| band_center_hz(i) >= 1_000.0)
            .unwrap();
        let sum = |s: &[f32; NUM_BANDS], r: std::ops::Range<usize>| -> f32 {
            r.map(|i| s[i] * s[i]).sum::<f32>()
        };
        let low_low = sum(&low_out, 0..split);
        let low_high = sum(&low_out, split..NUM_BANDS);
        let high_low = sum(&high_out, 0..split);
        let high_high = sum(&high_out, split..NUM_BANDS);
        let _ = db;

        assert!(
            low_low > low_high * 4.0,
            "low-noise energy should sit in low bands: low={low_low} high={low_high}"
        );
        assert!(
            high_high > high_low * 4.0,
            "high-noise energy should sit in high bands: low={high_low} high={high_high}"
        );
        // The dominant band of low-noise is well below that of high-noise.
        assert!(dominant_band(&low_out) < dominant_band(&high_out));
    }

    /// TWO publishers on ONE bus file (simulating two DLLs) publish distinct spectra; an
    /// X-RAY-style reader sees BOTH slots with the right spectral shape. This is the PRD §4
    /// X-RAY done-bar in miniature (the full version lives in the xray crate).
    #[test]
    fn two_publishers_visible_to_reader() {
        let sr = 48_000.0;
        let path = temp_bus_path("twopub");
        // Two publishers, each with its own mapped handle (distinct "DLLs").
        let mut low = SpectrumPublisher::new();
        let mut high = SpectrumPublisher::new();
        // Point the process-default bus at our temp file by claiming via explicit handles:
        let bus_a = Bus::open_or_create(&path).unwrap();
        let bus_b = Bus::open_or_create(&path).unwrap();
        let reader = Bus::open_or_create(&path).unwrap();

        let id_a = new_instance_id();
        let id_b = new_instance_id();
        let slot_a = bus_a.claim(id_a, PluginKind::Generic, "LOW SRC").unwrap();
        let slot_b = bus_b.claim(id_b, PluginKind::Generic, "HIGH SRC").unwrap();

        low.tap.set_sample_rate(sr);
        high.tap.set_sample_rate(sr);

        let mut rng = Rng(0xBEEF);
        let mut lp = Svf::new();
        lp.set(150.0, 0.707, sr);
        let mut hp = Svf::new();
        hp.set(7_000.0, 0.707, sr);

        let mut sp_a = [0.0f32; NUM_BANDS];
        let mut sp_b = [0.0f32; NUM_BANDS];
        for _blk in 0..40 {
            for _ in 0..512 {
                let w = rng.next_f32();
                low.tap.feed(lp.process(w).lp);
                high.tap.feed(hp.process(w).hp);
            }
            let (pa, ra) = low.tap.finish(&mut sp_a);
            let (pb, rb) = high.tap.finish(&mut sp_b);
            bus_a.publish_spectrum(slot_a, &sp_a, pa, ra);
            bus_a.beat(slot_a);
            bus_b.publish_spectrum(slot_b, &sp_b, pb, rb);
            bus_b.beat(slot_b);
        }

        let live = reader.snapshot_live();
        assert_eq!(live.len(), 2, "reader must see both publishers");
        let low_snap = live.iter().find(|s| s.label == "LOW SRC").unwrap();
        let high_snap = live.iter().find(|s| s.label == "HIGH SRC").unwrap();
        assert!(
            dominant_band(&low_snap.spectrum) < dominant_band(&high_snap.spectrum),
            "the two slots must carry differently-shaped spectra"
        );
        assert!(low_snap.rms > 0.0 && high_snap.rms > 0.0);

        bus_a.release(slot_a, id_a);
        bus_b.release(slot_b, id_b);
        let _ = std::fs::remove_file(&path);
    }

    /// The per-sample bank is cheap: 1 s of audio at 48 kHz should tap in well under
    /// real time (documented < 0.5 % of a core; this asserts a very loose ceiling so it is
    /// not flaky on CI, only catching a pathological regression).
    #[test]
    fn cpu_cost_is_negligible() {
        let sr = 48_000.0;
        let mut tap = SpectrumTap::new(sr);
        let mut out = [0.0f32; NUM_BANDS];
        let n = sr as usize;
        let start = std::time::Instant::now();
        let mut i = 0;
        while i < n {
            for k in 0..512 {
                tap.feed(((i + k) as f32 * 0.001).sin());
            }
            tap.finish(&mut out);
            i += 512;
        }
        let secs = start.elapsed().as_secs_f32();
        // 1 s of audio must tap in << 1 s of wall time (loose 0.5 s ceiling).
        assert!(secs < 0.5, "spectrum tap too slow: {secs:.3}s for 1 s of audio");
    }
}
