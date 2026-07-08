//! UNDERTOW — kick-to-rumble generator (Qeynos suite, taste-tailored for hard/melodic
//! techno low-end). The plugin sits **ON the kick track**: it passes the dry kick straight
//! through and adds a kick-derived, kick-ducked **sub-bass rumble** underneath it.
//!
//! The rumble is built by stripping the kick's click (keeping its body), saturating it,
//! smearing it through a small/dark 8×8 Householder FDN (`suite_core::fdn::Fdn8`), low-passing
//! it into the sub range, optionally ringing a **key-lockable resonant peak** at a chosen note
//! (C0..B2), then **ducking it with the dry kick's own envelope** so the rumble breathes
//! around each hit. Everything below ~150 Hz is mono; `width` only spreads the content above.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared with the offline harness tests) atop the
//! reusable `suite_core::fdn` FDN core.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

#[cfg(test)]
mod tests;

use dsp::{db_to_gain, Settings, UndertowCore};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Undertow {
    params: Arc<UndertowParams>,
    core: UndertowCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct UndertowParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "strip"] pub strip: FloatParam,
    #[id = "drive"] pub drive: FloatParam,
    #[id = "size"] pub size: FloatParam,
    #[id = "decay"] pub decay: FloatParam,
    #[id = "lpfreq"] pub lp_freq: FloatParam,
    #[id = "lpres"] pub lp_res: FloatParam,
    #[id = "tunenote"] pub tune_note: IntParam,
    #[id = "tuneamt"] pub tune_amount: FloatParam,
    #[id = "duckdepth"] pub duck_depth: FloatParam,
    #[id = "duckrel"] pub duck_release: FloatParam,
    #[id = "rumble"] pub rumble_level: FloatParam,
    #[id = "width"] pub width: FloatParam,
    #[id = "dry"] pub dry_level: FloatParam,
    #[id = "trim"] pub out_trim: FloatParam,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn hz(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-1.0) },
    )
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_rounded(0))
    .with_string_to_value(Arc::new(|s| {
        s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
    }))
}

fn ms(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" ms")
        .with_value_to_string(formatters::v2s_f32_rounded(0))
        .with_string_to_value(Arc::new(|s| {
            s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
        }))
}

/// A dB-valued FloatParam (the value itself is in dB; the DSP converts to linear).
fn db(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
        .with_string_to_value(Arc::new(|s| {
            s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
        }))
}

impl Default for UndertowParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 620),

            strip: pct("Strip", d.strip),
            drive: pct("Drive", d.drive),
            size: pct("Size", d.size),
            decay: FloatParam::new(
                "Decay",
                d.decay,
                FloatRange::Skewed { min: 0.2, max: 3.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(|s| {
                s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
            })),
            lp_freq: hz("LP Freq", d.lp_cutoff, 90.0, 250.0),
            lp_res: FloatParam::new("LP Res", d.lp_res, FloatRange::Skewed {
                min: 0.5,
                max: 8.0,
                factor: FloatRange::skew_factor(-1.0),
            })
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(|s| s.trim().parse::<f32>().ok())),
            // Tune note: index 0..35 → C0..B2. Default 21 = A1 = 55 Hz.
            tune_note: IntParam::new(
                "Tune Note",
                21,
                IntRange::Linear { min: 0, max: presets::NOTE_COUNT - 1 },
            )
            .with_value_to_string(Arc::new(|v| presets::note_name(v)))
            .with_string_to_value(Arc::new(|s| presets::note_name_to_index(s))),
            tune_amount: pct("Tune Amount", d.tune_amount),
            duck_depth: pct("Duck Depth", d.duck_depth),
            duck_release: ms("Duck Release", d.duck_release_ms, 80.0, 300.0),
            rumble_level: db("Rumble", -2.0, -60.0, 12.0),
            width: pct("Width", d.width),
            dry_level: db("Dry", 0.0, -60.0, 6.0),
            out_trim: db("Out Trim", 0.0, -24.0, 24.0),
        }
    }
}

