//! OVERSEER **Master** — the bus processor: 4-band EQ → 3-band multiband compressor
//! (LR4 splits) → lookahead limiter → LUFS meter (BS.1770, `suite_core::loudness`).
//!
//! Reports the limiter's lookahead as latency; the dry path is delayed to match so the
//! dry/wet `mix` nulls cleanly at 0. The Master GUI additionally reads the live Node slots
//! off the bus and can write overrides into them (handled in `lib.rs`).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use suite_core::classify::{
    infer_theme, FeatureExtractor, FeatureSummary, InstrumentType, MixAnalysis, NodeReport,
    SessionTheme,
};
use suite_core::dsp::{OnePole, Oversampler4x};
use suite_core::loudness::LoudnessMeter;

use crate::dynamics::{Limiter, MultibandComp};
use crate::enrich::{theme_assist_targets, AssistTargets};
use crate::eq::{EqSettings, FourBandEq};
use crate::node::load_f32;

/// Published-feature array width ([`FeatureSummary::NFIELDS`]).
const NFEAT: usize = 12;

/// Cross-thread state shared between the Master's audio core and its editor (OVERSEER-ENRICH
/// theme inference + assist + LEARN). Audio publishes the master mix features + transport
/// tempo and runs the LEARN capture; the GUI tick infers the theme and publishes the assist
/// targets that the audio applies. All lock-free atomics.
pub struct MasterShared {
    // audio → GUI
    feat: [AtomicU32; NFEAT],
    tempo: AtomicU32,
    sr: AtomicU32,
    // GUI → audio
    active_theme: AtomicU32,
    theme_conf: AtomicU32,
    assist: [AtomicU32; 4], // eq_tilt, eq_low, comp_character, limiter_drive
    // LEARN capture (GUI requests, audio runs, GUI polls the generation)
    learn_req: AtomicU32,
    learn_gen: AtomicU32,
    capturing: AtomicBool,
    capture_prog: AtomicU32,
}

impl Default for MasterShared {
    fn default() -> Self {
        Self {
            feat: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            tempo: AtomicU32::new(0.0f32.to_bits()),
            sr: AtomicU32::new(48_000.0f32.to_bits()),
            active_theme: AtomicU32::new(SessionTheme::Generic.index()),
            theme_conf: AtomicU32::new(0.0f32.to_bits()),
            assist: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            learn_req: AtomicU32::new(0),
            learn_gen: AtomicU32::new(0),
            capturing: AtomicBool::new(false),
            capture_prog: AtomicU32::new(0.0f32.to_bits()),
        }
    }
}

impl MasterShared {
    pub fn set_features(&self, f: &FeatureSummary) {
        let a = f.to_array();
        for (i, v) in a.iter().enumerate() {
            store_f32(&self.feat[i], *v);
        }
    }
    pub fn features(&self) -> FeatureSummary {
        let mut a = [0.0f32; NFEAT];
        for (i, s) in self.feat.iter().enumerate() {
            a[i] = load_f32(s);
        }
        FeatureSummary::from_array(&a)
    }
    pub fn set_tempo(&self, bpm: f32) {
        store_f32(&self.tempo, bpm);
    }
    pub fn tempo(&self) -> f32 {
        load_f32(&self.tempo)
    }
    pub fn set_sr(&self, sr: f32) {
        store_f32(&self.sr, sr);
    }
    pub fn sr(&self) -> f32 {
        load_f32(&self.sr)
    }
    pub fn set_theme(&self, theme: SessionTheme, conf: f32) {
        self.active_theme.store(theme.index(), Ordering::Relaxed);
        store_f32(&self.theme_conf, conf);
    }
    pub fn theme(&self) -> (SessionTheme, f32) {
        (
            SessionTheme::from_index(self.active_theme.load(Ordering::Relaxed)),
            load_f32(&self.theme_conf),
        )
    }
    pub fn set_assist(&self, t: &AssistTargets) {
        store_f32(&self.assist[0], t.eq_tilt_db);
        store_f32(&self.assist[1], t.eq_low_db);
        store_f32(&self.assist[2], t.comp_character);
        store_f32(&self.assist[3], t.limiter_drive_db);
    }
    pub fn assist_targets(&self) -> AssistTargets {
        AssistTargets {
            eq_tilt_db: load_f32(&self.assist[0]),
            eq_low_db: load_f32(&self.assist[1]),
            comp_character: load_f32(&self.assist[2]),
            limiter_drive_db: load_f32(&self.assist[3]),
        }
    }
    pub fn request_learn(&self, n: usize) {
        self.learn_req.store(n as u32, Ordering::Relaxed);
    }
    pub fn take_learn_req(&self) -> usize {
        self.learn_req.swap(0, Ordering::Relaxed) as usize
    }
    pub fn set_capturing(&self, on: bool) {
        self.capturing.store(on, Ordering::Relaxed);
    }
    pub fn capturing(&self) -> bool {
        self.capturing.load(Ordering::Relaxed)
    }
    pub fn set_capture_prog(&self, p: f32) {
        store_f32(&self.capture_prog, p);
    }
    pub fn capture_prog(&self) -> f32 {
        load_f32(&self.capture_prog)
    }
    pub fn bump_learn_gen(&self) {
        self.learn_gen.fetch_add(1, Ordering::Relaxed);
    }
    pub fn learn_gen(&self) -> u32 {
        self.learn_gen.load(Ordering::Relaxed)
    }
}

