//! CLEAVE — multi-slicer with a transport-locked step sequencer (Qeynos suite, Phase 2b;
//! Slice clone).
//!
//! A 2-bar rolling capture buffer is sliced (fixed grid 1/8–1/32, or transient onset detection
//! via spectral flux + zero-cross backtrack) and replayed by a step sequencer locked to the host
//! playhead. Each of 16–64 steps carries its own lanes — slice index (or "as played"), gate,
//! reverse, pitch (±12 st), roll (×2/3/4), probability, and level — edited on the step-grid
//! widget. Grain-windowed reads keep it click-free. The dry path is zero-latency, so `mix = 0`
//! nulls exactly. See [`dsp`] for the core (shared verbatim with the offline done-bar tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Sense, Vec2},
    EguiState,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use suite_core::bus::PluginKind;
use suite_core::modlisten::ModRoutes;
use suite_core::spectrum::SpectrumPublisher;

pub mod dsp;
pub mod presets;

use dsp::{
    build_pattern, randomize_grid, CleaveCore, GridDiv, Settings, SliceMode, StepData, StepGrid,
    MAX_STEPS, MIN_STEPS,
};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing enums, mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SliceModeParam {
    #[id = "transient"]
    #[name = "Transient"]
    Transient,
    #[id = "grid"]
    #[name = "Grid"]
    Grid,
}