impl UndertowParams {
    /// Snapshot the current parameter values into a DSP [`Settings`] (dB → linear, note → Hz).
    fn snapshot(&self) -> Settings {
        // Treat the very bottom of a dB range as true silence so "rumble muted → dry" nulls
        // exactly (a −60 dB residual would otherwise sit above the −80 dB null bar).
        let gain = |dbv: f32| if dbv <= -59.5 { 0.0 } else { db_to_gain(dbv) };
        Settings {
            strip: self.strip.value(),
            drive: self.drive.value(),
            size: self.size.value(),
            decay: self.decay.value(),
            lp_cutoff: self.lp_freq.value(),
            lp_res: self.lp_res.value(),
            tune_hz: presets::note_index_to_hz(self.tune_note.value()),
            tune_amount: self.tune_amount.value(),
            duck_depth: self.duck_depth.value(),
            duck_release_ms: self.duck_release.value(),
            rumble_gain: gain(self.rumble_level.value()),
            width: self.width.value(),
            dry_gain: gain(self.dry_level.value()),
            out_gain: db_to_gain(self.out_trim.value()),
        }
    }
}

impl Default for Undertow {
    fn default() -> Self {
        Self {
            params: Arc::new(UndertowParams::default()),
            core: UndertowCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &UndertowParams, setter: &ParamSetter, p: &Preset) {
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let get = |k: &str, fb: f32| p.get(k).unwrap_or(fb);
    set_f(&params.strip, get("strip", 0.5));
    set_f(&params.drive, get("drive", 0.35));
    set_f(&params.size, get("size", 0.5));
    set_f(&params.decay, get("decay", 0.8));
    set_f(&params.lp_freq, get("lpfreq", 140.0));
    set_f(&params.lp_res, get("lpres", 1.2));
    set_f(&params.tune_amount, get("tuneamt", 0.0));
    set_f(&params.duck_depth, get("duckdepth", 0.5));
    set_f(&params.duck_release, get("duckrel", 160.0));
    set_f(&params.rumble_level, get("rumble", -2.0));
    set_f(&params.width, get("width", 0.3));
    set_f(&params.dry_level, get("dry", 0.0));
    set_f(&params.out_trim, get("trim", 0.0));

    let idx = get("tunenote", 21.0).round() as i32;
    setter.begin_set_parameter(&params.tune_note);
    setter.set_parameter(&params.tune_note, idx);
    setter.end_set_parameter(&params.tune_note);
}

impl Plugin for Undertow {
    const NAME: &'static str = "Qeynos UNDERTOW";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            names: PortNames { layout: Some("Stereo"), ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            names: PortNames { layout: Some("Mono"), ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let egui_state = self.params.editor_state.clone();
        let presets = self.factory_presets.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-undertow-window", Vec2::new(560.0, 620.0))
                    .min_size(Vec2::new(500.0, 520.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · UNDERTOW").color(suite_core::ui::ACCENT));
                        ui.label(
                            egui::RichText::new("kick-to-rumble generator — sits on the kick track")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("undertow", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new("SOURCE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("undertow-source")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "STRIP", &params.strip, setter);
                                    row(ui, "DRIVE", &params.drive, setter);
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("RUMBLE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("undertow-rumble")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "SIZE", &params.size, setter);
                                    row(ui, "DECAY", &params.decay, setter);
                                    ui.end_row();
                                    row(ui, "LP FREQ", &params.lp_freq, setter);
                                    row(ui, "LP RES", &params.lp_res, setter);
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("TUNE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("undertow-tune")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "NOTE", &params.tune_note, setter);
                                    row(ui, "AMOUNT", &params.tune_amount, setter);
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("DUCK").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("undertow-duck")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "DEPTH", &params.duck_depth, setter);
                                    row(ui, "RELEASE", &params.duck_release, setter);
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            ui.separator();
                            ui.label(egui::RichText::new("OUTPUT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("undertow-output")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "RUMBLE", &params.rumble_level, setter);
                                    row(ui, "WIDTH", &params.width, setter);
                                    ui.end_row();
                                    row(ui, "DRY", &params.dry_level, setter);
                                    row(ui, "OUT TRIM", &params.out_trim, setter);
                                    ui.end_row();
                                });
                        });
                    });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        // Preallocate the FDN + filters for this sample rate off the audio thread so process()
        // is allocation-free.
        self.core = UndertowCore::new(buffer_config.sample_rate);
        // The wet path is a reverb (time-smearing) ⇒ zero reported latency.
        context.set_latency_samples(self.core.latency_samples());
        true
    }

    fn reset(&mut self) {
        self.core.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let s = self.params.snapshot();
        self.core.configure(&s);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l = main[0][n];
            let r = if num_main > 1 { main[1][n] } else { l };
            let (out_l, out_r) = self.core.process_sample(l, r);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Undertow {
    const CLAP_ID: &'static str = "com.qeynos.undertow";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Kick-to-rumble generator — kick-ducked sub-bass rumble that sits on the kick track");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Reverb,
        ClapFeature::Custom("bass"),
    ];
}

impl Vst3Plugin for Undertow {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosUNDERTOWr1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_clap!(Undertow);
nih_export_vst3!(Undertow);

#[cfg(test)]
mod render_tests {
    use crate::dsp::{db_to_gain, Settings, UndertowCore};
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig::{synth_kick, KickSpec};

    /// A 4-on-the-floor synthetic-kick pattern at `bpm` (`beats` kicks), `sr` Hz.
    fn kick_pattern(bpm: f32, beats: usize, sr: f32) -> Vec<f32> {
        let step = (60.0 / bpm * sr) as usize;
        let n = step * beats + (sr * 0.6) as usize;
        let mut buf = vec![0.0f32; n];
        let spec = KickSpec { f_start: 200.0, f_end: 50.0, amp_decay_s: 0.28, ..KickSpec::default() };
        let one = synth_kick(&spec, (sr * 0.5) as usize, sr);
        for b in 0..beats {
            let start = b * step;
            for (i, &v) in one.iter().enumerate() {
                if start + i < n {
                    buf[start + i] += v;
                }
            }
        }
        // Keep the summed dry kick well below 0 dBFS so dry + rumble stays inside the ceiling.
        for v in buf.iter_mut() {
            *v = (*v * 0.22).clamp(-0.999, 0.999);
        }
        buf
    }

    /// Render each factory preset over a kick pattern, assert universal, write full-mix WAVs.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let input = kick_pattern(130.0, 8, sr);
        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let mut core = UndertowCore::new(sr);
            let mut out = input.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
            let path = render_path("UNDERTOW", &fname);
            write_wav(&path, &out, sr as u32).expect("write render");
        }
    }

    /// Write one full-mix render and one rumble-only render (dry level 0) to renders/UNDERTOW/.
    #[test]
    fn full_and_rumble_only_renders() {
        let sr = 48_000.0f32;
        let input = kick_pattern(130.0, 8, sr);

        // Full mix (default-ish musical setting, tuned to A1).
        let full = Settings {
            tune_amount: 0.3,
            duck_depth: 0.6,
            rumble_gain: db_to_gain(-4.0),
            ..Settings::default()
        };
        let mut core = UndertowCore::new(sr);
        let mut out = input.clone();
        core.process_mono(&mut out, &full);
        assert_universal(&out);
        write_wav(&render_path("UNDERTOW", "full_mix"), &out, sr as u32).expect("write full");

        // Rumble only: dry level 0 (isolate the rumble bus).
        let rumble_only = Settings { dry_gain: 0.0, ..full };
        let mut core2 = UndertowCore::new(sr);
        let mut out2 = input.clone();
        core2.process_mono(&mut out2, &rumble_only);
        assert_universal(&out2);
        write_wav(&render_path("UNDERTOW", "rumble_only"), &out2, sr as u32).expect("write rumble");
    }
}
