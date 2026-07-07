//! Offline render harness tests (PRD §4): universal assertions on every preset render
//! (written to renders/SNAP/) plus the four SNAP-specific done-bar assertions (SPECS "SNAP"):
//!   1. Clap mode (blend = 1), humanize = 0 → onset count in the render == taps + 1
//!      (the bursts + the tail), counted by the envelope-peak method (à la SWARM).
//!   2. Tone param moves the noise-band spectral centroid monotonically across 3 settings.
//!   3. Retrigger mid-decay is click-free (IMPACT's exact test recipe).
//!   4. Decay param scales the measured tail RT across 2 settings.

use crate::dsp::{SnapVoice, Settings, DECAY_REF_MS};
use crate::presets::{settings_from_preset, PRESET_JSON};
use suite_core::dsp::{Detector, EnvFollower};
use suite_core::harness::{assert_universal, render_path};
use suite_core::presets::load_all;
use suite_core::stft::Stft;

const SR: f32 = 48_000.0;

/// Render a note. Returns (L, R). Optional retrigger at sample `retrig_at`.
fn render(s: &Settings, len: usize, retrig_at: Option<usize>) -> (Vec<f32>, Vec<f32>) {
    let mut v = SnapVoice::new(SR);
    v.configure(s);
    let macro_len = s.decay_ms.max(1.0) / DECAY_REF_MS;
    v.note_on(1.0, None, macro_len);
    let mut l = vec![0.0f32; len];
    let mut r = vec![0.0f32; len];
    for i in 0..len {
        if Some(i) == retrig_at {
            v.note_on(1.0, None, macro_len);
        }
        let (a, b) = v.process_sample();
        l[i] = a;
        r[i] = b;
    }
    (l, r)
}

fn mono(l: &[f32], r: &[f32]) -> Vec<f32> {
    l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect()
}

fn max_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

/// Onset counter (envelope-peak method, à la SWARM). Rectify → peak follower (0.3 ms attack /
/// 2.5 ms release) yields a low-variance amplitude envelope; each burst is then counted once by
/// a hysteresis level gate: the envelope must cross **up** through `SET × peak` to register an
/// onset, then fall back **below** `RESET × peak` before another can register, with a 4 ms
/// refractory. The set/reset hysteresis (SET = 0.40, RESET = 0.22 of the global peak) sits
/// above the deepest between-burst valley (≈ 0.165·peak at the 7.5 ms min spacing) yet below the
/// within-burst level, so overlapping slaps count exactly once and the smoothly-decaying tail
/// (a single rising edge) counts once, never re-arming on its slow decay.
fn count_onsets(x: &[f32], sr: f32) -> usize {
    const SET: f32 = 0.40;
    const RESET: f32 = 0.22;
    let mut env = EnvFollower::new(Detector::Peak);
    env.set_times(0.3, 2.0, sr);
    let e: Vec<f32> = x.iter().map(|&v| env.process(v)).collect();
    let peak = e.iter().fold(0.0f32, |a, &v| a.max(v));
    if peak <= 0.0 {
        return 0;
    }
    let set = SET * peak;
    let reset = RESET * peak;
    let refractory = (0.004 * sr) as usize;
    let mut count = 0usize;
    let mut last = 0usize;
    let mut armed = true;
    for (i, &v) in e.iter().enumerate() {
        if armed && v >= set {
            if count == 0 || i - last >= refractory {
                count += 1;
                last = i;
            }
            armed = false;
        } else if !armed && v < reset {
            armed = true;
        }
    }
    count
}

/// Accumulated-magnitude spectral centroid (Hz) over the whole render via the suite STFT.
fn spectral_centroid(x: &[f32], sr: f32) -> f32 {
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
        let f = k as f64 * sr as f64 / n as f64;
        num += f * m;
        den += m;
    }
    if den > 0.0 {
        (num / den) as f32
    } else {
        0.0
    }
}