impl SliceModeParam {
    fn to_dsp(self) -> SliceMode {
        match self {
            SliceModeParam::Transient => SliceMode::Transient,
            SliceModeParam::Grid => SliceMode::Grid,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum GridDivParam {
    #[id = "d8"]
    #[name = "1/8"]
    Eighth,
    #[id = "d16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "d32"]
    #[name = "1/32"]
    ThirtySecond,
}

impl GridDivParam {
    fn to_dsp(self) -> GridDiv {
        match self {
            GridDivParam::Eighth => GridDiv::Eighth,
            GridDivParam::Sixteenth => GridDiv::Sixteenth,
            GridDivParam::ThirtySecond => GridDiv::ThirtySecond,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Cleave {
    params: Arc<CleaveParams>,
    core: CleaveCore,
    /// Scratch grid snapshot copied out of the RwLock each block (avoids locking in process).
    grid_scratch: Vec<StepData>,
    /// Published from `process` for the GUI playhead.
    cur_step: Arc<AtomicU32>,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct CleaveParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// Per-step lanes — persisted host state, edited by the step-grid widget (NOT automatable:
    /// 16..64 steps × 8 lanes is far too many for the automation tree / validator fuzzer).
    #[persist = "grid"]
    pub grid: RwLock<StepGrid>,

    #[id = "slicemode"]
    pub slice_mode: EnumParam<SliceModeParam>,
    #[id = "sens"]
    pub sensitivity: FloatParam,
    #[id = "griddiv"]
    pub grid_div: EnumParam<GridDivParam>,
    #[id = "steps"]
    pub steps: IntParam,
    #[id = "swing"]
    pub swing: FloatParam,
    #[id = "density"]
    pub density: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for CleaveParams {
    fn default() -> Self {
        let d = Settings::default();
        // Default project state: a Straight Rechop pattern so a fresh instance makes sound.
        let mut grid = StepGrid::default();
        let pat = build_pattern(0, d.steps, 1);
        grid.steps[..MAX_STEPS].copy_from_slice(&pat);
        Self {
            editor_state: EguiState::from_size(680, 560),
            grid: RwLock::new(grid),
            slice_mode: EnumParam::new("Slice Mode", SliceModeParam::Grid),
            sensitivity: FloatParam::new(
                "Sensitivity",
                d.sensitivity,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            grid_div: EnumParam::new("Grid", GridDivParam::Sixteenth),
            steps: IntParam::new(
                "Steps",
                d.steps as i32,
                IntRange::Linear {
                    min: MIN_STEPS as i32,
                    max: MAX_STEPS as i32,
                },
            ),
            swing: FloatParam::new("Swing", d.swing, FloatRange::Linear { min: 0.0, max: 0.75 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            density: FloatParam::new("Density", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl CleaveParams {
    fn snapshot(&self) -> Settings {
        Settings {
            slice_mode: self.slice_mode.value().to_dsp(),
            sensitivity: self.sensitivity.value(),
            grid_div: self.grid_div.value().to_dsp(),
            steps: (self.steps.value() as usize).clamp(MIN_STEPS, MAX_STEPS),
            swing: self.swing.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Cleave {
    fn default() -> Self {
        Self {
            params: Arc::new(CleaveParams::default()),
            core: CleaveCore::new(48_000.0),
            grid_scratch: vec![StepData::default(); MAX_STEPS],
            cur_step: Arc::new(AtomicU32::new(0)),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset: scalar params through the host (automation/undo see them) + the
/// per-step grid written into the persisted state.
fn apply_preset(params: &CleaveParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
    setter.begin_set_parameter(&params.slice_mode);
    setter.set_parameter(
        &params.slice_mode,
        match s.slice_mode {
            SliceMode::Transient => SliceModeParam::Transient,
            SliceMode::Grid => SliceModeParam::Grid,
        },
    );
    setter.end_set_parameter(&params.slice_mode);
    setter.begin_set_parameter(&params.grid_div);
    setter.set_parameter(
        &params.grid_div,
        match s.grid_div {
            GridDiv::Eighth => GridDivParam::Eighth,
            GridDiv::Sixteenth => GridDivParam::Sixteenth,
            GridDiv::ThirtySecond => GridDivParam::ThirtySecond,
        },
    );
    setter.end_set_parameter(&params.grid_div);

    let set_i = |param: &IntParam, v: i32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_i(&params.steps, s.steps as i32);
    set_f(&params.sensitivity, s.sensitivity);
    set_f(&params.swing, s.swing);
    set_f(&params.mix, s.mix);
    set_f(&params.out, s.out_db);

    // Per-step grid (persisted, not automatable).
    let pat = presets::grid_from_preset(p);
    if let Ok(mut g) = params.grid.write() {
        g.ensure_len(MAX_STEPS);
        g.steps[..MAX_STEPS].copy_from_slice(&pat);
    }
}

impl Plugin for Cleave {
    const NAME: &'static str = "Qeynos CLEAVE";
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

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
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
        let cur_step = self.cur_step.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-cleave-window", Vec2::new(680.0, 560.0))
                    .min_size(Vec2::new(560.0, 440.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · CLEAVE").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(
                                "multi-slicer — transport-locked step sequencer over a 2-bar buffer",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("cleave", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("mix", "MIX"), ("swing", "SWING"), ("sens", "SENS"), ("out", "OUT")],
                        );
                        ui.separator();

                        // Pattern actions: Randomize (density) + Clear.
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("PATTERN")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            let steps = (params.steps.value() as usize).clamp(MIN_STEPS, MAX_STEPS);
                            if ui.button("Randomize").clicked() {
                                let seed = egui_ctx.input(|i| (i.time * 1000.0) as u32).max(1);
                                if let Ok(mut g) = params.grid.write() {
                                    g.ensure_len(MAX_STEPS);
                                    randomize_grid(&mut g.steps, steps, params.density.value(), seed);
                                }
                            }
                            if ui.button("Clear").clicked() {
                                if let Ok(mut g) = params.grid.write() {
                                    for s in g.steps.iter_mut() {
                                        s.active = false;
                                    }
                                }
                            }
                            if ui.button("Fill").clicked() {
                                if let Ok(mut g) = params.grid.write() {
                                    g.ensure_len(MAX_STEPS);
                                    g.steps[..MAX_STEPS].copy_from_slice(&build_pattern(0, steps, 1));
                                }
                            }
                        });
                        ui.add_space(4.0);

                        // The step-grid editor.
                        let playhead = cur_step.load(Ordering::Relaxed) as usize;
                        step_grid_widget(ui, &params, playhead);

                        ui.add_space(6.0);
                        egui::Grid::new("cleave-controls")
                            .num_columns(4)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "SLICE MODE", &params.slice_mode, setter);
                                row(ui, "GRID", &params.grid_div, setter);
                                row(ui, "SENSITIVITY", &params.sensitivity, setter);
                                row(ui, "STEPS", &params.steps, setter);
                                ui.end_row();
                                row(ui, "SWING", &params.swing, setter);
                                row(ui, "DENSITY", &params.density, setter);
                                row(ui, "MIX", &params.mix, setter);
                                row(ui, "OUT", &params.out, setter);
                                ui.end_row();
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
        self.core = CleaveCore::new(buffer_config.sample_rate);
        // Dry path is zero-latency; the wet is a re-timed creative signal (no PDC).
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "CLEAVE");
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
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
                s.swing = routes.modulated_float("swing", &self.params.swing, bus);
                s.sensitivity = routes.modulated_float("sens", &self.params.sensitivity, bus);
                s.out_db = routes.modulated_float("out", &self.params.out, bus);
            }
        }
        self.core.configure(&s);

        // Snapshot the per-step grid without locking in the audio callback: try_read, else keep
        // the core's previous snapshot.
        if let Ok(g) = self.params.grid.try_read() {
            let n = g.steps.len().min(MAX_STEPS);
            self.grid_scratch[..n].copy_from_slice(&g.steps[..n]);
            self.core.set_grid(&self.grid_scratch[..n]);
        }

        // Transport → shared TransportFrame.
        let t = context.transport();
        let sr = self.core_sr();
        let tempo = t.tempo.unwrap_or(120.0).max(1.0);
        let tsn = t.time_sig_numerator.unwrap_or(4).max(1) as f64;
        let tsd = t.time_sig_denominator.unwrap_or(4).max(1) as f64;
        let beats_per_bar = (tsn * 4.0 / tsd).max(1.0e-3);
        let bars_per_sample = (tempo / 60.0 / sr) / beats_per_bar;
        let bar_pos = t.pos_beats().unwrap_or(0.0) / beats_per_bar;
        let frame = suite_core::testsig::TransportFrame {
            playing: t.playing,
            tempo,
            ppq_pos: t.pos_beats().unwrap_or(0.0),
            bar_pos,
            bars_per_sample,
            beats_per_bar,
        };
        self.core.set_transport(&frame);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_ch = main.len();
        if num_ch == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l_in = main[0][n];
            let r_in = if num_ch > 1 { main[1][n] } else { l_in };
            let (ol, or) = self.core.process_sample(l_in, r_in);
            main[0][n] = ol;
            if num_ch > 1 {
                main[1][n] = or;
            }
        }

        if self.params.editor_state.is_open() {
            self.cur_step
                .store(self.core.current_step() as u32, Ordering::Relaxed);
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

impl Drop for Cleave {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl Cleave {
    fn core_sr(&self) -> f64 {
        // The core stores its own SR; expose via a small accessor to avoid re-reading params.
        self.core.sample_rate() as f64
    }
}

// ---------------------------------------------------------------------------
// Step-grid widget: one row of `steps` columns; a lane selector re-targets what a
// click/drag edits. Follows the FLYBY xy_pad convention (allocate → handle drag → paint).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Lane {
    Level,
    Gate,
    Reverse,
    Roll,
    Pitch,
    Prob,
    Active,
}

fn step_grid_widget(ui: &mut egui::Ui, params: &CleaveParams, playhead: usize) {
    // Lane selector.
    let lane_id = ui.id().with("cleave-lane");
    let mut lane: Lane = ui.memory(|m| m.data.get_temp(lane_id).unwrap_or(Lane::Level));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("LANE").color(suite_core::ui::TEXT_DIM).small());
        for (l, name) in [
            (Lane::Level, "Level"),
            (Lane::Gate, "Gate"),
            (Lane::Pitch, "Pitch"),
            (Lane::Roll, "Roll"),
            (Lane::Reverse, "Rev"),
            (Lane::Prob, "Prob"),
            (Lane::Active, "On"),
        ] {
            if ui.selectable_label(lane == l, name).clicked() {
                lane = l;
            }
        }
    });
    ui.memory_mut(|m| m.data.insert_temp(lane_id, lane));

    let steps = (params.steps.value() as usize).clamp(MIN_STEPS, MAX_STEPS);
    let size = Vec2::new(ui.available_width(), 130.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let col_w = rect.width() / steps as f32;

    // Which column is the pointer over?
    let hovered_col = response.interact_pointer_pos().map(|p| {
        (((p.x - rect.left()) / col_w).floor() as i32).clamp(0, steps as i32 - 1) as usize
    });

    // --- interaction ---
    if let Some(col) = hovered_col {
        if let Ok(mut g) = params.grid.write() {
            g.ensure_len(MAX_STEPS);
            let sd = &mut g.steps[col];
            // Vertical position → normalized value (top = 1, bottom = 0).
            let vy = response
                .interact_pointer_pos()
                .map(|p| (1.0 - (p.y - rect.top()) / rect.height()).clamp(0.0, 1.0))
                .unwrap_or(0.0);
            let just_clicked = response.drag_started() || response.clicked();
            match lane {
                Lane::Level => {
                    if response.dragged() || just_clicked {
                        sd.level = vy;
                        sd.active = true;
                    }
                }
                Lane::Gate => {
                    if response.dragged() || just_clicked {
                        sd.gate = vy.clamp(0.05, 1.0);
                    }
                }
                Lane::Prob => {
                    if response.dragged() || just_clicked {
                        sd.probability = vy;
                    }
                }
                Lane::Pitch => {
                    if response.dragged() || just_clicked {
                        sd.pitch = ((vy * 2.0 - 1.0) * 12.0).round() as i32;
                    }
                }
                Lane::Reverse => {
                    if just_clicked {
                        sd.reverse = !sd.reverse;
                    }
                }
                Lane::Roll => {
                    if just_clicked {
                        sd.roll = match sd.roll {
                            1 => 2,
                            2 => 3,
                            3 => 4,
                            _ => 1,
                        };
                    }
                }
                Lane::Active => {
                    if just_clicked {
                        sd.active = !sd.active;
                    }
                }
            }
        }
    }

    // --- paint ---
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, suite_core::ui::PANEL);
        let grid = params.grid.read().ok();
        for i in 0..steps {
            let x0 = rect.left() + i as f32 * col_w;
            let cell = egui::Rect::from_min_size(
                egui::pos2(x0 + 1.0, rect.top() + 1.0),
                egui::vec2((col_w - 2.0).max(1.0), rect.height() - 2.0),
            );
            // beat shading every 4 steps
            if (i / 4) % 2 == 0 {
                painter.rect_filled(cell, 2.0, egui::Color32::from_rgb(30, 33, 38));
            }
            if let Some(g) = &grid {
                let sd = g.steps.get(i).copied().unwrap_or_default();
                let val = match lane {
                    Lane::Level => sd.level,
                    Lane::Gate => sd.gate,
                    Lane::Prob => sd.probability,
                    Lane::Pitch => (sd.pitch as f32 / 12.0) * 0.5 + 0.5,
                    Lane::Reverse => {
                        if sd.reverse {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    Lane::Roll => (sd.roll.saturating_sub(1) as f32 / 3.0).clamp(0.0, 1.0),
                    Lane::Active => {
                        if sd.active {
                            1.0
                        } else {
                            0.0
                        }
                    }
                };
                if sd.active {
                    let h = (rect.height() - 2.0) * val.clamp(0.02, 1.0);
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(cell.left(), cell.bottom() - h),
                        egui::vec2(cell.width(), h),
                    );
                    let mut col = suite_core::ui::ACCENT;
                    if sd.reverse {
                        col = egui::Color32::from_rgb(120, 180, 240);
                    }
                    painter.rect_filled(bar, 1.0, col);
                    // roll subdivisions
                    if sd.roll > 1 {
                        for k in 1..sd.roll {
                            let rx = cell.left() + cell.width() * (k as f32 / sd.roll as f32);
                            painter.line_segment(
                                [egui::pos2(rx, cell.top()), egui::pos2(rx, cell.bottom())],
                                egui::Stroke::new(1.0, suite_core::ui::BG),
                            );
                        }
                    }
                } else {
                    // inactive marker
                    painter.line_segment(
                        [
                            egui::pos2(cell.left(), cell.bottom()),
                            egui::pos2(cell.right(), cell.bottom()),
                        ],
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 54, 60)),
                    );
                }
            }
        }
        // playhead
        if playhead < steps {
            let px = rect.left() + (playhead as f32 + 0.5) * col_w;
            painter.line_segment(
                [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(240, 240, 245)),
            );
        }
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 43, 48)),
            egui::StrokeKind::Middle,
        );
    }
    ui.ctx().request_repaint();
}

impl ClapPlugin for Cleave {
    const CLAP_ID: &'static str = "com.qeynos.cleave";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Multi-slicer with a transport-locked step sequencer — 2-bar rolling buffer, grid or \
         transient slicing, per-step gate/reverse/pitch/roll/probability/level, grain-windowed \
         reads",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Glitch,
        ClapFeature::Custom("slicer"),
    ];
}

impl Vst3Plugin for Cleave {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosCLEAVEslc1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Custom("Slicer")];
}

nih_export_clap!(Cleave);
nih_export_vst3!(Cleave);

#[cfg(test)]
mod tests;
