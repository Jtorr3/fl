//! CHAMBER done-bar + property tests (PRD §4, CHAMBER-specific + universal). Shared DSP core is
//! exercised exactly as shipped. Run under `cargo test --release` (build.ps1 test step).

use super::*;
use crate::presets::{settings_from_preset, PRESET_JSON};
use std::time::Instant;
use suite_core::fdn::measure_rt60;
use suite_core::harness::{assert_universal, null_residual_db};
use suite_core::presets::load_all;
use suite_core::testsig;

const SR: f32 = 48_000.0;

fn mono_sum(l: &[f32], r: &[f32]) -> Vec<f32> {
    l.iter().zip(r.iter()).map(|(&a, &b)| 0.5 * (a + b)).collect()
}

/// Done-bar (1): an impulse's first arrival (the direct path, image order 0) lands at the
/// geometric delay `r_direct / c` within ±1 sample.
#[test]
fn first_arrival_equals_direct_r_over_c() {
    let mut s = Settings::default();
    s.mix = 1.0;
    s.er_late = 0.0; // ER only — isolate the image cluster from the diffuse tail.
    s.predelay = 0.0;
    s.er_order = 3;
    // Place source & listener so the direct path is not hard-panned (both channels carry it).
    s.src_x = 3.0;
    s.src_y = 2.0;
    s.lis_x = 4.5;
    s.lis_y = 7.5;

    let r_direct = direct_distance(&s);
    let expected = (r_direct / SPEED * SR).round() as i64;

    let mut core = ChamberCore::new(SR);
    let mut imp = vec![0.0f32; expected as usize + 512];
    imp[0] = 1.0;
    let (l, r) = core.process_stereo(&imp, &s);

    // The direct path is the loudest (gain 1) and earliest arrival → global argmax = its onset.
    let mut best = (0usize, 0.0f32);
    for i in 0..l.len() {
        let m = l[i].abs().max(r[i].abs());
        if m > best.1 {
            best = (i, m);
        }
    }
    assert!(best.1 > 1.0e-3, "no arrival found (silent)");
    let diff = (best.0 as i64 - expected).abs();
    assert!(
        diff <= 1,
        "first arrival at sample {} but direct r/c = {} (r_direct {:.3} m, diff {} samples)",
        best.0,
        expected,
        r_direct,
        diff
    );
}

/// Done-bar (2): the measured late-tail RT60 is within ±25% of the Sabine prediction for two
/// rooms — a small dead room and a large live room.
#[test]
fn late_rt60_within_25_percent_of_sabine() {
    // Room A: small, absorptive (curtain floor) → short RT60.
    let mut small = Settings::default();
    small.w = 3.0;
    small.d = 3.5;
    small.h = 2.5;
    small.mat_walls = Material::Wood;
    small.mat_floor = Material::Curtain;
    small.mat_ceiling = Material::Wood;

    // Room B: large, live (wood) → long RT60.
    let mut large = Settings::default();
    large.w = 12.0;
    large.d = 16.0;
    large.h = 7.0;
    large.mat_walls = Material::Wood;
    large.mat_floor = Material::Wood;
    large.mat_ceiling = Material::Wood;

    for (label, base) in [("small dead", small), ("large live", large)] {
        let mut s = base;
        s.er_late = 1.0; // late field only — measure the diffuse tail RT60.
        s.mix = 1.0;
        s.predelay = 0.0;
        s.rt60_override = 0.0;

        let predicted = sabine_rt60(&s);
        let len = (((predicted * 2.0).max(0.6) * SR) as usize).max(SR as usize);
        let mut imp = vec![0.0f32; len];
        imp[0] = 1.0;

        let mut core = ChamberCore::new(SR);
        let (l, r) = core.process_stereo(&imp, &s);
        let ir = mono_sum(&l, &r);
        let measured = measure_rt60(&ir, SR)
            .unwrap_or_else(|| panic!("{label}: RT60 not measurable (tail too short?)"));
        let err = (measured - predicted).abs() / predicted;
        assert!(
            err <= 0.25,
            "{label}: measured RT60 {measured:.2}s vs Sabine {predicted:.2}s (err {:.1}%)",
            err * 100.0
        );
    }
}

