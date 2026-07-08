//! NERVE offline harness tests, incl. the PRD §4 done-bar: a published mod signal
//! measurably modulates a listening plugin's param (round-trip over two bus handles).

use crate::dsp::{NerveCore, Settings, Shape};
use nih_plug::prelude::{FloatParam, FloatRange};
use std::path::PathBuf;
use suite_core::bus::{new_instance_id, Bus, PluginKind};
use suite_core::modlisten::{Curve, ModRoutes, Route};

fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "qeynos-bus-nerve-{}-{}-{}",
        tag,
        std::process::id(),
        new_instance_id()
    ))
}

/// DONE-BAR: NERVE's LFO A, published to the tier-2 bus, drives a listener's "drive" param
/// (as a normalized offset) — instantiated in one process via two bus handles (two DLLs).
/// The listener's effective value must track the LFO in shape (spans the range) and rate
/// (crosses its base ~2×/s for a 2 Hz LFO).
#[test]
fn nerve_lfo_modulates_listener_drive_round_trip() {
    let path = temp_path("roundtrip");
    let pub_bus = Bus::open_or_create(&path).unwrap();
    let listen_bus = Bus::open_or_create(&path).unwrap();

    let src_id = new_instance_id();
    let idx = pub_bus.claim(src_id, PluginKind::Nerve, "NERVE").unwrap();

    // Publisher: NERVE core, LFO A = sine 2 Hz depth 1, everything else off.
    let sr = 48_000.0;
    let mut core = NerveCore::new(sr);
    let mut set = Settings::default();
    for l in set.lfo.iter_mut() {
        l.depth = 0.0;
    }
    set.lfo[0].shape = Shape::Sine;
    set.lfo[0].rate_hz = 2.0;
    set.lfo[0].depth = 1.0;
    set.lfo[0].synced = false;

    // Listener: a GRIT-style drive param, 0..24 dB, base 12 dB (normalized 0.5). Route it to
    // the source's stream 0 with depth 0.5 → normalized offset ∈ [-0.5, 0.5] → plain 0..24 dB.
    let drive = FloatParam::new("Drive", 12.0, FloatRange::Linear { min: 0.0, max: 24.0 });
    let mut routes = ModRoutes::new();
    routes.set(Route {
        param_id: "drive".into(),
        source_instance: src_id,
        source_index: 0,
        depth: 0.5,
        curve: Curve::Linear,
    });

    let block = 64usize;
    let blocks = sr as usize / block; // ~1 second
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut crossings = 0usize;
    let mut prev_sign = 0i32;
    for _ in 0..blocks {
        let outs = core.advance(block, &set);
        pub_bus.publish_mods(idx, &outs);
        pub_bus.beat(idx);

        let d = routes.modulated_float("drive", &drive, Some(&listen_bus));
        assert!(d.is_finite() && (0.0..=24.0).contains(&d), "drive out of range: {d}");
        min = min.min(d);
        max = max.max(d);
        let sign = if d > 12.0 { 1 } else { -1 };
        if prev_sign != 0 && sign != prev_sign {
            crossings += 1;
        }
        prev_sign = sign;
    }

    // Shape: the modulated drive spans (near) the whole range as the sine swings ±1.
    assert!(max > 22.0, "modulated drive should approach 24 dB, got max {max}");
    assert!(min < 2.0, "modulated drive should approach 0 dB, got min {min}");
    // Rate: a 2 Hz LFO crosses the 12 dB base ~4×/s → at least 3 in ~1 s.
    assert!(crossings >= 3, "expected ~2 Hz oscillation, saw {crossings} base crossings");

    // Control: with no route the listener sits at its base value, untouched.
    let empty = ModRoutes::new();
    let base = empty.modulated_float("drive", &drive, Some(&listen_bus));
    assert!((base - 12.0).abs() < 1e-4, "unrouted param must equal base 12 dB: {base}");

    pub_bus.release(idx, src_id);
    let _ = std::fs::remove_file(&path);
}

/// Two live NERVE sources publish distinct streams; a reader distinguishes them by instance
/// id (the shape X-RAY / the listen layer relies on for multi-source routing).
#[test]
fn two_nerve_sources_are_independently_addressable() {
    let path = temp_path("twosrc");
    let a = Bus::open_or_create(&path).unwrap();
    let b = Bus::open_or_create(&path).unwrap();
    let reader = Bus::open_or_create(&path).unwrap();

    let id_a = new_instance_id();
    let id_b = new_instance_id();
    let ia = a.claim(id_a, PluginKind::Nerve, "A").unwrap();
    let ib = b.claim(id_b, PluginKind::Nerve, "B").unwrap();

    a.publish_mods(ia, &[0.11; 8]);
    a.beat(ia);
    b.publish_mods(ib, &[0.99; 8]);
    b.beat(ib);

    assert_ne!(id_a, id_b, "instance ids must be distinct");
    let sa = reader.find_by_instance(id_a).unwrap();
    let sb = reader.find_by_instance(id_b).unwrap();
    assert_eq!(sa.label, "A");
    assert_eq!(sb.label, "B");
    assert!((sa.mods[3] - 0.11).abs() < 1e-6);
    assert!((sb.mods[3] - 0.99).abs() < 1e-6);

    a.release(ia, id_a);
    b.release(ib, id_b);
    let _ = std::fs::remove_file(&path);
}
