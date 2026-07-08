//! SHAPESHIFT — XY-morphing distortion (Qeynos suite, Phase 2b; Teuri clone).
//!
//! Four **corners** (A/B/C/D) each pick a waveshaper from an 8-curve bank and carry a per-corner
//! gain trim. An **XY position** sets bilinear blend weights so the output morphs continuously
//! between the four shaper characters. A built-in **orbit LFO** rotates the XY point around the
//! user position (circle / figure-8, free or BPM-synced) for automatic, evolving distortion.
//! The whole morph runs at **4x oversampling**; the dry path is delay-compensated by the
//! oversampler group delay (reported as latency) so partial mix does not comb-filter. A post
//! low-pass tames fold/crush harshness, and an optional auto-gain matches output loudness.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared verbatim with the offline harness tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Sense, Vec2},
    EguiState,
};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use suite_core::modlisten::ModRoutes;

pub mod dsp;
pub mod presets;

use dsp::{Corner, OrbitShape, Settings, ShapeshiftCore, SyncDivision, NUM_CORNERS};
use suite_core::bus::PluginKind;
use suite_core::presets::{load_all, Preset};
use suite_core::spectrum::SpectrumPublisher;

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/SHAPESHIFT.md");

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum CornerParam {
    #[id = "tube"]
    #[name = "Tube tanh"]
    Tube,
    #[id = "tape"]
    #[name = "Tape soft"]
    Tape,
    #[id = "diode"]
    #[name = "Diode asym"]
    Diode,
    #[id = "hard"]
    #[name = "Hard clip"]
    Hard,
    #[id = "sinefold"]
    #[name = "Sine fold"]
    SineFold,
    #[id = "trifold"]
    #[name = "Tri fold"]
    TriFold,
    #[id = "cheby3"]
    #[name = "Cheby-3"]
    Cheby3,
    #[id = "bitsoft"]
    #[name = "Bit soft"]
    BitSoft,
}

