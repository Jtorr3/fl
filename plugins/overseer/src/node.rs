//! OVERSEER **Node** — a per-track channel strip whose meters and key params live in a
//! shared bus [`Slot`] so the Master can watch and remote-control it.
//!
//! Chain (SPECS): input meter → 4-band EQ → feed-forward compressor → tanh saturation →
//! M/S width → output trim → output meter. A global dry/wet `mix` gives a clean null at 0.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use suite_core::classify::FeatureExtractor;
use suite_core::dsp::{DelayLine, OnePole, Oversampler2x};
use suite_core::loudness::LoudnessMeter;

use crate::bus::{Slot, NUM_OVERRIDES, OVR_DRIVE, OVR_RATIO, OVR_THRESHOLD, OVR_TRIM, OVR_WIDTH};
use crate::eq::{EqSettings, FourBandEq};

/// Control sub-block length for block-advanced smoothing of the EQ-gain and compressor
/// coefficients (continuous scalar params — drive/width/trim/mix — are smoothed per
/// sample). 32 samples ≈ 0.67 ms at 48 kHz.
const CTRL_CHUNK: usize = 32;

/// Param smoothing time constant (ms) for the core-internal smoothers. Matches the
/// intent of the params' `SmoothingStyle::Linear(20.0)`.
const SMOOTH_MS: f32 = 20.0;

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
#[inline]
fn lin_to_db(x: f32) -> f32 {
    if x <= 1.0e-9 {
        f32::NEG_INFINITY
    } else {
        20.0 * x.log10()
    }
}
#[inline]
fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}
#[inline]
pub fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

/// Shared meter block for a Node's own GUI (the Master reads the coarser trio off the slot).
#[derive(Debug)]
pub struct NodeMeters {
    pub in_peak: AtomicU32,
    pub in_rms: AtomicU32,
    pub out_peak: AtomicU32,
    pub out_rms: AtomicU32,
    pub lufs_m: AtomicU32,
    pub gr_db: AtomicU32,
    /// Live sample rate (f32 bits) — published for the GUI's LEARN capture sizing.
    pub sr: AtomicU32,
}

impl Default for NodeMeters {
    fn default() -> Self {
        let ninf = f32::NEG_INFINITY.to_bits();
        Self {
            in_peak: AtomicU32::new(ninf),
            in_rms: AtomicU32::new(ninf),
            out_peak: AtomicU32::new(ninf),
            out_rms: AtomicU32::new(ninf),
            lufs_m: AtomicU32::new(ninf),
            gr_db: AtomicU32::new(0.0f32.to_bits()),
            sr: AtomicU32::new(48_000.0f32.to_bits()),
        }
    }
}

/// Full Node parameter snapshot (plain units).
#[derive(Clone, Copy, Debug)]
pub struct NodeSettings {
    pub eq: EqSettings,
    pub comp_threshold: f32,
    pub comp_ratio: f32,
    pub comp_knee: f32,
    pub comp_attack: f32,
    pub comp_release: f32,
    pub comp_makeup: f32,
    pub drive_db: f32,
    pub width: f32,
    pub trim_db: f32,
    pub mix: f32,
}

impl Default for NodeSettings {
    fn default() -> Self {
        Self {
            eq: EqSettings::default(),
            comp_threshold: -18.0,
            comp_ratio: 2.5,
            comp_knee: 6.0,
            comp_attack: 12.0,
            comp_release: 140.0,
            comp_makeup: 0.0,
            drive_db: 0.0,
            width: 1.0,
            trim_db: 0.0,
            mix: 1.0,
        }
    }
}

use crate::dynamics::Compressor;