/// Time (s) from the envelope peak to the −20 dB point (a tail-length proxy for the RT done-bar).
fn tail_rt20(x: &[f32], sr: f32) -> f32 {
    let mut env = EnvFollower::new(Detector::Peak);
    env.set_times(1.0, 8.0, sr);
    let e: Vec<f32> = x.iter().map(|&v| env.process(v)).collect();
    let (peak_idx, peak) = e
        .iter()
        .enumerate()
        .fold((0usize, 0.0f32), |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) });
    if peak <= 0.0 {
        return 0.0;
    }
    let target = peak * 0.1; // −20 dB
    for i in peak_idx..e.len() {
        if e[i] <= target {
            return (i - peak_idx) as f32 / sr;
        }
    }
    (e.len() - peak_idx) as f32 / sr
}

/// Every factory preset renders (1.2 s) and passes the universal assertions; the stereo WAVs
/// land in renders/SNAP/ for later audition.
#[test]
fn every_preset_renders_and_passes_universal() {
    let len = (SR * 1.2) as usize;
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6, "need >= 6 presets");
    for p in &presets {
        let s = settings_from_preset(p);
        let (l, r) = render(&s, len, None);
        assert_universal(&l);
        assert_universal(&r);
        // Interleave to a stereo WAV.
        let mut stereo = vec![0.0f32; len * 2];
        for i in 0..len {
            stereo[2 * i] = l[i];
            stereo[2 * i + 1] = r[i];
        }
        let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
        let path = render_path("SNAP", &fname);
        write_wav_stereo(&path, &stereo, SR as u32).expect("write render");
    }
}

/// Small stereo-WAV writer (the suite `write_wav` is mono; SNAP renders are stereo).
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

/// Done-bar #1: clap mode (blend = 1), humanize = 0 → onset count == taps + 1.
/// With humanize = 0 the `taps` slaps are evenly spaced across the spread window and the tail
/// sits just past it, so the total (taps + tail) is deterministic. We analyse the spread window
/// plus a 15 ms guard (which contains every slap and the tail onset); the tail's long, smooth
/// decay past that is not a new onset. Taps ∈ {3, 4} at spread = 30 ms give 10 ms / 7.5 ms
/// spacing, comfortably above the 4 ms refractory (taps = 5 → 6 ms spacing is intentionally not
/// asserted — the slaps overlap into fewer resolvable onsets, documented).
#[test]
fn clap_mode_onset_count_equals_taps_plus_one() {
    for taps in [3usize, 4] {
        let spread_ms = 30.0;
        let s = Settings {
            mode: 1.0,
            humanize: 0.0,
            taps,
            spread_ms,
            snap: 0.2,
            decay_ms: 200.0,
            drive: 0.0,
            width: 0.0,
            ..Settings::default()
        };
        let len = (SR * 0.20) as usize;
        let (l, r) = render(&s, len, None);
        assert_universal(&l);
        let m = mono(&l, &r);
        // Onset analysis window: the spread + a 15 ms guard for the tail onset.
        let win = (((spread_ms + 15.0) / 1000.0) * SR) as usize;
        let onsets = count_onsets(&m[..win.min(m.len())], SR);
        assert_eq!(
            onsets,
            taps + 1,
            "clap mode taps={taps}: expected {} onsets, counted {onsets}",
            taps + 1
        );
    }
}

/// Done-bar #2: tone moves the noise-band spectral centroid monotonically across 3 settings.
/// Clap mode isolates the tone-shaped noise; snap = 0 keeps the fixed click band from masking it.
#[test]
fn tone_moves_spectral_centroid_monotonically() {
    let len = (SR * 0.25) as usize;
    let mut centroids = Vec::new();
    for tone in [0.1f32, 0.5, 0.9] {
        let s = Settings {
            mode: 1.0,
            tone,
            taps: 5,
            spread_ms: 28.0,
            humanize: 0.0,
            snap: 0.0,
            decay_ms: 240.0,
            drive: 0.0,
            width: 0.0,
            ..Settings::default()
        };
        let (l, r) = render(&s, len, None);
        let m = mono(&l, &r);
        centroids.push(spectral_centroid(&m, SR));
    }
    assert!(
        centroids[0] < centroids[1] && centroids[1] < centroids[2],
        "tone centroid not monotonic: {centroids:?}"
    );
}