impl CornerParam {
    fn to_dsp(self) -> Corner {
        match self {
            CornerParam::Tube => Corner::TubeTanh,
            CornerParam::Tape => Corner::TapeSoft,
            CornerParam::Diode => Corner::DiodeAsym,
            CornerParam::Hard => Corner::HardClip,
            CornerParam::SineFold => Corner::SineFold,
            CornerParam::TriFold => Corner::WavefoldTri,
            CornerParam::Cheby3 => Corner::Cheby3,
            CornerParam::BitSoft => Corner::BitcrushSoft,
        }
    }
    fn from_index(i: usize) -> CornerParam {
        match i {
            0 => CornerParam::Tube,
            1 => CornerParam::Tape,
            2 => CornerParam::Diode,
            3 => CornerParam::Hard,
            4 => CornerParam::SineFold,
            5 => CornerParam::TriFold,
            6 => CornerParam::Cheby3,
            _ => CornerParam::BitSoft,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ShapeParam {
    #[id = "circle"]
    Circle,
    #[id = "figure8"]
    #[name = "Figure-8"]
    Figure8,
}

impl ShapeParam {
    fn to_dsp(self) -> OrbitShape {
        match self {
            ShapeParam::Circle => OrbitShape::Circle,
            ShapeParam::Figure8 => OrbitShape::Figure8,
        }
    }
    fn from_index(i: usize) -> ShapeParam {
        match i {
            1 => ShapeParam::Figure8,
            _ => ShapeParam::Circle,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DivisionParam {
    #[id = "half"]
    #[name = "1/2"]
    Half,
    #[id = "bar"]
    #[name = "1 Bar"]
    Bar,
    #[id = "bar2"]
    #[name = "2 Bar"]
    TwoBars,
    #[id = "bar4"]
    #[name = "4 Bar"]
    FourBars,
}

impl DivisionParam {
    fn to_dsp(self) -> SyncDivision {
        match self {
            DivisionParam::Half => SyncDivision::Half,
            DivisionParam::Bar => SyncDivision::Bar,
            DivisionParam::TwoBars => SyncDivision::TwoBars,
            DivisionParam::FourBars => SyncDivision::FourBars,
        }
    }
    fn from_index(i: usize) -> DivisionParam {
        match i {
            0 => DivisionParam::Half,
            1 => DivisionParam::Bar,
            2 => DivisionParam::TwoBars,
            _ => DivisionParam::FourBars,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Shapeshift {
    params: Arc<ShapeshiftParams>,
    core: ShapeshiftCore,
    /// Orbit phase published from `process` for the GUI moving dot.
    orbit_meter: Arc<AtomicF32>,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct ShapeshiftParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "x"]
    pub x: FloatParam,
    #[id = "y"]
    pub y: FloatParam,

    #[id = "corner_a"]
    pub corner_a: EnumParam<CornerParam>,
    #[id = "corner_b"]
    pub corner_b: EnumParam<CornerParam>,
    #[id = "corner_c"]
    pub corner_c: EnumParam<CornerParam>,
    #[id = "corner_d"]
    pub corner_d: EnumParam<CornerParam>,

    #[id = "gain_a"]
    pub gain_a: FloatParam,
    #[id = "gain_b"]
    pub gain_b: FloatParam,
    #[id = "gain_c"]
    pub gain_c: FloatParam,
    #[id = "gain_d"]
    pub gain_d: FloatParam,

    #[id = "pre"]
    pub pre: FloatParam,
    #[id = "orbit"]
    pub orbit_on: BoolParam,
    #[id = "orate"]
    pub orbit_rate: FloatParam,
    #[id = "osync"]
    pub orbit_sync: BoolParam,
    #[id = "odiv"]
    pub orbit_div: EnumParam<DivisionParam>,
    #[id = "oradius"]
    pub orbit_radius: FloatParam,
    #[id = "oshape"]
    pub orbit_shape: EnumParam<ShapeParam>,
    #[id = "ophase"]
    pub orbit_phase: FloatParam,
    #[id = "postlp"]
    pub post_lp: FloatParam,
    #[id = "autogain"]
    pub auto_gain: BoolParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

fn gain_db(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: -24.0, max: 24.0 })
        .with_unit(" dB")
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_value_to_string(formatters::v2s_f32_rounded(2))
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

fn unit01(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl Default for ShapeshiftParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(600, 640),
            x: FloatParam::new("X", d.x, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            y: FloatParam::new("Y", d.y, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            corner_a: EnumParam::new("Corner A", CornerParam::Tube),
            corner_b: EnumParam::new("Corner B", CornerParam::Tape),
            corner_c: EnumParam::new("Corner C", CornerParam::Cheby3),
            corner_d: EnumParam::new("Corner D", CornerParam::Hard),
            gain_a: gain_db("Gain A", 0.0),
            gain_b: gain_db("Gain B", 0.0),
            gain_c: gain_db("Gain C", 0.0),
            gain_d: gain_db("Gain D", 0.0),
            pre: FloatParam::new("Pre-Gain", d.pre_db, FloatRange::Linear { min: -12.0, max: 36.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            orbit_on: BoolParam::new("Orbit", d.orbit_on),
            orbit_rate: FloatParam::new(
                "Orbit Rate",
                d.orbit_rate_hz,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            orbit_sync: BoolParam::new("Orbit Sync", d.orbit_sync),
            orbit_div: EnumParam::new("Orbit Division", DivisionParam::Bar),
            orbit_radius: FloatParam::new(
                "Orbit Radius",
                d.orbit_radius,
                FloatRange::Linear { min: 0.0, max: 0.5 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            orbit_shape: EnumParam::new("Orbit Shape", ShapeParam::Circle),
            orbit_phase: unit01("Orbit Phase", d.orbit_phase0),
            post_lp: hz("Post LP", d.post_lp_hz, 200.0, 20_000.0),
            auto_gain: BoolParam::new("Auto-Gain", d.auto_gain),
            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl ShapeshiftParams {
    /// Snapshot the live parameters into a DSP [`Settings`]. `tempo_bpm` comes from the host.
    fn snapshot(&self, tempo_bpm: f32) -> Settings {
        Settings {
            x: self.x.value(),
            y: self.y.value(),
            corner: [
                self.corner_a.value().to_dsp(),
                self.corner_b.value().to_dsp(),
                self.corner_c.value().to_dsp(),
                self.corner_d.value().to_dsp(),
            ],
            gain_db: [
                self.gain_a.value(),
                self.gain_b.value(),
                self.gain_c.value(),
                self.gain_d.value(),
            ],
            pre_db: self.pre.value(),
            orbit_on: self.orbit_on.value(),
            orbit_rate_hz: self.orbit_rate.value(),
            orbit_sync: self.orbit_sync.value(),
            orbit_div: self.orbit_div.value().to_dsp(),
            orbit_radius: self.orbit_radius.value(),
            orbit_shape: self.orbit_shape.value().to_dsp(),
            orbit_phase0: self.orbit_phase.value(),
            tempo_bpm,
            post_lp_hz: self.post_lp.value(),
            auto_gain: self.auto_gain.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Shapeshift {
    fn default() -> Self {
        Self {
            params: Arc::new(ShapeshiftParams::default()),
            core: ShapeshiftCore::new(48_000.0),
            orbit_meter: Arc::new(AtomicF32::new(0.0)),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &ShapeshiftParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_c = |param: &EnumParam<CornerParam>, v: CornerParam| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_b = |param: &BoolParam, v: bool| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };

    set_f(&params.x, g("x", 0.5));
    set_f(&params.y, g("y", 0.5));
    set_c(&params.corner_a, CornerParam::from_index(g("cA", 0.0) as usize));
    set_c(&params.corner_b, CornerParam::from_index(g("cB", 1.0) as usize));
    set_c(&params.corner_c, CornerParam::from_index(g("cC", 6.0) as usize));
    set_c(&params.corner_d, CornerParam::from_index(g("cD", 3.0) as usize));
    set_f(&params.gain_a, g("gA", 0.0));
    set_f(&params.gain_b, g("gB", 0.0));
    set_f(&params.gain_c, g("gC", 0.0));
    set_f(&params.gain_d, g("gD", 0.0));
    set_f(&params.pre, g("pre", 6.0));
    set_b(&params.orbit_on, g("orbit", 0.0) >= 0.5);
    set_f(&params.orbit_rate, g("orate", 0.5));
    set_b(&params.orbit_sync, g("osync", 0.0) >= 0.5);
    setter.begin_set_parameter(&params.orbit_div);
    setter.set_parameter(&params.orbit_div, DivisionParam::from_index(g("odiv", 1.0) as usize));
    setter.end_set_parameter(&params.orbit_div);
    set_f(&params.orbit_radius, g("oradius", 0.3));
    setter.begin_set_parameter(&params.orbit_shape);
    setter.set_parameter(&params.orbit_shape, ShapeParam::from_index(g("oshape", 0.0) as usize));
    setter.end_set_parameter(&params.orbit_shape);
    set_f(&params.orbit_phase, g("ophase", 0.0));
    set_f(&params.post_lp, g("postlp", 16_000.0));
    set_b(&params.auto_gain, g("autogain", 0.0) >= 0.5);
    set_f(&params.mix, g("mix", 1.0));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Shapeshift {
    const NAME: &'static str = "Qeynos SHAPESHIFT";
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
        let orbit_meter = self.orbit_meter.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-shapeshift-window", Vec2::new(600.0, 640.0))
                    .min_size(Vec2::new(500.0, 540.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · SHAPESHIFT").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "shapeshift", "SHAPESHIFT", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("XY-morphing distortion — blend four shapers")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("shapeshift", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("x", "X"), ("y", "Y"), ("postlp", "POST LP"), ("oradius", "ORBIT RADIUS")],
                        );
                        ui.separator();

                        // The XY morph pad: drag the user point, watch the orbit dot — housed in
                        // the CONSOLE v2 CRT bay (glass + scanlines when console is on, plain panel
                        // in THEME-OFF). The pad stays fully draggable inside the glass.
                        let phase = orbit_meter.load(Ordering::Relaxed);
                        suite_core::ui::crt_frame(ui, "shapeshift-crt", 316.0, |ui| {
                            xy_pad(ui, &params, setter, phase);
                        });

                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("shapeshift-corners")
                                .num_columns(4)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "CORNER A", &params.corner_a, setter);
                                    row(ui, "GAIN A", &params.gain_a, setter);
                                    row(ui, "CORNER B", &params.corner_b, setter);
                                    row(ui, "GAIN B", &params.gain_b, setter);
                                    ui.end_row();
                                    row(ui, "CORNER C", &params.corner_c, setter);
                                    row(ui, "GAIN C", &params.gain_c, setter);
                                    row(ui, "CORNER D", &params.corner_d, setter);
                                    row(ui, "GAIN D", &params.gain_d, setter);
                                    ui.end_row();
                                });
                            ui.add_space(6.0);
                            egui::Grid::new("shapeshift-controls")
                                .num_columns(4)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "X", &params.x, setter);
                                    row(ui, "Y", &params.y, setter);
                                    row(ui, "PRE-GAIN", &params.pre, setter);
                                    row(ui, "POST LP", &params.post_lp, setter);
                                    ui.end_row();
                                    row(ui, "ORBIT RATE", &params.orbit_rate, setter);
                                    row(ui, "ORBIT DIV", &params.orbit_div, setter);
                                    row(ui, "ORBIT RADIUS", &params.orbit_radius, setter);
                                    row(ui, "ORBIT SHAPE", &params.orbit_shape, setter);
                                    ui.end_row();
                                    row(ui, "ORBIT", &params.orbit_on, setter);
                                    row(ui, "ORBIT SYNC", &params.orbit_sync, setter);
                                    row(ui, "MIX", &params.mix, setter);
                                    row(ui, "OUT", &params.out, setter);
                                    ui.end_row();
                                    row(ui, "AUTO-GAIN", &params.auto_gain, setter);
                                    row(ui, "ORBIT PHASE", &params.orbit_phase, setter);
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
        self.core = ShapeshiftCore::new(buffer_config.sample_rate);
        // Report the oversampler group delay the dry path is compensated by (PDC).
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "SHAPESHIFT");
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

        let tempo = context.transport().tempo.unwrap_or(120.0) as f32;
        let mut base = self.params.snapshot(tempo);
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                base.x = routes.modulated_float("x", &self.params.x, bus);
                base.y = routes.modulated_float("y", &self.params.y, bus);
                base.post_lp_hz = routes.modulated_float("postlp", &self.params.post_lp, bus);
                base.orbit_radius = routes.modulated_float("oradius", &self.params.orbit_radius, bus);
            }
        }
        self.core.configure(&base);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l_in = main[0][n];
            let r_in = if num_main > 1 { main[1][n] } else { l_in };

            // Per-sample smoothed scalar fields (X/Y are smoothed inside the core).
            let mut s = base;
            s.pre_db = self.params.pre.smoothed.next();
            s.gain_db = [
                self.params.gain_a.smoothed.next(),
                self.params.gain_b.smoothed.next(),
                self.params.gain_c.smoothed.next(),
                self.params.gain_d.smoothed.next(),
            ];
            s.mix = self.params.mix.smoothed.next();
            s.out_db = self.params.out.smoothed.next();

            let (out_l, out_r) = self.core.process_sample(l_in, r_in, &s);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        // Publish the orbit phase for the GUI (cheap, once per block).
        if self.params.editor_state.is_open() {
            self.orbit_meter.store(self.core.orbit_phase(), Ordering::Relaxed);
        }

        // Publish this instance's output spectrum to the suite bus (X-RAY reads it).
        for mut xr_frame in buffer.iter_samples() {
            let xr_n = xr_frame.len().max(1) as f32;
            let mut xr_m = 0.0f32;
            for xr_s in xr_frame.iter_mut() {
                xr_m += *xr_s;
            }
            self.spectrum.feed(xr_m / xr_n);
        }
        self.spectrum.publish();

        ProcessStatus::Normal
    }
}

impl Drop for Shapeshift {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

// ---------------------------------------------------------------------------
// Custom XY morph pad (draggable user point + live orbit dot). Follows the FLYBY xy_pad
// precedent: allocate one click-and-drag rect, map screen↔[0,1]², bracket the drag.
// ---------------------------------------------------------------------------

/// Map an XY point (0..1 × 0..1) to a pixel inside `rect`. Y is up in data space, down in screen
/// space, so it is flipped (corner A=(0,0) is bottom-left).
fn xy_to_screen(rect: egui::Rect, x: f32, y: f32) -> egui::Pos2 {
    let u = x.clamp(0.0, 1.0);
    let v = 1.0 - y.clamp(0.0, 1.0);
    egui::pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height())
}

fn screen_to_xy(rect: egui::Rect, p: egui::Pos2) -> (f32, f32) {
    let u = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    let v = ((p.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
    (u, 1.0 - v)
}

fn xy_pad(ui: &mut egui::Ui, params: &ShapeshiftParams, setter: &ParamSetter, phase: f32) {
    let size = Vec2::new(ui.available_width().min(360.0), 300.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    let ux = params.x.value();
    let uy = params.y.value();

    // --- Drag: move the user point (click anywhere jumps it there). ---
    if response.drag_started() {
        setter.begin_set_parameter(&params.x);
        setter.begin_set_parameter(&params.y);
    }
    if response.dragged() || response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (nx, ny) = screen_to_xy(rect, pos);
            setter.set_parameter(&params.x, nx);
            setter.set_parameter(&params.y, ny);
        }
    }
    if response.drag_stopped() {
        setter.end_set_parameter(&params.x);
        setter.end_set_parameter(&params.y);
    }

    // --- Paint ---
    // CONSOLE re-skin: on the CRT glass the opaque panel backing is dropped (glass shows
    // through) and decorative marks glow phosphor amber; THEME-OFF keeps the original panel +
    // accent. The user point and the orbit dot stay visually distinct (they identify two
    // different positions) — user point solid amber, orbit dot a bright near-white halo.
    let console = suite_core::ui::console_on(ui.ctx());
    let accent = if console { suite_core::ui::PHOSPHOR } else { suite_core::ui::ACCENT };
    let dim = if console { suite_core::ui::PHOSPHOR_DIM } else { suite_core::ui::TEXT_DIM };
    let grid_col = if console {
        suite_core::ui::PHOSPHOR_DIM.linear_multiply(0.35)
    } else {
        egui::Color32::from_rgb(34, 37, 42)
    };
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        if !console {
            painter.rect_filled(rect, 4.0, suite_core::ui::PANEL);
        }
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, if console { suite_core::ui::PHOSPHOR_DIM } else { egui::Color32::from_rgb(40, 43, 48) }),
            egui::StrokeKind::Middle,
        );
        // Corner labels (which shaper each corner uses). Each label is placed at its DSP corner
        // in data space — A=(0,0) bottom-left, B=(1,0) bottom-right, C=(0,1) top-left,
        // D=(1,1) top-right — so the pad labels match the DSP blend weights
        // (`dsp::bilinear_weights`) and the manual (docs/SHAPESHIFT.md). GUI-ONLY fix: the DSP
        // and the pointer↔param mapping (`xy_to_screen`/`screen_to_xy`) are unchanged, so presets
        // sound identical; only the label positions (previously vertically mirrored) are corrected.
        // `xy_to_screen` performs the data→screen y-flip; the anchor keeps each label's text inside
        // the pad at the named corner.
        let labels = [
            (0.02, 0.02, egui::Align2::LEFT_BOTTOM, format!("A · {}", corner_name(params.corner_a.value()))),
            (0.98, 0.02, egui::Align2::RIGHT_BOTTOM, format!("B · {}", corner_name(params.corner_b.value()))),
            (0.02, 0.98, egui::Align2::LEFT_TOP, format!("C · {}", corner_name(params.corner_c.value()))),
            (0.98, 0.98, egui::Align2::RIGHT_TOP, format!("D · {}", corner_name(params.corner_d.value()))),
        ];
        for (dx, dy, anchor, text) in labels {
            let pos = xy_to_screen(rect, dx, dy);
            painter.text(
                pos,
                anchor,
                text,
                egui::FontId::proportional(11.0),
                dim,
            );
        }
        // Faint grid quadrants.
        let mid = xy_to_screen(rect, 0.5, 0.5);
        painter.line_segment(
            [egui::pos2(rect.left(), mid.y), egui::pos2(rect.right(), mid.y)],
            egui::Stroke::new(1.0, grid_col),
        );
        painter.line_segment(
            [egui::pos2(mid.x, rect.top()), egui::pos2(mid.x, rect.bottom())],
            egui::Stroke::new(1.0, grid_col),
        );

        // Orbit path + moving dot (when the orbit is on).
        if params.orbit_on.value() {
            let radius = params.orbit_radius.value();
            let shape = params.orbit_shape.value().to_dsp();
            let steps = 128;
            let pts: Vec<egui::Pos2> = (0..=steps)
                .map(|k| {
                    let ph = k as f32 / steps as f32;
                    let (ox, oy) = dsp::orbit_offset(shape, ph, radius);
                    xy_to_screen(rect, (ux + ox).clamp(0.0, 1.0), (uy + oy).clamp(0.0, 1.0))
                })
                .collect();
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, accent.linear_multiply(0.4)),
            ));
            let (ox, oy) = dsp::orbit_offset(shape, phase, radius);
            let dot = xy_to_screen(rect, (ux + ox).clamp(0.0, 1.0), (uy + oy).clamp(0.0, 1.0));
            painter.circle_filled(dot, 5.0, egui::Color32::from_rgb(240, 240, 245));
            painter.circle_stroke(dot, 7.0, egui::Stroke::new(1.5, accent));
        }

        // The user point.
        let up = xy_to_screen(rect, ux, uy);
        painter.circle_filled(up, 6.0, accent);
        painter.circle_stroke(up, 6.0, egui::Stroke::new(1.0, suite_core::ui::BG));
    }
    // Keep the orbit dot animating — but honor the CRT-motion pref + ~8 fps idle guarantee
    // (guardrails #2/#6) rather than free-running unconditionally.
    suite_core::ui::scope_repaint(ui.ctx());
}

fn corner_name(c: CornerParam) -> &'static str {
    match c {
        CornerParam::Tube => "Tube",
        CornerParam::Tape => "Tape",
        CornerParam::Diode => "Diode",
        CornerParam::Hard => "Hard",
        CornerParam::SineFold => "SineFold",
        CornerParam::TriFold => "TriFold",
        CornerParam::Cheby3 => "Cheby3",
        CornerParam::BitSoft => "BitSoft",
    }
}

impl ClapPlugin for Shapeshift {
    const CLAP_ID: &'static str = "com.qeynos.shapeshift";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "XY-morphing distortion — bilinear blend of four selectable waveshapers with a built-in \
         orbit LFO, 4x oversampled",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
    ];
}

impl Vst3Plugin for Shapeshift {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosSHAPESHFT1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Shapeshift);
nih_export_vst3!(Shapeshift);

// A compile-time nudge that the DSP corner count and the four param corners agree.
const _: () = assert!(NUM_CORNERS == 4);

#[cfg(test)]
mod render_tests {
    use crate::dsp::ShapeshiftCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(
            crate::MANUAL_DOC,
            &crate::ShapeshiftParams::default(),
        );
    }

    /// Render each factory preset over pink noise and a full-band chirp, write the WAVs (L
    /// channel) into renders/SHAPESHIFT/, and assert the universal properties on each channel.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.5, (sr * 3.0) as usize, 7331);
        let chirp = testsig::log_chirp(40.0, 12_000.0, 0.5, (sr * 3.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");

            for (tag, input) in [("pink", &pink), ("chirp", &chirp)] {
                let mut core = ShapeshiftCore::new(sr);
                let (l, r) = core.process_stereo(input, &s);
                assert_universal(&l);
                assert_universal(&r);
                let path = render_path("SHAPESHIFT", &format!("{fname}_{tag}"));
                write_wav(&path, &l, sr as u32).expect("write render");
            }
        }
    }
}
