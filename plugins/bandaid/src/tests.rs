//! BANDAID offline done-bar + universal tests (PRD §4). The pure `dsp::BandaidCore` is the
//! shipped path; these drive it through `suite_core::harness::render_offline`.
//!
//! Done-bars (BANDAID-specific, PRD §4):
//!   1. all gains 0 / solos off → null vs input < −80 dB (parallel-delta / allpass-flat proof)
//!   2. low-band attack +12 dB raises the LOW-band onset-peak-to-sustain ratio only
//!      (mid/high band ratios within ±1 dB of neutral)
//!   3. sustain −12 dB on the MID band lowers MID-band inter-onset RMS only
//!   4. attack sweep −12/0/+12 → monotonic LOW-band onset-peak ratio

use crate::dsp::{BandaidCore, Settings, NUM_BANDS};
use suite_core::dsp::Svf;
use suite_core::harness::{
    assert_universal, null_residual_db, render_and_write, render_offline, rms_dbfs,
};
use suite_core::testsig::{self, KickSpec};

/// BUILT-IN-MANUALS cross-check: the embedded manual documents every param and has recipes.
#[test]
fn manual_covers_all_params_and_has_recipes() {
    suite_core::manual::assert_manual_covers_params(
        crate::MANUAL_DOC,
        &crate::BandaidParams::default(),
    );
}

const SR: f32 = 48_000.0;
const XLOW: f32 = 300.0;
const XHIGH: f32 = 2500.0;

/// A neutral base setting with fixed crossovers.
fn base() -> Settings {
    Settings {
        xover_low: XLOW,
        xover_high: XHIGH,
        ..Settings::default()
    }
}

fn render_with(s: Settings, input: &[f32]) -> Vec<f32> {
    let mut core = BandaidCore::new(SR);
    core.configure(&s);
    render_offline(core, input, 512)
}

// ---------------------------------------------------------------------------
// Measurement helpers
// ---------------------------------------------------------------------------

/// Extract the three LR4 bands of a signal with the same split the core uses, so a
/// measurement "in band X" matches the internal band.
fn split_bands(sig: &[f32]) -> [Vec<f32>; NUM_BANDS] {
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let mk = |fc: f32| {
        let mut s = Svf::new();
        s.set(fc, q, SR);
        s
    };
    let (mut l1, mut l2) = (mk(XLOW), mk(XLOW));
    let (mut h1, mut h2) = (mk(XLOW), mk(XLOW));
    let (mut ml1, mut ml2) = (mk(XHIGH), mk(XHIGH));
    let (mut hh1, mut hh2) = (mk(XHIGH), mk(XHIGH));
    let mut low = Vec::with_capacity(sig.len());
    let mut mid = Vec::with_capacity(sig.len());
    let mut high = Vec::with_capacity(sig.len());
    for &x in sig {
        let lo = l2.process(l1.process(x).lp).lp;
        let rest = h2.process(h1.process(x).hp).hp;
        let m = ml2.process(ml1.process(rest).lp).lp;
        let hi = hh2.process(hh1.process(rest).hp).hp;
        low.push(lo);
        mid.push(m);
        high.push(hi);
    }
    [low, mid, high]
}

fn secs(t: f32) -> usize {
    (t * SR) as usize
}

