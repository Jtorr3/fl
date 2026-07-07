//! GRIT — sidechained distortion (Qeynos suite, Phase 1).
//!
//! A sidechain signal shapes the character of a saturation stage. Two shipped modes:
//! **A · Env-Drive** (SC envelope raises drive) and **B · Waveshape** (SC injects a
//! dynamic bias into the waveshaper). Nonlinear stages run at 4x oversampling; an
//! auto-gain stage matches output loudness back to the input. Mode C (spectral STFT)
//! is deferred — see DEFERRED.md.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared with the offline harness tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    resizable_window::ResizableWindow,
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

use dsp::{GritCore, Mode, Settings, ShapeKind};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ModeParam {
    #[id = "env-drive"]
    #[name = "A · Env-Drive"]
    EnvDrive,
    #[id = "waveshape"]
    #[name = "B · Waveshape"]
    Waveshape,
}

impl ModeParam {
    fn to_dsp(self) -> Mode {
        match self {
            ModeParam::EnvDrive => Mode::EnvDrive,
            ModeParam::Waveshape => Mode::WaveshapeSc,
        }
    }
    fn from_index(i: usize) -> ModeParam {
        match i {
            1 => ModeParam::Waveshape,
            _ => ModeParam::EnvDrive,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ShapeParam {
    #[id = "tube"]
    Tube,
    #[id = "tape"]
    Tape,
    #[id = "fold"]
    Fold,
    #[id = "hard"]
    Hard,
}

impl ShapeParam {
    fn to_dsp(self) -> ShapeKind {
        match self {
            ShapeParam::Tube => ShapeKind::Tube,
            ShapeParam::Tape => ShapeKind::Tape,
            ShapeParam::Fold => ShapeKind::Fold,
            ShapeParam::Hard => ShapeKind::Hard,
        }
    }
    fn from_index(i: usize) -> ShapeParam {
        match i {
            1 => ShapeParam::Tape,
            2 => ShapeParam::Fold,
            3 => ShapeParam::Hard,
            _ => ShapeParam::Tube,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Grit {
    params: Arc<GritParams>,
    core: GritCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct GritParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "mode"]
    pub mode: EnumParam<ModeParam>,
    #[id = "shape"]
    pub shape: EnumParam<ShapeParam>,
    #[id = "trim"]
    pub trim: FloatParam,
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "depth"]
    pub depth: FloatParam,
    #[id = "curve"]
    pub curve: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "scfocus"]
    pub sc_focus: FloatParam,
    #[id = "scwidth"]
    pub sc_width: FloatParam,
    #[id = "sclisten"]
    pub sc_listen: BoolParam,
    #[id = "prehp"]
    pub pre_hp: FloatParam,
    #[id = "prelp"]
    pub pre_lp: FloatParam,
    #[id = "posthp"]
    pub post_hp: FloatParam,
    #[id = "postlp"]
    pub post_lp: FloatParam,
    #[id = "autogain"]
    pub auto_gain: BoolParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,
}

fn hz(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

impl Default for GritParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(560, 460),
            mode: EnumParam::new("Mode", ModeParam::EnvDrive),
            shape: EnumParam::new("Shape", ShapeParam::Tube),
            trim: FloatParam::new("Trim", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            drive: FloatParam::new("Drive", 12.0, FloatRange::Linear { min: 0.0, max: 48.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            depth: FloatParam::new("Depth", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            curve: FloatParam::new(
                "Curve",
                1.0,
                FloatRange::Skewed {
                    min: 0.25,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            attack: FloatParam::new(
                "Attack",
                5.0,
                FloatRange::Skewed { min: 0.1, max: 200.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            release: FloatParam::new(
                "Release",
                120.0,
                FloatRange::Skewed { min: 5.0, max: 2000.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            sc_focus: hz("SC Focus", 100.0, 20.0, 20_000.0),
            sc_width: FloatParam::new(
                "SC Width",
                1.5,
                FloatRange::Linear { min: 0.2, max: 4.0 },
            )
            .with_unit(" oct")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            sc_listen: BoolParam::new("SC Listen", false),
            pre_hp: hz("Pre HP", 20.0, 20.0, 2000.0),
            pre_lp: hz("Pre LP", 20_000.0, 200.0, 20_000.0),
            post_hp: hz("Post HP", 20.0, 20.0, 2000.0),
            post_lp: hz("Post LP", 20_000.0, 200.0, 20_000.0),
            auto_gain: BoolParam::new("Auto-Gain", true),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

impl GritParams {
    /// Snapshot the current (un-smoothed) values into a DSP [`Settings`]. Used for
    /// block-rate filter configuration; per-sample fields are overwritten from
    /// smoothers in `process`.
    fn snapshot(&self) -> Settings {
        Settings {
            mode: self.mode.value().to_dsp(),
            shape: self.shape.value().to_dsp(),
            trim_db: self.trim.value(),
            drive_db: self.drive.value(),
            depth: self.depth.value(),
            curve: self.curve.value(),
            attack_ms: self.attack.value(),
            release_ms: self.release.value(),
            sc_focus_hz: self.sc_focus.value(),
            sc_width_oct: self.sc_width.value(),
            sc_listen: self.sc_listen.value(),
            pre_hp_hz: self.pre_hp.value(),
            pre_lp_hz: self.pre_lp.value(),
            post_hp_hz: self.post_hp.value(),
            post_lp_hz: self.post_lp.value(),
            auto_gain: self.auto_gain.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Grit {
    fn default() -> Self {
        Self {
            params: Arc::new(GritParams::default()),
            core: GritCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo
/// see the change).
fn apply_preset(params: &GritParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    setter.begin_set_parameter(&params.mode);
    setter.set_parameter(&params.mode, ModeParam::from_index(g("mode", 0.0) as usize));
    setter.end_set_parameter(&params.mode);
    setter.begin_set_parameter(&params.shape);
    setter.set_parameter(&params.shape, ShapeParam::from_index(g("shape", 0.0) as usize));
    setter.end_set_parameter(&params.shape);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.trim, g("trim", 0.0));
    set_f(&params.drive, g("drive", 12.0));
    set_f(&params.depth, g("depth", 0.5));
    set_f(&params.curve, g("curve", 1.0));
    set_f(&params.attack, g("attack", 5.0));
    set_f(&params.release, g("release", 120.0));
    set_f(&params.sc_focus, g("sc_focus", 100.0));
    set_f(&params.sc_width, g("sc_width", 1.5));
    set_f(&params.pre_hp, g("pre_hp", 20.0));
    set_f(&params.pre_lp, g("pre_lp", 20_000.0));
    set_f(&params.post_hp, g("post_hp", 20.0));
    set_f(&params.post_lp, g("post_lp", 20_000.0));
    set_f(&params.mix, g("mix", 1.0));
    set_f(&params.out, g("out", 0.0));

    setter.begin_set_parameter(&params.sc_listen);
    setter.set_parameter(&params.sc_listen, g("sc_listen", 0.0) >= 0.5);
    setter.end_set_parameter(&params.sc_listen);
    setter.begin_set_parameter(&params.auto_gain);
    setter.set_parameter(&params.auto_gain, g("auto_gain", 1.0) >= 0.5);
    setter.end_set_parameter(&params.auto_gain);
}

impl Plugin for Grit {
    const NAME: &'static str = "Qeynos GRIT";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames {
                layout: Some("Stereo"),
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
            names: PortNames {
                layout: Some("Mono"),
                ..PortNames::const_default()
            },
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
                ResizableWindow::new("qeynos-grit-window")
                    .min_size(Vec2::new(460.0, 380.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · GRIT").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new("sidechained distortion")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset selector
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("PRESET").color(suite_core::ui::TEXT_DIM).small(),
                            );
                            egui::ComboBox::from_id_salt("grit-preset")
                                .selected_text("select…")
                                .show_ui(ui, |ui| {
                                    for p in presets.iter() {
                                        if ui.selectable_label(false, &p.name).clicked() {
                                            apply_preset(&params, setter, p);
                                        }
                                    }
                                });
                        });
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            use suite_core::ui::labeled_slider as row;
                            egui::Grid::new("grit-params")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "MODE", &params.mode, setter);
                                    row(ui, "SHAPE", &params.shape, setter);
                                    ui.end_row();
                                    row(ui, "TRIM", &params.trim, setter);
                                    row(ui, "DRIVE", &params.drive, setter);
                                    ui.end_row();
                                    row(ui, "DEPTH", &params.depth, setter);
                                    row(ui, "CURVE", &params.curve, setter);
                                    ui.end_row();
                                    row(ui, "ATTACK", &params.attack, setter);
                                    row(ui, "RELEASE", &params.release, setter);
                                    ui.end_row();
                                    row(ui, "SC FOCUS", &params.sc_focus, setter);
                                    row(ui, "SC WIDTH", &params.sc_width, setter);
                                    ui.end_row();
                                    row(ui, "PRE HP", &params.pre_hp, setter);
                                    row(ui, "PRE LP", &params.pre_lp, setter);
                                    ui.end_row();
                                    row(ui, "POST HP", &params.post_hp, setter);
                                    row(ui, "POST LP", &params.post_lp, setter);
                                    ui.end_row();
                                    row(ui, "MIX", &params.mix, setter);
                                    row(ui, "OUT", &params.out, setter);
                                    ui.end_row();
                                });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("SC LISTEN")
                                        .color(suite_core::ui::TEXT_DIM)
                                        .small(),
                                );
                                let mut listen = params.sc_listen.value();
                                if ui.checkbox(&mut listen, "").changed() {
                                    setter.begin_set_parameter(&params.sc_listen);
                                    setter.set_parameter(&params.sc_listen, listen);
                                    setter.end_set_parameter(&params.sc_listen);
                                }
                                ui.add_space(16.0);
                                ui.label(
                                    egui::RichText::new("AUTO-GAIN")
                                        .color(suite_core::ui::TEXT_DIM)
                                        .small(),
                                );
                                let mut ag = params.auto_gain.value();
                                if ui.checkbox(&mut ag, "").changed() {
                                    setter.begin_set_parameter(&params.auto_gain);
                                    setter.set_parameter(&params.auto_gain, ag);
                                    setter.end_set_parameter(&params.auto_gain);
                                }
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
        self.core = GritCore::new(buffer_config.sample_rate);
        // Report the oversampler group delay the dry path is compensated by (PDC).
        context.set_latency_samples(self.core.latency_samples());
        true
    }

    fn reset(&mut self) {
        self.core.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // Block-rate filter configuration from the current param snapshot.
        let base = self.params.snapshot();
        self.core.configure(&base);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        // Sidechain: mono-sum the first aux port's channels (0 if absent).
        let sc_slice: Option<&[&mut [f32]]> = if aux.inputs.is_empty() {
            None
        } else {
            Some(aux.inputs[0].as_slice_immutable())
        };

        for n in 0..num_samples {
            let l_in = main[0][n];
            let r_in = if num_main > 1 { main[1][n] } else { l_in };

            let sc = match sc_slice {
                Some(chs) if !chs.is_empty() => {
                    let mut acc = 0.0f32;
                    for ch in chs.iter() {
                        acc += ch[n];
                    }
                    acc / chs.len() as f32
                }
                _ => 0.0,
            };

            let mut s = base;
            s.trim_db = self.params.trim.smoothed.next();
            s.drive_db = self.params.drive.smoothed.next();
            s.depth = self.params.depth.smoothed.next();
            s.curve = self.params.curve.smoothed.next();
            s.mix = self.params.mix.smoothed.next();
            s.out_db = self.params.out.smoothed.next();

            let (out_l, out_r) = self.core.process_sample(l_in, r_in, sc, &s);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Grit {
    const CLAP_ID: &'static str = "com.qeynos.grit";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Sidechained distortion — envelope- and waveshape-driven saturation");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
   ];
}

impl Vst3Plugin for Grit {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosGRITdist01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Grit);
nih_export_vst3!(Grit);

#[cfg(test)]
mod render_tests {
    use crate::dsp::GritCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;

    /// Render each factory preset with a 1 kHz main sine and a pulsed sidechain,
    /// write the WAV into renders/GRIT/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let n = (sr * 2.0) as usize;
        let main: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / sr).sin())
            .collect();
        let sc: Vec<f32> = (0..n)
            .map(|i| {
                let ph = (i as f32 / sr) % 0.25;
                if ph < 0.06 {
                    0.9 * (std::f32::consts::TAU * 100.0 * i as f32 / sr).sin()
                } else {
                    0.0
                }
            })
            .collect();

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5);
        for p in &presets {
            let s = settings_from_preset(p);
            let mut core = GritCore::new(sr);
            let mut out = main.clone();
            core.process_mono(&mut out, &sc, &s);
            assert_universal(&out);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
            let path = render_path("GRIT", &fname);
            write_wav(&path, &out, sr as u32).expect("write render");
        }
    }
}
