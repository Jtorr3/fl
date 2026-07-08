//! CHAMBER — pure-DSP core for the image-source space simulator (SPECS "CHAMBER", Eigen clone).
//!
//! A **shoebox** room (`W×D×H` metres) with a draggable **source** and **listener**. The room
//! response is synthesised as two stages, summed as the *wet* signal:
//!
//! ```text
//!   in(mono) ─┬─▶ shared input delay line ─▶ [image cluster: order-≤3 mirror images]
//!             │        per image i:  read @ rᵢ/c  ·  gain(1/rᵢ × reflectⁿ)  ·  HF-damp  ·  pan(azimuthᵢ)
//!             │                                                         └────────────────┐  (early reflections)
//!             │                                                                          ▼
//!             └─▶ pre-delay (ER window + user) ─▶ Fdn8 late field ───────────────────── + ─▶ wet
//!                     RT60 from Sabine(V, ΣSα), damping from material HF character         (ER/late balance)
//!
//!   out = (1−mix)·in  +  mix·wet·outTrim          (mix = 0 ⇒ exact input passthrough)
//! ```
//!
//! ## Image-source model (shoebox)
//! For reflection indices `(kx,ky,kz) ∈ ℤ³` with `|kx|+|ky|+|kz| ≤ order` the mirror-image
//! source coordinate on each axis is `image(k,L,s) = k·L + s` (k even) or `k·L + (L−s)` (k odd).
//! The count is the 3-D L1 ball: **order 1 → 7, order 2 → 25, order 3 → 63** images (incl. the
//! direct path, `k=(0,0,0)`, which is the true source). Per image: delay `= r/c`, amplitude
//! `= (r_direct/r)^dist × Πᵍ reflect_gᵇᵍ` where `g` ranges over the wall-groups
//! (walls / floor / ceiling), `bᵍ` is that group's bounce count and `reflect_g = √(1−αᵍ)`; a
//! one-pole HF damp models the per-bounce high-frequency loss; equal-power pan from the image's
//! horizontal azimuth. The **direct** path has zero bounces ⇒ gain `1`, HF-damp off — it *is*
//! the dry (SPECS: this plugin replaces the room; `mix` blends processed vs input).
//!
//! ## Late field
//! [`suite_core::fdn::Fdn8`] (Householder FDN, reused from MURMUR). Line lengths scale from the
//! room's mean free path `4V/S`; **RT60 from Sabine** `0.161·V/A` (`A = ΣSα`, clamped 0.1–12 s,
//! or a manual override); damping tilt from the mean material HF character. The FDN input is
//! pre-delayed by the early-reflection window (room diagonal `/c`) plus the user pre-delay, so
//! the diffuse tail crossfades in *after* the discrete image cluster.
//!
//! ## Moving source
//! Image delays are recomputed at block (control) rate and the per-image read delay is slewed
//! per sample with a **rate clamp** (like FLYBY) ⇒ a moving source produces natural, click-free
//! doppler with bounded pitch. Gains and pan are per-sample one-pole smoothed.
//!
//! ## Latency / null
//! The direct image sits at its geometric delay `r_direct/c` (sound takes time to arrive), so the
//! wet is not aligned with the dry at lag 0 — exactly like FLYBY/MURMUR, CHAMBER reports **zero**
//! processing latency and `mix = 0` nulls against the dry input (there is no lag-0 wet to comb).
//!
//! Pure Rust, shared verbatim between the nih-plug `process` path and the offline/done-bar tests.

use suite_core::dsp::{DelayLine, OnePole};
use suite_core::fdn::{Fdn8, N};

use std::f32::consts::FRAC_PI_4;

/// Speed of sound (m/s) — real air, so geometry maps to physically plausible delays/RT60.
pub const SPEED: f32 = 343.0;

/// Near-field reference distance (m): the direct/image distance is clamped to at least this for
/// the inverse-distance law, and it is the normalisation reference for the direct path.
pub const R0: f32 = 0.5;