/// A synthetic kick + tonal pad mix (SPECS done-bar signal). The pad is a steady 3-tone chord
/// (80 Hz low / 900 Hz mid / 4 kHz high, faded in) giving each band a sustained body; kicks add
/// a low transient (+ HF click) every 0.5 s. Amplitudes leave headroom so a +12 dB attack boost
/// stays under the 0 dBFS ceiling. Returns the signal and the onset sample indices.
fn hit_train() -> (Vec<f32>, Vec<usize>) {
    use std::f32::consts::TAU;
    let total = secs(2.6);
    let mut sig = vec![0.0f32; total];
    // Tonal pad in the MID and HIGH bands only (900 Hz + 4 kHz), faded in over 50 ms. The LOW
    // band is left to the kick alone so its onset ratio is a clean transient-vs-decay measure
    // (a strong steady low tone would ripple the low-band peak detector).
    let (fm, fh) = (900.0f32, 4000.0f32);
    for (n, s) in sig.iter_mut().enumerate() {
        let t = n as f32 / SR;
        let fade = (t / 0.05).min(1.0);
        *s = fade * (0.10 * (TAU * fm * t).sin() + 0.07 * (TAU * fh * t).sin());
    }
    // Kicks (low transient + HF click) every 0.5 s.
    let hit_len = secs(0.45);
    // Kick body kept below the 300 Hz low crossover from t=0 (f_start 140 → f_end 50) so the
    // LOW band gets a genuine onset transient rather than energy that builds as the pitch
    // sweeps down into the band; the HF click still feeds the high band.
    let spec = KickSpec {
        f_start: 140.0,
        f_end: 50.0,
        amp_decay_s: 0.22,
        click: 0.35,
        ..KickSpec::default()
    };
    let kick = testsig::synth_kick(&spec, hit_len, SR);
    let onsets: Vec<usize> = [0.1f32, 0.6, 1.1, 1.6, 2.1].iter().map(|&t| secs(t)).collect();
    for &o in &onsets {
        for i in 0..hit_len {
            if o + i >= total {
                break;
            }
            sig[o + i] += 0.18 * kick[i];
        }
    }
    (sig, onsets)
}