/// The Node DSP core. Holds the shared bus slot and the per-GUI meter block.
pub struct NodeCore {
    fs: f32,
    eq: [FourBandEq; 2],
    comp: Compressor,
    // Level-preserving tanh saturation, per channel, oversampled 2x so its harmonics do
    // not alias (MINOR 5 — the only formerly-un-oversampled nonlinearity in the suite).
    sat_os: [Oversampler2x; 2],
    // Momentary-only meter: the Node only reads momentary LUFS, so it must not pay the
    // integrated-gating cost in the audio callback (MINOR 6).
    lufs: LoudnessMeter,
    slot: Arc<Slot>,
    pub meters: Arc<NodeMeters>,
    // OVERSEER-ENRICH: rolling audio-feature extractor tapping the Node's own INPUT. Feeds
    // the bus feature/type publishing + the LEARN capture. Allocation-free at block rate;
    // never alters the audio.
    extractor: FeatureExtractor,
    // Dry-path latency compensation for the oversampled saturation stage (keeps partial
    // mix free of comb filtering); reported to the host as latency.
    dry_delay: [DelayLine; 2],
    latency: usize,

    // --- Param smoothing (MAJOR 3) --------------------------------------------------
    // Continuous scalar params applied per sample → per-sample smoothed (zipper-free).
    sm_drive: OnePole,
    sm_width: OnePole,
    sm_trim: OnePole,
    sm_mix: OnePole,
    // EQ gains + comp threshold/makeup → smoothed too, but consumed via block-advanced
    // coefficient recompute every CTRL_CHUNK samples.
    sm_low_gain: OnePole,
    sm_b1_gain: OnePole,
    sm_b2_gain: OnePole,
    sm_high_gain: OnePole,
    sm_threshold: OnePole,
    sm_makeup: OnePole,
    /// Resolved (post-override) target settings for this block.
    tgt: NodeSettings,
    /// Snap smoothers to their targets on the first configure after (re)construction/reset.
    primed: bool,
}

impl NodeCore {
    pub fn new(fs: f32, slot: Arc<Slot>, meters: Arc<NodeMeters>) -> Self {
        // Empirically-measured saturation-oversampler group delay for exact dry alignment.
        let latency = Oversampler2x::measure_group_delay();
        store_f32(&meters.sr, fs);
        let sm = || {
            let mut p = OnePole::new();
            p.set_time(SMOOTH_MS, fs);
            p
        };
        Self {
            fs,
            eq: [FourBandEq::new(), FourBandEq::new()],
            comp: Compressor::new(fs),
            sat_os: [Oversampler2x::new(), Oversampler2x::new()],
            lufs: LoudnessMeter::new_momentary(fs, 2),
            slot,
            meters,
            extractor: FeatureExtractor::new(fs),
            dry_delay: [DelayLine::new(latency), DelayLine::new(latency)],
            latency,
            sm_drive: sm(),
            sm_width: sm(),
            sm_trim: sm(),
            sm_mix: sm(),
            sm_low_gain: sm(),
            sm_b1_gain: sm(),
            sm_b2_gain: sm(),
            sm_high_gain: sm(),
            sm_threshold: sm(),
            sm_makeup: sm(),
            tgt: NodeSettings::default(),
            primed: false,
        }
    }

    /// Reported plugin latency (samples) — the saturation oversampler group delay.
    pub fn latency_samples(&self) -> u32 {
        self.latency as u32
    }

    pub fn slot(&self) -> &Arc<Slot> {
        &self.slot
    }

    pub fn reset(&mut self) {
        for e in self.eq.iter_mut() {
            e.reset();
        }
        self.comp.reset();
        for o in self.sat_os.iter_mut() {
            o.reset();
        }
        for d in self.dry_delay.iter_mut() {
            d.reset();
        }
        self.lufs.reset();
        self.extractor.reset();
        self.primed = false;
    }

    fn snap_smoothers(&mut self) {
        self.sm_drive.reset(self.tgt.drive_db);
        self.sm_width.reset(self.tgt.width);
        self.sm_trim.reset(self.tgt.trim_db);
        self.sm_mix.reset(self.tgt.mix.clamp(0.0, 1.0));
        self.sm_low_gain.reset(self.tgt.eq.low_gain);
        self.sm_b1_gain.reset(self.tgt.eq.b1_gain);
        self.sm_b2_gain.reset(self.tgt.eq.b2_gain);
        self.sm_high_gain.reset(self.tgt.eq.high_gain);
        self.sm_threshold.reset(self.tgt.comp_threshold);
        self.sm_makeup.reset(self.tgt.comp_makeup);
    }

