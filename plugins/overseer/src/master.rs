//! OVERSEER **Master** — the bus processor: 4-band EQ → 3-band multiband compressor
//! (LR4 splits) → lookahead limiter → LUFS meter (BS.1770, `suite_core::loudness`).
//!
//! Reports the limiter's lookahead as latency; the dry path is delayed to match so the
//! dry/wet `mix` nulls cleanly at 0. The Master GUI additionally reads the live Node slots
//! off the bus and can write overrides into them (handled in `lib.rs`).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use suite_core::dsp::Oversampler4x;
use suite_core::loudness::LoudnessMeter;

use crate::dynamics::{Limiter, MultibandComp};
use crate::eq::{EqSettings, FourBandEq};
use crate::node::load_f32;

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

/// Shared meter block for the Master GUI.
#[derive(Debug)]
pub struct MasterMeters {
    pub out_peak: AtomicU32,
    pub true_peak: AtomicU32,
    pub out_rms: AtomicU32,
    pub lufs_m: AtomicU32,
    pub lufs_s: AtomicU32,
    pub lufs_i: AtomicU32,
    pub band_gr: [AtomicU32; 3],
    pub limiter_gr: AtomicU32,
}

impl Default for MasterMeters {
    fn default() -> Self {
        let ninf = f32::NEG_INFINITY.to_bits();
        Self {
            out_peak: AtomicU32::new(ninf),
            true_peak: AtomicU32::new(ninf),
            out_rms: AtomicU32::new(ninf),
            lufs_m: AtomicU32::new(ninf),
            lufs_s: AtomicU32::new(ninf),
            lufs_i: AtomicU32::new(ninf),
            band_gr: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            limiter_gr: AtomicU32::new(0.0f32.to_bits()),
        }
    }
}

impl MasterMeters {
    pub fn lufs_integrated(&self) -> f32 {
        load_f32(&self.lufs_i)
    }
}

/// Per-band multiband-compressor settings.
#[derive(Clone, Copy, Debug)]
pub struct BandComp {
    pub threshold: f32,
    pub ratio: f32,
    pub makeup: f32,
}

/// Full Master parameter snapshot (plain units).
#[derive(Clone, Copy, Debug)]
pub struct MasterSettings {
    pub eq: EqSettings,
    pub xo_low: f32,
    pub xo_high: f32,
    pub bands: [BandComp; 3],
    pub comp_knee: f32,
    pub comp_attack: f32,
    pub comp_release: f32,
    pub ceiling_db: f32,
    pub limiter_release: f32,
    pub mix: f32,
}

impl Default for MasterSettings {
    fn default() -> Self {
        Self {
            eq: EqSettings::default(),
            xo_low: 180.0,
            xo_high: 2500.0,
            bands: [
                BandComp {
                    threshold: -24.0,
                    ratio: 2.0,
                    makeup: 0.0,
                },
                BandComp {
                    threshold: -22.0,
                    ratio: 2.0,
                    makeup: 0.0,
                },
                BandComp {
                    threshold: -20.0,
                    ratio: 2.0,
                    makeup: 0.0,
                },
            ],
            comp_knee: 6.0,
            comp_attack: 15.0,
            comp_release: 160.0,
            ceiling_db: -1.0,
            limiter_release: 100.0,
            mix: 1.0,
        }
    }
}

/// The Master DSP core.
pub struct MasterCore {
    fs: f32,
    eq: [FourBandEq; 2],
    mb: [MultibandComp; 2],
    limiter: Limiter,
    lufs: LoudnessMeter,
    tp_os: [Oversampler4x; 2],
    // Latency-matched dry path for the null at mix=0.
    latency: usize,
    dry_l: Vec<f32>,
    dry_r: Vec<f32>,
    dpos: usize,
    mix: f32,
    pub meters: Arc<MasterMeters>,
}

impl MasterCore {
    pub fn new(fs: f32, meters: Arc<MasterMeters>) -> Self {
        let limiter = Limiter::new(fs);
        let latency = limiter.lookahead_samples();
        Self {
            fs,
            eq: [FourBandEq::new(), FourBandEq::new()],
            mb: [MultibandComp::new(fs), MultibandComp::new(fs)],
            limiter,
            lufs: LoudnessMeter::new(fs, 2),
            tp_os: [Oversampler4x::new(), Oversampler4x::new()],
            latency,
            dry_l: vec![0.0; latency.max(1) + 1],
            dry_r: vec![0.0; latency.max(1) + 1],
            dpos: 0,
            mix: 1.0,
            meters,
        }
    }