/// Done-bar (3): dragging the source mid-render produces no click — the moving render's maximum
/// sample-to-sample delta stays close to a static reference (the rate-clamped delays + smoothed
/// gains guarantee continuity).
#[test]
fn moving_source_produces_no_click() {
    let n = (SR * 1.0) as usize;
    let input = testsig::sine(1_000.0, 0.6, n, SR);
    let block = 64usize;

    let render = |sweep: bool| -> Vec<f32> {
        let mut core = ChamberCore::new(SR);
        let mut out = Vec::with_capacity(n);
        let mut i = 0usize;
        while i < n {
            let t = i as f32 / n as f32;
            let mut s = Settings::default();
            s.mix = 1.0;
            // Sweep the source across the room width when sweeping, else hold at the midpoint.
            s.src_x = if sweep { 1.0 + 6.0 * t } else { 4.0 };
            core.configure(&s);
            let end = (i + block).min(n);
            for j in i..end {
                let (ol, _or) = core.process_sample(input[j], input[j], &s);
                out.push(ol);
            }
            i = end;
        }
        out
    };

    let max_delta = |sig: &[f32]| sig.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);

    let moving = render(true);
    let static_ref = render(false);
    assert!(moving.iter().all(|v| v.is_finite()));

    let md_move = max_delta(&moving);
    let md_static = max_delta(&static_ref);
    // Motion may add a little doppler chirp but never a discontinuity: bounded well under a click.
    assert!(
        md_move <= md_static * 2.0 + 0.02,
        "moving max sample delta {md_move:.4} >> static {md_static:.4} — motion introduced a click"
    );
    assert!(md_move < 0.3, "moving max sample delta {md_move:.4} too large (click?)");
}

/// Done-bar (4): `mix = 0` is an exact passthrough of the input (both channels).
#[test]
fn mix_zero_nulls_against_input() {
    let n = (SR * 2.0) as usize;
    let input = testsig::pink_noise(0.5, n, 4242);
    let mut s = Settings::default();
    s.mix = 0.0;
    s.out_db = 0.0;

    let mut core = ChamberCore::new(SR);
    let (l, r) = core.process_stereo(&input, &s);
    let res_l = null_residual_db(&input, &l);
    let res_r = null_residual_db(&input, &r);
    assert!(res_l < -80.0, "mix=0 L residual {res_l:.1} dB (want < -80)");
    assert!(res_r < -80.0, "mix=0 R residual {res_r:.1} dB (want < -80)");
}

/// The image count follows the 3-D L1 ball (7 / 25 / 63 at order 1 / 2 / 3).
#[test]
fn image_counts_match_order() {
    for (order, expect) in [(1usize, 7usize), (2, 25), (3, 63)] {
        let mut s = Settings::default();
        s.er_order = order;
        let mut core = ChamberCore::new(SR);
        core.configure(&s);
        assert_eq!(core.active_images(), expect, "order {order} image count");
    }
}

