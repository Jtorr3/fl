//! OVERSEER factory presets — a set for the Node strip and a set for the Master bus
//! (≥5 across the pair, PRD §1.4 step 6). Flat-JSON blobs parsed by `suite_core::presets`;
//! the same lists drive the GUI selectors and the offline render tests. Values are plain
//! (dB, Hz, ratio, 0..1 mix).

use suite_core::presets::Preset;

use crate::eq::EqSettings;
use crate::master::{BandComp, MasterSettings};
use crate::node::NodeSettings;

/// Node-strip presets (menu order). OVERSEER-ENRICH thematic banks: each preset carries a
/// `"category"` tag so the Node preset bar filters by the current instrument type. ≥6 per
/// common type (KICK / BASS / VOCAL / PAD / PERC / BUS), purpose-named to the user's taste.
pub const NODE_PRESET_JSON: &[&str] = &[
    // ---- KICK ----------------------------------------------------------------
    r#"{ "name": "Warehouse Thump", "category": "KICK",
         "low_freq": 55.0, "low_gain": 4.0, "b1_freq": 350.0, "b1_gain": -4.0, "b1_q": 1.2,
         "b2_freq": 2500.0, "b2_gain": 1.0, "b2_q": 1.0, "high_freq": 9000.0, "high_gain": 0.0,
         "threshold": -16.0, "ratio": 4.0, "knee": 6.0, "attack": 4.0, "release": 110.0,
         "makeup": 3.0, "drive": 4.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Rumble Bed Glue", "category": "KICK",
         "low_freq": 45.0, "low_gain": 3.0, "b1_freq": 250.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 2000.0, "b2_gain": 0.0, "b2_q": 0.9, "high_freq": 8000.0, "high_gain": -1.0,
         "threshold": -22.0, "ratio": 2.5, "knee": 8.0, "attack": 20.0, "release": 200.0,
         "makeup": 2.0, "drive": 1.0, "width": 0.1, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Psy Click Forward", "category": "KICK",
         "low_freq": 60.0, "low_gain": 2.0, "b1_freq": 500.0, "b1_gain": -2.0, "b1_q": 1.1,
         "b2_freq": 4000.0, "b2_gain": 4.0, "b2_q": 0.9, "high_freq": 9000.0, "high_gain": 2.0,
         "threshold": -18.0, "ratio": 5.0, "knee": 4.0, "attack": 1.0, "release": 90.0,
         "makeup": 3.0, "drive": 5.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Sub Punch", "category": "KICK",
         "low_freq": 50.0, "low_gain": 5.0, "b1_freq": 400.0, "b1_gain": -3.0, "b1_q": 1.2,
         "b2_freq": 3000.0, "b2_gain": 1.0, "b2_q": 1.0, "high_freq": 9000.0, "high_gain": 0.0,
         "threshold": -20.0, "ratio": 4.0, "knee": 5.0, "attack": 6.0, "release": 120.0,
         "makeup": 3.0, "drive": 3.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Distorted Stomp", "category": "KICK",
         "low_freq": 55.0, "low_gain": 3.0, "b1_freq": 350.0, "b1_gain": -2.0, "b1_q": 1.1,
         "b2_freq": 2800.0, "b2_gain": 2.0, "b2_q": 1.0, "high_freq": 9000.0, "high_gain": 1.0,
         "threshold": -16.0, "ratio": 6.0, "knee": 3.0, "attack": 3.0, "release": 100.0,
         "makeup": 2.0, "drive": 8.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Tight Techno Kick", "category": "KICK",
         "low_freq": 58.0, "low_gain": 3.0, "b1_freq": 300.0, "b1_gain": -3.0, "b1_q": 1.2,
         "b2_freq": 3000.0, "b2_gain": 2.0, "b2_q": 1.0, "high_freq": 9000.0, "high_gain": 0.0,
         "threshold": -18.0, "ratio": 4.0, "knee": 5.0, "attack": 2.0, "release": 100.0,
         "makeup": 3.0, "drive": 3.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    // ---- BASS ----------------------------------------------------------------
    r#"{ "name": "Rolling Reese", "category": "BASS",
         "low_freq": 70.0, "low_gain": 2.0, "b1_freq": 250.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 1500.0, "b2_gain": 1.0, "b2_q": 0.9, "high_freq": 8000.0, "high_gain": 0.0,
         "threshold": -20.0, "ratio": 3.0, "knee": 6.0, "attack": 15.0, "release": 180.0,
         "makeup": 2.0, "drive": 3.0, "width": 0.4, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Sub Weight", "category": "BASS",
         "low_freq": 60.0, "low_gain": 3.0, "b1_freq": 300.0, "b1_gain": -1.0, "b1_q": 0.9,
         "b2_freq": 1200.0, "b2_gain": 0.0, "b2_q": 0.8, "high_freq": 8000.0, "high_gain": -1.0,
         "threshold": -22.0, "ratio": 2.5, "knee": 8.0, "attack": 20.0, "release": 200.0,
         "makeup": 2.0, "drive": 1.0, "width": 0.2, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Growl Mid Bass", "category": "BASS",
         "low_freq": 80.0, "low_gain": 1.0, "b1_freq": 400.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 1200.0, "b2_gain": 3.0, "b2_q": 1.0, "high_freq": 9000.0, "high_gain": 1.0,
         "threshold": -18.0, "ratio": 4.0, "knee": 5.0, "attack": 10.0, "release": 160.0,
         "makeup": 2.0, "drive": 5.0, "width": 0.3, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Dark Sub Sine", "category": "BASS",
         "low_freq": 55.0, "low_gain": 2.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 1500.0, "b2_gain": 0.0, "b2_q": 0.8, "high_freq": 8000.0, "high_gain": -3.0,
         "threshold": -24.0, "ratio": 2.0, "knee": 8.0, "attack": 25.0, "release": 220.0,
         "makeup": 1.0, "drive": 0.0, "width": 0.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Plucky 808", "category": "BASS",
         "low_freq": 65.0, "low_gain": 3.0, "b1_freq": 300.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 1800.0, "b2_gain": 1.0, "b2_q": 0.9, "high_freq": 9000.0, "high_gain": 0.0,
         "threshold": -18.0, "ratio": 3.0, "knee": 6.0, "attack": 8.0, "release": 150.0,
         "makeup": 2.0, "drive": 2.0, "width": 0.3, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Warehouse Bassline", "category": "BASS",
         "low_freq": 72.0, "low_gain": 2.0, "b1_freq": 250.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 1400.0, "b2_gain": 1.0, "b2_q": 0.9, "high_freq": 8000.0, "high_gain": 0.0,
         "threshold": -20.0, "ratio": 3.5, "knee": 6.0, "attack": 12.0, "release": 170.0,
         "makeup": 2.0, "drive": 4.0, "width": 0.3, "trim": 0.0, "mix": 1.0 }"#,
    // ---- VOCAL ---------------------------------------------------------------
    r#"{ "name": "Drowned Ghost Sit", "category": "VOCAL",
         "low_freq": 100.0, "low_gain": -3.0, "b1_freq": 350.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 4000.0, "b2_gain": 2.0, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 1.0,
         "threshold": -22.0, "ratio": 3.0, "knee": 10.0, "attack": 10.0, "release": 150.0,
         "makeup": 3.0, "drive": 2.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Upfront Dark Pop", "category": "VOCAL",
         "low_freq": 120.0, "low_gain": -2.0, "b1_freq": 400.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 5000.0, "b2_gain": 4.0, "b2_q": 0.8, "high_freq": 12000.0, "high_gain": 3.0,
         "threshold": -20.0, "ratio": 4.0, "knee": 6.0, "attack": 5.0, "release": 140.0,
         "makeup": 3.0, "drive": 2.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Tape Choir Bed", "category": "VOCAL",
         "low_freq": 90.0, "low_gain": -1.0, "b1_freq": 300.0, "b1_gain": -1.0, "b1_q": 0.9,
         "b2_freq": 3500.0, "b2_gain": 1.0, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 1.0,
         "threshold": -24.0, "ratio": 2.0, "knee": 12.0, "attack": 20.0, "release": 220.0,
         "makeup": 2.0, "drive": 3.0, "width": 1.3, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Sibilant Tamed", "category": "VOCAL",
         "low_freq": 110.0, "low_gain": -2.0, "b1_freq": 350.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 6500.0, "b2_gain": -3.0, "b2_q": 1.2, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -22.0, "ratio": 3.0, "knee": 8.0, "attack": 8.0, "release": 150.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Whisper Close", "category": "VOCAL",
         "low_freq": 130.0, "low_gain": -3.0, "b1_freq": 400.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 5000.0, "b2_gain": 3.0, "b2_q": 0.8, "high_freq": 12000.0, "high_gain": 2.0,
         "threshold": -18.0, "ratio": 4.0, "knee": 6.0, "attack": 3.0, "release": 130.0,
         "makeup": 3.0, "drive": 2.0, "width": 0.8, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Grief Lead Vox", "category": "VOCAL",
         "low_freq": 100.0, "low_gain": -2.0, "b1_freq": 350.0, "b1_gain": -3.0, "b1_q": 1.1,
         "b2_freq": 4000.0, "b2_gain": 2.0, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 1.0,
         "threshold": -22.0, "ratio": 3.0, "knee": 10.0, "attack": 8.0, "release": 150.0,
         "makeup": 3.0, "drive": 3.0, "width": 1.1, "trim": 0.0, "mix": 1.0 }"#,
    // ---- PAD -----------------------------------------------------------------
    r#"{ "name": "Grief Wash", "category": "PAD",
         "low_freq": 80.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": -1.0, "b1_q": 0.8,
         "b2_freq": 4000.0, "b2_gain": 1.0, "b2_q": 0.6, "high_freq": 12000.0, "high_gain": 1.0,
         "threshold": -24.0, "ratio": 2.0, "knee": 12.0, "attack": 25.0, "release": 220.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.5, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Afterlife Wide", "category": "PAD",
         "low_freq": 70.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 5000.0, "b2_gain": 1.0, "b2_q": 0.6, "high_freq": 13000.0, "high_gain": 2.0,
         "threshold": -26.0, "ratio": 1.6, "knee": 12.0, "attack": 30.0, "release": 250.0,
         "makeup": 2.0, "drive": 0.0, "width": 1.8, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Frozen Choir Pad", "category": "PAD",
         "low_freq": 90.0, "low_gain": -1.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 3000.0, "b2_gain": 1.0, "b2_q": 0.7, "high_freq": 12000.0, "high_gain": 1.0,
         "threshold": -24.0, "ratio": 2.0, "knee": 12.0, "attack": 25.0, "release": 230.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.4, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Dark Drone Bed", "category": "PAD",
         "low_freq": 60.0, "low_gain": 1.0, "b1_freq": 250.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 2500.0, "b2_gain": 0.0, "b2_q": 0.7, "high_freq": 9000.0, "high_gain": -2.0,
         "threshold": -26.0, "ratio": 1.8, "knee": 12.0, "attack": 30.0, "release": 250.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.2, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Shimmer Air", "category": "PAD",
         "low_freq": 90.0, "low_gain": 0.0, "b1_freq": 400.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 6000.0, "b2_gain": 2.0, "b2_q": 0.6, "high_freq": 14000.0, "high_gain": 3.0,
         "threshold": -24.0, "ratio": 2.0, "knee": 10.0, "attack": 25.0, "release": 220.0,
         "makeup": 2.0, "drive": 0.0, "width": 1.6, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Mourning Strings", "category": "PAD",
         "low_freq": 80.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": -1.0, "b1_q": 0.9,
         "b2_freq": 2500.0, "b2_gain": 1.0, "b2_q": 0.7, "high_freq": 11000.0, "high_gain": 1.0,
         "threshold": -24.0, "ratio": 2.0, "knee": 10.0, "attack": 25.0, "release": 220.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.5, "trim": 0.0, "mix": 1.0 }"#,
    // ---- PERC ----------------------------------------------------------------
    r#"{ "name": "Warehouse Tops", "category": "PERC",
         "low_freq": 200.0, "low_gain": -6.0, "b1_freq": 800.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 6000.0, "b2_gain": 2.0, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 3.0,
         "threshold": -20.0, "ratio": 3.0, "knee": 4.0, "attack": 1.0, "release": 70.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.2, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Psy Hats", "category": "PERC",
         "low_freq": 300.0, "low_gain": -8.0, "b1_freq": 900.0, "b1_gain": -2.0, "b1_q": 1.0,
         "b2_freq": 8000.0, "b2_gain": 3.0, "b2_q": 0.7, "high_freq": 12000.0, "high_gain": 4.0,
         "threshold": -22.0, "ratio": 3.0, "knee": 3.0, "attack": 0.5, "release": 60.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.3, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Clap Snap", "category": "PERC",
         "low_freq": 150.0, "low_gain": -2.0, "b1_freq": 500.0, "b1_gain": 1.0, "b1_q": 1.0,
         "b2_freq": 3000.0, "b2_gain": 3.0, "b2_q": 0.9, "high_freq": 9000.0, "high_gain": 2.0,
         "threshold": -20.0, "ratio": 4.0, "knee": 4.0, "attack": 3.0, "release": 120.0,
         "makeup": 2.0, "drive": 2.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Break Kit Glue", "category": "PERC",
         "low_freq": 90.0, "low_gain": 0.0, "b1_freq": 400.0, "b1_gain": -1.0, "b1_q": 0.9,
         "b2_freq": 3500.0, "b2_gain": 2.0, "b2_q": 0.8, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -20.0, "ratio": 3.0, "knee": 6.0, "attack": 2.0, "release": 100.0,
         "makeup": 2.0, "drive": 2.0, "width": 1.1, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Ride Wash", "category": "PERC",
         "low_freq": 400.0, "low_gain": -6.0, "b1_freq": 1000.0, "b1_gain": -1.0, "b1_q": 0.9,
         "b2_freq": 7000.0, "b2_gain": 2.0, "b2_q": 0.7, "high_freq": 13000.0, "high_gain": 3.0,
         "threshold": -22.0, "ratio": 2.5, "knee": 5.0, "attack": 2.0, "release": 90.0,
         "makeup": 2.0, "drive": 0.0, "width": 1.3, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Rim Click", "category": "PERC",
         "low_freq": 250.0, "low_gain": -4.0, "b1_freq": 700.0, "b1_gain": 0.0, "b1_q": 1.0,
         "b2_freq": 4000.0, "b2_gain": 3.0, "b2_q": 0.9, "high_freq": 10000.0, "high_gain": 2.0,
         "threshold": -20.0, "ratio": 4.0, "knee": 4.0, "attack": 1.0, "release": 80.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    // ---- BUS -----------------------------------------------------------------
    r#"{ "name": "Drum Bus Glue", "category": "BUS",
         "low_freq": 90.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 2500.0, "b2_gain": 1.0, "b2_q": 0.7, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -20.0, "ratio": 2.0, "knee": 10.0, "attack": 25.0, "release": 200.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.2, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Master-ish Glue", "category": "BUS",
         "low_freq": 80.0, "low_gain": 1.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 3000.0, "b2_gain": 1.0, "b2_q": 0.7, "high_freq": 12000.0, "high_gain": 1.0,
         "threshold": -22.0, "ratio": 2.0, "knee": 12.0, "attack": 30.0, "release": 220.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.1, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Parallel Smash", "category": "BUS",
         "low_freq": 90.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 2500.0, "b2_gain": 2.0, "b2_q": 0.8, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -30.0, "ratio": 6.0, "knee": 4.0, "attack": 5.0, "release": 120.0,
         "makeup": 4.0, "drive": 4.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Warm Sum", "category": "BUS",
         "low_freq": 70.0, "low_gain": 1.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 3000.0, "b2_gain": 1.0, "b2_q": 0.7, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -24.0, "ratio": 1.8, "knee": 12.0, "attack": 30.0, "release": 220.0,
         "makeup": 2.0, "drive": 2.0, "width": 1.2, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Dark Group", "category": "BUS",
         "low_freq": 85.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 2500.0, "b2_gain": 0.0, "b2_q": 0.7, "high_freq": 9000.0, "high_gain": -2.0,
         "threshold": -22.0, "ratio": 2.0, "knee": 10.0, "attack": 25.0, "release": 200.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.1, "trim": 0.0, "mix": 1.0 }"#,
    r#"{ "name": "Tight Group Comp", "category": "BUS",
         "low_freq": 90.0, "low_gain": 0.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 2800.0, "b2_gain": 1.0, "b2_q": 0.8, "high_freq": 10000.0, "high_gain": 1.0,
         "threshold": -20.0, "ratio": 3.0, "knee": 6.0, "attack": 10.0, "release": 140.0,
         "makeup": 2.0, "drive": 1.0, "width": 1.0, "trim": 0.0, "mix": 1.0 }"#,
];

