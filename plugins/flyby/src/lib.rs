//! FLYBY — doppler spatializer (Qeynos suite, Phase 2b; Transfer clone).
//!
//! A mono source is flown around an editable closed **path** on an XY pad (listener at the
//! origin). Its distance and azimuth to the listener drive four cues: a moving fractional-delay
//! **doppler** shift (rate-clamped so sharp corners don't spike the pitch), inverse-distance
//! **level**, distance-dependent **air absorption** (a one-pole low-pass), and an equal-power
//! **pan** with an optional sub-millisecond **micro-ITD**, followed by a **width** control. The
//! traversal is phase-driven, free (Hz) or BPM-synced.
//!
//! The fractional delay *is* the effect (distance = delay), so FLYBY reports **zero latency** and
//! `mix = 0` nulls against the dry input. See [`dsp`] for the DSP core, shared verbatim with the
//! offline harness / done-bar tests.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Sense, Vec2},
    EguiState,
};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use suite_core::bus::PluginKind;
use suite_core::modlisten::ModRoutes;
use suite_core::spectrum::SpectrumPublisher;

pub mod dsp;
pub mod presets;

use dsp::{FlybyCore, PathShape, Settings, SyncDivision, MAX_NODES, MIN_NODES};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/FLYBY.md");

// ---------------------------------------------------------------------------
// Param-facing enum (nih-plug `Enum`), mapped onto the pure-DSP enum.
// ---------------------------------------------------------------------------

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
// Per-node param group (an XY control point of the path)
// ---------------------------------------------------------------------------

#[derive(Params)]
pub struct NodeParams {
    #[id = "x"]
    pub x: FloatParam,
    #[id = "y"]
    pub y: FloatParam,
}

