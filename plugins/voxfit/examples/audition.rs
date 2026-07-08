//! SOUND-PASS audition render harness for VOXFIT.
//!
//! Renders the default state + every factory preset over synth_vocal and harder synthesized
//! cases (breathy, sibilant, high female range) into `renders/_audition/VOXFIT/<subdir>/`.
//! `<subdir>` comes from `QVS_AUD` (default `before`).
//!
//! ```text
//! QVS_AUD=before cargo run -p voxfit --example audition --release
//! QVS_AUD=after  cargo run -p voxfit --example audition --release
//! ```

use suite_core::dsp::Svf;
use suite_core::harness::write_wav;
use suite_core::presets::load_all;
use suite_core::testsig::{synth_vocal, white_noise};
use voxfit::dsp::{Settings, VoxFitCore};
use voxfit::presets::{settings_from_preset, PRESET_JSON};

const SR: f32 = 48_000.0;

fn render(input: &[f32], s: &Settings) -> Vec<f32> {
    let mut core = VoxFitCore::new(SR);
    let mut buf = input.to_vec();
    core.process_mono(&mut buf, s);
    buf
}

/// Vowel tone with HP-noise sibilant bursts (esses) — de-esser / air probe.
fn sibilant(f0: f32, secs: f32) -> Vec<f32> {
    let len = (SR * secs) as usize;
    let vowel = synth_vocal(f0, len, SR);
    let noise = white_noise(1.0, len, 0x51B1_A5E1);
    let mut hp = Svf::new();
    hp.set(6000.0, 0.707, SR);
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / SR;
        let phase = (t * 2.5).fract();
        let gate = if phase < 0.13 { 1.0 } else { 0.0 };
        let sib = hp.process(noise[i]).hp * 0.5 * gate;
        out.push((vowel[i] * 0.7 + sib).clamp(-0.98, 0.98));
    }
    out
}

/// Breathy vocal: vocal tone + band-limited breath-noise bed.
fn breathy(f0: f32, secs: f32) -> Vec<f32> {
    let len = (SR * secs) as usize;
    let voc = synth_vocal(f0, len, SR);
    let noise = white_noise(1.0, len, 0x0B1E_A711);
    let mut hp = Svf::new();
    hp.set(2000.0, 0.707, SR);
    let mut lp = Svf::new();
    lp.set(7000.0, 0.707, SR);
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / SR;
        let breath_amp = 0.18 * (0.5 + 0.5 * (std::f32::consts::TAU * 0.7 * t).sin());
        let n = lp.process(hp.process(noise[i]).hp).lp * breath_amp;
        out.push((voc[i] * 0.75 + n).clamp(-0.98, 0.98));
    }
    out
}

fn slug(name: &str) -> String {
    name.to_lowercase().replace([' ', '·', '-', '/'], "_")
}

fn main() {
    let subdir = std::env::var("QVS_AUD").unwrap_or_else(|_| "before".to_string());
    let dir = format!("renders/_audition/VOXFIT/{subdir}");
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, buf: &[f32]| {
        let path = std::path::Path::new(&dir).join(format!("{name}.wav"));
        write_wav(&path, buf, SR as u32).expect("write wav");
    };

    // Sources.
    let vocal = synth_vocal(220.0, (SR * 1.5) as usize, SR);
    let highfem = synth_vocal(400.0, (SR * 1.5) as usize, SR); // high female — lifter stress
    let sibilant_src = sibilant(150.0, 1.6);
    let breathy_src = breathy(200.0, 1.6);
    let sustain = synth_vocal(300.0, (SR * 20.0) as usize, SR); // long-session stability

    // Default (formant unity → PV bypass; should be transparent).
    let d = Settings::default();
    write("default__vocal", &render(&vocal, &d));
    write("default__highfem", &render(&highfem, &d));
    write("default__sibilant", &render(&sibilant_src, &d));
    write("default__sustain", &render(&sustain, &d));

    // Every preset over the main vocal source.
    let presets = load_all(PRESET_JSON);
    for p in &presets {
        let s = settings_from_preset(p);
        write(&format!("{}__vocal", slug(&p.name)), &render(&vocal, &s));
    }

    // Diagnostic hard cases on representative presets.
    let by_name = |n: &str| presets.iter().find(|p| p.name == n).map(settings_from_preset);
    // De-ess presets on the sibilant source (does it duck esses without killing air?).
    for name in ["De-Harsh Rip", "Nyquist Sibilance Kill", "Neutral Cleanup", "Airy Feature"] {
        if let Some(s) = by_name(name) {
            write(&format!("{}__sibilant", slug(name)), &render(&sibilant_src, &s));
        }
    }
    // Formant presets on the high female range (comb probe).
    for name in ["Deeper Voice", "Helium Sprite", "Sewer Choir", "Crushed Angel"] {
        if let Some(s) = by_name(name) {
            write(&format!("{}__highfem", slug(name)), &render(&highfem, &s));
        }
    }
    // Backing beds on breathy material.
    for name in ["Drowned Choir Bed", "Sit In Dark Mix"] {
        if let Some(s) = by_name(name) {
            write(&format!("{}__breathy", slug(name)), &render(&breathy_src, &s));
        }
    }

    eprintln!("VOXFIT audition renders written to {dir}");
}