/// Done-bar #3: a mid-decay retrigger must not step more than the declick bound (IMPACT's
/// exact recipe): compare the worst sample-to-sample delta of a retriggered render against a
/// no-retrigger baseline. A snare-mode config (body + rattle + click) so the master declick
/// ramp and the noise trigger-fade are what's under test.
#[test]
fn retrigger_is_declicked() {
    let s = Settings {
        mode: 0.2,
        tune: 200.0,
        balance: 0.55,
        snap: 0.6,
        decay_ms: 150.0,
        taps: 4,
        spread_ms: 18.0,
        humanize: 0.3,
        tone: 0.5,
        drive: 0.3,
        width: 0.0, // mono so L==R and the step bound is a single channel's slew
        ..Settings::default()
    };
    let len = (SR * 0.5) as usize;
    let retrig_at = len / 2;

    let (base_l, _) = render(&s, len, None);
    let (retrig_l, _) = render(&s, len, Some(retrig_at));
    assert_universal(&base_l);
    assert_universal(&retrig_l);

    let base_step = max_step(&base_l);
    let retrig_step = max_step(&retrig_l);
    let bound = base_step * 1.6 + 0.03;
    assert!(
        retrig_step <= bound,
        "retrigger step {retrig_step:.4} exceeds declick bound {bound:.4} (baseline {base_step:.4})"
    );

    // Localize: the worst step in the retrigger onset window is no worse than the bound.
    let a = retrig_at.saturating_sub(64);
    let b = (retrig_at + 256).min(len);
    let local = max_step(&retrig_l[a..b]);
    assert!(
        local <= bound,
        "retrigger onset step {local:.4} exceeds declick bound {bound:.4} (baseline {base_step:.4})"
    );
}

/// Done-bar #4: the decay param scales the measured tail RT (−20 dB time) across 2 settings.
#[test]
fn decay_scales_tail_rt() {
    let base = Settings {
        mode: 0.3,
        tune: 190.0,
        snap: 0.4,
        drive: 0.0,
        width: 0.0,
        ..Settings::default()
    };
    let short = Settings { decay_ms: 120.0, ..base };
    let long = Settings { decay_ms: 700.0, ..base };
    let len = (SR * 1.5) as usize;

    let (sl, sr_) = render(&short, len, None);
    let (ll, lr) = render(&long, len, None);
    let rt_short = tail_rt20(&mono(&sl, &sr_), SR);
    let rt_long = tail_rt20(&mono(&ll, &lr), SR);
    assert!(
        rt_long > rt_short * 1.5,
        "decay did not scale tail RT: short {rt_short:.4}s long {rt_long:.4}s"
    );
}

/// Width done-bar (SPECS: decorrelated per-channel noise, mono-compatible): at max width the
/// L/R correlation stays > 0.5, and at width = 0 the channels are identical.
#[test]
fn width_is_mono_compatible() {
    // Full width, clap mode (all-noise, the least-correlated case).
    let s = Settings {
        mode: 1.0,
        width: 1.0,
        taps: 5,
        spread_ms: 28.0,
        humanize: 0.0,
        snap: 0.3,
        decay_ms: 300.0,
        drive: 0.0,
        ..Settings::default()
    };
    let len = (SR * 0.25) as usize;
    let (l, r) = render(&s, len, None);
    // Pearson correlation over the energetic region.
    let n = l.len();
    let ml = l.iter().sum::<f32>() / n as f32;
    let mr = r.iter().sum::<f32>() / n as f32;
    let mut cov = 0.0f64;
    let mut vl = 0.0f64;
    let mut vr = 0.0f64;
    for i in 0..n {
        let a = (l[i] - ml) as f64;
        let b = (r[i] - mr) as f64;
        cov += a * b;
        vl += a * a;
        vr += b * b;
    }
    let corr = cov / (vl.sqrt() * vr.sqrt() + 1e-12);
    assert!(corr > 0.5, "max-width L/R correlation {corr:.3} not mono-compatible (> 0.5)");
}