/// Maximum early-reflection order and the resulting image count (3-D L1 ball radius 3).
pub const MAX_ORDER: usize = 3;
/// Preallocated image slots = images at [`MAX_ORDER`] (`(2n+1)(2n²+2n+3)/3` at n=3).
pub const MAX_IMAGES: usize = 63;

/// The order chosen for `ER Order = Auto` — set from the CPU bench (PRD §4 rule). See
/// `tests::cpu_budget_and_order`. Order 3 (63 images) benches within budget on the target
/// machine, so Auto uses full order.
pub const AUTO_ORDER: usize = 3;

/// Rate clamp on the per-sample image read-delay change (samples/sample). At 0.5 the read
/// pointer speed stays in `[0.5, 1.5]×`, bounding doppler pitch and preventing click steps when
/// the source is dragged quickly (done-bar 3).
const RATE_CLAMP: f32 = 0.5;

/// Per-image gain/pan smoothing time (ms).
const SMOOTH_MS: f32 = 5.0;

// ---------------------------------------------------------------------------
// Materials
// ---------------------------------------------------------------------------

/// One of the four wall-group material presets. Each carries a broadband absorption `α`
/// (energy) used for the Sabine RT60 and the per-bounce amplitude reflectance `√(1−α)`, plus a
/// per-bounce high-frequency **keep** factor for the one-pole HF damping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Concrete,
    Wood,
    Curtain,
    Glass,
}

impl Material {
    pub fn from_index(i: usize) -> Material {
        match i {
            0 => Material::Concrete,
            1 => Material::Wood,
            2 => Material::Curtain,
            _ => Material::Glass,
        }
    }

    /// Broadband energy absorption coefficient `α ∈ (0,1)`.
    #[inline]
    pub fn absorption(self) -> f32 {
        match self {
            Material::Concrete => 0.03, // very reflective, live
            Material::Wood => 0.12,     // warm, moderate
            Material::Curtain => 0.55,  // absorptive, dead
            Material::Glass => 0.07,    // reflective, bright
        }
    }

    /// Fraction of high-frequency amplitude kept **per bounce** (1 = no HF loss).
    #[inline]
    pub fn hf_keep(self) -> f32 {
        match self {
            Material::Concrete => 0.90, // bright
            Material::Wood => 0.72,     // warms with each bounce
            Material::Curtain => 0.35,  // strongly damps highs
            Material::Glass => 0.86,    // bright/edgy
        }
    }

    /// Amplitude reflectance per bounce `√(1−α)`.
    #[inline]
    pub fn reflect(self) -> f32 {
        (1.0 - self.absorption()).max(0.0).sqrt()
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// A full snapshot of CHAMBER's controls (plain, un-normalised values). Cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Room width / depth / height (metres).
    pub w: f32,
    pub d: f32,
    pub h: f32,
    /// Source position (metres, within the room). `(x=width, y=depth)`; z (height) fixed sensibly.
    pub src_x: f32,
    pub src_y: f32,
    pub src_z: f32,
    /// Listener position (metres).
    pub lis_x: f32,
    pub lis_y: f32,
    pub lis_z: f32,
    /// Wall-group materials.
    pub mat_walls: Material,
    pub mat_floor: Material,
    pub mat_ceiling: Material,
    /// Early-reflection order actually used (1..=MAX_ORDER); the plugin maps Auto→AUTO_ORDER.
    pub er_order: usize,
    /// ER/late balance, 0 = only early reflections, 1 = only late field, 0.5 = both.
    pub er_late: f32,
    /// Distance amount — exaggerates the inverse-distance rolloff exponent (1 = physical).
    pub distance: f32,
    /// Pre-delay of the late field (seconds), on top of the ER-window onset.
    pub predelay: f32,
    /// RT60 override (seconds); 0 = use the Sabine prediction.
    pub rt60_override: f32,
    /// Stereo width of the wet signal (0 = mono, 1 = as-panned, up to 2 = widened).
    pub width: f32,
    /// Dry/wet mix, 0..1.
    pub mix: f32,
    /// Output trim (dB).
    pub out_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            w: 8.0,
            d: 10.0,
            h: 4.0,
            src_x: 2.5,
            src_y: 2.0,
            src_z: 1.6,
            lis_x: 5.5,
            lis_y: 7.5,
            lis_z: 1.6,
            mat_walls: Material::Wood,
            mat_floor: Material::Wood,
            mat_ceiling: Material::Concrete,
            er_order: AUTO_ORDER,
            er_late: 0.5,
            distance: 1.0,
            predelay: 0.0,
            rt60_override: 0.0,
            width: 1.0,
            mix: 0.35,
            out_db: 0.0,
        }
    }
}