/// Master-bus presets (menu order). OVERSEER-ENRICH: tagged by session THEME so the Master
/// preset bar filters by the inferred theme (DARK-TECHNO / DNB-BREAKS / AMBIENT /
/// HOUSE-GROOVE / GENERIC).
pub const MASTER_PRESET_JSON: &[&str] = &[
    // ---- DARK-TECHNO ---------------------------------------------------------
    r#"{ "name": "Warehouse Master", "category": "DARK-TECHNO",
         "low_freq": 50.0, "low_gain": 1.5, "b1_freq": 300.0, "b1_gain": -1.5, "b1_q": 1.0,
         "b2_freq": 3500.0, "b2_gain": 1.0, "b2_q": 0.8, "high_freq": 10000.0, "high_gain": -1.0,
         "xo_low": 150.0, "xo_high": 2800.0,
         "b1_thr": -20.0, "b1_ratio": 2.5, "b1_makeup": 2.0,
         "b2_thr": -18.0, "b2_ratio": 2.0, "b2_makeup": 1.5,
         "b3_thr": -16.0, "b3_ratio": 2.0, "b3_makeup": 1.5,
         "knee": 6.0, "attack": 15.0, "release": 160.0,
         "ceiling": -1.0, "lim_release": 100.0, "mix": 1.0 }"#,
    r#"{ "name": "Dark Peak Time", "category": "DARK-TECHNO",
         "low_freq": 48.0, "low_gain": 2.0, "b1_freq": 350.0, "b1_gain": -2.0, "b1_q": 1.1,
         "b2_freq": 3000.0, "b2_gain": 1.0, "b2_q": 0.8, "high_freq": 9000.0, "high_gain": -1.5,
         "xo_low": 140.0, "xo_high": 2600.0,
         "b1_thr": -22.0, "b1_ratio": 2.5, "b1_makeup": 2.5,
         "b2_thr": -20.0, "b2_ratio": 2.2, "b2_makeup": 2.0,
         "b3_thr": -18.0, "b3_ratio": 2.0, "b3_makeup": 1.5,
         "knee": 5.0, "attack": 12.0, "release": 150.0,
         "ceiling": -0.8, "lim_release": 90.0, "mix": 1.0 }"#,
    // ---- DNB-BREAKS ----------------------------------------------------------
    r#"{ "name": "Neurofunk Master", "category": "DNB-BREAKS",
         "low_freq": 45.0, "low_gain": 2.0, "b1_freq": 500.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 4000.0, "b2_gain": 2.0, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 2.5,
         "xo_low": 120.0, "xo_high": 2800.0,
         "b1_thr": -24.0, "b1_ratio": 3.0, "b1_makeup": 3.0,
         "b2_thr": -22.0, "b2_ratio": 2.5, "b2_makeup": 2.5,
         "b3_thr": -20.0, "b3_ratio": 2.5, "b3_makeup": 2.5,
         "knee": 4.0, "attack": 5.0, "release": 110.0,
         "ceiling": -0.5, "lim_release": 70.0, "mix": 1.0 }"#,
    r#"{ "name": "Liquid Roller", "category": "DNB-BREAKS",
         "low_freq": 50.0, "low_gain": 1.5, "b1_freq": 450.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 3500.0, "b2_gain": 1.5, "b2_q": 0.8, "high_freq": 12000.0, "high_gain": 2.0,
         "xo_low": 130.0, "xo_high": 2900.0,
         "b1_thr": -22.0, "b1_ratio": 2.5, "b1_makeup": 2.0,
         "b2_thr": -20.0, "b2_ratio": 2.0, "b2_makeup": 1.5,
         "b3_thr": -18.0, "b3_ratio": 2.0, "b3_makeup": 1.5,
         "knee": 6.0, "attack": 8.0, "release": 130.0,
         "ceiling": -0.8, "lim_release": 80.0, "mix": 1.0 }"#,
    // ---- AMBIENT -------------------------------------------------------------
    r#"{ "name": "Ambient Bed", "category": "AMBIENT",
         "low_freq": 60.0, "low_gain": 0.5, "b1_freq": 400.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 4000.0, "b2_gain": 0.5, "b2_q": 0.6, "high_freq": 12000.0, "high_gain": 1.0,
         "xo_low": 200.0, "xo_high": 3200.0,
         "b1_thr": -24.0, "b1_ratio": 1.5, "b1_makeup": 0.5,
         "b2_thr": -24.0, "b2_ratio": 1.5, "b2_makeup": 0.5,
         "b3_thr": -24.0, "b3_ratio": 1.5, "b3_makeup": 0.5,
         "knee": 12.0, "attack": 30.0, "release": 260.0,
         "ceiling": -1.5, "lim_release": 200.0, "mix": 1.0 }"#,
    r#"{ "name": "Drone Master", "category": "AMBIENT",
         "low_freq": 55.0, "low_gain": 1.0, "b1_freq": 300.0, "b1_gain": 0.0, "b1_q": 0.8,
         "b2_freq": 3000.0, "b2_gain": 0.5, "b2_q": 0.6, "high_freq": 10000.0, "high_gain": 0.5,
         "xo_low": 170.0, "xo_high": 3000.0,
         "b1_thr": -26.0, "b1_ratio": 1.6, "b1_makeup": 0.5,
         "b2_thr": -26.0, "b2_ratio": 1.6, "b2_makeup": 0.5,
         "b3_thr": -26.0, "b3_ratio": 1.6, "b3_makeup": 0.5,
         "knee": 12.0, "attack": 35.0, "release": 280.0,
         "ceiling": -1.5, "lim_release": 220.0, "mix": 1.0 }"#,
    // ---- HOUSE-GROOVE --------------------------------------------------------
    r#"{ "name": "House Punch", "category": "HOUSE-GROOVE",
         "low_freq": 55.0, "low_gain": 1.5, "b1_freq": 400.0, "b1_gain": -1.0, "b1_q": 1.0,
         "b2_freq": 4000.0, "b2_gain": 1.5, "b2_q": 0.8, "high_freq": 11000.0, "high_gain": 2.0,
         "xo_low": 160.0, "xo_high": 2900.0,
         "b1_thr": -22.0, "b1_ratio": 2.5, "b1_makeup": 2.0,
         "b2_thr": -20.0, "b2_ratio": 2.0, "b2_makeup": 1.5,
         "b3_thr": -18.0, "b3_ratio": 2.0, "b3_makeup": 1.5,
         "knee": 6.0, "attack": 8.0, "release": 120.0,
         "ceiling": -0.8, "lim_release": 90.0, "mix": 1.0 }"#,
    r#"{ "name": "Groove Glue", "category": "HOUSE-GROOVE",
         "low_freq": 60.0, "low_gain": 1.0, "b1_freq": 400.0, "b1_gain": -0.5, "b1_q": 0.9,
         "b2_freq": 3500.0, "b2_gain": 1.0, "b2_q": 0.8, "high_freq": 12000.0, "high_gain": 1.5,
         "xo_low": 170.0, "xo_high": 3000.0,
         "b1_thr": -22.0, "b1_ratio": 2.2, "b1_makeup": 1.5,
         "b2_thr": -20.0, "b2_ratio": 2.0, "b2_makeup": 1.5,
         "b3_thr": -20.0, "b3_ratio": 2.0, "b3_makeup": 1.5,
         "knee": 8.0, "attack": 15.0, "release": 160.0,
         "ceiling": -0.9, "lim_release": 120.0, "mix": 1.0 }"#,
    // ---- GENERIC -------------------------------------------------------------
    r#"{ "name": "Gentle Master", "category": "GENERIC",
         "low_freq": 60.0, "low_gain": 0.5, "b1_freq": 400.0, "b1_gain": 0.0, "b1_q": 0.9,
         "b2_freq": 4000.0, "b2_gain": 0.5, "b2_q": 0.7, "high_freq": 12000.0, "high_gain": 1.0,
         "xo_low": 200.0, "xo_high": 3000.0,
         "b1_thr": -22.0, "b1_ratio": 1.6, "b1_makeup": 1.0,
         "b2_thr": -22.0, "b2_ratio": 1.6, "b2_makeup": 1.0,
         "b3_thr": -22.0, "b3_ratio": 1.6, "b3_makeup": 1.0,
         "knee": 10.0, "attack": 25.0, "release": 220.0,
         "ceiling": -1.0, "lim_release": 150.0, "mix": 1.0 }"#,
    r#"{ "name": "Loud & Proud", "category": "GENERIC",
         "low_freq": 45.0, "low_gain": 2.0, "b1_freq": 500.0, "b1_gain": -2.0, "b1_q": 1.1,
         "b2_freq": 3000.0, "b2_gain": 2.5, "b2_q": 0.8, "high_freq": 9000.0, "high_gain": 3.0,
         "xo_low": 120.0, "xo_high": 2500.0,
         "b1_thr": -26.0, "b1_ratio": 3.0, "b1_makeup": 4.0,
         "b2_thr": -24.0, "b2_ratio": 3.0, "b2_makeup": 4.0,
         "b3_thr": -22.0, "b3_ratio": 3.0, "b3_makeup": 4.0,
         "knee": 4.0, "attack": 5.0, "release": 100.0,
         "ceiling": -0.3, "lim_release": 60.0, "mix": 1.0 }"#,
];