/// Control sub-block length for block-advanced smoothing of the EQ-gain, band-comp and
/// ceiling coefficients (mix is smoothed per sample).
const CTRL_CHUNK: usize = 32;
/// Param smoothing time constant (ms) — matches the params' `SmoothingStyle::Linear(20.0)`.
const SMOOTH_MS: f32 = 20.0;

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
    pub meters: Arc<MasterMeters>,

    // OVERSEER-ENRICH: master-bus mix analysis + LEARN capture. Taps the INPUT; never alters
    // the audio. Publishes features to `shared` for the GUI's theme inference.
    extractor: FeatureExtractor,
    shared: Arc<MasterShared>,
    // OVERSEER-ENRICH: assist targets computed on the AUDIO thread each block (so ASSIST works
    // with the editor closed). The editor only DISPLAYS them.
    assist: AssistTargets,

    // --- Param smoothing (MAJOR 3) --------------------------------------------------
    sm_mix: OnePole,      // per sample
    sm_ceiling: OnePole,  // per CTRL_CHUNK
    sm_low_gain: OnePole, // EQ gains, per CTRL_CHUNK
    sm_b1_gain: OnePole,
    sm_b2_gain: OnePole,
    sm_high_gain: OnePole,
    sm_thr: [OnePole; 3],    // band thresholds, per CTRL_CHUNK
    sm_makeup: [OnePole; 3], // band makeups, per CTRL_CHUNK
    tgt: MasterSettings,
    primed: bool,
}

impl MasterCore {
    pub fn new(fs: f32, meters: Arc<MasterMeters>, shared: Arc<MasterShared>) -> Self {
        shared.set_sr(fs);
        let limiter = Limiter::new(fs);
        let latency = limiter.lookahead_samples();
        let sm = || {
            let mut p = OnePole::new();
            p.set_time(SMOOTH_MS, fs);
            p
        };
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
            meters,
            extractor: FeatureExtractor::new(fs),
            shared,
            assist: AssistTargets::default(),
            sm_mix: sm(),
            sm_ceiling: sm(),
            sm_low_gain: sm(),
            sm_b1_gain: sm(),
            sm_b2_gain: sm(),
            sm_high_gain: sm(),
            sm_thr: [sm(), sm(), sm()],
            sm_makeup: [sm(), sm(), sm()],
            tgt: MasterSettings::default(),
            primed: false,
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
        self.extractor.reset();
        self.primed = false;
    }

    /// Access the shared theme/assist state (GUI wiring).
    pub fn shared(&self) -> &Arc<MasterShared> {
        &self.shared
    }

    /// OVERSEER-ENRICH: recompute the assist targets on the AUDIO thread (block rate, cheap
    /// math, allocation-free) so ASSIST keeps working with the Master GUI closed — previously
    /// this ran only inside the editor tick, so the audio thread applied stale/absent targets.
    /// `locked` is the persisted theme lock resolved by the caller; the editor only DISPLAYS
    /// the result. Reuses the already-computed master mix features + the Nodes' published
    /// reports off the bus. On a momentary structural-lock contention the last targets are kept.
    pub fn update_assist(&mut self, tempo: f32, locked: Option<SessionTheme>) {
        let theme = match locked {
            Some(t) => {
                self.shared.set_theme(t, 1.0);
                t
            }
            None => {
                let mut reports = [NodeReport {
                    ty: InstrumentType::Generic,
                    features: FeatureSummary::default(),
                }; 32];
                let n = match crate::bus::bus().try_node_reports(&mut reports) {
                    Some(n) => n,
                    // Bus structurally locked this instant → keep the last targets.
                    None => return,
                };
                let mfeat = self.extractor.summary();
                let mut onset = mfeat.onset_rate;
                for r in reports[..n].iter() {
                    onset = onset.max(r.features.onset_rate);
                }
                let mix = MixAnalysis {
                    tempo_bpm: tempo,
                    tilt: mfeat.tilt,
                    onset_density: onset,
                    dynamic_range_db: 20.0 * mfeat.crest.max(1.0).log10(),
                };
                let (theme, conf) = infer_theme(&reports[..n], &mix);
                self.shared.set_theme(theme, conf);
                theme
            }
        };
        let targets = theme_assist_targets(theme);
        self.assist = targets;
        self.shared.set_assist(&targets);
    }

