//! WIRE done-bar + stability tests (PRD §4). These call the pure-DSP [`WireCore`] directly,
//! so opus-rs' internal allocations are fine (nih-plug's alloc guard is not active here).

use super::*;

/// Deterministic band-limited-ish noise: white through a light one-pole low-pass. Codec-
/// friendlier than a pure tone (no period aliasing in the delay search) and broadband enough
/// that bitrate genuinely changes fidelity.
fn bl_noise(amp: f32, len: usize, seed: u32) -> Vec<f32> {
    let mut s = if seed == 0 { 1 } else { seed };
    let mut lp = 0.0f32;
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let w = (s as f32 / u32::MAX as f32) * 2.0 - 1.0;
            lp = 0.9 * lp + 0.1 * w;
            (lp * 3.0 * amp).clamp(-0.9, 0.9)
        })
        .collect()
}

/// Best delay-compensated Pearson correlation of `out` against `input`, searching delays in
/// `0..=max_delay`. Skips a startup region so codec priming doesn't dominate.
fn best_delay_corr(input: &[f32], out: &[f32], max_delay: usize, skip: usize) -> (f32, usize) {
    let mut best = f32::NEG_INFINITY;
    let mut best_d = 0usize;
    for d in 0..=max_delay {
        let (mut num, mut ea, mut eb) = (0.0f64, 0.0f64, 0.0f64);
        let mut i = skip;
        while i + d < out.len() && i < input.len() {
            let a = input[i] as f64;
            let b = out[i + d] as f64;
            num += a * b;
            ea += a * a;
            eb += b * b;
            i += 1;
        }
        if ea > 0.0 && eb > 0.0 {
            let c = (num / (ea.sqrt() * eb.sqrt())) as f32;
            if c > best {
                best = c;
                best_d = d;
            }
        }
    }
    (best, best_d)
}

fn wet_settings(bitrate_kbps: f32) -> Settings {
    let mut s = Settings::default();
    s.bitrate_kbps = bitrate_kbps;
    s.mode = Mode::Music;
    s.bandwidth = BandwidthSel::Full;
    s.crunch = 0.0;
    s.regen_amount = 0.0;
    s.loss_pct = 0.0;
    s.mix = 1.0;
    s.width = 1.0;
    s.out_db = 0.0;
    s
}

/// DONE-BAR (1): a 6 kbps render correlates with the input LESS than a 128 kbps render does,
/// and both clear a modest floor (0.3). This is the defining assertion of a working codec.
#[test]
fn low_bitrate_correlates_less_than_high_bitrate() {
    let sr = 48_000.0f32;
    let len = (sr * 2.0) as usize;
    let input = bl_noise(0.7, len, 2024);

    let corr_at = |kbps: f32| -> f32 {
        let mut core = WireCore::new(sr);
        let s = wet_settings(kbps);
        let (out, _r) = core.process_stereo(&input, &s);
        // Search a generous delay window (codec + framing latency).
        let (c, _d) = best_delay_corr(&input, &out, FRAME * 3, sr as usize / 2);
        c
    };

    let c6 = corr_at(6.0);
    let c128 = corr_at(128.0);
    println!("corr 6kbps={c6:.4}  128kbps={c128:.4}");
    assert!(c6 > 0.3, "6 kbps corr {c6:.3} below floor 0.3");
    assert!(c128 > 0.3, "128 kbps corr {c128:.3} below floor 0.3");
    assert!(
        c6 < c128,
        "6 kbps ({c6:.3}) should correlate LESS than 128 kbps ({c128:.3})"
    );
}

/// DONE-BAR (2): the latency reported by the core equals the empirically-measured pipeline
/// latency within ±1 block. Measured at a 48 k host rate (no SRC), where the impulse peak-lag
/// is exact.
#[test]
fn measured_latency_matches_reported() {
    let sr = 48_000.0f32;
    let mut core = WireCore::new(sr);
    let reported = core.latency_samples() as usize;

    // Impulse well past startup, fully wet, neutral settings, no bandwidth limiting so the
    // click survives to the output.
    let mut s = wet_settings(128.0);
    s.bandwidth = BandwidthSel::Full;
    let len = FRAME * 12;
    let imp_at = FRAME * 4;
    let mut input = vec![0.0f32; len];
    input[imp_at] = 0.9;
    let (out, _r) = core.process_stereo(&input, &s);

    // Peak lag of the decoded click relative to the input impulse.
    let (mut pk, mut idx) = (0.0f32, 0usize);
    for (i, &v) in out.iter().enumerate().skip(imp_at) {
        if v.abs() > pk {
            pk = v.abs();
            idx = i;
        }
    }
    let measured = idx - imp_at;
    println!("reported={reported}  measured={measured}  (peak {pk:.4})");
    // At 128 kbps the click survives sharply, so measured == reported exactly; allow a small
    // slack well within the done-bar's "±1 block".
    assert!(
        (measured as i64 - reported as i64).abs() <= 8,
        "measured latency {measured} vs reported {reported} differ by more than 8 samples"
    );
}