impl Settings {
    /// Clamp positions inside the room and dimensions to sane bounds. Returns a corrected copy.
    fn sanitized(&self) -> Settings {
        let mut s = *self;
        s.w = s.w.clamp(2.0, 40.0);
        s.d = s.d.clamp(2.0, 40.0);
        s.h = s.h.clamp(2.0, 20.0);
        let clampp = |v: f32, hi: f32| v.clamp(0.05 * hi, 0.95 * hi);
        s.src_x = clampp(s.src_x, s.w);
        s.src_y = clampp(s.src_y, s.d);
        s.src_z = clampp(s.src_z, s.h);
        s.lis_x = clampp(s.lis_x, s.w);
        s.lis_y = clampp(s.lis_y, s.d);
        s.lis_z = clampp(s.lis_z, s.h);
        s.er_order = s.er_order.clamp(1, MAX_ORDER);
        s.er_late = s.er_late.clamp(0.0, 1.0);
        s.distance = s.distance.clamp(0.5, 3.0);
        s.predelay = s.predelay.clamp(0.0, 0.2);
        s.rt60_override = s.rt60_override.clamp(0.0, 12.0);
        s.width = s.width.clamp(0.0, 2.0);
        s.mix = s.mix.clamp(0.0, 1.0);
        s
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers (also used by the done-bar tests and the GUI)
// ---------------------------------------------------------------------------

/// 1-D mirror-image coordinate for reflection index `k` on an axis of length `l` from a source
/// at `s`. Even `k` = translated copy, odd `k` = mirrored copy.
#[inline]
pub fn image_1d(k: i32, l: f32, s: f32) -> f32 {
    if k & 1 == 0 {
        k as f32 * l + s
    } else {
        k as f32 * l + (l - s)
    }
}

/// Split the `|kz|` vertical reflections into (floor, ceiling) bounce counts. A ray with `kz>0`
/// first hits the ceiling; `kz<0` first hits the floor; they then alternate.
#[inline]
fn z_bounces(kz: i32) -> (i32, i32) {
    let n = kz.abs();
    if n == 0 {
        (0, 0)
    } else if kz > 0 {
        (n / 2, (n + 1) / 2) // (floor, ceiling)
    } else {
        ((n + 1) / 2, n / 2)
    }
}

/// Direct source→listener distance (metres), clamped to [`R0`].
pub fn direct_distance(s: &Settings) -> f32 {
    let s = s.sanitized();
    let dx = s.src_x - s.lis_x;
    let dy = s.src_y - s.lis_y;
    let dz = s.src_z - s.lis_z;
    (dx * dx + dy * dy + dz * dz).sqrt().max(R0)
}

/// Sabine RT60 prediction (seconds) for the room in `s`, clamped to [0.1, 12].
pub fn sabine_rt60(s: &Settings) -> f32 {
    let s = s.sanitized();
    let (w, d, h) = (s.w, s.d, s.h);
    let v = w * d * h;
    let s_floor = w * d;
    let s_ceiling = w * d;
    let s_walls = 2.0 * (w * h) + 2.0 * (d * h);
    let a = s_floor * s.mat_floor.absorption()
        + s_ceiling * s.mat_ceiling.absorption()
        + s_walls * s.mat_walls.absorption();
    if a <= 1.0e-6 {
        return 12.0;
    }
    (0.161 * v / a).clamp(0.1, 12.0)
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

/// A mono fractional delay line read with 4-point Catmull-Rom interpolation (shared ER input
/// line). Preallocated; allocation-free in `process`.
#[derive(Clone)]
struct FracDelay {
    buf: Vec<f32>,
    wpos: usize,
}

impl FracDelay {
    fn new(max_delay: usize) -> Self {
        Self {
            buf: vec![0.0; max_delay.max(8) + 4],
            wpos: 0,
        }
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.wpos = 0;
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.wpos] = x;
        self.wpos += 1;
        if self.wpos == self.buf.len() {
            self.wpos = 0;
        }
    }
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let len = self.buf.len() as isize;
        let d = delay.clamp(1.0, (self.buf.len() - 3) as f32);
        let rpos = self.wpos as f32 - d;
        let base = rpos.floor();
        let frac = rpos - base;
        let i1 = base as isize;
        let get = |k: isize| -> f32 {
            let idx = ((i1 + k).rem_euclid(len)) as usize;
            self.buf[idx]
        };
        catmull_rom(get(-1), get(0), get(1), get(2), frac)
    }
}

#[inline]
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// One-pole low-pass HF damp: `y = (1−a)·x + a·y⁻¹`. `a = 0` is exact passthrough (direct path);
/// larger `a` is darker (more accumulated bounce loss).
#[derive(Clone, Copy, Default)]
struct Damp {
    a: f32,
    z: f32,
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

/// First-order DC blocker for the wet output.
#[derive(Clone, Copy, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
}
impl DcBlock {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + 0.995 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// Wet safety clip: identity for |x| ≤ knee, tanh-compressed above so |y| < 1 (never touches the
/// mix=0 null, which uses the dry path only).
#[inline]
fn safety_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.9;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// One early-reflection image: a moving tap into the shared ER delay line.
#[derive(Clone, Copy, Default)]
struct Image {
    delay_target: f32,
    delay_slew: f32,
    gain_target: f32,
    gain_cur: f32,
    panl_target: f32,
    panl_cur: f32,
    panr_target: f32,
    panr_cur: f32,
    damp: Damp,
}

// ---------------------------------------------------------------------------
// ChamberCore
// ---------------------------------------------------------------------------

/// CHAMBER's full stereo DSP core.
pub struct ChamberCore {
    sr: f32,
    er_delay: FracDelay,
    er_max: usize,
    images: [Image; MAX_IMAGES],
    active: usize,