impl NodeParams {
    fn new(idx: usize, pos: (f32, f32)) -> Self {
        Self {
            x: FloatParam::new(
                format!("Node {idx} X"),
                pos.0,
                FloatRange::Linear { min: -2.0, max: 2.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            y: FloatParam::new(
                format!("Node {idx} Y"),
                pos.1,
                FloatRange::Linear { min: -2.0, max: 2.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Flyby {
    params: Arc<FlybyParams>,
    core: FlybyCore,
    /// Traversal phase published from `process` for the GUI moving dot.
    phase_meter: Arc<AtomicF32>,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct FlybyParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[nested(id_prefix = "n0", group = "Node 0")]
    pub n0: NodeParams,
    #[nested(id_prefix = "n1", group = "Node 1")]
    pub n1: NodeParams,
    #[nested(id_prefix = "n2", group = "Node 2")]
    pub n2: NodeParams,
    #[nested(id_prefix = "n3", group = "Node 3")]
    pub n3: NodeParams,
    #[nested(id_prefix = "n4", group = "Node 4")]
    pub n4: NodeParams,
    #[nested(id_prefix = "n5", group = "Node 5")]
    pub n5: NodeParams,
    #[nested(id_prefix = "n6", group = "Node 6")]
    pub n6: NodeParams,
    #[nested(id_prefix = "n7", group = "Node 7")]
    pub n7: NodeParams,

    #[id = "nodes"]
    pub node_count: IntParam,
    #[id = "speed"]
    pub speed: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "division"]
    pub division: EnumParam<DivisionParam>,
    #[id = "size"]
    pub size: FloatParam,
    #[id = "doppler"]
    pub doppler: FloatParam,
    #[id = "air"]
    pub air: FloatParam,
    #[id = "itd"]
    pub itd: BoolParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for FlybyParams {
    fn default() -> Self {
        let d = Settings::default();
        let n = d.nodes;
        Self {
            editor_state: EguiState::from_size(560, 640),
            n0: NodeParams::new(0, n[0]),
            n1: NodeParams::new(1, n[1]),
            n2: NodeParams::new(2, n[2]),
            n3: NodeParams::new(3, n[3]),
            n4: NodeParams::new(4, n[4]),
            n5: NodeParams::new(5, n[5]),
            n6: NodeParams::new(6, n[6]),
            n7: NodeParams::new(7, n[7]),
            node_count: IntParam::new(
                "Nodes",
                d.node_count as i32,
                IntRange::Linear {
                    min: MIN_NODES as i32,
                    max: MAX_NODES as i32,
                },
            ),
            speed: FloatParam::new(
                "Speed",
                d.speed_hz,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            sync: BoolParam::new("Sync", d.sync),
            division: EnumParam::new("Division", DivisionParam::Bar),
            size: FloatParam::new(
                "Size",
                d.size,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 30.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            doppler: FloatParam::new("Doppler", d.doppler, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            air: FloatParam::new("Air", d.air, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            itd: BoolParam::new("ITD", d.itd),
            width: FloatParam::new("Width", d.width, FloatRange::Linear { min: 0.0, max: 2.0 })
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

impl FlybyParams {
    fn node_refs(&self) -> [&NodeParams; MAX_NODES] {
        [
            &self.n0, &self.n1, &self.n2, &self.n3, &self.n4, &self.n5, &self.n6, &self.n7,
        ]
    }

    /// Snapshot the live parameters into a DSP [`Settings`]. `tempo_bpm` comes from the host.
    fn snapshot(&self, tempo_bpm: f32) -> Settings {
        let refs = self.node_refs();
        let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
        for (i, np) in refs.iter().enumerate() {
            nodes[i] = (np.x.value(), np.y.value());
        }
        Settings {
            nodes,
            node_count: self.node_count.value() as usize,
            speed_hz: self.speed.value(),
            sync: self.sync.value(),
            division: self.division.value().to_dsp(),
            tempo_bpm,
            size: self.size.value(),
            doppler: self.doppler.value(),
            air: self.air.value(),
            itd: self.itd.value(),
            width: self.width.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Flyby {
    fn default() -> Self {
        Self {
            params: Arc::new(FlybyParams::default()),
            core: FlybyCore::new(48_000.0),
            phase_meter: Arc::new(AtomicF32::new(0.0)),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Write a shape's node layout into the node params through the host (used by both factory
/// presets and the GUI shape buttons).
fn apply_shape(params: &FlybyParams, setter: &ParamSetter, shape: PathShape, count: usize) {
    let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
    let n = shape.layout(&mut nodes, count);
    setter.begin_set_parameter(&params.node_count);
    setter.set_parameter(&params.node_count, n as i32);
    setter.end_set_parameter(&params.node_count);
    let refs = params.node_refs();
    for (i, np) in refs.iter().enumerate() {
        setter.begin_set_parameter(&np.x);
        setter.set_parameter(&np.x, nodes[i].0);
        setter.end_set_parameter(&np.x);
        setter.begin_set_parameter(&np.y);
        setter.set_parameter(&np.y, nodes[i].1);
        setter.end_set_parameter(&np.y);
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &FlybyParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let count = g("nodes", params.node_count.value() as f32) as usize;

    // Path: explicit nodes (user-style) if present, else expand the named shape.
    if p.get("n0x").is_some() {
        setter.begin_set_parameter(&params.node_count);
        setter.set_parameter(&params.node_count, count.clamp(MIN_NODES, MAX_NODES) as i32);
        setter.end_set_parameter(&params.node_count);
        for (i, np) in params.node_refs().iter().enumerate() {
            let x = p.get(&format!("n{i}x")).unwrap_or(0.0);
            let y = p.get(&format!("n{i}y")).unwrap_or(0.0);
            setter.begin_set_parameter(&np.x);
            setter.set_parameter(&np.x, x);
            setter.end_set_parameter(&np.x);
            setter.begin_set_parameter(&np.y);
            setter.set_parameter(&np.y, y);
            setter.end_set_parameter(&np.y);
        }
    } else {
        apply_shape(params, setter, PathShape::from_index(g("shape", 0.0) as usize), count);
    }

    setter.begin_set_parameter(&params.sync);
    setter.set_parameter(&params.sync, g("sync", 0.0) >= 0.5);
    setter.end_set_parameter(&params.sync);
    setter.begin_set_parameter(&params.itd);
    setter.set_parameter(&params.itd, g("itd", 1.0) >= 0.5);
    setter.end_set_parameter(&params.itd);
    setter.begin_set_parameter(&params.division);
    setter.set_parameter(&params.division, DivisionParam::from_index(g("division", 1.0) as usize));
    setter.end_set_parameter(&params.division);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.speed, g("speed", 0.5));
    set_f(&params.size, g("size", 8.0));
    set_f(&params.doppler, g("doppler", 0.7));
    set_f(&params.air, g("air", 0.5));
    set_f(&params.width, g("width", 1.0));
    set_f(&params.mix, g("mix", 1.0));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Flyby {
    const NAME: &'static str = "Qeynos FLYBY";
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
            main_output_channels: NonZeroU32::new(2),
            names: PortNames {
                layout: Some("Mono→Stereo"),
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
        let phase_meter = self.phase_meter.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-flyby-window", Vec2::new(560.0, 640.0))
                    .min_size(Vec2::new(460.0, 520.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        // The optional node-coordinate type-in section (below) can grow the content
                        // past the base height, so scroll rather than clip.
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · FLYBY").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "flyby", "FLYBY", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("doppler spatializer — fly a source around the listener")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("flyby", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("mix", "MIX"), ("doppler", "DOPPLER"), ("size", "SIZE"), ("width", "WIDTH")],
                        );
                        ui.separator();

                        // Shape buttons.
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("PATH").color(suite_core::ui::TEXT_DIM).small());
                            let count = params.node_count.value() as usize;
                            if ui.button("Circle").clicked() {
                                apply_shape(&params, setter, PathShape::Circle, count);
                            }
                            if ui.button("Ellipse").clicked() {
                                apply_shape(&params, setter, PathShape::Ellipse, count);
                            }
                            if ui.button("Figure-8").clicked() {
                                apply_shape(&params, setter, PathShape::Figure8, count);
                            }
                        });
                        ui.add_space(4.0);

                        // The XY pad: draw + edit the path, show the moving source dot.
                        // It is FLYBY's primary interactive display surface, so it lives in
                        // the CONSOLE v2 CRT telemetry bay (drag/edit still work identically;
                        // THEME-OFF degrades the bay to a plain panel with the original colors).
                        let phase = phase_meter.load(Ordering::Relaxed);
                        suite_core::ui::crt_frame(ui, "flyby-crt", 316.0, |ui| {
                            xy_pad(ui, &params, setter, phase);
                        });

                        ui.add_space(6.0);
                        egui::Grid::new("flyby-controls")
                            .num_columns(4)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "NODES", &params.node_count, setter);
                                row(ui, "SPEED", &params.speed, setter);
                                row(ui, "SIZE", &params.size, setter);
                                row(ui, "DIVISION", &params.division, setter);
                                ui.end_row();
                                row(ui, "DOPPLER", &params.doppler, setter);
                                row(ui, "AIR", &params.air, setter);
                                row(ui, "WIDTH", &params.width, setter);
                                row(ui, "MIX", &params.mix, setter);
                                ui.end_row();
                                row(ui, "SYNC", &params.sync, setter);
                                row(ui, "ITD", &params.itd, setter);
                                row(ui, "OUT", &params.out, setter);
                                ui.end_row();
                            });

                        // Guardrail #3: the path node X/Y are automatable params settable by
                        // dragging in the pad, so they must ALSO be readable + type-in-able outside
                        // the glass. Collapsed by default (the pad is the primary editor); the
                        // enclosing ScrollArea keeps it from clipping when expanded.
                        ui.add_space(6.0);
                        let count = (params.node_count.value() as usize).clamp(MIN_NODES, MAX_NODES);
                        egui::CollapsingHeader::new("NODE X/Y (type-in)")
                            .id_salt("flyby-node-coords")
                            .show(ui, |ui| {
                                let refs = params.node_refs();
                                egui::Grid::new("flyby-node-grid")
                                    .num_columns(4)
                                    .spacing([10.0, 6.0])
                                    .show(ui, |ui| {
                                        for i in 0..count {
                                            row(ui, &format!("N{} X", i + 1), &refs[i].x, setter);
                                            row(ui, &format!("N{} Y", i + 1), &refs[i].y, setter);
                                            if i % 2 == 1 {
                                                ui.end_row();
                                            }
                                        }
                                        if count % 2 == 1 {
                                            ui.end_row();
                                        }
                                    });
                            });
                        }); // ScrollArea
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
        self.core = FlybyCore::new(buffer_config.sample_rate);
        // The delay line IS the effect — zero reported processing latency.
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum
            .init(buffer_config.sample_rate, PluginKind::Generic, "FLYBY");
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
        let mut s = self.params.snapshot(tempo);
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
                s.doppler = routes.modulated_float("doppler", &self.params.doppler, bus);
                s.size = routes.modulated_float("size", &self.params.size, bus);
                s.width = routes.modulated_float("width", &self.params.width, bus);
            }
        }
        self.core.configure(&s);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l_in = main[0][n];
            let r_in = if num_main > 1 { main[1][n] } else { l_in };
            let (out_l, out_r) = self.core.process_sample(l_in, r_in, &s);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        // Publish the traversal phase for the GUI (cheap, once per block).
        if self.params.editor_state.is_open() {
            self.phase_meter.store(self.core.phase(), Ordering::Relaxed);
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

impl Drop for Flyby {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

// ---------------------------------------------------------------------------
// Custom XY-pad widget (path editor + moving source dot). Follows the suite knob
// input/paint conventions (allocate → handle drag with begin/end_set_parameter → paint).
// ---------------------------------------------------------------------------

/// Map a node-space coordinate (x,y in ~[-1.6,1.6]) to a pixel inside `rect`. y is up in
/// node-space, down in screen-space, so it is flipped.
fn node_to_screen(rect: egui::Rect, x: f32, y: f32) -> egui::Pos2 {
    const RANGE: f32 = 1.6;
    let u = (x / RANGE * 0.5 + 0.5).clamp(0.0, 1.0);
    let v = (0.5 - y / RANGE * 0.5).clamp(0.0, 1.0);
    egui::pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height())
}

/// Inverse of [`node_to_screen`].
fn screen_to_node(rect: egui::Rect, p: egui::Pos2) -> (f32, f32) {
    const RANGE: f32 = 1.6;
    let u = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let v = ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    ((u - 0.5) * 2.0 * RANGE, (0.5 - v) * 2.0 * RANGE)
}

fn xy_pad(ui: &mut egui::Ui, params: &FlybyParams, setter: &ParamSetter, phase: f32) {
    let size = Vec2::new(ui.available_width().min(360.0), 300.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let count = (params.node_count.value() as usize).clamp(MIN_NODES, MAX_NODES);
    let refs = params.node_refs();

    // Current node positions.
    let mut nodes = [(0.0f32, 0.0f32); MAX_NODES];
    for (i, np) in refs.iter().enumerate() {
        nodes[i] = (np.x.value(), np.y.value());
    }

    // --- Drag handling: pick the nearest node on press, drag it around. ---
    let drag_id = response.id.with("drag-node");
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut best = (f32::INFINITY, usize::MAX);
            for i in 0..count {
                let sp = node_to_screen(rect, nodes[i].0, nodes[i].1);
                let d = sp.distance(pos);
                if d < best.0 {
                    best = (d, i);
                }
            }
            // Only grab if reasonably close to a handle.
            if best.0 < 28.0 {
                ui.memory_mut(|m| m.data.insert_temp(drag_id, best.1));
                setter.begin_set_parameter(&refs[best.1].x);
                setter.begin_set_parameter(&refs[best.1].y);
            } else {
                ui.memory_mut(|m| m.data.insert_temp(drag_id, usize::MAX));
            }
        }
    }
    if response.dragged() {
        let idx: usize = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or(usize::MAX));
        if idx < count {
            if let Some(pos) = response.interact_pointer_pos() {
                let (nx, ny) = screen_to_node(rect, pos);
                setter.set_parameter(&refs[idx].x, nx.clamp(-2.0, 2.0));
                setter.set_parameter(&refs[idx].y, ny.clamp(-2.0, 2.0));
                nodes[idx] = (nx, ny);
            }
        }
    }
    if response.drag_stopped() {
        let idx: usize = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or(usize::MAX));
        if idx < count {
            setter.end_set_parameter(&refs[idx].x);
            setter.end_set_parameter(&refs[idx].y);
        }
        ui.memory_mut(|m| m.data.insert_temp(drag_id, usize::MAX));
    }

    // --- Paint ---
    if ui.is_rect_visible(rect) {
        // CONSOLE re-skins the decorative pad (glass background, phosphor path/handles);
        // THEME-OFF keeps the original panel + amber. The moving SOURCE dot stays bright
        // white in both themes so the live position reads clearly against the trace.
        let console = suite_core::ui::console_on(ui.ctx());
        let trace = if console { suite_core::ui::PHOSPHOR } else { suite_core::ui::ACCENT };
        let mark = if console { suite_core::ui::PHOSPHOR_DIM } else { suite_core::ui::TEXT_DIM };
        let grid = if console {
            suite_core::ui::PHOSPHOR_DIM.linear_multiply(0.35)
        } else {
            egui::Color32::from_rgb(34, 37, 42)
        };
        let handle_edge = if console { suite_core::ui::GLASS_BG } else { suite_core::ui::BG };
        let painter = ui.painter_at(rect);
        if console {
            // Let the CRT glass + scanlines from `crt_frame` show through.
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, suite_core::ui::PHOSPHOR_DIM.linear_multiply(0.5)),
                egui::StrokeKind::Middle,
            );
        } else {
            painter.rect_filled(rect, 4.0, suite_core::ui::PANEL);
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 43, 48)),
                egui::StrokeKind::Middle,
            );
        }
        // Center cross-hairs (listener).
        let center = node_to_screen(rect, 0.0, 0.0);
        painter.line_segment(
            [egui::pos2(rect.left(), center.y), egui::pos2(rect.right(), center.y)],
            egui::Stroke::new(1.0, grid),
        );
        painter.line_segment(
            [egui::pos2(center.x, rect.top()), egui::pos2(center.x, rect.bottom())],
            egui::Stroke::new(1.0, grid),
        );
        // Listener marker.
        painter.circle_stroke(center, 5.0, egui::Stroke::new(1.5, mark));
        painter.circle_filled(center, 1.5, mark);

        // Path curve (sampled Catmull-Rom).
        let steps = 240;
        let pts: Vec<egui::Pos2> = (0..=steps)
            .map(|k| {
                let p = k as f32 / steps as f32;
                let (x, y) = dsp::path_position(&nodes, count, p % 1.0);
                node_to_screen(rect, x, y)
            })
            .collect();
        painter.add(egui::Shape::line(
            pts,
            egui::Stroke::new(1.5, trace.linear_multiply(0.7)),
        ));

        // Node handles.
        for i in 0..count {
            let sp = node_to_screen(rect, nodes[i].0, nodes[i].1);
            painter.circle_filled(sp, 5.0, trace);
            painter.circle_stroke(sp, 5.0, egui::Stroke::new(1.0, handle_edge));
        }

        // Moving source dot at the current traversal phase.
        let (sx, sy) = dsp::path_position(&nodes, count, phase);
        let dot = node_to_screen(rect, sx, sy);
        painter.circle_filled(dot, 6.0, egui::Color32::from_rgb(240, 240, 245));
        painter.circle_stroke(dot, 8.0, egui::Stroke::new(1.5, trace));
        // A faint line from the listener to the source (the current distance/azimuth).
        painter.line_segment(
            [center, dot],
            egui::Stroke::new(1.0, trace.linear_multiply(0.35)),
        );
    }
    // Guardrail #6: only free-run repaint while CRT motion is on AND the source dot is actually
    // moving (transport running → phase advancing). When idle (paused, or motion off) drop to the
    // ~8 fps idle cadence so the editor stops spinning the CPU. The cursor blink in `crt_frame`
    // keeps its own slow repaint alive independently.
    let last_id = response.id.with("flyby-last-phase");
    let last: f32 = ui.memory(|m| m.data.get_temp(last_id).unwrap_or(f32::NAN));
    ui.memory_mut(|m| m.data.insert_temp(last_id, phase));
    let animating = last.is_finite() && (phase - last).abs() > 1e-4;
    if suite_core::ui::crt_motion_on(ui.ctx()) && animating {
        ui.ctx().request_repaint();
    } else {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(125));
    }
}

impl ClapPlugin for Flyby {
    const CLAP_ID: &'static str = "com.qeynos.flyby";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Doppler spatializer — fly a mono source around an editable path with distance, air \
         absorption, equal-power pan, micro-ITD, and rate-clamped fractional-delay doppler",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::PitchShifter,
        ClapFeature::Custom("spatial"),
    ];
}

impl Vst3Plugin for Flyby {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosFLYBYspat1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Spatial];
}

nih_export_clap!(Flyby);
nih_export_vst3!(Flyby);

#[cfg(test)]
mod render_tests {
    use crate::dsp::FlybyCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(
            crate::MANUAL_DOC,
            &crate::FlybyParams::default(),
        );
    }

    /// Render each factory preset over pink noise and a full-band chirp, write the WAVs (L
    /// channel) into renders/FLYBY/, and assert the universal properties on each channel.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.5, (sr * 4.0) as usize, 4242);
        let chirp = testsig::log_chirp(40.0, 12_000.0, 0.5, (sr * 4.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");

            for (tag, input) in [("pink", &pink), ("chirp", &chirp)] {
                let mut core = FlybyCore::new(sr);
                let (l, r) = core.process_stereo(input, &s);
                assert_universal(&l);
                assert_universal(&r);
                let path = render_path("FLYBY", &format!("{fname}_{tag}"));
                write_wav(&path, &l, sr as u32).expect("write render");
            }
        }
    }
}