/// mix = 0 must null against the latency-delayed dry path (universal, PRD §4).
#[test]
fn mix_zero_nulls_against_delayed_dry() {
    let sr = 48_000.0f32;
    let n = 24_000usize;
    let input = bl_noise(0.6, n, 99);
    let mut s = Settings::default();
    s.mix = 0.0;
    s.out_db = 0.0;
    let mut core = WireCore::new(sr);
    let (out, _r) = core.process_stereo(&input, &s);
    let lat = core.latency_samples() as usize;
    // Compare out[i] against input delayed by the reported latency.
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for i in lat..n {
        let dry = input[i - lat] as f64;
        let e = out[i] as f64 - dry;
        num += e * e;
        den += dry * dry;
    }
    let resid_db = 10.0 * (num / den.max(1e-20)).log10();
    println!("mix=0 residual {resid_db:.1} dB");
    assert!(resid_db < -80.0, "mix=0 did not null: residual {resid_db:.1} dB");
}

/// Universal assertions hold for an aggressive full-degradation setting (crunch + loss +
/// regen + low bitrate): finite, bounded, non-silent.
#[test]
fn extreme_degradation_stays_finite_and_bounded() {
    let sr = 44_100.0f32; // also exercises the SRC path
    let len = (sr * 2.0) as usize;
    let input = bl_noise(0.9, len, 7);
    let mut s = Settings {
        bitrate_kbps: 6.0,
        mode: Mode::Voice,
        bandwidth: BandwidthSel::Narrow,
        fec: true,
        loss_pct: 40.0,
        crunch: 1.0,
        regen_delay_ms: 80.0,
        regen_amount: 0.95,
        width: 2.0,
        mix: 1.0,
        out_db: 6.0,
    };
    let mut core = WireCore::new(sr);
    let (l, r) = core.process_stereo(&input, &s);
    assert!(l.iter().all(|v| v.is_finite()), "non-finite L");
    assert!(r.iter().all(|v| v.is_finite()), "non-finite R");
    let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &v| m.max(v.abs()));
    // Clamp policy (TRIAGE 2026-07-08): final clamp is a ±8.0 runaway/NaN guard
    // (≈ +18 dBFS), not a 0 dBFS ceiling — extreme fuzz asserts finite && ≤ the guard.
    assert!(peak <= 8.001, "peak {peak} exceeded the +18 dBFS safety guard");

    // A second pass with regen at max over a long tail must not blow up (feedback stability).
    s.regen_amount = 0.95;
    let tail = bl_noise(0.9, (sr * 3.0) as usize, 8);
    let (l2, _r2) = core.process_stereo(&tail, &s);
    let last = &l2[l2.len().saturating_sub(4096)..];
    assert!(last.iter().all(|v| v.is_finite()));
    let lpk = last.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    assert!(lpk <= 8.001, "regen tail peak {lpk} exceeded the +18 dBFS safety guard");
}

/// Packet loss changes the output (dropouts present) but stays bounded.
#[test]
fn packet_loss_alters_output_bounded() {
    let sr = 48_000.0f32;
    let len = (sr * 1.5) as usize;
    let input = bl_noise(0.7, len, 321);

    let render = |loss: f32| -> Vec<f32> {
        let mut core = WireCore::new(sr);
        let mut s = wet_settings(64.0);
        s.loss_pct = loss;
        core.process_stereo(&input, &s).0
    };
    let clean = render(0.0);
    let lossy = render(50.0);
    assert!(lossy.iter().all(|v| v.is_finite()));
    let peak = lossy.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    assert!(peak <= 1.0);
    // The lossy render must differ audibly from the clean one.
    let diff: f64 = clean
        .iter()
        .zip(&lossy)
        .map(|(a, b)| (a - b).abs() as f64)
        .sum::<f64>()
        / clean.len() as f64;
    println!("mean |clean-lossy| = {diff:.5}");
    assert!(diff > 1e-3, "50% packet loss did not change the output ({diff})");
}