    fdn: Fdn8,
    predelay_l: DelayLine,
    predelay_r: DelayLine,
    predelay_max: usize,

    er_gain_s: OnePole,
    late_gain_s: OnePole,
    width_s: OnePole,
    mix_s: OnePole,
    out_s: OnePole,

    dc_l: DcBlock,
    dc_r: DcBlock,

    smooth_coef: f32,
    // Cached geometry to skip FDN reconfiguration when the room/material is unchanged.
    last_room: (f32, f32, f32),
    last_rt60: f32,
    last_damp: f32,
    primed: bool,
}

impl ChamberCore {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        // ER line: longest order-3 image distance ≈ ~170 m at the 40 m room cap → ~0.5 s; +margin.
        let er_max = ((0.7 * sr) as usize).max(64);
        // FDN lines: mean free path of the largest room / c → allocate generously.
        let fdn_max = ((0.25 * sr) as usize).max(64);
        // Pre-delay: ER window (room diagonal / c ≈ up to 0.2 s) + user pre-delay (0.2 s) + margin.
        let predelay_max = ((0.5 * sr) as usize).max(16);

        let mut me = Self {
            sr,
            er_delay: FracDelay::new(er_max),
            er_max,
            images: [Image::default(); MAX_IMAGES],
            active: 0,
            fdn: Fdn8::new(fdn_max, sr),
            predelay_l: DelayLine::new(predelay_max),
            predelay_r: DelayLine::new(predelay_max),
            predelay_max,
            er_gain_s: OnePole::new(),
            late_gain_s: OnePole::new(),
            width_s: OnePole::new(),
            mix_s: OnePole::new(),
            out_s: OnePole::new(),
            dc_l: DcBlock::default(),
            dc_r: DcBlock::default(),
            smooth_coef: 0.0,
            last_room: (0.0, 0.0, 0.0),
            last_rt60: -1.0,
            last_damp: -2.0,
            primed: false,
        };
        me.set_sample_rate(sr);
        me
    }

