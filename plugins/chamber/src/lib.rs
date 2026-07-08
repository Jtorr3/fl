//! CHAMBER — image-source space simulator (Qeynos suite, Phase 2b; Eigen clone).
//!
//! A **shoebox** room with a draggable **source** and **listener** on a top-down floor-plan pad.
//! Early reflections are synthesised as an order-≤3 **image-source cluster** (delay `r/c`, gain
//! `1/r × reflectⁿ`, per-bounce HF damp, azimuth pan); the diffuse tail is a **Sabine-tuned FDN**
//! ([`suite_core::fdn::Fdn8`], reused from MURMUR) crossfaded in after the early-reflection
//! window. The direct path is image order 0 — it *is* the dry — so `mix = 0` passes the input
//! through exactly. See [`dsp`] for the DSP core, shared verbatim with the offline/done-bar tests.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Sense, Vec2},
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

use dsp::{ChamberCore, Material, Settings, AUTO_ORDER, MAX_ORDER};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing enums
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum MaterialParam {
    #[id = "concrete"]
    #[name = "Concrete"]
    Concrete,
    #[id = "wood"]
    #[name = "Wood"]
    Wood,
    #[id = "curtain"]
    #[name = "Curtain"]
    Curtain,
    #[id = "glass"]
    #[name = "Glass"]
    Glass,
}

