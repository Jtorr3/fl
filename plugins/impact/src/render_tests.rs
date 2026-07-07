//! Offline render harness tests (PRD §4): universal assertions on every preset render
//! (written to renders/IMPACT/) plus the two IMPACT-specific done-bar assertions:
//!   1. STFT-measured f0 track starts within 10% of f_start, ends within 5% of f_end.
//!   2. Retrigger mid-decay introduces no sample-to-sample step beyond the declick bound
//!      (compared against an otherwise identical no-retrigger render).

use crate::dsp::{KickVoice, Settings};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::harness::{assert_universal, render_path, write_wav};
use suite_core::presets::load_all;
use suite_core::stft::Stft;

const SR: f32 = 48_000.0;

/// Render a single voice: note-on at sample 0, optional retrigger at `retrig_at`.
fn render_note(s: &Settings, len: usize, retrig_at: Option<usize>) -> Vec<f32> {
    let mut v = KickVoice::new(SR);
    v.configure(s);
    v.note_on(1.0, None);
    let mut out = vec![0.0f32; len];
    for (i, o) in out.iter_mut().enumerate() {
        if Some(i) == retrig_at {
            v.note_on(1.0, None);
        }
        *o = v.process_sample();
    }
    out
}

fn max_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

/// Every factory preset renders (1.5 s) and passes the universal assertions; the WAVs land in
/// renders/IMPACT/ for later audition.
#[test]
fn every_preset_renders_and_passes_universal() {
    let len = (SR * 1.5) as usize;
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 5, "need >= 5 presets");
    for p in &presets {
        let s = settings_from_preset(p);
        let out = render_note(&s, len, None);
        assert_universal(&out);
        let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
        let path = render_path("IMPACT", &fname);
        write_wav(&path, &out, SR as u32).expect("write render");
    }
}

/// Done-bar #1: measured f0 starts within 10% of f_start and ends within 5% of f_end.
/// A clean single-sine config (no click/sub/transient) so the STFT peak is the body osc.
#[test]
fn pitch_track_hits_start_and_end() {
    let f_start = 120.0f32;
    let f_end = 55.0f32;
    let s = Settings {
        f_start,
        f_end,
        pitch_decay_ms: 350.0,
        pitch_curve: 0.5,
        length: 1.0,
        amp_decay_ms: 2500.0,
        amp_curve: 0.5,
        tone: 0.0,
        drive: 0.0,
        click_level: 0.0,
        transient: 0,
        sub_level: 0.0,
        clip_soft: true,
        ..Settings::default()
    };
    let len = (SR * 2.5) as usize;
    let audio = render_note(&s, len, None);
    assert_universal(&audio);

    // Measure a per-frame f0 track with the suite streaming STFT + quadratic peak interp.
    let n = 4096usize;
    let hop = 1024usize;
    let mut stft = Stft::new(n, hop);
    // Only search the low band around the fundamental (< 300 Hz).
    let max_k = ((300.0 * n as f32 / SR) as usize).min(n / 2 - 1).max(2);
    let mut frames: Vec<(f32, f32)> = Vec::new(); // (energy, f0 Hz)
    let mut cb = |spec: &mut [suite_core::stft::Complex<f32>]| {
        let mut best = 1usize;
        let mut best_m = 0.0f32;
        for k in 1..=max_k {
            let m = spec[k].norm();
            if m > best_m {
                best_m = m;
                best = k;
            }
        }
        let m0 = spec[best - 1].norm();
        let m1 = spec[best].norm();
        let m2 = spec[best + 1].norm();
        let denom = m0 - 2.0 * m1 + m2;
        let delta = if denom.abs() > 1e-12 { 0.5 * (m0 - m2) / denom } else { 0.0 };
        let f0 = (best as f32 + delta.clamp(-0.5, 0.5)) * SR / n as f32;
        frames.push((best_m, f0));
    };
    for &x in &audio {
        stft.process(x, &mut cb);
    }

    let max_e = frames.iter().fold(0.0f32, |a, &(e, _)| a.max(e));
    let thr = max_e * 0.05;
    let energetic: Vec<(f32, f32)> = frames.into_iter().filter(|&(e, _)| e > thr).collect();
    assert!(energetic.len() >= 4, "not enough energetic STFT frames: {}", energetic.len());

    // Start estimate: the pitch begins highest, so take the max f0 over the first few frames.
    let start_est = energetic.iter().take(5).fold(0.0f32, |a, &(_, f)| a.max(f));
    // End estimate: the last energetic frame (pitch has settled to f_end).
    let end_est = energetic.last().unwrap().1;

    let start_err = (start_est - f_start).abs() / f_start;
    let end_err = (end_est - f_end).abs() / f_end;
    assert!(
        start_err <= 0.10,
        "f0 start {start_est:.2} Hz not within 10% of f_start {f_start} (err {:.1}%)",
        start_err * 100.0
    );
    assert!(
        end_err <= 0.05,
        "f0 end {end_est:.2} Hz not within 5% of f_end {f_end} (err {:.1}%)",
        end_err * 100.0
    );
}

/// Done-bar #2: a mid-decay retrigger must not step more than the declick bound. We compare
/// the worst sample-to-sample delta of a retriggered render against a no-retrigger baseline;
/// a phase-continuous, declick-ramped retrigger stays within a small margin of it, whereas a
/// hard (un-declicked) retrigger would jump by a large fraction of the envelope level.
#[test]
fn retrigger_is_declicked() {
    // A stressful config: click + snap transient + sub + drive all active.
    let s = Settings {
        f_start: 220.0,
        f_end: 55.0,
        pitch_decay_ms: 40.0,
        amp_decay_ms: 500.0,
        tone: 0.1,
        drive: 0.3,
        click_level: 0.3,
        click_decay_ms: 12.0,
        click_freq: 3500.0,
        transient: 2,
        transient_level: 0.5,
        sub_level: 0.2,
        clip_soft: true,
        ..Settings::default()
    };
    let len = (SR * 0.5) as usize;
    let retrig_at = len / 2;

    let baseline = render_note(&s, len, None);
    let retrig = render_note(&s, len, Some(retrig_at));
    assert_universal(&baseline);
    assert_universal(&retrig);

    let base_step = max_step(&baseline);
    let retrig_step = max_step(&retrig);

    // The retrigger's worst step must stay within the declick bound: at most the baseline's
    // worst onset step plus a small margin. A gross (un-declicked) retrigger would produce a
    // step several times larger than the baseline onset.
    let bound = base_step * 1.6 + 0.03;
    assert!(
        retrig_step <= bound,
        "retrigger step {retrig_step:.4} exceeds declick bound {bound:.4} (baseline {base_step:.4})"
    );

    // Also localize: the worst step within the retrigger onset window must be no worse than a
    // normal kick onset's natural slew (the baseline's worst step) plus the same small margin.
    // A phase-reset or envelope-jump discontinuity would step by a large fraction of full scale,
    // far above this bound; a phase-continuous, ramped retrigger stays within it.
    let a = retrig_at.saturating_sub(64);
    let b = (retrig_at + 256).min(len);
    let local = max_step(&retrig[a..b]);
    assert!(
        local <= bound,
        "retrigger onset step {local:.4} exceeds declick bound {bound:.4} (baseline {base_step:.4})"
    );
}