    /// The image delays ARE the effect ⇒ report zero processing latency.
    pub fn latency_samples(&self) -> u32 {
        0
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sr = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let t = 8.0;
        self.er_gain_s.set_time(t, self.sr);
        self.late_gain_s.set_time(t, self.sr);
        self.width_s.set_time(t, self.sr);
        self.mix_s.set_time(t, self.sr);
        self.out_s.set_time(t, self.sr);
        self.smooth_coef = 1.0 - (-1.0 / (SMOOTH_MS * 0.001 * self.sr).max(1.0)).exp();
        self.primed = false;
        self.last_rt60 = -1.0;
        self.last_damp = -2.0;
        self.last_room = (0.0, 0.0, 0.0);
    }

    pub fn reset(&mut self) {
        self.er_delay.reset();
        self.fdn.reset();
        self.predelay_l.reset();
        self.predelay_r.reset();
        self.dc_l.reset();
        self.dc_r.reset();
        for img in self.images.iter_mut() {
            img.damp.reset();
        }
        self.primed = false;
    }

    /// Number of active images at the current order (test/diagnostic hook).
    pub fn active_images(&self) -> usize {
        self.active
    }

    /// Recompute all image targets from the room/source/listener geometry (control rate).
    fn recompute_images(&mut self, s: &Settings) {
        let order = s.er_order.clamp(1, MAX_ORDER) as i32;
        let r_direct = {
            let dx = s.src_x - s.lis_x;
            let dy = s.src_y - s.lis_y;
            let dz = s.src_z - s.lis_z;
            (dx * dx + dy * dy + dz * dz).sqrt().max(R0)
        };
        let dist_exp = s.distance;
        let refl_wall = s.mat_walls.reflect();
        let refl_floor = s.mat_floor.reflect();
        let refl_ceil = s.mat_ceiling.reflect();
        let hf_wall = s.mat_walls.hf_keep();
        let hf_floor = s.mat_floor.hf_keep();
        let hf_ceil = s.mat_ceiling.hf_keep();

        let mut idx = 0usize;
        for kx in -order..=order {
            let bx = kx.abs();
            let rem_x = order - bx;
            for ky in -rem_x..=rem_x {
                let by = ky.abs();
                let rem_y = rem_x - by;
                for kz in -rem_y..=rem_y {
                    if idx >= MAX_IMAGES {
                        break;
                    }
                    let ix = image_1d(kx, s.w, s.src_x);
                    let iy = image_1d(ky, s.d, s.src_y);
                    let iz = image_1d(kz, s.h, s.src_z);
                    let dx = ix - s.lis_x;
                    let dy = iy - s.lis_y;
                    let dz = iz - s.lis_z;
                    let r = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0e-3);

                    // Reflectance gain over the wall-groups.
                    let (nf, nc) = z_bounces(kz);
                    let n_walls = bx + by;
                    let refl = refl_wall.powi(n_walls) * refl_floor.powi(nf) * refl_ceil.powi(nc);

                    // Distance gain, normalised so the direct path is unity, exaggeration `dist_exp`.
                    let g_dist = (r_direct / r.max(r_direct)).powf(dist_exp);
                    let gain = g_dist * refl;

                    // HF damp: accumulated per-bounce keep → one-pole coefficient.
                    let hf_keep =
                        hf_wall.powi(n_walls) * hf_floor.powi(nf) * hf_ceil.powi(nc);
                    let a = (1.0 - hf_keep).clamp(0.0, 0.95);

                    // Equal-power pan from the horizontal azimuth (x = left/right, y = depth).
                    let r_h = (dx * dx + dy * dy).sqrt().max(1.0e-3);
                    let pan = (dx / r_h).clamp(-1.0, 1.0);
                    let ang = (pan + 1.0) * FRAC_PI_4; // 0..π/2
                    let panl = ang.cos();
                    let panr = ang.sin();

                    let delay = ((r / SPEED) * self.sr).clamp(1.0, (self.er_max - 4) as f32);

                    let img = &mut self.images[idx];
                    img.delay_target = delay;
                    img.gain_target = gain;
                    img.panl_target = panl;
                    img.panr_target = panr;
                    img.damp.a = a;
                    idx += 1;
                }
            }
        }
        self.active = idx;
    }