    /// Resolve overrides, latch the per-block targets, and mirror the effective key params
    /// into the slot. Call once per block before [`process_block`](Self::process_block).
    /// The actual EQ/comp coefficients and scalar gains are smoothed inside `process_block`.
    pub fn configure(&mut self, s: &NodeSettings) {
        // Resolve the five overridable params against the bus (Master override vs local).
        let threshold = self.slot.effective(OVR_THRESHOLD, s.comp_threshold);
        let ratio = self.slot.effective(OVR_RATIO, s.comp_ratio);
        let drive = self.slot.effective(OVR_DRIVE, s.drive_db);
        let width = self.slot.effective(OVR_WIDTH, s.width);
        let trim = self.slot.effective(OVR_TRIM, s.trim_db);

        self.tgt = *s;
        self.tgt.comp_threshold = threshold;
        self.tgt.comp_ratio = ratio;
        self.tgt.drive_db = drive;
        self.tgt.width = width;
        self.tgt.trim_db = trim;

        if !self.primed {
            self.snap_smoothers();
            self.primed = true;
        }

        // Mirror the effective values for the Master GUI.
        let mut mir = [0.0f32; NUM_OVERRIDES];
        mir[OVR_THRESHOLD] = threshold;
        mir[OVR_RATIO] = ratio;
        mir[OVR_DRIVE] = drive;
        mir[OVR_WIDTH] = width;
        mir[OVR_TRIM] = trim;
        for (i, v) in mir.iter().enumerate() {
            self.slot.set_mirror(i, *v);
        }
    }

    /// Recompute the EQ + compressor coefficients from the currently-smoothed gain /
    /// threshold / makeup values (called at each CTRL_CHUNK boundary).
    fn config_coeffs_from_smoothers(&mut self) {
        let eqs = EqSettings {
            low_freq: self.tgt.eq.low_freq,
            low_gain: self.sm_low_gain.value(),
            b1_freq: self.tgt.eq.b1_freq,
            b1_gain: self.sm_b1_gain.value(),
            b1_q: self.tgt.eq.b1_q,
            b2_freq: self.tgt.eq.b2_freq,
            b2_gain: self.sm_b2_gain.value(),
            b2_q: self.tgt.eq.b2_q,
            high_freq: self.tgt.eq.high_freq,
            high_gain: self.sm_high_gain.value(),
        };
        self.eq[0].configure(&eqs, self.fs);
        self.eq[1].configure(&eqs, self.fs);
        self.comp.configure(
            self.sm_threshold.value(),
            self.tgt.comp_ratio,
            self.tgt.comp_knee,
            self.tgt.comp_attack,
            self.tgt.comp_release,
            self.sm_makeup.value(),
            self.fs,
        );
    }