fn eq_from(p: &Preset, d: &EqSettings) -> EqSettings {
    let g = |k: &str, f: f32| p.get(k).unwrap_or(f);
    EqSettings {
        low_freq: g("low_freq", d.low_freq),
        low_gain: g("low_gain", d.low_gain),
        b1_freq: g("b1_freq", d.b1_freq),
        b1_gain: g("b1_gain", d.b1_gain),
        b1_q: g("b1_q", d.b1_q),
        b2_freq: g("b2_freq", d.b2_freq),
        b2_gain: g("b2_gain", d.b2_gain),
        b2_q: g("b2_q", d.b2_q),
        high_freq: g("high_freq", d.high_freq),
        high_gain: g("high_gain", d.high_gain),
    }
}

/// Build [`NodeSettings`] from a parsed Node preset (defaults fill omitted keys).
pub fn node_settings_from_preset(p: &Preset) -> NodeSettings {
    let d = NodeSettings::default();
    let g = |k: &str, f: f32| p.get(k).unwrap_or(f);
    NodeSettings {
        eq: eq_from(p, &d.eq),
        comp_threshold: g("threshold", d.comp_threshold),
        comp_ratio: g("ratio", d.comp_ratio),
        comp_knee: g("knee", d.comp_knee),
        comp_attack: g("attack", d.comp_attack),
        comp_release: g("release", d.comp_release),
        comp_makeup: g("makeup", d.comp_makeup),
        drive_db: g("drive", d.drive_db),
        width: g("width", d.width),
        trim_db: g("trim", d.trim_db),
        mix: g("mix", d.mix),
    }
}