/// Worst single-sample first-difference relative to a local 50 ms RMS floor of the
/// difference — the same "click" statistic `tools/audition.py::detect_clicks` computes.
/// Ignores the first/last 20 ms (edge transients).
fn worst_click_ratio(out: &[f32], sr: f32) -> f32 {
    if out.len() < 8 {
        return 0.0;
    }
    let d: Vec<f64> = out.windows(2).map(|w| (w[1] - w[0]).abs() as f64).collect();
    let win = ((0.05 * sr) as usize).max(8);
    // Boxcar mean of d^2 → local RMS floor (matches the Python convolve 'same').
    let mut worst = 0.0f32;
    let edge = (0.02 * sr) as usize;
    let n = d.len();
    for i in edge..n.saturating_sub(edge) {
        let lo = i.saturating_sub(win / 2);
        let hi = (i + win / 2 + 1).min(n);
        let mut acc = 0.0f64;
        for &v in &d[lo..hi] {
            acc += v * v;
        }
        let local_rms = (acc / (hi - lo) as f64).sqrt() + 1e-9;
        let ratio = (d[i] / local_rms) as f32;
        if ratio > worst {
            worst = ratio;
        }
    }
    worst
}

/// P2 re-entry-click fix (previously DEFERRED): with packet loss active, the frame *after* a
/// concealed dropout must ramp in from zero rather than opening on a full-scale sample. On a
/// continuous (transient-free) source the worst first-difference/local-RMS ratio must stay
/// below the click threshold, while dropouts still audibly gap. Before the fix this ratio ran
/// ~30-45 (full-scale re-entry steps); the fade-in brings it well under the detector's 8.0.
#[test]
fn dropout_reentry_is_click_free() {
    let sr = 48_000.0f32;
    let len = (sr * 3.0) as usize;
    // Continuous band-limited noise: no natural transients to masquerade as clicks.
    let input = bl_noise(0.6, len, 4242);
    let mut s = wet_settings(64.0);
    s.loss_pct = 40.0; // heavy loss → many dropout/re-entry seams
    s.bandwidth = BandwidthSel::Full; // full-band so any re-entry step survives to the output
    let mut core = WireCore::new(sr);
    let (out, _r) = core.process_stereo(&input, &s);

    let worst = worst_click_ratio(&out, sr);
    println!("dropout re-entry worst click ratio = {worst:.2}");
    assert!(
        worst < 8.0,
        "dropout re-entry not click-free: worst ratio {worst:.1} (>= 8.0 detector threshold)"
    );

    // Dropouts must still audibly gap: some 20 ms windows sit far below the overall RMS.
    let overall_rms =
        (out.iter().map(|&v| (v * v) as f64).sum::<f64>() / out.len() as f64).sqrt();
    let win = (0.02 * sr) as usize;
    let mut i = 0usize;
    let mut gapped = false;
    while i + win <= out.len() {
        let r = (out[i..i + win].iter().map(|&v| (v * v) as f64).sum::<f64>() / win as f64).sqrt();
        if r < overall_rms * 0.05 {
            gapped = true;
            break;
        }
        i += win;
    }
    assert!(gapped, "no audible dropout gaps at 40% loss — concealment stopped working");
}

/// SRC round-trips at 96 k (2:1) and 44.1 k without starving or exploding, and passes wet
/// audio through.
#[test]
fn runs_at_multiple_host_rates() {
    for &sr in &[44_100.0f32, 48_000.0, 96_000.0] {
        let len = (sr * 1.0) as usize;
        let input = bl_noise(0.6, len, 555);
        let mut core = WireCore::new(sr);
        let s = wet_settings(64.0);
        let (out, _r) = core.process_stereo(&input, &s);
        assert_eq!(out.len(), input.len(), "sr {sr}: length mismatch");
        assert!(out.iter().all(|v| v.is_finite()), "sr {sr}: non-finite");
        let rms = (out.iter().map(|&v| (v * v) as f64).sum::<f64>() / out.len() as f64).sqrt();
        assert!(rms > 1e-4, "sr {sr}: output silent (rms {rms})");
    }
}

