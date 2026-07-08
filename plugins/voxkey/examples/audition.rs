//! SOUND-PASS audition render harness for VOXKEY.
//!
//! Renders the default state + every factory preset over synth_vocal and a set of harder
//! synthesized vocal cases (breathy, high female range, a two-note glide, a long sustain)
//! into `renders/_audition/VOXKEY/<subdir>/`. `<subdir>` comes from `QVS_AUD` (default
//! `before`). Run once for BEFORE, apply fixes, run again for AFTER, then diff pairs with
//! `tools/audition.py compare`.
//!
//! ```text
//! QVS_AUD=before cargo run -p voxkey --example audition --release
//! QVS_AUD=after  cargo run -p voxkey --example audition --release
//! ```

use suite_core::dsp::Svf;
use suite_core::harness::write_wav;
use suite_core::presets::load_all;
use suite_core::testsig::{synth_vocal, white_noise, Rng};
use voxkey::dsp::{Settings, VoxCore};
use voxkey::presets::{settings_from_preset, PRESET_JSON};

const SR: f32 = 48_000.0;

/// A vibrato-free vocal-like source (saw glottal pulse → three /a/ formant band-passes).
fn saw_formant(f0: f32, len: usize) -> Vec<f32> {
    let formants = [(700.0f32, 6.0f32, 1.0f32), (1220.0, 8.0, 0.55), (2600.0, 10.0, 0.28)];
    let mut bp = [Svf::new(), Svf::new(), Svf::new()];
    for (i, &(fc, q, _)) in formants.iter().enumerate() {
        bp[i].set(fc.min(SR * 0.45), q, SR);
    }
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(len);
    let mut peak = 1.0e-6f32;
    for _ in 0..len {
        phase += f0 / SR;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
        let saw = 2.0 * phase - 1.0;
        let mut y = 0.0f32;
        for (i, &(_, _, g)) in formants.iter().enumerate() {
            y += bp[i].process(saw).bp * g;
        }
        peak = peak.max(y.abs());
        out.push(y);
    }
    let norm = 0.7 / peak;
    for v in out.iter_mut() {
        *v *= norm;
    }
    out
}

/// Two-note glide with steady dwells (concatenated vibrato-free segments) — zipper probe.
fn glide(f_a: f32, f_b: f32) -> Vec<f32> {
    let mut out = saw_formant(f_a, (SR * 0.6) as usize);
    // short interpolating region so the tracker sees a real slide
    let n_slide = (SR * 0.15) as usize;
    let mut phase = 0.0f32;
    for i in 0..n_slide {
        let t = i as f32 / n_slide as f32;
        let f = f_a * (f_b / f_a).powf(t);
        phase += f / SR;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
        out.push(0.4 * (2.0 * phase - 1.0));
    }
    out.extend_from_slice(&saw_formant(f_b, (SR * 0.6) as usize));
    out
}

/// Breathy vocal: a low-level vocal tone with a band-limited breath-noise bed riding on top.
fn breathy(f0: f32, secs: f32) -> Vec<f32> {
    let len = (SR * secs) as usize;
    let voc = synth_vocal(f0, len, SR);
    let noise = white_noise(1.0, len, 0x0B1E_A711);
    let mut hp = Svf::new();
    hp.set(2000.0, 0.707, SR);
    let mut lp = Svf::new();
    lp.set(7000.0, 0.707, SR);
    let mut rng = Rng::new(0x0B1E_A711);
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        // slow breath amplitude wobble
        let t = i as f32 / SR;
        let breath_amp = 0.18 * (0.5 + 0.5 * (std::f32::consts::TAU * 0.7 * t).sin());
        let n = lp.process(hp.process(noise[i]).hp).lp * breath_amp;
        // a touch of extra jitter so confidence genuinely dips between phrases
        let j = 0.02 * rng.next_bipolar();
        out.push((voc[i] * 0.75 + n + j).clamp(-0.98, 0.98));
    }
    out
}

fn render(input: &[f32], s: &Settings) -> Vec<f32> {
    let mut core = VoxCore::new(SR);
    let mut buf = input.to_vec();
    core.process_mono(&mut buf, s);
    buf
}

fn slug(name: &str) -> String {
    name.to_lowercase().replace([' ', '·', '-', '/'], "_")
}

fn main() {
    let subdir = std::env::var("QVS_AUD").unwrap_or_else(|_| "before".to_string());
    let dir = format!("renders/_audition/VOXKEY/{subdir}");
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, buf: &[f32]| {
        let path = std::path::Path::new(&dir).join(format!("{name}.wav"));
        write_wav(&path, buf, SR as u32).expect("write wav");
    };

    // Sources.
    let vocal = synth_vocal(220.0, (SR * 1.5) as usize, SR); // alto, 5 Hz vibrato
    let highfem = synth_vocal(400.0, (SR * 1.5) as usize, SR); // high female — lifter stress
    let breathy_src = breathy(200.0, 1.6);
    let glide_src = glide(220.0, 330.0); // up a fifth
    let sustain = synth_vocal(300.0, (SR * 20.0) as usize, SR); // long-session stability (phase-wrap)

    // Default state.
    let d = Settings::default();
    write("default__vocal", &render(&vocal, &d));
    write("default__highfem", &render(&highfem, &d));
    write("default__breathy", &render(&breathy_src, &d));
    write("default__glide", &render(&glide_src, &d));
    write("default__sustain", &render(&sustain, &d));

    // Every preset over the main vocal source (the user's audition pairs).
    let presets = load_all(PRESET_JSON);
    for p in &presets {
        let s = settings_from_preset(p);
        write(&format!("{}__vocal", slug(&p.name)), &render(&vocal, &s));
    }

    // Diagnostic hard cases on representative presets:
    //  - Hard Snap Am: hard-snap zipper on the glide, gurgle on breathy.
    //  - Doll Formant / Deep Throat Formant: formant offset on the high female range (comb probe).
    //  - Cynthoni Ghost Vox: breathy dnb rip.
    let by_name = |n: &str| presets.iter().find(|p| p.name == n).map(settings_from_preset);
    if let Some(s) = by_name("Hard Snap Am") {
        write("hard_snap_am__glide", &render(&glide_src, &s));
        write("hard_snap_am__breathy", &render(&breathy_src, &s));
        write("hard_snap_am__highfem", &render(&highfem, &s));
    }
    if let Some(s) = by_name("Doll Formant") {
        write("doll_formant__highfem", &render(&highfem, &s));
        write("doll_formant__vocal", &render(&vocal, &s)); // already in loop, but keep grouped
    }
    if let Some(s) = by_name("Deep Throat Formant") {
        write("deep_throat_formant__highfem", &render(&highfem, &s));
    }
    if let Some(s) = by_name("Cynthoni Ghost Vox") {
        write("cynthoni_ghost_vox__breathy", &render(&breathy_src, &s));
        write("cynthoni_ghost_vox__highfem", &render(&highfem, &s));
    }

    eprintln!("VOXKEY audition renders written to {dir}");
}