    /// The assist targets computed by the last [`update_assist`](Self::update_assist).
    pub fn assist_targets(&self) -> AssistTargets {
        self.assist
    }

    fn snap_smoothers(&mut self) {
        self.sm_mix.reset(self.tgt.mix.clamp(0.0, 1.0));
        self.sm_ceiling.reset(self.tgt.ceiling_db);
        self.sm_low_gain.reset(self.tgt.eq.low_gain);
        self.sm_b1_gain.reset(self.tgt.eq.b1_gain);
        self.sm_b2_gain.reset(self.tgt.eq.b2_gain);
        self.sm_high_gain.reset(self.tgt.eq.high_gain);
        for bi in 0..3 {
            self.sm_thr[bi].reset(self.tgt.bands[bi].threshold);
            self.sm_makeup[bi].reset(self.tgt.bands[bi].makeup);
        }
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
        self.tgt = *s;
        // Crossovers + comp ratio/knee/atk/rel + limiter release are block-rate (not
        // declared smoothed); the smoothed EQ gains / band thresholds / band makeups /
        // ceiling / mix are applied inside `process_block`.
        for m in self.mb.iter_mut() {
            m.set_crossovers(s.xo_low, s.xo_high, self.fs);
        }
        self.limiter.set_release_ms(s.limiter_release, self.fs);
        if !self.primed {
            self.snap_smoothers();
            self.primed = true;
        }
    }

    /// Recompute EQ + band-comp coefficients + limiter ceiling from the currently-smoothed
    /// values (called at each CTRL_CHUNK boundary).
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
        for e in self.eq.iter_mut() {
            e.configure(&eqs, self.fs);
        }
        for m in self.mb.iter_mut() {
            for (bi, c) in m.comps.iter_mut().enumerate() {
                c.configure(
                    self.sm_thr[bi].value(),
                    self.tgt.bands[bi].ratio,
                    self.tgt.comp_knee,
                    self.tgt.comp_attack,
                    self.tgt.comp_release,
                    self.sm_makeup[bi].value(),
                    self.fs,
                );
            }
        }
        self.limiter.set_ceiling_db(self.sm_ceiling.value());
    }

    /// Process a stereo block in place.
    pub fn process_block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());

        // OVERSEER-ENRICH: tap the INPUT mix for theme analysis + LEARN (lock-free; audio
        // untouched). The GUI reads these features to infer the session theme.
        let req = self.shared.take_learn_req();
        if req > 0 {
            self.extractor.begin_capture(req);
        }
        self.extractor.process_block(&l[..n], &r[..n]);
        self.shared.set_features(&self.extractor.summary());
        self.shared.set_capturing(self.extractor.capturing());
        self.shared.set_capture_prog(self.extractor.capture_progress());
        if self.extractor.take_capture().is_some() {
            // The GUI reads the (rolling) features + live node reports at commit time.
            self.shared.bump_learn_gen();
        }

        let mut out_peak = 0.0f32;
        let mut true_peak = 0.0f32;
        let mut out_sq = 0.0f32;
        let dlen = self.dry_l.len();

        let mut i0 = 0;
        while i0 < n {
            let end = (i0 + CTRL_CHUNK).min(n);
            // Block-advanced coefficient smoothing for EQ gains / band comp / ceiling.
            self.config_coeffs_from_smoothers();

            for i in i0..end {
                // Per-sample mix smoothing; advance the coefficient smoothers per sample.
                let mix = self.sm_mix.process(self.tgt.mix).clamp(0.0, 1.0);
                self.sm_ceiling.process(self.tgt.ceiling_db);
                self.sm_low_gain.process(self.tgt.eq.low_gain);
                self.sm_b1_gain.process(self.tgt.eq.b1_gain);
                self.sm_b2_gain.process(self.tgt.eq.b2_gain);
                self.sm_high_gain.process(self.tgt.eq.high_gain);
                for bi in 0..3 {
                    self.sm_thr[bi].process(self.tgt.bands[bi].threshold);
                    self.sm_makeup[bi].process(self.tgt.bands[bi].makeup);
                }

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

                let ol = mix * ll + (1.0 - mix) * dry_l;
                let or = mix * lr + (1.0 - mix) * dry_r;

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
            i0 = end;
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