    /// Reconfigure the FDN late field from the room/materials (only when they change).
    fn configure_late(&mut self, s: &Settings) {
        let room = (s.w, s.d, s.h);
        let rt60 = if s.rt60_override > 0.0 {
            s.rt60_override.clamp(0.1, 12.0)
        } else {
            sabine_rt60(s)
        };
        // Damping tilt from mean material HF character (bright → −, dark → +).
        let mean_keep =
            (s.mat_walls.hf_keep() + s.mat_floor.hf_keep() + s.mat_ceiling.hf_keep()) / 3.0;
        let damp_tilt = ((1.0 - mean_keep) * 2.0 - 1.0).clamp(-1.0, 1.0);

        let room_changed = (room.0 - self.last_room.0).abs() > 1.0e-3
            || (room.1 - self.last_room.1).abs() > 1.0e-3
            || (room.2 - self.last_room.2).abs() > 1.0e-3;

        if room_changed {
            // Delay lengths scale from the room mean free path 4V/S.
            let v = s.w * s.d * s.h;
            let surf = 2.0 * (s.w * s.d) + 2.0 * (s.w * s.h) + 2.0 * (s.d * s.h);
            let mfp = if surf > 1.0e-3 { 4.0 * v / surf } else { 1.0 };
            let base = ((mfp / SPEED) * self.sr * 0.5).clamp(64.0, (self.fdn.max_delay() - 2) as f32);
            const RATIOS: [f32; N] = [1.00, 1.13, 1.27, 1.41, 1.55, 1.69, 1.83, 1.97];
            let mut delays = [0usize; N];
            for i in 0..N {
                delays[i] = (base * RATIOS[i])
                    .clamp(64.0, (self.fdn.max_delay() - 2) as f32)
                    as usize;
            }
            make_coprime_ish(&mut delays, self.fdn.max_delay());
            self.fdn.set_delays(&delays);
            self.fdn.set_diffusion(0.6);
            // Anti-metallic delay modulation (SOUND-PASS): the static image-source + FDN
            // late field rang badly on discrete modes; ~0.25 ms of slow per-line wobble
            // diffuses them into a smooth cavernous tail without audible pitch drift.
            self.fdn.set_modulation(0.00008 * self.sr, 0.5);
            self.last_room = room;
        }
        if (rt60 - self.last_rt60).abs() > 1.0e-3 {
            self.fdn.set_rt60(rt60);
            self.last_rt60 = rt60;
        }
        if (damp_tilt - self.last_damp).abs() > 1.0e-3 {
            self.fdn.set_damping(damp_tilt);
            self.last_damp = damp_tilt;
        }

        // Pre-delay: ER window (room diagonal / c) + user pre-delay.
        let diag = (s.w * s.w + s.d * s.d + s.h * s.h).sqrt();
        let er_window = diag / SPEED;
        let pd = (((er_window + s.predelay) * self.sr) as usize).clamp(1, self.predelay_max - 1);
        self.predelay_l.set_delay(pd);
        self.predelay_r.set_delay(pd);
    }

    fn prime(&mut self, s: &Settings) {
        self.recompute_images(s);
        for i in 0..self.active {
            let img = &mut self.images[i];
            img.delay_slew = img.delay_target;
            img.gain_cur = img.gain_target;
            img.panl_cur = img.panl_target;
            img.panr_cur = img.panr_target;
        }
        // ER/late balance → the two output gains (equal-power-ish split).
        let (eg, lg) = balance_gains(s.er_late);
        self.er_gain_s.reset(eg);
        self.late_gain_s.reset(lg);
        self.width_s.reset(s.width);
        self.mix_s.reset(s.mix);
        self.out_s.reset(s.out_db);
        self.primed = true;
    }

    /// Latch a settings snapshot (call once per block before the sample loop).
    pub fn configure(&mut self, s_in: &Settings) {
        let s = s_in.sanitized();
        self.configure_late(&s);
        if !self.primed {
            self.prime(&s);
        } else {
            self.recompute_images(&s);
        }
    }

