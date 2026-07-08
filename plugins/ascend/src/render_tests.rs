//! Offline render harness tests (PRD §4): universal assertions on every preset render (written to
//! renders/ASCEND/) plus the ASCEND-specific done-bar assertions (SPECS "ASCEND" / PRD §4):
//!   1. Render 8 bars at 130 BPM with target = 8 → the spectral centroid rises monotonically
//!      (windowed, ±tolerance) over the countdown.
//!   2. The impact lands on the target bar ±5 ms, and (impact off) the auto-cut silences the
//!      sources within 50 ms after the boundary.
//!   3. Downlifter mode → the spectral centroid falls monotonically after the boundary.

use crate::dsp::{AscendEngine, Settings, SyncTarget, TransportFrame};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::harness::{assert_universal, render_path};
use suite_core::presets::load_all;
use suite_core::stft::Stft;

const SR: f32 = 48_000.0;
const BLOCK: usize = 512;

/// Samples spanning `bars` bars at `bpm` (4/4).
fn bars_to_samples(bars: f32, bpm: f32) -> usize {
    let secs = bars * 4.0 * (60.0 / bpm);
    (secs * SR).round() as usize
}

/// Bars advanced per sample at `bpm` (4/4).
fn bars_per_sample(bpm: f32) -> f64 {
    (bpm as f64 / 60.0 / SR as f64) / 4.0
}

/// Drive the engine block-by-block with a fake continuous 4/4 transport (à la the plugin's
/// `process`): resync bar position at each block start, advance per sample within the block.
fn render_transport(s: &Settings, bpm: f32, total: usize) -> (Vec<f32>, Vec<f32>) {
    let bps = bars_per_sample(bpm);
    let mut e = AscendEngine::new(SR);
    e.configure(s);
    let mut l = vec![0.0f32; total];
    let mut r = vec![0.0f32; total];
    let mut i = 0usize;
    while i < total {
        let bar_pos = i as f64 * bps;
        e.set_transport(TransportFrame { playing: true, bar_pos, bars_per_sample: bps });
        let end = (i + BLOCK).min(total);
        for j in i..end {
            let (a, b) = e.process_sample();
            l[j] = a;
            r[j] = b;
        }
        i = end;
    }
    (l, r)
}

fn mono(l: &[f32], r: &[f32]) -> Vec<f32> {
    l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect()
}

/// Accumulated-magnitude spectral centroid (Hz) over a slice via the suite STFT.
fn spectral_centroid(x: &[f32]) -> f32 {
    let n = 2048usize;
    let hop = 512usize;
    let mut stft = Stft::new(n, hop);
    let mut mag = vec![0.0f64; n / 2 + 1];
    let mut cb = |spec: &mut [suite_core::stft::Complex<f32>]| {
        for (k, m) in mag.iter_mut().enumerate() {
            *m += spec[k].norm() as f64;
        }
    };
    for &v in x {
        stft.process(v, &mut cb);
    }
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for (k, &m) in mag.iter().enumerate() {
        let f = k as f64 * SR as f64 / n as f64;
        num += f * m;
        den += m;
    }
    if den > 0.0 {
        (num / den) as f32
    } else {
        0.0
    }
}

/// Centroid measured in `windows` equal slices across `x`.
fn windowed_centroids(x: &[f32], windows: usize) -> Vec<f32> {
    let step = x.len() / windows;
    (0..windows)
        .map(|k| {
            let a = k * step;
            let b = if k + 1 == windows { x.len() } else { (k + 1) * step };
            spectral_centroid(&x[a..b])
        })
        .collect()
}

