//! TRACER — pitch-tracking multiband saturation (Qeynos suite, Phase 1).
//!
//! A pitch detector (MPM, `suite_core::pitch`) locks a Linkwitz-Riley crossover tree to
//! the input's fundamental: crossover cutoffs ride harmonic multiples of the detected f0
//! (Smart Frequency knob), so each band always saturates the same harmonic region as the
//! note glides. When the detector loses confidence the crossovers freeze; a MIDI note can
//! replace the detector entirely. Each band is driven through the suite waveshaper bank at
//! 2x oversampling, with an optional inverse equal-loudness "constant color" drive trim.
//!
//! The time-varying LR4 tree is built from TPT state-variable filters (unconditionally
//! stable under cutoff modulation) with a NaN/blow-up reset-and-crossfade guard — see
//! [`dsp`], which holds the pure-DSP math shared with the offline harness tests.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::{Arc, RwLock};
use suite_core::modlisten::ModRoutes;

pub mod dsp;
pub mod presets;

use dsp::{PitchMode, Settings, ShapeKind, TracerCore, XoMode};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum PitchModeParam {
    #[id = "detect"]
    #[name = "Detect"]
    Detect,
    #[id = "midi"]
    #[name = "MIDI"]
    Midi,
}

impl PitchModeParam {
    fn to_dsp(self) -> PitchMode {
        match self {
            PitchModeParam::Detect => PitchMode::Detect,
            PitchModeParam::Midi => PitchMode::Midi,
        }
    }
    fn from_index(i: usize) -> PitchModeParam {
        match i {
            1 => PitchModeParam::Midi,
            _ => PitchModeParam::Detect,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum BandCountParam {
    #[id = "two"]
    #[name = "2 Bands"]
    Two,
    #[id = "three"]
    #[name = "3 Bands"]
    Three,
    #[id = "four"]
    #[name = "4 Bands"]
    Four,
}

impl BandCountParam {
    fn to_count(self) -> usize {
        match self {
            BandCountParam::Two => 2,
            BandCountParam::Three => 3,
            BandCountParam::Four => 4,
        }
    }
    fn from_count(n: usize) -> BandCountParam {
        match n {
            2 => BandCountParam::Two,
            4 => BandCountParam::Four,
            _ => BandCountParam::Three,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum XoModeParam {
    #[id = "track"]
    #[name = "Track"]
    Track,
    #[id = "fixed"]
    #[name = "Fixed"]
    Fixed,
}

impl XoModeParam {
    fn to_dsp(self) -> XoMode {
        match self {
            XoModeParam::Track => XoMode::Track,
            XoModeParam::Fixed => XoMode::Fixed,
        }
    }
    fn from_index(i: usize) -> XoModeParam {
        match i {
            1 => XoModeParam::Fixed,
            _ => XoModeParam::Track,
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

pub struct Tracer {
    params: Arc<TracerParams>,
    core: TracerCore,
    factory_presets: Arc<Vec<Preset>>,
    /// Last MIDI note frequency (Hz) for MIDI pitch mode.
    last_note_hz: Option<f32>,
}

#[derive(Params)]
pub struct TracerParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "pitchmode"]
    pub pitch_mode: EnumParam<PitchModeParam>,
    #[id = "bands"]
    pub bands: EnumParam<BandCountParam>,
    #[id = "smartfreq"]
    pub smart_freq: FloatParam,
    #[id = "color"]
    pub const_color: BoolParam,
    #[id = "slew"]
    pub slew: FloatParam,
    #[id = "trim"]
    pub trim: FloatParam,

    #[id = "xo1mode"]
    pub xo1_mode: EnumParam<XoModeParam>,
    #[id = "xo1hz"]
    pub xo1_hz: FloatParam,
    #[id = "xo2mode"]
    pub xo2_mode: EnumParam<XoModeParam>,
    #[id = "xo2hz"]
    pub xo2_hz: FloatParam,
    #[id = "xo3mode"]
    pub xo3_mode: EnumParam<XoModeParam>,
    #[id = "xo3hz"]
    pub xo3_hz: FloatParam,

    #[id = "b1drive"]
    pub b1_drive: FloatParam,
    #[id = "b1shape"]
    pub b1_shape: EnumParam<ShapeParam>,
    #[id = "b1level"]
    pub b1_level: FloatParam,
    #[id = "b2drive"]
    pub b2_drive: FloatParam,
    #[id = "b2shape"]
    pub b2_shape: EnumParam<ShapeParam>,
    #[id = "b2level"]
    pub b2_level: FloatParam,
    #[id = "b3drive"]
    pub b3_drive: FloatParam,
    #[id = "b3shape"]
    pub b3_shape: EnumParam<ShapeParam>,
    #[id = "b3level"]
    pub b3_level: FloatParam,
    #[id = "b4drive"]
    pub b4_drive: FloatParam,
    #[id = "b4shape"]
    pub b4_shape: EnumParam<ShapeParam>,
    #[id = "b4level"]
    pub b4_level: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
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

fn drive_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 48.0 })
        .with_unit(" dB")
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn level_param(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -36.0, max: 12.0 })
        .with_unit(" dB")
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_value_to_string(formatters::v2s_f32_rounded(2))
}

impl Default for TracerParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(600, 560),
            pitch_mode: EnumParam::new("Pitch Mode", PitchModeParam::Detect),
            bands: EnumParam::new("Bands", BandCountParam::Three),
            smart_freq: FloatParam::new(
                "Smart Freq",
                0.0,
                FloatRange::Linear { min: -2.0, max: 3.0 },
            )
            .with_unit(" oct")
            .with_smoother(SmoothingStyle::Linear(30.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            const_color: BoolParam::new("Constant Color", d.const_color),
            slew: FloatParam::new(
                "Slew",
                d.slew_hz_per_ms,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 2000.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" Hz/ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            trim: FloatParam::new("Trim", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            xo1_mode: EnumParam::new("XO1 Mode", XoModeParam::Track),
            xo1_hz: hz("XO1 Fixed", d.xo_fixed_hz[0], 20.0, 20_000.0),
            xo2_mode: EnumParam::new("XO2 Mode", XoModeParam::Track),
            xo2_hz: hz("XO2 Fixed", d.xo_fixed_hz[1], 20.0, 20_000.0),
            xo3_mode: EnumParam::new("XO3 Mode", XoModeParam::Track),
            xo3_hz: hz("XO3 Fixed", d.xo_fixed_hz[2], 20.0, 20_000.0),

            b1_drive: drive_param("Band 1 Drive", d.band_drive_db[0]),
            b1_shape: EnumParam::new("Band 1 Shape", ShapeParam::Tube),
            b1_level: level_param("Band 1 Level"),
            b2_drive: drive_param("Band 2 Drive", d.band_drive_db[1]),
            b2_shape: EnumParam::new("Band 2 Shape", ShapeParam::Tube),
            b2_level: level_param("Band 2 Level"),
            b3_drive: drive_param("Band 3 Drive", d.band_drive_db[2]),
            b3_shape: EnumParam::new("Band 3 Shape", ShapeParam::Tape),
            b3_level: level_param("Band 3 Level"),
            b4_drive: drive_param("Band 4 Drive", d.band_drive_db[3]),
            b4_shape: EnumParam::new("Band 4 Shape", ShapeParam::Tape),
            b4_level: level_param("Band 4 Level"),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl TracerParams {
    /// Snapshot the current (un-smoothed) values into a DSP [`Settings`]. Per-sample
    /// fields (trim/drives/levels/mix/out) are overwritten from smoothers in `process`.
    fn snapshot(&self) -> Settings {
        Settings {
            pitch_mode: self.pitch_mode.value().to_dsp(),
            midi_note_hz: None,
            band_count: self.bands.value().to_count(),
            // Placeholder: the smart-freq smoother is advanced per sample in `process`
            // (MINOR 4) — advancing it once per block here would clock it at the wrong
            // rate. This field is overwritten before the DSP core ever reads it.
            smart_freq_oct: 0.0,
            xo_mode: [
                self.xo1_mode.value().to_dsp(),
                self.xo2_mode.value().to_dsp(),
                self.xo3_mode.value().to_dsp(),
            ],
            xo_fixed_hz: [self.xo1_hz.value(), self.xo2_hz.value(), self.xo3_hz.value()],
            const_color: self.const_color.value(),
            trim_db: self.trim.value(),
            band_drive_db: [
                self.b1_drive.value(),
                self.b2_drive.value(),
                self.b3_drive.value(),
                self.b4_drive.value(),
            ],
            band_shape: [
                self.b1_shape.value().to_dsp(),
                self.b2_shape.value().to_dsp(),
                self.b3_shape.value().to_dsp(),
                self.b4_shape.value().to_dsp(),
            ],
            band_level_db: [
                self.b1_level.value(),
                self.b2_level.value(),
                self.b3_level.value(),
                self.b4_level.value(),
            ],
            slew_hz_per_ms: self.slew.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self {
            params: Arc::new(TracerParams::default()),
            core: TracerCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            last_note_hz: None,
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see
/// the change).
fn apply_preset(params: &TracerParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    let set_enum_pitch = |v: PitchModeParam| {
        setter.begin_set_parameter(&params.pitch_mode);
        setter.set_parameter(&params.pitch_mode, v);
        setter.end_set_parameter(&params.pitch_mode);
    };
    set_enum_pitch(PitchModeParam::from_index(g("pitch_mode", 0.0) as usize));

    setter.begin_set_parameter(&params.bands);
    setter.set_parameter(
        &params.bands,
        BandCountParam::from_count((g("bands", 3.0) as usize).clamp(2, 4)),
    );
    setter.end_set_parameter(&params.bands);

    setter.begin_set_parameter(&params.const_color);
    setter.set_parameter(&params.const_color, g("const_color", 1.0) >= 0.5);
    setter.end_set_parameter(&params.const_color);

    let set_xo_mode = |param: &EnumParam<XoModeParam>, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, XoModeParam::from_index(v as usize));
        setter.end_set_parameter(param);
    };
    set_xo_mode(&params.xo1_mode, g("xo1_mode", 0.0));
    set_xo_mode(&params.xo2_mode, g("xo2_mode", 0.0));
    set_xo_mode(&params.xo3_mode, g("xo3_mode", 0.0));

    let set_shape = |param: &EnumParam<ShapeParam>, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, ShapeParam::from_index(v as usize));
        setter.end_set_parameter(param);
    };
    set_shape(&params.b1_shape, g("b1_shape", 0.0));
    set_shape(&params.b2_shape, g("b2_shape", 0.0));
    set_shape(&params.b3_shape, g("b3_shape", 0.0));
    set_shape(&params.b4_shape, g("b4_shape", 0.0));

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.smart_freq, g("smart_freq", 0.0));
    set_f(&params.slew, g("slew", 200.0));
    set_f(&params.trim, g("trim", 0.0));
    set_f(&params.xo1_hz, g("xo1_hz", 200.0));
    set_f(&params.xo2_hz, g("xo2_hz", 1000.0));
    set_f(&params.xo3_hz, g("xo3_hz", 4000.0));
    set_f(&params.b1_drive, g("b1_drive", 10.0));
    set_f(&params.b2_drive, g("b2_drive", 8.0));
    set_f(&params.b3_drive, g("b3_drive", 6.0));
    set_f(&params.b4_drive, g("b4_drive", 4.0));
    set_f(&params.b1_level, g("b1_level", 0.0));
    set_f(&params.b2_level, g("b2_level", 0.0));
    set_f(&params.b3_level, g("b3_level", 0.0));
    set_f(&params.b4_level, g("b4_level", 0.0));
    set_f(&params.mix, g("mix", 1.0));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Tracer {
    const NAME: &'static str = "Qeynos TRACER";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            names: PortNames {
                layout: Some("Stereo"),
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            names: PortNames {
                layout: Some("Mono"),
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
    ];

    // MIDI note input can replace the detector (SPECS: MIDI mode). An effect with
    // MidiConfig::Basic is fine (PRD-verified).
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
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
                suite_core::ui::ScaledWindow::new("qeynos-tracer-window", Vec2::new(600.0, 560.0))
                    .min_size(Vec2::new(520.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · TRACER").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new("pitch-tracking multiband saturation")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("tracer", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("trim", "TRIM"), ("mix", "MIX"), ("out", "OUT")],
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("PITCH / TRACKING")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("tracer-pitch")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "PITCH MODE", &params.pitch_mode, setter);
                                    row(ui, "BANDS", &params.bands, setter);
                                    ui.end_row();
                                    row(ui, "SMART FREQ", &params.smart_freq, setter);
                                    row(ui, "SLEW", &params.slew, setter);
                                    ui.end_row();
                                    row(ui, "TRIM", &params.trim, setter);
                                    ui.end_row();
                                });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("CONSTANT COLOR")
                                        .color(suite_core::ui::TEXT_DIM)
                                        .small(),
                                );
                                let mut cc = params.const_color.value();
                                if ui.checkbox(&mut cc, "").changed() {
                                    setter.begin_set_parameter(&params.const_color);
                                    setter.set_parameter(&params.const_color, cc);
                                    setter.end_set_parameter(&params.const_color);
                                }
                            });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("CROSSOVERS (Track → harmonic × f0, or Fixed Hz)")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("tracer-xover")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "XO1 MODE", &params.xo1_mode, setter);
                                    row(ui, "XO1 FIXED", &params.xo1_hz, setter);
                                    ui.end_row();
                                    row(ui, "XO2 MODE", &params.xo2_mode, setter);
                                    row(ui, "XO2 FIXED", &params.xo2_hz, setter);
                                    ui.end_row();
                                    row(ui, "XO3 MODE", &params.xo3_mode, setter);
                                    row(ui, "XO3 FIXED", &params.xo3_hz, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("BANDS")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("tracer-bands")
                                .num_columns(3)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "B1 DRIVE", &params.b1_drive, setter);
                                    row(ui, "B1 SHAPE", &params.b1_shape, setter);
                                    row(ui, "B1 LEVEL", &params.b1_level, setter);
                                    ui.end_row();
                                    row(ui, "B2 DRIVE", &params.b2_drive, setter);
                                    row(ui, "B2 SHAPE", &params.b2_shape, setter);
                                    row(ui, "B2 LEVEL", &params.b2_level, setter);
                                    ui.end_row();
                                    row(ui, "B3 DRIVE", &params.b3_drive, setter);
                                    row(ui, "B3 SHAPE", &params.b3_shape, setter);
                                    row(ui, "B3 LEVEL", &params.b3_level, setter);
                                    ui.end_row();
                                    row(ui, "B4 DRIVE", &params.b4_drive, setter);
                                    row(ui, "B4 SHAPE", &params.b4_shape, setter);
                                    row(ui, "B4 LEVEL", &params.b4_level, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("OUTPUT")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("tracer-out")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "MIX", &params.mix, setter);
                                    row(ui, "OUT", &params.out, setter);
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
        self.core = TracerCore::new(buffer_config.sample_rate);
        // Report the per-band oversampler group delay the dry path is compensated by (PDC).
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
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let mut base = self.params.snapshot();
        base.midi_note_hz = self.last_note_hz;
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                base.trim_db = routes.modulated_float("trim", &self.params.trim, bus);
                base.mix = routes.modulated_float("mix", &self.params.mix, bus);
                base.out_db = routes.modulated_float("out", &self.params.out, bus);
            }
        }
        self.core.configure(&base);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        let mut next_event = context.next_event();
        for n in 0..num_samples {
            // Track MIDI notes (for MIDI pitch mode). Held-note priority: last NoteOn wins.
            while let Some(event) = next_event {
                if event.timing() > n as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, .. } => {
                        self.last_note_hz = Some(util::midi_note_to_freq(note));
                    }
                    NoteEvent::NoteOff { .. } => {}
                    _ => {}
                }
                next_event = context.next_event();
            }

            let l_in = main[0][n];
            let r_in = if num_main > 1 { main[1][n] } else { l_in };

            let mut s = base;
            // Advance the smart-freq smoother once per sample (MINOR 4): the DSP core
            // samples this value only at its 32-sample control-block boundaries, so a
            // per-sample advance clocks the smoother at exactly the right rate.
            s.smart_freq_oct = self.params.smart_freq.smoothed.next();
            s.trim_db = self.params.trim.smoothed.next();
            s.mix = self.params.mix.smoothed.next();
            s.out_db = self.params.out.smoothed.next();
            s.band_drive_db = [
                self.params.b1_drive.smoothed.next(),
                self.params.b2_drive.smoothed.next(),
                self.params.b3_drive.smoothed.next(),
                self.params.b4_drive.smoothed.next(),
            ];
            s.band_level_db = [
                self.params.b1_level.smoothed.next(),
                self.params.b2_level.smoothed.next(),
                self.params.b3_level.smoothed.next(),
                self.params.b4_level.smoothed.next(),
            ];

            let (out_l, out_r) = self.core.process_sample(l_in, r_in, &s);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Tracer {
    const CLAP_ID: &'static str = "com.qeynos.tracer";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Pitch-tracking multiband saturation — LR4 crossovers locked to detected f0");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
    ];
}

impl Vst3Plugin for Tracer {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosTRACERmb01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Tracer);
nih_export_vst3!(Tracer);

#[cfg(test)]
mod render_tests {
    use crate::dsp::TracerCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    /// Render each factory preset over a sliding-saw glide and a steady synthetic vocal,
    /// write the WAVs into renders/TRACER/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let saw = testsig::sliding_saw(80.0, 160.0, 0.7, (sr * 2.0) as usize, sr);
        let vocal = testsig::synth_vocal(160.0, (sr * 2.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");

            let mut core = TracerCore::new(sr);
            let mut out = saw.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("TRACER", &format!("{fname}_saw"));
            write_wav(&path, &out, sr as u32).expect("write saw render");

            let mut core = TracerCore::new(sr);
            let mut out = vocal.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("TRACER", &format!("{fname}_vocal"));
            write_wav(&path, &out, sr as u32).expect("write vocal render");
        }
    }
}