    /// Process one stereo sample against the latched settings `s`.
    #[inline]
    pub fn process_sample(&mut self, l: f32, r: f32, s: &Settings) -> (f32, f32) {
        let mono = 0.5 * (l + r);

        // Early reflections: sum the moving image taps. Read BEFORE writing this sample so a tap
        // at delay `d` returns the input exactly `d` samples ago (direct arrival lands at r/c).
        let mut er_l = 0.0f32;
        let mut er_r = 0.0f32;
        let coef = self.smooth_coef;
        for i in 0..self.active {
            let img = &mut self.images[i];
            // Rate-clamp the read delay toward its target (doppler, no click steps).
            let dd = (img.delay_target - img.delay_slew).clamp(-RATE_CLAMP, RATE_CLAMP);
            img.delay_slew += dd;
            let mut smp = self.er_delay.read(img.delay_slew);
            smp = img.damp.process(smp);
            img.gain_cur += coef * (img.gain_target - img.gain_cur);
            img.panl_cur += coef * (img.panl_target - img.panl_cur);
            img.panr_cur += coef * (img.panr_target - img.panr_cur);
            let g = smp * img.gain_cur;
            er_l += g * img.panl_cur;
            er_r += g * img.panr_cur;
        }
        // Now commit this sample to the shared ER line (read-before-write convention above).
        self.er_delay.write(mono);

        // Late field: pre-delayed input through the FDN.
        let pl = self.predelay_l.process(mono);
        let pr = self.predelay_r.process(mono);
        let (late_l, late_r) = self.fdn.process(pl, pr);

        // ER/late balance.
        let (eg_t, lg_t) = balance_gains(s.er_late.clamp(0.0, 1.0));
        let eg = self.er_gain_s.process(eg_t);
        let lg = self.late_gain_s.process(lg_t);
        let mut wet_l = eg * er_l + lg * late_l;
        let mut wet_r = eg * er_r + lg * late_r;

        // Width (mid/side).
        let width = self.width_s.process(s.width.clamp(0.0, 2.0));
        let mid = 0.5 * (wet_l + wet_r);
        let side = 0.5 * (wet_l - wet_r) * width;
        wet_l = mid + side;
        wet_r = mid - side;

        // DC block + safety clip (wet only).
        wet_l = safety_clip(self.dc_l.process(wet_l));
        wet_r = safety_clip(self.dc_r.process(wet_r));

        // Dry/wet mix + output trim.
        let mix = self.mix_s.process(s.mix.clamp(0.0, 1.0));
        let out_lin = db_to_lin(self.out_s.process(s.out_db));
        let ol = (l * (1.0 - mix) + wet_l * mix) * out_lin;
        let or = (r * (1.0 - mix) + wet_r * mix) * out_lin;
        (ol.clamp(-8.0, 8.0), or.clamp(-8.0, 8.0))
    }

    /// Offline stereo render from a mono input (fed to both channels of the source).
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

/// Split the ER/late balance knob into two output gains. Equal-power crossfade so the perceived
/// loudness is roughly constant across the sweep; both are audible near 0.5.
#[inline]
fn balance_gains(bal: f32) -> (f32, f32) {
    let b = bal.clamp(0.0, 1.0);
    let theta = b * std::f32::consts::FRAC_PI_2;
    (theta.cos(), theta.sin())
}

/// Greatest common divisor (Euclid).
#[inline]
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Nudge the eight FDN delays mutually prime-ish (avoids commensurate-length flutter).
fn make_coprime_ish(delays: &mut [usize; N], max_delay: usize) {
    const MIN_D: usize = 32;
    for i in 0..N {
        if delays[i] < MIN_D {
            delays[i] = MIN_D + i * 2;
        }
        if delays[i] % 2 == 0 {
            delays[i] += 1;
        }
        let mut tries = 0;
        while tries < 64 && (0..i).any(|j| delays[i] == delays[j] || gcd(delays[i], delays[j]) > 1) {
            delays[i] += 2;
            if delays[i] > max_delay {
                delays[i] = (MIN_D | 1) + i * 2;
            }
            tries += 1;
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