    pub fn latency_samples(&self) -> u32 {
        self.latency as u32
    }

    pub fn reset(&mut self) {
        for e in self.eq.iter_mut() {
            e.reset();
        }
        for m in self.mb.iter_mut() {
            m.reset();
        }
        self.limiter.reset();
        self.lufs.reset();
        for o in self.tp_os.iter_mut() {
            o.reset();
        }
        for v in self.dry_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.dry_r.iter_mut() {
            *v = 0.0;
        }
        self.dpos = 0;
    }

    /// Reset only the LUFS integrator (GUI "reset loudness" action).
    pub fn reset_lufs(&mut self) {
        self.lufs.reset();
    }

    /// Test hook: disable K-weighting on the LUFS meter (see `suite_core::loudness`).
    pub fn set_kweighting(&mut self, enabled: bool) {
        self.lufs.set_kweighting(enabled);
    }

    pub fn configure(&mut self, s: &MasterSettings) {
        for e in self.eq.iter_mut() {
            e.configure(&s.eq, self.fs);
        }
        for m in self.mb.iter_mut() {
            m.set_crossovers(s.xo_low, s.xo_high, self.fs);
            for (bi, c) in m.comps.iter_mut().enumerate() {
                let b = s.bands[bi];
                c.configure(
                    b.threshold,
                    b.ratio,
                    s.comp_knee,
                    s.comp_attack,
                    s.comp_release,
                    b.makeup,
                    self.fs,
                );
            }
        }
        self.limiter.set_ceiling_db(s.ceiling_db);
        self.limiter.set_release_ms(s.limiter_release, self.fs);
        self.mix = s.mix.clamp(0.0, 1.0);
    }

    /// Process a stereo block in place.
    pub fn process_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let mut out_peak = 0.0f32;
        let mut true_peak = 0.0f32;
        let mut out_sq = 0.0f32;
        let dlen = self.dry_l.len();

        for i in 0..n {
            let in_l = l[i];
            let in_r = r[i];

            // Delayed dry (latency-matched to the limiter).
            let read = (self.dpos + dlen - self.latency) % dlen;
            let dry_l = self.dry_l[read];
            let dry_r = self.dry_r[read];
            self.dry_l[self.dpos] = in_l;
            self.dry_r[self.dpos] = in_r;
            self.dpos = (self.dpos + 1) % dlen;

            // Wet: EQ → multiband comp → limiter.
            let el = self.eq[0].process(in_l);
            let er = self.eq[1].process(in_r);
            let ml = self.mb[0].process(el);
            let mr = self.mb[1].process(er);
            let (ll, lr) = self.limiter.process(ml, mr);

            let ol = self.mix * ll + (1.0 - self.mix) * dry_l;
            let or = self.mix * lr + (1.0 - self.mix) * dry_r;

            out_peak = out_peak.max(ol.abs()).max(or.abs());
            out_sq += ol * ol + or * or;
            // 4x-oversampled true-peak approximation.
            let tpl = &mut true_peak;
            self.tp_os[0].process(ol, |v| {
                *tpl = tpl.max(v.abs());
                v
            });
            self.tp_os[1].process(or, |v| {
                *tpl = tpl.max(v.abs());
                v
            });
            self.lufs.push(&[ol, or]);

            l[i] = ol;
            r[i] = or;
        }

        if n > 0 {
            let out_rms = (out_sq / (2 * n) as f32).sqrt();
            store_f32(&self.meters.out_peak, lin_to_db(out_peak));
            store_f32(&self.meters.true_peak, lin_to_db(true_peak));
            store_f32(&self.meters.out_rms, lin_to_db(out_rms));
            store_f32(&self.meters.lufs_m, self.lufs.momentary_lufs());
            store_f32(&self.meters.lufs_s, self.lufs.short_lufs());
            store_f32(&self.meters.lufs_i, self.lufs.integrated_lufs());
            for bi in 0..3 {
                // Report the min (deepest) GR across channels for the band.
                let gr = self.mb[0].comps[bi]
                    .gain_reduction_db()
                    .min(self.mb[1].comps[bi].gain_reduction_db());
                store_f32(&self.meters.band_gr[bi], gr);
            }
            store_f32(&self.meters.limiter_gr, self.limiter.gain_reduction_db());
        }
    }

    /// Mono convenience for the offline harness.
    pub fn process_mono(&mut self, buf: &mut [f32], s: &MasterSettings) {
        self.configure(s);
        let mut l = buf.to_vec();
        let mut r = buf.to_vec();
        self.process_block(&mut l, &mut r);
        buf.copy_from_slice(&l);
    }
}
