//! OVERSEER **Node** — a per-track channel strip whose meters and key params live in a
//! shared bus [`Slot`] so the Master can watch and remote-control it.
//!
//! Chain (SPECS): input meter → 4-band EQ → feed-forward compressor → tanh saturation →
//! M/S width → output trim → output meter. A global dry/wet `mix` gives a clean null at 0.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use suite_core::loudness::LoudnessMeter;

use crate::bus::{Slot, NUM_OVERRIDES, OVR_DRIVE, OVR_RATIO, OVR_THRESHOLD, OVR_TRIM, OVR_WIDTH};
use crate::eq::{EqSettings, FourBandEq};

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
    lufs: LoudnessMeter,
    slot: Arc<Slot>,
    pub meters: Arc<NodeMeters>,
    // Effective (post-override) block values.
    eff_drive_lin: f32,
    eff_width: f32,
    eff_trim_lin: f32,
    mix: f32,
}

impl NodeCore {
    pub fn new(fs: f32, slot: Arc<Slot>, meters: Arc<NodeMeters>) -> Self {
        Self {
            fs,
            eq: [FourBandEq::new(), FourBandEq::new()],
            comp: Compressor::new(fs),
            lufs: LoudnessMeter::new(fs, 2),
            slot,
            meters,
            eff_drive_lin: 1.0,
            eff_width: 1.0,
            eff_trim_lin: 1.0,
            mix: 1.0,
        }
    }

    pub fn slot(&self) -> &Arc<Slot> {
        &self.slot
    }

    pub fn reset(&mut self) {
        for e in self.eq.iter_mut() {
            e.reset();
        }
        self.comp.reset();
        self.lufs.reset();
    }

    /// Resolve overrides, configure the EQ/compressor, and mirror the effective key params
    /// into the slot. Call once per block before [`process_block`](Self::process_block).
    pub fn configure(&mut self, s: &NodeSettings) {
        // Resolve the five overridable params against the bus (Master override vs local).
        let threshold = self.slot.effective(OVR_THRESHOLD, s.comp_threshold);
        let ratio = self.slot.effective(OVR_RATIO, s.comp_ratio);
        let drive = self.slot.effective(OVR_DRIVE, s.drive_db);
        let width = self.slot.effective(OVR_WIDTH, s.width);
        let trim = self.slot.effective(OVR_TRIM, s.trim_db);

        self.eq[0].configure(&s.eq, self.fs);
        self.eq[1].configure(&s.eq, self.fs);
        self.comp.configure(
            threshold,
            ratio,
            s.comp_knee,
            s.comp_attack,
            s.comp_release,
            s.comp_makeup,
            self.fs,
        );
        self.eff_drive_lin = db_to_lin(drive);
        self.eff_width = width;
        self.eff_trim_lin = db_to_lin(trim);
        self.mix = s.mix.clamp(0.0, 1.0);

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

    #[inline]
    fn sat(&self, x: f32) -> f32 {
        // Unity for small signals, tanh saturation for large ones (level-preserving).
        let pre = self.eff_drive_lin;
        (x * pre).tanh() / pre
    }

    /// Process a stereo block in place. `configure` must have been called for this block.
    pub fn process_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let mut in_peak = 0.0f32;
        let mut in_sq = 0.0f32;
        let mut out_peak = 0.0f32;
        let mut out_sq = 0.0f32;
        let mut gr_min = 0.0f32;

        for i in 0..n {
            let dry_l = l[i];
            let dry_r = r[i];
            in_peak = in_peak.max(dry_l.abs()).max(dry_r.abs());
            in_sq += dry_l * dry_l + dry_r * dry_r;

            // EQ (per channel).
            let mut wl = self.eq[0].process(dry_l);
            let mut wr = self.eq[1].process(dry_r);

            // Compressor (channel-linked detector), applied to both.
            let det = wl.abs().max(wr.abs());
            let g = self.comp.process(det);
            wl *= g;
            wr *= g;
            gr_min = gr_min.min(self.comp.gain_reduction_db());

            // Saturation.
            wl = self.sat(wl);
            wr = self.sat(wr);

            // M/S width.
            let mid = (wl + wr) * 0.5;
            let side = (wl - wr) * 0.5 * self.eff_width;
            wl = mid + side;
            wr = mid - side;

            // Output trim.
            wl *= self.eff_trim_lin;
            wr *= self.eff_trim_lin;

            // Dry/wet mix (clean null at mix=0).
            let ol = self.mix * wl + (1.0 - self.mix) * dry_l;
            let or = self.mix * wr + (1.0 - self.mix) * dry_r;

            out_peak = out_peak.max(ol.abs()).max(or.abs());
            out_sq += ol * ol + or * or;
            self.lufs.push(&[ol, or]);

            l[i] = ol;
            r[i] = or;
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