/// Small stereo-WAV writer (the suite `write_wav` is mono; ASCEND renders are stereo).
fn write_wav_stereo(path: &std::path::Path, interleaved: &[f32], sr: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    for &s in interleaved {
        w.write_sample(s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    }
    w.finalize()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}

/// Every factory preset renders (its own window at 130 BPM + a 1 s impact tail) and passes the
/// universal assertions; the stereo WAVs land in renders/ASCEND/ for later audition.
#[test]
fn every_preset_renders_and_passes_universal() {
    let bpm = 130.0;
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
    for p in &presets {
        let s = settings_from_preset(p);
        let countdown = bars_to_samples(s.window_bars(), bpm);
        let total = countdown + SR as usize; // + 1 s tail for the impact
        let (l, r) = render_transport(&s, bpm, total);
        assert_universal(&l);
        assert_universal(&r);
        let mut stereo = vec![0.0f32; total * 2];
        for i in 0..total {
            stereo[2 * i] = l[i];
            stereo[2 * i + 1] = r[i];
        }
        let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
        let path = render_path("ASCEND", &fname);
        write_wav_stereo(&path, &stereo, SR as u32).expect("write render");
    }
}

/// Free-run render: trigger the one-shot envelope (transport stopped) and capture `total`
/// samples as a mono mixdown. Used by the audition probe to hold a steady tonal source.
fn render_free_run(s: &Settings, total: usize) -> Vec<f32> {
    let mut e = AscendEngine::new(SR);
    e.configure(s);
    e.set_transport(TransportFrame { playing: false, bar_pos: 0.0, bars_per_sample: 0.0 });
    e.trigger_free();
    let mut m = vec![0.0f32; total];
    for j in 0..total {
        let (a, b) = e.process_sample();
        m[j] = 0.5 * (a + b);
    }
    m
}

/// SOUND-PASS audition render (permanent infra, `#[ignore]`d in normal runs).
///
/// Renders every factory preset AND `Settings::default()` as a full transport-driven riser
/// build (its own countdown window at 130 BPM + a 1 s impact tail, mono mixdown) into
/// renders/_audition/ASCEND/<QVS_AUDITION_DIR or "before">/<preset>.wav, plus a sustained
/// pure-saw tonal probe that exposes the tonal stack's aliasing.
///
/// The probe holds a fixed high pitch (C5 root ≈ 523 Hz, above any factory preset's peak) with
/// the SVF wide open (18 kHz) and the tonal source solo'd (all tonal, pure saw). Analyze it with
/// `tools/audition.py analyze <probe> --sine-probe 523`. NOTE: the tonal stack always sums a
/// root + a fifth (0.6·root + 0.4·fifth at 1.498·root), so the sine-probe's single "worst
/// inharmonic" bin is dominated by the FIFTH partial, not aliasing — read the harmonic ladder and
/// the sub-fundamental band ([<0.9·root]) for the true naive-saw alias floor instead.
#[test]
#[ignore]
fn audition_render_musical_sources() {
    let bpm = 130.0;
    let subdir = std::env::var("QVS_AUDITION_DIR").unwrap_or_else(|_| "before".into());

    // Every factory preset + the default state, as a full riser build.
    let presets = load_all(PRESET_JSON);
    let mut jobs: Vec<(String, Settings)> = presets
        .iter()
        .map(|p| (p.name.to_lowercase().replace([' ', '·', '-'], "_"), settings_from_preset(p)))
        .collect();
    jobs.push(("default".into(), Settings::default()));

    for (fname, s) in &jobs {
        let countdown = bars_to_samples(s.window_bars(), bpm);
        let total = countdown + SR as usize; // + 1 s impact tail
        let (l, r) = render_transport(s, bpm, total);
        let m = mono(&l, &r);
        let path = render_path("_audition/ASCEND", &format!("{subdir}/{fname}"));
        suite_core::harness::write_wav(&path, &m, SR as u32).expect("write audition render");
    }

    // Tonal aliasing probe: pure saw, tonal solo'd, filter wide open, fixed high pitch (C5 root
    // ≈ 523 Hz — higher than any factory preset reaches at env=1), no pitch rise so the pitch
    // stays steady while the free-run envelope swells the level. This maximally exposes the
    // naive-saw aliasing floor for tools/audition.py.
    let probe = Settings {
        key: 0,
        octave: 5, // C5 ≈ 523 Hz root, fifth ≈ 784 Hz
        balance: 1.0,           // all tonal
        color: 0.0,
        wave: 0.0,              // pure saw (worst case for aliasing)
        filter_start_hz: 18_000.0,
        filter_end_hz: 18_000.0, // SVF wide open throughout — never masks the aliasing
        rise_st: 0.0,           // steady pitch
        width: 0.0,
        impact_on: false,
        auto_cut: false,
        free_len_s: 3.0,
        level_db: -6.0,
        ..Settings::default()
    };
    let m = render_free_run(&probe, (SR * 3.0) as usize);
    let path = render_path("_audition/ASCEND", &format!("{subdir}/_saw_probe_c5"));
    suite_core::harness::write_wav(&path, &m, SR as u32).expect("write saw probe");
    eprintln!("[audition] wrote ASCEND renders + _saw_probe_c5 (root 523 Hz) to '{subdir}'");
}

/// Done-bar #1: 8 bars at 130 BPM, target = 8 → spectral centroid rises monotonically (windowed,
/// ±tolerance) over the countdown.
#[test]
fn centroid_rises_monotonically_over_countdown() {
    let bpm = 130.0;
    let s = Settings {
        sync: SyncTarget::Bars8,
        curve: 0.5,
        impact_on: false, // isolate the riser sources from the impact
        ..Settings::default()
    };
    let countdown = bars_to_samples(8.0, bpm);
    let (l, r) = render_transport(&s, bpm, countdown);
    assert_universal(&l);
    let m = mono(&l, &r);
    // Measure over the countdown, dropping the final block so the boundary sample is excluded.
    let span = &m[..countdown.saturating_sub(BLOCK)];
    let c = windowed_centroids(span, 8);
    // Monotonic within a 12% tolerance, and a clear net rise start→end.
    for k in 1..c.len() {
        assert!(
            c[k] >= c[k - 1] * 0.88,
            "centroid not monotonic at window {k}: {:?}",
            c
        );
    }
    assert!(
        c[c.len() - 1] > c[0] * 1.5,
        "centroid did not rise enough over the countdown: {:?}",
        c
    );
}

/// Done-bar #2a: the impact lands on the target bar ±5 ms. Isolated by differencing an
/// impact-on render against an otherwise-identical impact-off render (deterministic engine).
#[test]
fn impact_lands_on_target_bar() {
    let bpm = 130.0;
    let base = Settings {
        sync: SyncTarget::Bars8,
        auto_cut: true,
        ..Settings::default()
    };
    let on = Settings { impact_on: true, ..base };
    let off = Settings { impact_on: false, ..base };
    let countdown = bars_to_samples(8.0, bpm);
    let total = countdown + SR as usize / 2;
    let (lon, _) = render_transport(&on, bpm, total);
    let (loff, _) = render_transport(&off, bpm, total);

    // Impact-only signal = on − off (everything else is identical & deterministic).
    let diff: Vec<f32> = lon.iter().zip(&loff).map(|(a, b)| a - b).collect();
    let peak = diff.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    assert!(peak > 0.01, "no isolated impact found (peak {peak})");
    // Onset = first sample whose |impact| exceeds a small fraction of the impact peak.
    let thresh = peak * 0.02;
    let onset = diff.iter().position(|&v| v.abs() >= thresh).expect("impact onset");

    // The boundary (bar 8) sample the engine crosses at.
    let bps = bars_per_sample(bpm);
    let boundary = (8.0 / bps).round() as usize;
    let tol = (0.005 * SR as f64) as usize; // ±5 ms
    let delta = (onset as isize - boundary as isize).unsigned_abs();
    assert!(
        delta <= tol,
        "impact onset {onset} vs boundary {boundary} = {delta} samples (> {tol} = 5 ms)"
    );
}

/// Done-bar #2b: with auto-cut on (impact off), the sources fall silent within 50 ms after the
/// boundary.
#[test]
fn auto_cut_silences_after_boundary() {
    let bpm = 130.0;
    let s = Settings {
        sync: SyncTarget::Bars8,
        auto_cut: true,
        impact_on: false, // measure the SOURCE path only
        ..Settings::default()
    };
    let countdown = bars_to_samples(8.0, bpm);
    let total = countdown + SR as usize / 4;
    let (l, r) = render_transport(&s, bpm, total);
    let m = mono(&l, &r);

    let bps = bars_per_sample(bpm);
    let boundary = (8.0 / bps).round() as usize;
    // Pre-boundary the riser is at full tilt (loud reference).
    let pre_a = boundary.saturating_sub((0.05 * SR) as usize);
    let pre_peak = m[pre_a..boundary].iter().fold(0.0f32, |x, &v| x.max(v.abs()));
    assert!(pre_peak > 0.05, "riser was not loud before the boundary: {pre_peak}");
    // Window [boundary+10 ms, boundary+50 ms] must be effectively silent (auto-cut engaged).
    let a = boundary + (0.010 * SR) as usize;
    let b = boundary + (0.050 * SR) as usize;
    let post_peak = m[a..b.min(m.len())].iter().fold(0.0f32, |x, &v| x.max(v.abs()));
    assert!(
        post_peak < pre_peak * 0.05,
        "auto-cut did not silence sources 10–50 ms after boundary: post {post_peak} vs pre {pre_peak}"
    );
}

/// Done-bar #3: downlifter mode → spectral centroid falls monotonically after the boundary. The
/// fall begins at the song-start boundary (env starts full and decays over the window).
#[test]
fn downlifter_centroid_falls_monotonically() {
    let bpm = 130.0;
    let s = Settings {
        sync: SyncTarget::Bars8,
        downlifter: true,
        auto_cut: false,
        impact_on: false,
        curve: 0.5,
        filter_start_hz: 300.0,
        filter_end_hz: 9000.0,
        ..Settings::default()
    };
    let countdown = bars_to_samples(8.0, bpm);
    let (l, r) = render_transport(&s, bpm, countdown);
    assert_universal(&l);
    let m = mono(&l, &r);
    let span = &m[..countdown.saturating_sub(BLOCK)];
    let c = windowed_centroids(span, 8);
    for k in 1..c.len() {
        assert!(
            c[k] <= c[k - 1] * 1.12,
            "downlifter centroid not monotonically falling at window {k}: {:?}",
            c
        );
    }
    assert!(
        c[0] > c[c.len() - 1] * 1.5,
        "downlifter centroid did not fall enough: {:?}",
        c
    );
}