/// Build [`MasterSettings`] from a parsed Master preset.
pub fn master_settings_from_preset(p: &Preset) -> MasterSettings {
    let d = MasterSettings::default();
    let g = |k: &str, f: f32| p.get(k).unwrap_or(f);
    MasterSettings {
        eq: eq_from(p, &d.eq),
        xo_low: g("xo_low", d.xo_low),
        xo_high: g("xo_high", d.xo_high),
        bands: [
            BandComp {
                threshold: g("b1_thr", d.bands[0].threshold),
                ratio: g("b1_ratio", d.bands[0].ratio),
                makeup: g("b1_makeup", d.bands[0].makeup),
            },
            BandComp {
                threshold: g("b2_thr", d.bands[1].threshold),
                ratio: g("b2_ratio", d.bands[1].ratio),
                makeup: g("b2_makeup", d.bands[1].makeup),
            },
            BandComp {
                threshold: g("b3_thr", d.bands[2].threshold),
                ratio: g("b3_ratio", d.bands[2].ratio),
                makeup: g("b3_makeup", d.bands[2].makeup),
            },
        ],
        comp_knee: g("knee", d.comp_knee),
        comp_attack: g("attack", d.comp_attack),
        comp_release: g("release", d.comp_release),
        drive_db: g("drive", d.drive_db),
        ceiling_db: g("ceiling", d.ceiling_db),
        limiter_release: g("lim_release", d.limiter_release),
        mix: g("mix", d.mix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::presets::load_all;

    #[test]
    fn at_least_five_presets_across_the_pair() {
        let n = load_all(NODE_PRESET_JSON).len();
        let m = load_all(MASTER_PRESET_JSON).len();
        assert!(n + m >= 5, "need >=5 presets across the pair, got {}", n + m);
    }

    #[test]
    fn thematic_banks_have_at_least_six_per_common_type() {
        let node = load_all(NODE_PRESET_JSON);
        for cat in ["KICK", "BASS", "VOCAL", "PAD", "PERC", "BUS"] {
            let n = node
                .iter()
                .filter(|p| p.category.as_deref() == Some(cat))
                .count();
            assert!(n >= 6, "type bank {cat} has only {n} presets (need >=6)");
        }
        // Every node preset is tagged.
        assert!(
            node.iter().all(|p| p.category.is_some()),
            "all node bank presets must carry a category tag"
        );
        // Master presets tagged by theme; every theme profile represented.
        let master = load_all(MASTER_PRESET_JSON);
        for theme in ["DARK-TECHNO", "DNB-BREAKS", "AMBIENT", "HOUSE-GROOVE", "GENERIC"] {
            let n = master
                .iter()
                .filter(|p| p.category.as_deref() == Some(theme))
                .count();
            assert!(n >= 1, "theme bank {theme} is empty");
        }
    }

    /// Every EQ field, compared with a loose epsilon.
    fn eq_diffs(a: &EqSettings, b: &EqSettings) -> usize {
        let fs = [
            (a.low_freq, b.low_freq), (a.low_gain, b.low_gain),
            (a.b1_freq, b.b1_freq), (a.b1_gain, b.b1_gain), (a.b1_q, b.b1_q),
            (a.b2_freq, b.b2_freq), (a.b2_gain, b.b2_gain), (a.b2_q, b.b2_q),
            (a.high_freq, b.high_freq), (a.high_gain, b.high_gain),
        ];
        fs.iter().filter(|(x, y)| (x - y).abs() > 1e-3).count()
    }

    /// Count differing `NodeSettings` fields (all float), for the default-diff and
    /// pairwise-distinctness quality gates.
    fn node_diffs(a: &NodeSettings, b: &NodeSettings) -> usize {
        let fs = [
            (a.comp_threshold, b.comp_threshold), (a.comp_ratio, b.comp_ratio),
            (a.comp_knee, b.comp_knee), (a.comp_attack, b.comp_attack),
            (a.comp_release, b.comp_release), (a.comp_makeup, b.comp_makeup),
            (a.drive_db, b.drive_db), (a.width, b.width),
            (a.trim_db, b.trim_db), (a.mix, b.mix),
        ];
        eq_diffs(&a.eq, &b.eq) + fs.iter().filter(|(x, y)| (x - y).abs() > 1e-3).count()
    }

    /// Count differing `MasterSettings` fields (all float).
    fn master_diffs(a: &MasterSettings, b: &MasterSettings) -> usize {
        let mut n = eq_diffs(&a.eq, &b.eq);
        let fs = [
            (a.xo_low, b.xo_low), (a.xo_high, b.xo_high),
            (a.comp_knee, b.comp_knee), (a.comp_attack, b.comp_attack),
            (a.comp_release, b.comp_release), (a.ceiling_db, b.ceiling_db),
            (a.limiter_release, b.limiter_release), (a.mix, b.mix),
        ];
        n += fs.iter().filter(|(x, y)| (x - y).abs() > 1e-3).count();
        for k in 0..3 {
            if (a.bands[k].threshold - b.bands[k].threshold).abs() > 1e-3 { n += 1; }
            if (a.bands[k].ratio - b.bands[k].ratio).abs() > 1e-3 { n += 1; }
            if (a.bands[k].makeup - b.bands[k].makeup).abs() > 1e-3 { n += 1; }
        }
        n
    }

    /// Full PRESET-EXPANSION quality gate over the Node bank: every preset differs from
    /// the default in >=4 params, differs from EVERY other in >=2 (no near-duplicates),
    /// and names are unique. (Universal render assertions ride in lib.rs render_tests.)
    #[test]
    fn node_bank_meets_expansion_quality_gate() {
        let presets = load_all(NODE_PRESET_JSON);
        let d = NodeSettings::default();
        let settings: Vec<NodeSettings> =
            presets.iter().map(node_settings_from_preset).collect();
        for (p, s) in presets.iter().zip(&settings) {
            let diffs = node_diffs(s, &d);
            assert!(diffs >= 4, "node preset '{}' differs from default in only {diffs}", p.name);
        }
        for i in 0..settings.len() {
            for j in (i + 1)..settings.len() {
                let diffs = node_diffs(&settings[i], &settings[j]);
                assert!(
                    diffs >= 2,
                    "node presets '{}' and '{}' differ in only {diffs} (near-duplicate)",
                    presets[i].name, presets[j].name
                );
                assert_ne!(presets[i].name, presets[j].name, "duplicate node preset name");
            }
        }
    }

    /// Full PRESET-EXPANSION quality gate over the Master bank.
    #[test]
    fn master_bank_meets_expansion_quality_gate() {
        let presets = load_all(MASTER_PRESET_JSON);
        let d = MasterSettings::default();
        let settings: Vec<MasterSettings> =
            presets.iter().map(master_settings_from_preset).collect();
        for (p, s) in presets.iter().zip(&settings) {
            let diffs = master_diffs(s, &d);
            assert!(diffs >= 4, "master preset '{}' differs from default in only {diffs}", p.name);
        }
        for i in 0..settings.len() {
            for j in (i + 1)..settings.len() {
                let diffs = master_diffs(&settings[i], &settings[j]);
                assert!(
                    diffs >= 2,
                    "master presets '{}' and '{}' differ in only {diffs} (near-duplicate)",
                    presets[i].name, presets[j].name
                );
                assert_ne!(presets[i].name, presets[j].name, "duplicate master preset name");
            }
        }
    }
}