impl MaterialParam {
    fn to_dsp(self) -> Material {
        match self {
            MaterialParam::Concrete => Material::Concrete,
            MaterialParam::Wood => Material::Wood,
            MaterialParam::Curtain => Material::Curtain,
            MaterialParam::Glass => Material::Glass,
        }
    }
    fn from_index(i: usize) -> MaterialParam {
        match i {
            0 => MaterialParam::Concrete,
            1 => MaterialParam::Wood,
            2 => MaterialParam::Curtain,
            _ => MaterialParam::Glass,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderParam {
    #[id = "auto"]
    #[name = "Auto"]
    Auto,
    #[id = "three"]
    #[name = "3"]
    Three,
    #[id = "two"]
    #[name = "2"]
    Two,
    #[id = "one"]
    #[name = "1"]
    One,
}

impl OrderParam {
    fn to_order(self) -> usize {
        match self {
            OrderParam::Auto => AUTO_ORDER,
            OrderParam::Three => 3,
            OrderParam::Two => 2,
            OrderParam::One => 1,
        }
    }
    fn from_index(i: usize) -> OrderParam {
        match i {
            0 => OrderParam::Auto,
            1 => OrderParam::Three,
            2 => OrderParam::Two,
            _ => OrderParam::One,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Chamber {
    params: Arc<ChamberParams>,
    core: ChamberCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct ChamberParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "w"]
    pub w: FloatParam,
    #[id = "d"]
    pub d: FloatParam,
    #[id = "h"]
    pub h: FloatParam,

    // Source / listener positions as fractions of the room dimensions (0..1) — room-independent.
    #[id = "srcx"]
    pub src_x: FloatParam,
    #[id = "srcy"]
    pub src_y: FloatParam,
    #[id = "srcz"]
    pub src_z: FloatParam,
    #[id = "lisx"]
    pub lis_x: FloatParam,
    #[id = "lisy"]
    pub lis_y: FloatParam,
    #[id = "lisz"]
    pub lis_z: FloatParam,

    #[id = "matwall"]
    pub mat_walls: EnumParam<MaterialParam>,
    #[id = "matfloor"]
    pub mat_floor: EnumParam<MaterialParam>,
    #[id = "matceil"]
    pub mat_ceiling: EnumParam<MaterialParam>,

    #[id = "order"]
    pub order: EnumParam<OrderParam>,
    #[id = "balance"]
    pub balance: FloatParam,
    #[id = "distance"]
    pub distance: FloatParam,
    #[id = "predelay"]
    pub predelay: FloatParam,
    #[id = "rt60"]
    pub rt60: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,
}

impl Default for ChamberParams {
    fn default() -> Self {
        let d = Settings::default();
        let m = |v: MaterialParam| v; // clarity
        Self {
            editor_state: EguiState::from_size(600, 720),
            w: FloatParam::new(
                "Width",
                d.w,
                FloatRange::Skewed { min: 2.0, max: 40.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" m")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            d: FloatParam::new(
                "Depth",
                d.d,
                FloatRange::Skewed { min: 2.0, max: 40.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" m")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            h: FloatParam::new(
                "Height",
                d.h,
                FloatRange::Skewed { min: 2.0, max: 20.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" m")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            src_x: pos_param("Source X", d.src_x / d.w),
            src_y: pos_param("Source Y", d.src_y / d.d),
            src_z: pos_param("Source Height", d.src_z / d.h),
            lis_x: pos_param("Listener X", d.lis_x / d.w),
            lis_y: pos_param("Listener Y", d.lis_y / d.d),
            lis_z: pos_param("Listener Height", d.lis_z / d.h),

            mat_walls: EnumParam::new("Walls", m(MaterialParam::Wood)),
            mat_floor: EnumParam::new("Floor", m(MaterialParam::Wood)),
            mat_ceiling: EnumParam::new("Ceiling", m(MaterialParam::Concrete)),

            order: EnumParam::new("ER Order", OrderParam::Auto),
            balance: FloatParam::new("ER/Late", d.er_late, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            distance: FloatParam::new(
                "Distance",
                d.distance,
                FloatRange::Linear { min: 0.5, max: 3.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            predelay: FloatParam::new(
                "Pre-Delay",
                d.predelay,
                FloatRange::Linear { min: 0.0, max: 0.2 },
            )
            .with_unit(" ms")
            .with_value_to_string(Arc::new(|v| format!("{:.0}", v * 1000.0)))
            .with_string_to_value(Arc::new(|s| leading_number(s).map(|v| v / 1000.0))),
            rt60: FloatParam::new(
                "RT60",
                d.rt60_override,
                FloatRange::Skewed { min: 0.0, max: 12.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" s")
            .with_value_to_string(Arc::new(|v| {
                if v <= 0.0 {
                    "Auto".to_string()
                } else {
                    format!("{v:.2}")
                }
            }))
            .with_string_to_value(Arc::new(|s| {
                let t = s.trim();
                if t.to_ascii_lowercase().starts_with("auto") {
                    Some(0.0)
                } else {
                    leading_number(t)
                }
            })),
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
        }
    }
}

/// Parse the leading signed decimal number out of a possibly unit-suffixed string (e.g.
/// `"8 ms"` → `8.0`, `"-1.5 s"` → `-1.5`). Keeps `text_to_value` supported for all params so
/// clap-validator's all-or-none `param-conversions` check passes.
fn leading_number(s: &str) -> Option<f32> {
    let t = s.trim();
    let num: String = t
        .char_indices()
        .take_while(|&(i, c)| {
            c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+'))
        })
        .map(|(_, c)| c)
        .collect();
    num.parse::<f32>().ok()
}

/// A 0..1 position fraction parameter.
fn pos_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default.clamp(0.0, 1.0), FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl ChamberParams {
    /// Snapshot the live parameters into a DSP [`Settings`] (fractions → metres).
    fn snapshot(&self) -> Settings {
        let w = self.w.value();
        let d = self.d.value();
        let h = self.h.value();
        Settings {
            w,
            d,
            h,
            src_x: self.src_x.value() * w,
            src_y: self.src_y.value() * d,
            src_z: self.src_z.value() * h,
            lis_x: self.lis_x.value() * w,
            lis_y: self.lis_y.value() * d,
            lis_z: self.lis_z.value() * h,
            mat_walls: self.mat_walls.value().to_dsp(),
            mat_floor: self.mat_floor.value().to_dsp(),
            mat_ceiling: self.mat_ceiling.value().to_dsp(),
            er_order: self.order.value().to_order(),
            er_late: self.balance.value(),
            distance: self.distance.value(),
            predelay: self.predelay.value(),
            rt60_override: self.rt60.value(),
            width: self.width.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Chamber {
    fn default() -> Self {
        Self {
            params: Arc::new(ChamberParams::default()),
            core: ChamberCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &ChamberParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_e = |param: &EnumParam<MaterialParam>, v: MaterialParam| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };

    let w = g("w", params.w.value());
    let d = g("d", params.d.value());
    let h = g("h", params.h.value());
    set_f(&params.w, w);
    set_f(&params.d, d);
    set_f(&params.h, h);
    // Positions in the preset are metres → store as fractions.
    set_f(&params.src_x, (g("sx", params.src_x.value() * w) / w).clamp(0.0, 1.0));
    set_f(&params.src_y, (g("sy", params.src_y.value() * d) / d).clamp(0.0, 1.0));
    set_f(&params.src_z, (g("sz", params.src_z.value() * h) / h).clamp(0.0, 1.0));
    set_f(&params.lis_x, (g("lx", params.lis_x.value() * w) / w).clamp(0.0, 1.0));
    set_f(&params.lis_y, (g("ly", params.lis_y.value() * d) / d).clamp(0.0, 1.0));
    set_f(&params.lis_z, (g("lz", params.lis_z.value() * h) / h).clamp(0.0, 1.0));

    set_e(&params.mat_walls, MaterialParam::from_index(g("matw", 1.0) as usize));
    set_e(&params.mat_floor, MaterialParam::from_index(g("matf", 1.0) as usize));
    set_e(&params.mat_ceiling, MaterialParam::from_index(g("matc", 0.0) as usize));

    setter.begin_set_parameter(&params.order);
    setter.set_parameter(&params.order, OrderParam::from_index(g("orderp", 0.0) as usize));
    setter.end_set_parameter(&params.order);

    set_f(&params.balance, g("balance", 0.5));
    set_f(&params.distance, g("distance", 1.0));
    set_f(&params.predelay, g("predelay", 0.0));
    set_f(&params.rt60, g("rt60", 0.0));
    set_f(&params.width, g("width", 1.0));
    set_f(&params.mix, g("mix", 0.35));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Chamber {
    const NAME: &'static str = "Qeynos CHAMBER";
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
            main_output_channels: NonZeroU32::new(2),
            names: PortNames { layout: Some("Mono→Stereo"), ..PortNames::const_default() },
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
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-chamber-window", Vec2::new(600.0, 720.0))
                    .min_size(Vec2::new(500.0, 600.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · CHAMBER").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(
                                "image-source space simulator — drag the source & listener",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("chamber", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        ui.separator();

                        // Floor-plan XY pad.
                        floor_plan(ui, &params, setter);

                        // Live geometry readout (RT60 + direct arrival).
                        let s = params.snapshot();
                        let rt60 = if s.rt60_override > 0.0 {
                            s.rt60_override
                        } else {
                            dsp::sabine_rt60(&s)
                        };
                        let r_direct = dsp::direct_distance(&s);
                        ui.label(
                            egui::RichText::new(format!(
                                "RT60 ≈ {:.2} s   ·   direct {:.1} m ({:.0} ms)   ·   {} images",
                                rt60,
                                r_direct,
                                r_direct / dsp::SPEED * 1000.0,
                                images_at(s.er_order),
                            ))
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(4.0);

                        egui::Grid::new("chamber-room")
                            .num_columns(4)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "WIDTH", &params.w, setter);
                                row(ui, "DEPTH", &params.d, setter);
                                row(ui, "HEIGHT", &params.h, setter);
                                row(ui, "ER ORDER", &params.order, setter);
                                ui.end_row();
                                row(ui, "WALLS", &params.mat_walls, setter);
                                row(ui, "FLOOR", &params.mat_floor, setter);
                                row(ui, "CEILING", &params.mat_ceiling, setter);
                                row(ui, "SRC HT", &params.src_z, setter);
                                ui.end_row();
                                row(ui, "ER/LATE", &params.balance, setter);
                                row(ui, "DISTANCE", &params.distance, setter);
                                row(ui, "PRE-DELAY", &params.predelay, setter);
                                row(ui, "LIS HT", &params.lis_z, setter);
                                ui.end_row();
                                row(ui, "RT60", &params.rt60, setter);
                                row(ui, "WIDTH", &params.width, setter);
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
        self.core = ChamberCore::new(buffer_config.sample_rate);
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
            let l_in = main[0][n];
            let r_in = if num_main > 1 { main[1][n] } else { l_in };
            let (ol, or) = self.core.process_sample(l_in, r_in, &s);
            main[0][n] = ol;
            if num_main > 1 {
                main[1][n] = or;
            }
        }
        ProcessStatus::Normal
    }
}

/// Image count at a given order (3-D L1 ball) — for the GUI readout.
fn images_at(order: usize) -> usize {
    let n = order.clamp(1, MAX_ORDER) as i64;
    (((2 * n + 1) * (2 * n * n + 2 * n + 3)) / 3) as usize
}

// ---------------------------------------------------------------------------
// Floor-plan widget — top-down room with draggable source & listener handles.
// ---------------------------------------------------------------------------

fn frac_to_screen(rect: egui::Rect, fx: f32, fy: f32) -> egui::Pos2 {
    egui::pos2(
        rect.left() + fx.clamp(0.0, 1.0) * rect.width(),
        rect.top() + fy.clamp(0.0, 1.0) * rect.height(),
    )
}
fn screen_to_frac(rect: egui::Rect, p: egui::Pos2) -> (f32, f32) {
    (
        ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    )
}

fn floor_plan(ui: &mut egui::Ui, params: &ChamberParams, setter: &ParamSetter) {
    let size = Vec2::new(ui.available_width().min(420.0), 300.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    let src = (params.src_x.value(), params.src_y.value());
    let lis = (params.lis_x.value(), params.lis_y.value());

    // Drag: grab whichever handle (0 = source, 1 = listener) is nearest on press.
    let drag_id = response.id.with("chamber-drag");
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let ds = frac_to_screen(rect, src.0, src.1).distance(pos);
            let dl = frac_to_screen(rect, lis.0, lis.1).distance(pos);
            let pick = if ds <= dl { 0usize } else { 1usize };
            let nearest = ds.min(dl);
            if nearest < 40.0 {
                ui.memory_mut(|m| m.data.insert_temp(drag_id, pick));
                let (px, py) = if pick == 0 {
                    (&params.src_x, &params.src_y)
                } else {
                    (&params.lis_x, &params.lis_y)
                };
                setter.begin_set_parameter(px);
                setter.begin_set_parameter(py);
            } else {
                ui.memory_mut(|m| m.data.insert_temp(drag_id, usize::MAX));
            }
        }
    }
    if response.dragged() {
        let pick: usize = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or(usize::MAX));
        if pick != usize::MAX {
            if let Some(pos) = response.interact_pointer_pos() {
                let (fx, fy) = screen_to_frac(rect, pos);
                let (px, py) = if pick == 0 {
                    (&params.src_x, &params.src_y)
                } else {
                    (&params.lis_x, &params.lis_y)
                };
                setter.set_parameter(px, fx);
                setter.set_parameter(py, fy);
            }
        }
    }
    if response.drag_stopped() {
        let pick: usize = ui.memory(|m| m.data.get_temp(drag_id).unwrap_or(usize::MAX));
        if pick != usize::MAX {
            let (px, py) = if pick == 0 {
                (&params.src_x, &params.src_y)
            } else {
                (&params.lis_x, &params.lis_y)
            };
            setter.end_set_parameter(px);
            setter.end_set_parameter(py);
        }
        ui.memory_mut(|m| m.data.insert_temp(drag_id, usize::MAX));
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, suite_core::ui::PANEL);
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 64, 72)),
            egui::StrokeKind::Middle,
        );
        // Faint grid.
        for k in 1..4 {
            let fx = k as f32 / 4.0;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + fx * rect.width(), rect.top()),
                    egui::pos2(rect.left() + fx * rect.width(), rect.bottom()),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(34, 37, 42)),
            );
            painter.line_segment(
                [
                    egui::pos2(rect.left(), rect.top() + fx * rect.height()),
                    egui::pos2(rect.right(), rect.top() + fx * rect.height()),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(34, 37, 42)),
            );
        }
        // Line between source and listener (direct path).
        let sp = frac_to_screen(rect, src.0, src.1);
        let lp = frac_to_screen(rect, lis.0, lis.1);
        painter.line_segment(
            [sp, lp],
            egui::Stroke::new(1.0, suite_core::ui::ACCENT.linear_multiply(0.4)),
        );

        // Listener (dim ring + cross).
        painter.circle_stroke(lp, 8.0, egui::Stroke::new(2.0, suite_core::ui::TEXT_DIM));
        painter.line_segment(
            [egui::pos2(lp.x - 5.0, lp.y), egui::pos2(lp.x + 5.0, lp.y)],
            egui::Stroke::new(1.5, suite_core::ui::TEXT_DIM),
        );
        painter.line_segment(
            [egui::pos2(lp.x, lp.y - 5.0), egui::pos2(lp.x, lp.y + 5.0)],
            egui::Stroke::new(1.5, suite_core::ui::TEXT_DIM),
        );
        painter.text(
            egui::pos2(lp.x + 10.0, lp.y - 10.0),
            egui::Align2::LEFT_CENTER,
            "LISTEN",
            egui::FontId::proportional(10.0),
            suite_core::ui::TEXT_DIM,
        );

        // Source (amber dot).
        painter.circle_filled(sp, 7.0, suite_core::ui::ACCENT);
        painter.circle_stroke(sp, 7.0, egui::Stroke::new(1.0, suite_core::ui::BG));
        painter.text(
            egui::pos2(sp.x + 10.0, sp.y - 10.0),
            egui::Align2::LEFT_CENTER,
            "SRC",
            egui::FontId::proportional(10.0),
            suite_core::ui::ACCENT,
        );

        // Axis labels.
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            "◄ WIDTH ►",
            egui::FontId::proportional(9.0),
            egui::Color32::from_rgb(70, 74, 82),
        );
    }
}

impl ClapPlugin for Chamber {
    const CLAP_ID: &'static str = "com.qeynos.chamber";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Image-source space simulator — a shoebox room with draggable source/listener, an \
         order-≤3 early-reflection image cluster and a Sabine-tuned FDN late field",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Reverb,
        ClapFeature::Custom("spatial"),
    ];
}

impl Vst3Plugin for Chamber {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosCHAMBERsp1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_clap!(Chamber);
nih_export_vst3!(Chamber);

#[cfg(test)]
mod render_tests {
    use crate::dsp::ChamberCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    /// Render each factory preset over pink noise and a chirp, write the WAVs (L channel) into
    /// renders/CHAMBER/, and assert the universal properties on each channel.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.5, (sr * 4.0) as usize, 7373);
        let chirp = testsig::log_chirp(40.0, 12_000.0, 0.5, (sr * 4.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6, "need >= 6 presets, got {}", presets.len());
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");
            for (tag, input) in [("pink", &pink), ("chirp", &chirp)] {
                let mut core = ChamberCore::new(sr);
                let (l, r) = core.process_stereo(input, &s);
                assert_universal(&l);
                assert_universal(&r);
                let path = render_path("CHAMBER", &format!("{fname}_{tag}"));
                write_wav(&path, &l, sr as u32).expect("write render");
            }
        }
    }
}