/// Onset level: RMS of the band over the 30 ms attack window from each onset, averaged over
/// onsets. RMS (energy) is used rather than the instantaneous sample peak because the
/// parallel-delta output's low band, once re-filtered for measurement, carries a small
/// group-delay comb whose *peak* is phase-scrambled; RMS averages the comb out and recovers
/// the true per-band gain scaling — the transient "level" the attack control sets.
fn onset_peak(band: &[f32], onsets: &[usize]) -> f32 {
    let win = secs(0.030);
    let mut acc = 0.0f64;
    let mut n = 0usize;
    for &o in onsets {
        let end = (o + win).min(band.len());
        for &v in &band[o..end] {
            acc += (v * v) as f64;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    (acc / n as f64).sqrt() as f32
}

/// RMS of the band over the inter-onset (decaying body / sustain) region [onset+100ms,
/// onset+460ms] — starting past the ~40 ms attack region so the sustain measure is isolated
/// from the transient the attack control acts on.
fn inter_onset_rms(band: &[f32], onsets: &[usize]) -> f32 {
    let a = secs(0.10);
    let b = secs(0.46);
    let mut acc = 0.0f64;
    let mut n = 0usize;
    for &o in onsets {
        let start = (o + a).min(band.len());
        let end = (o + b).min(band.len());
        for &v in &band[start..end] {
            acc += (v * v) as f64;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    (acc / n as f64).sqrt() as f32
}

/// onset-peak / inter-onset-RMS ratio for a band, in dB.
fn peak_to_sustain_db(band: &[f32], onsets: &[usize]) -> f32 {
    let p = onset_peak(band, onsets);
    let s = inter_onset_rms(band, onsets).max(1e-9);
    20.0 * (p / s).log10()
}

// ---------------------------------------------------------------------------
// Done-bar 1 — neutral nulls to the input (parallel-delta / allpass-flat proof)
// ---------------------------------------------------------------------------

#[test]
fn neutral_nulls_against_input() {
    // Broadband: pink + a full-range chirp so the null is proven across the crossovers.
    let mut sig = testsig::pink_noise(0.4, SR as usize * 1, 0x1234);
    let chirp = testsig::log_chirp(20.0, 20_000.0, 0.4, SR as usize, SR);
    for (s, c) in sig.iter_mut().zip(chirp.iter()) {
        *s = (*s + *c) * 0.5;
    }
    let out = render_with(base(), &sig);
    let residual = null_residual_db(&sig, &out);
    assert!(
        residual < -80.0,
        "neutral (all gains 0, solos off) should null vs input; residual {residual:.2} dB"
    );
}

#[test]
fn mix_zero_nulls_against_input() {
    let sig = testsig::pink_noise(0.4, SR as usize, 0x9999);
    let mut s = base();
    s.mix = 0.0;
    // Even with extreme shaping, mix=0 returns the dry input.
    s.attack_db = [12.0, 12.0, 12.0];
    s.sustain_db = [-12.0, -12.0, -12.0];
    let out = render_with(s, &sig);
    let residual = null_residual_db(&sig, &out);
    assert!(residual < -80.0, "mix=0 should null vs input; residual {residual:.2} dB");
}

// ---------------------------------------------------------------------------
// Done-bar 2 — low attack +12 raises LOW onset ratio only
// ---------------------------------------------------------------------------

#[test]
fn low_attack_raises_low_band_onset_ratio_only() {
    let (sig, onsets) = hit_train();

    let neutral = render_with(base(), &sig);
    let mut boosted_s = base();
    boosted_s.attack_db[0] = 12.0; // low band attack +12
    let boosted = render_with(boosted_s, &sig);

    let nb = split_bands(&neutral);
    let bb = split_bands(&boosted);

    let r0: Vec<f32> = (0..3).map(|b| peak_to_sustain_db(&nb[b], &onsets)).collect();
    let r1: Vec<f32> = (0..3).map(|b| peak_to_sustain_db(&bb[b], &onsets)).collect();

    // LOW band ratio rises clearly (attack transient boosted).
    assert!(
        r1[0] > r0[0] + 3.0,
        "low-band onset ratio should rise with +12 attack: {:.2} -> {:.2} dB",
        r0[0],
        r1[0]
    );
    // MID and HIGH bands essentially unchanged (within ±1 dB).
    assert!(
        (r1[1] - r0[1]).abs() <= 1.0,
        "mid-band ratio moved {:.2} dB (want ±1)",
        r1[1] - r0[1]
    );
    assert!(
        (r1[2] - r0[2]).abs() <= 1.0,
        "high-band ratio moved {:.2} dB (want ±1)",
        r1[2] - r0[2]
    );
}

// ---------------------------------------------------------------------------
// Done-bar 3 — mid sustain −12 lowers MID inter-onset RMS only
// ---------------------------------------------------------------------------

#[test]
fn mid_sustain_cut_lowers_mid_inter_onset_rms_only() {
    let (sig, onsets) = hit_train();

    let neutral = render_with(base(), &sig);
    let mut cut_s = base();
    cut_s.sustain_db[1] = -12.0; // mid band sustain -12
    let cut = render_with(cut_s, &sig);

    let nb = split_bands(&neutral);
    let cb = split_bands(&cut);

    let db = |x: f32| 20.0 * x.max(1e-9).log10();
    let n_rms: Vec<f32> = (0..3).map(|b| db(inter_onset_rms(&nb[b], &onsets))).collect();
    let c_rms: Vec<f32> = (0..3).map(|b| db(inter_onset_rms(&cb[b], &onsets))).collect();

    // MID inter-onset RMS drops clearly.
    assert!(
        c_rms[1] < n_rms[1] - 3.0,
        "mid inter-onset RMS should drop with -12 sustain: {:.2} -> {:.2} dB",
        n_rms[1],
        c_rms[1]
    );
    // LOW and HIGH inter-onset RMS essentially unchanged (within ±1 dB).
    assert!(
        (c_rms[0] - n_rms[0]).abs() <= 1.0,
        "low inter-onset RMS moved {:.2} dB (want ±1)",
        c_rms[0] - n_rms[0]
    );
    assert!(
        (c_rms[2] - n_rms[2]).abs() <= 1.0,
        "high inter-onset RMS moved {:.2} dB (want ±1)",
        c_rms[2] - n_rms[2]
    );
}

// ---------------------------------------------------------------------------
// Done-bar 4 — attack sweep is monotonic in the LOW onset ratio
// ---------------------------------------------------------------------------

#[test]
fn low_attack_sweep_is_monotonic() {
    let (sig, onsets) = hit_train();
    let ratio_for = |atk: f32| {
        let mut s = base();
        s.attack_db[0] = atk;
        let out = render_with(s, &sig);
        let bands = split_bands(&out);
        peak_to_sustain_db(&bands[0], &onsets)
    };
    let r_lo = ratio_for(-12.0);
    let r_mid = ratio_for(0.0);
    let r_hi = ratio_for(12.0);
    assert!(
        r_lo < r_mid && r_mid < r_hi,
        "low-band onset ratio must increase monotonically with attack gain: \
         {r_lo:.2} < {r_mid:.2} < {r_hi:.2} dB"
    );
}

// ---------------------------------------------------------------------------
// Solo isolates a single band
// ---------------------------------------------------------------------------

#[test]
fn solo_outputs_only_the_soloed_band() {
    let (sig, _) = hit_train();
    let mut s = base();
    s.solo = [false, true, false]; // mid solo
    let out = render_with(s, &sig);
    let bands = split_bands(&out);
    // The soloed (mid) band carries essentially all the energy.
    let e_low = rms_dbfs(&bands[0]);
    let e_mid = rms_dbfs(&bands[1]);
    let e_high = rms_dbfs(&bands[2]);
    assert!(
        e_mid > e_low + 12.0 && e_mid > e_high + 12.0,
        "mid solo should dominate: low {e_low:.1}, mid {e_mid:.1}, high {e_high:.1} dBFS"
    );
}

// ---------------------------------------------------------------------------
// Universal assertions on every preset render (+ write to renders/BANDAID/)
// ---------------------------------------------------------------------------

#[test]
fn presets_pass_universal_assertions() {
    let (sig, _) = hit_train();
    let presets = suite_core::presets::load_all(crate::presets::PRESET_JSON);
    assert!(presets.len() >= 6);
    for p in &presets {
        let s = crate::presets::settings_from_preset(p);
        let mut core = BandaidCore::new(SR);
        core.configure(&s);
        let safe = p.name.replace(' ', "_").replace('/', "-");
        let out = render_and_write("BANDAID", &safe, core, &sig, 512, SR as u32);
        assert_universal(&out);
    }
}

// ---------------------------------------------------------------------------
// SOUND-PASS regression: a hard attack boost PUNCHES (crest up) without clipping
// ---------------------------------------------------------------------------

/// A hot four-on-the-floor kick with all-band Attack +12 dB must drive the onset transient
/// *above unity* with a *higher* crest than the dry — proving the final clamp is the ±8.0
/// runaway guard, not the old ±0.999 ceiling (which would flat-top the transient at ~0 dBFS
/// and collapse the crest). Guards the CEILING policy in dsp.rs against regressions.
#[test]
fn boosted_transient_not_clipped() {
    let kick = testsig::synth_kick_loop(140.0, 2, SR);
    let neutral = render_with(base(), &kick);

    let mut s = base();
    s.attack_db = [12.0, 12.0, 12.0]; // hard attack boost, out 0 dB, mix 100 %
    let boosted = render_with(s, &kick);

    let peak = |b: &[f32]| b.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    let crest = |b: &[f32]| suite_core::harness::peak_dbfs(b) - rms_dbfs(b);
    let pk_n = peak(&neutral);
    let pk_b = peak(&boosted);

    // Attack boost lifts the transient peak clearly above the dry.
    assert!(pk_b > pk_n + 0.1, "attack boost should raise the peak: {pk_n:.3} -> {pk_b:.3}");
    // ...and above unity — impossible under a ±0.999 clamp in the mixed output path.
    assert!(pk_b > 1.05, "boosted transient peak {pk_b:.3} pinned near unity — clamp clipping?");
    // Finite and under the ±8.0 safety guard.
    assert!(!suite_core::harness::has_nan_or_inf(&boosted), "boosted output has NaN/inf");
    assert!(pk_b <= 8.001, "boosted peak {pk_b:.3} exceeds the ±8.0 safety guard");
    // Crest (punch) rises — the transient designer is adding attack, not squashing it.
    assert!(
        crest(&boosted) > crest(&neutral) + 1.0,
        "attack boost should increase crest: {:.2} -> {:.2} dB",
        crest(&neutral),
        crest(&boosted)
    );
}

// ---------------------------------------------------------------------------
// SOUND-PASS regression: stereo transient image stays put (normalized detector)
// ---------------------------------------------------------------------------

/// A level-panned kick (L, R = 0.7·L) run through the two independent per-channel cores with
/// a hard attack boost must keep its input L/R ratio at the output — the transient image must
/// not pump or drift. The detector's attack weight is the *normalized* overshoot
/// `(fast−slow)/slow`, which is scale-invariant, so both channels apply the SAME gain even
/// though they are unlinked; the image is preserved without needing an explicit stereo-link.
#[test]
fn stereo_transient_image_stable() {
    let left = testsig::synth_kick_loop(140.0, 2, SR);
    let right: Vec<f32> = left.iter().map(|&v| 0.7 * v).collect();

    let mut s = base();
    s.attack_db = [12.0, 12.0, 12.0];
    let mut cl = BandaidCore::new(SR);
    let mut cr = BandaidCore::new(SR);
    cl.configure(&s);
    cr.configure(&s);
    let ol: Vec<f32> = left.iter().map(|&x| cl.process_sample(x)).collect();
    let or_: Vec<f32> = right.iter().map(|&x| cr.process_sample(x)).collect();

    let rms = |b: &[f32]| (b.iter().map(|&v| (v * v) as f64).sum::<f64>() / b.len() as f64).sqrt();
    let ratio = (rms(&or_) / rms(&ol)) as f32;
    // Output ratio must track the 0.70 input pan within a tight tolerance (no image drift).
    assert!(
        (ratio - 0.70).abs() < 0.02,
        "stereo transient image drifted: out R/L {ratio:.4} (input 0.70)"
    );
}

// ---------------------------------------------------------------------------
// SOUND-PASS audition render (permanent infra, `#[ignore]`d in normal runs)
// ---------------------------------------------------------------------------

/// Render every factory preset AND `Settings::default()` over TWO genre-right transient
/// sources — an atmospheric-dnb break and a four-on-the-floor kick loop (140 BPM) — into
/// renders/_audition/BANDAID/<QVS_AUDITION_DIR or "before">/<preset>_{break,kick}.wav.
/// The kick loop is the real transient-punch test: it proves an attack-boosted onset
/// PUNCHES (higher crest, clean peak) instead of digitally clipping. Analyzed offline by
/// tools/audition.py; `Peak Slam Kick` deliberately exercises the safety clamp.
#[test]
#[ignore]
fn audition_render_musical_sources() {
    use suite_core::harness::{render_offline, render_path, write_wav};

    let sr = 48_000.0f32;
    let subdir = std::env::var("QVS_AUDITION_DIR").unwrap_or_else(|_| "before".into());

    let brk = testsig::synth_break(140.0, 4, sr);
    let kick = testsig::synth_kick_loop(140.0, 4, sr);

    let presets = suite_core::presets::load_all(crate::presets::PRESET_JSON);
    let mut jobs: Vec<(String, Settings)> = presets
        .iter()
        .map(|p| {
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");
            (fname, crate::presets::settings_from_preset(p))
        })
        .collect();
    jobs.push(("default".into(), Settings::default()));

    for (fname, s) in &jobs {
        for (src, tag) in [(&brk, "break"), (&kick, "kick")] {
            let mut core = BandaidCore::new(sr);
            core.configure(s);
            let out = render_offline(core, src, 512);
            let path = render_path("_audition/BANDAID", &format!("{subdir}/{fname}_{tag}"));
            write_wav(&path, &out, sr as u32).expect("write audition render");
        }
    }
}

// ---------------------------------------------------------------------------
// Fuzz / robustness: extreme + degenerate settings stay finite and bounded
// ---------------------------------------------------------------------------

#[test]
fn extremes_stay_finite_and_bounded() {
    let (sig, _) = hit_train();
    // Degenerate/inverted crossovers + everything maxed; must not NaN or exceed the ceiling.
    let s = Settings {
        xover_low: 20_000.0, // inverted vs high on purpose — configure must sanitize
        xover_high: 20.0,
        attack_db: [12.0, 12.0, 12.0],
        sustain_db: [12.0, 12.0, 12.0],
        solo: [true, true, true],
        det_scale: 0.5,
        mix: 1.0,
        out_db: 24.0,
    };
    let out = render_with(s, &sig);
    assert!(!suite_core::harness::has_nan_or_inf(&out), "output has NaN/inf");
    let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    // Clamp policy (TRIAGE 2026-07-08): the final clamp is a runaway/NaN guard at ±8.0
    // (≈ +18 dBFS), not a 0 dBFS ceiling — extreme-fuzz output is finite and ≤ that guard.
    assert!(peak <= 8.001, "output exceeds the +18 dBFS safety guard: peak {peak}");
}