/// Extremes stay finite and ≤ 0 dBFS (bounds fuzz): biggest room, order 3, everything pushed.
#[test]
fn extremes_stay_bounded() {
    let input = testsig::pink_noise(0.9, (SR * 2.0) as usize, 13);
    let configs = [
        Settings {
            w: 40.0, d: 40.0, h: 20.0, er_order: 3, er_late: 0.5, distance: 3.0,
            predelay: 0.2, rt60_override: 12.0, width: 2.0, mix: 1.0, out_db: 12.0,
            mat_walls: Material::Concrete, mat_floor: Material::Glass, mat_ceiling: Material::Concrete,
            ..Settings::default()
        },
        Settings {
            w: 2.0, d: 2.0, h: 2.0, er_order: 3, er_late: 0.0, distance: 0.5,
            predelay: 0.0, rt60_override: 0.0, width: 0.0, mix: 1.0, out_db: 0.0,
            mat_walls: Material::Concrete, mat_floor: Material::Concrete, mat_ceiling: Material::Glass,
            ..Settings::default()
        },
    ];
    for (i, s) in configs.iter().enumerate() {
        let mut core = ChamberCore::new(SR);
        let (l, r) = core.process_stereo(&input, s);
        assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()), "cfg {i} not finite");
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &v| m.max(v.abs()));
        // Clamp policy (TRIAGE 2026-07-08): final clamp is a ±8.0 runaway/NaN guard
        // (≈ +18 dBFS), not a 0 dBFS ceiling — extreme fuzz asserts finite && ≤ the guard.
        assert!(peak <= 8.001, "cfg {i} peak {peak} exceeds the +18 dBFS safety guard");
    }
}

/// Every factory preset renders finite / non-silent / ≤ 0 dBFS over pink + chirp (universal bar).
#[test]
fn presets_pass_universal() {
    let pink = testsig::pink_noise(0.5, (SR * 3.0) as usize, 8181);
    let chirp = testsig::log_chirp(40.0, 12_000.0, 0.5, (SR * 3.0) as usize, SR);
    let presets = load_all(PRESET_JSON);
    assert!(presets.len() >= 6);
    for p in &presets {
        let s = settings_from_preset(p);
        for input in [&pink, &chirp] {
            let mut core = ChamberCore::new(SR);
            let (l, r) = core.process_stereo(input, &s);
            assert_universal(&l);
            assert_universal(&r);
        }
    }
}

/// PRD §4 CPU rule: bench mean `process()` per 512-sample block @48k (release). Records the
/// numbers for every ER order and enforces that the `Auto` order stays within 30% of the
/// real-time budget (else the design must drop the order — see `dsp::AUTO_ORDER`).
#[test]
fn cpu_budget_and_order() {
    let block = 512usize;
    let input = testsig::pink_noise(0.5, block, 5);
    let budget_ns = (block as f64 / SR as f64) * 1.0e9;

    let bench = |order: usize| -> (f64, f64, usize) {
        let mut s = Settings::default();
        s.er_order = order;
        s.mix = 0.4;
        let mut core = ChamberCore::new(SR);
        core.configure(&s);
        let images = core.active_images();
        // Warm up (fill delay lines / FDN).
        for _ in 0..60 {
            for j in 0..block {
                core.process_sample(input[j], input[j], &s);
            }
        }
        let iters = 400usize;
        let t0 = Instant::now();
        for _ in 0..iters {
            core.configure(&s);
            for j in 0..block {
                core.process_sample(input[j], input[j], &s);
            }
        }
        let mean_ns = t0.elapsed().as_nanos() as f64 / iters as f64;
        (mean_ns, mean_ns / budget_ns * 100.0, images)
    };

    println!("--- CHAMBER CPU bench (512-block @48k, budget {budget_ns:.0} ns) ---");
    let mut pct_by_order = [0.0f64; 4];
    for order in [1usize, 2, 3] {
        let (ns, pct, images) = bench(order);
        pct_by_order[order] = pct;
        println!("  order {order}: {ns:8.0} ns/block  {pct:5.1}% RT  ({images} images)");
    }
    let auto_pct = pct_by_order[AUTO_ORDER];
    println!("  AUTO_ORDER = {AUTO_ORDER} ({auto_pct:.1}% RT)");

    assert!(
        auto_pct < 30.0,
        "Auto ER order {AUTO_ORDER} costs {auto_pct:.1}% of the RT budget (>30%); drop AUTO_ORDER \
         per the PRD §4 CPU ladder (3 → 2 → 1 + bigger late field)"
    );
}