    /// Process a stereo block in place. `configure` must have been called for this block.
    pub fn process_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());

        // OVERSEER-ENRICH: tap the INPUT (before the DSP mutates the buffers) for feature
        // extraction + LEARN. All lock-free / allocation-free; the audio is untouched.
        let req = self.slot.take_learn_req();
        if req > 0 {
            self.extractor.begin_capture(req);
        }
        self.extractor.process_block(&l[..n], &r[..n]);
        self.slot.set_features(&self.extractor.summary());
        self.slot.set_capturing(self.extractor.capturing());
        self.slot.set_capture_prog(self.extractor.capture_progress());
        if let Some(cap) = self.extractor.take_capture() {
            self.slot.publish_learn_result(&cap);
        }

        let mut in_peak = 0.0f32;
        let mut in_sq = 0.0f32;
        let mut out_peak = 0.0f32;
        let mut out_sq = 0.0f32;
        let mut gr_min = 0.0f32;

        let mut i = 0;
        while i < n {
            let end = (i + CTRL_CHUNK).min(n);
            // Block-advanced coefficient smoothing for EQ gains + comp threshold/makeup.
            self.config_coeffs_from_smoothers();

            for k in i..end {
                // Per-sample smoothing of the directly-applied scalar params.
                let drive = db_to_lin(self.sm_drive.process(self.tgt.drive_db));
                let width = self.sm_width.process(self.tgt.width);
                let trim = db_to_lin(self.sm_trim.process(self.tgt.trim_db));
                let mix = self.sm_mix.process(self.tgt.mix).clamp(0.0, 1.0);
                // Advance the coefficient smoothers per sample so the next chunk's recompute
                // sees continuous progress (32-sample-granular EQ/comp smoothing).
                self.sm_low_gain.process(self.tgt.eq.low_gain);
                self.sm_b1_gain.process(self.tgt.eq.b1_gain);
                self.sm_b2_gain.process(self.tgt.eq.b2_gain);
                self.sm_high_gain.process(self.tgt.eq.high_gain);
                self.sm_threshold.process(self.tgt.comp_threshold);
                self.sm_makeup.process(self.tgt.comp_makeup);

                let dry_l = l[k];
                let dry_r = r[k];
                in_peak = in_peak.max(dry_l.abs()).max(dry_r.abs());
                in_sq += dry_l * dry_l + dry_r * dry_r;

                // Latency-compensated dry (aligned with the oversampled wet path).
                let dd_l = self.dry_delay[0].process(dry_l);
                let dd_r = self.dry_delay[1].process(dry_r);

                // EQ (per channel).
                let mut wl = self.eq[0].process(dry_l);
                let mut wr = self.eq[1].process(dry_r);

                // Compressor (channel-linked detector), applied to both.
                let det = wl.abs().max(wr.abs());
                let g = self.comp.process(det);
                wl *= g;
                wr *= g;
                gr_min = gr_min.min(self.comp.gain_reduction_db());

                // Saturation — 2x oversampled, level-preserving tanh (drive smoothed).
                wl = self.sat_os[0].process(wl, |x| (x * drive).tanh() / drive);
                wr = self.sat_os[1].process(wr, |x| (x * drive).tanh() / drive);

                // M/S width.
                let mid = (wl + wr) * 0.5;
                let side = (wl - wr) * 0.5 * width;
                wl = mid + side;
                wr = mid - side;

                // Output trim.
                wl *= trim;
                wr *= trim;

                // Dry/wet mix (clean null at mix=0 against the delayed dry).
                let ol = mix * wl + (1.0 - mix) * dd_l;
                let or = mix * wr + (1.0 - mix) * dd_r;

                out_peak = out_peak.max(ol.abs()).max(or.abs());
                out_sq += ol * ol + or * or;
                self.lufs.push(&[ol, or]);

                l[k] = ol;
                r[k] = or;
            }
            i = end;
        }

        if n > 0 {
            let in_rms = (in_sq / (2 * n) as f32).sqrt();
            let out_rms = (out_sq / (2 * n) as f32).sqrt();
            let lufs_m = self.lufs.momentary_lufs();
            store_f32(&self.meters.in_peak, lin_to_db(in_peak));
            store_f32(&self.meters.in_rms, lin_to_db(in_rms));
            store_f32(&self.meters.out_peak, lin_to_db(out_peak));
            store_f32(&self.meters.out_rms, lin_to_db(out_rms));
            store_f32(&self.meters.lufs_m, lufs_m);
            store_f32(&self.meters.gr_db, gr_min);
            // Publish the coarse trio + heartbeat to the shared slot for the Master.
            self.slot
                .set_meters(lin_to_db(out_peak), lin_to_db(out_rms), lufs_m);
            self.slot.beat();
        }
    }

    /// Mono convenience for the offline harness (duplicates the channel).
    pub fn process_mono(&mut self, buf: &mut [f32], s: &NodeSettings) {
        self.configure(s);
        let mut r = buf.to_vec();
        let mut l = buf.to_vec();
        self.process_block(&mut l, &mut r);
        buf.copy_from_slice(&l);
    }
}
