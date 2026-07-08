//! Shared egui UI for the suite: minimal-dark theme, the standard **rotary knob**
//! param control, real click-to-type value entry, and a uniform window-scaling
//! wrapper. Gated behind the `gui` feature (depends on nih_plug_egui).
//!
//! Interaction model (see `docs/UI.md` — this is the suite-wide "controls" contract;
//! PEDAL-UI later only re-skins these widgets, it never changes the interaction):
//! * **Drag** a knob vertically — up = increase, down = decrease.
//! * **Ctrl (fine) drag** — ~10× finer resolution for precise values.
//! * **Double-click** — reset the parameter to its default.
//! * **Scroll wheel** — step the value (one detent for stepped params).
//! * **Click the value text** — opens a real text field; Enter commits (parsed through
//!   the param's `string_to_value`), Esc cancels, clicking away commits.
//! * **Uniform scaling** — the whole editor scales as one unit (egui zoom); snap points
//!   75/100/125/150 % in the corner size menu; the chosen size persists in plugin state.
//!
//! Every call site funnels through [`labeled_slider`] / [`labeled_knob`] / [`param_widget`],
//! so the widget swap is suite-wide with no per-call-site churn. Bool params render a
//! toggle; stepped params (Int/Enum) render a detented knob.

use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{self, Sense, Vec2};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::EguiState;

/// Near-black window background.
pub const BG: egui::Color32 = egui::Color32::from_rgb(14, 15, 17);
/// Slightly raised panel / widget fill.
pub const PANEL: egui::Color32 = egui::Color32::from_rgb(24, 26, 30);
/// Primary text.
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(220, 223, 228);
/// Muted / secondary text.
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(140, 145, 152);
/// The single accent color (amber). Used for active controls and meters.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(232, 168, 82);

/// Apply the suite's minimal-dark visuals to an egui context. Call once per frame
/// (cheap) from the editor's build/update closure.
pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = egui::Color32::from_rgb(10, 11, 13);
    visuals.faint_bg_color = PANEL;

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.inactive.bg_fill = PANEL;
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(34, 37, 42);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.5);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    ctx.set_visuals(visuals);
}

// ===========================================================================
// Uniform window scaling
// ===========================================================================

/// Discrete zoom stops surfaced in the corner size menu.
pub const SCALE_SNAPS: [f32; 4] = [0.75, 1.0, 1.25, 1.5];
/// Hard clamp on the derived zoom factor (keeps the editor usable at extremes).
pub const SCALE_MIN: f32 = 0.5;
pub const SCALE_MAX: f32 = 3.0;
/// A derived scale within this fraction of a snap stop is pulled onto it, so a
/// free window-drag "clicks into" 75/100/125/150 %.
const SNAP_BAND: f32 = 0.06;

/// Pull a raw scale onto the nearest snap stop when within [`SNAP_BAND`], else leave
/// it continuous. Always clamped to [[`SCALE_MIN`], [`SCALE_MAX`]].
pub fn snap_scale(raw: f32) -> f32 {
    let raw = raw.clamp(SCALE_MIN, SCALE_MAX);
    let mut best = raw;
    let mut best_d = SNAP_BAND;
    for &s in &SCALE_SNAPS {
        let d = (raw - s).abs();
        if d <= best_d {
            best_d = d;
            best = s;
        }
    }
    best
}

/// Pure mapping used by both the runtime and the unit tests: current window logical
/// width and the editor's base logical width map to a zoom (pixels-per-point
/// multiplier). Width is the master axis so content aspect never distorts.
pub fn scale_for_size(window_w: f32, base_w: f32) -> f32 {
    if base_w <= 0.0 {
        return 1.0;
    }
    snap_scale(window_w / base_w)
}

/// The standard editor window for every Qeynos plugin: wraps nih_plug_egui's
/// [`ResizableWindow`] and adds uniform, aspect-safe zoom scaling plus a corner size
/// menu. Retrofit is a 1:1 swap for `ResizableWindow::new(id)` — pass the editor's
/// base logical size (the `EguiState::from_size` dimensions).
pub struct ScaledWindow {
    id: String,
    base: Vec2,
    min_scale_size: Option<Vec2>,
}

impl ScaledWindow {
    /// `id` must match the plugin's window id (e.g. `"qeynos-grit-window"`); `base` is
    /// the design/logical size the content is laid out for.
    pub fn new(id: impl Into<String>, base: Vec2) -> Self {
        Self {
            id: id.into(),
            base,
            min_scale_size: None,
        }
    }

    /// Accepted for call-site parity with `ResizableWindow::min_size`; the effective
    /// minimum is the base size (content never clips at 100 %), so this is advisory.
    pub fn min_size(mut self, m: impl Into<Vec2>) -> Self {
        self.min_scale_size = Some(m.into());
        self
    }

    pub fn show<R>(
        self,
        ctx: &egui::Context,
        egui_state: &EguiState,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let (lw, _lh) = egui_state.size();
        let override_id = egui::Id::new((self.id.as_str(), "scale-override"));
        let last_size_id = egui::Id::new((self.id.as_str(), "last-size"));

        // A window resize (corner drag changes the persisted logical size) drops any
        // menu-selected snap lock, handing control back to continuous scaling.
        let last: Option<(u32, u32)> = ctx.memory(|m| m.data.get_temp(last_size_id));
        let cur = egui_state.size();
        if last.map_or(false, |l| l != cur) {
            ctx.memory_mut(|m| m.data.remove::<f32>(override_id));
        }
        ctx.memory_mut(|m| m.data.insert_temp(last_size_id, cur));

        let scale = match ctx.memory(|m| m.data.get_temp::<f32>(override_id)) {
            Some(s) => s.clamp(SCALE_MIN, SCALE_MAX),
            None => scale_for_size(lw as f32, self.base.x),
        };
        // Uniform zoom: everything (layout, text, knobs) scales as one unit. Takes
        // effect on the next pass, so at rest this converges in one frame.
        ctx.set_zoom_factor(scale);

        // Content is authored at `base` points; keep the window from shrinking below
        // that so nothing clips at 100 %.
        let id_for_menu = self.id.clone();
        ResizableWindow::new(self.id)
            .min_size(self.base)
            .show(ctx, egui_state, |ui| {
                let r = add_contents(ui);
                size_menu(ui, &id_for_menu, scale, &override_id);
                r
            })
            .inner
    }
}

/// A small "NN%" button anchored to the window's top-right corner. Opens a popup with
/// the snap stops; picking one locks the zoom (session state) until the window is
/// dragged. The effective size persists via `EguiState`, so scale survives reloads.
fn size_menu(ui: &egui::Ui, id: &str, current: f32, override_id: &egui::Id) {
    let ctx = ui.ctx().clone();
    egui::Area::new(egui::Id::new((id, "size-menu")))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-6.0, 6.0))
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            let label = format!("{}%", (current * 100.0).round() as i32);
            ui.menu_button(egui::RichText::new(label).small().color(TEXT_DIM), |ui| {
                for &s in &SCALE_SNAPS {
                    let txt = format!("{}%", (s * 100.0).round() as i32);
                    let mark = if (s - current).abs() < 0.001 { "• " } else { "  " };
                    if ui.button(format!("{mark}{txt}")).clicked() {
                        ui.ctx()
                            .memory_mut(|m| m.data.insert_temp(*override_id, s));
                        ui.close_menu();
                    }
                }
            });
        });
}

// ===========================================================================
// Param controls
// ===========================================================================

/// Coarse drag sensitivity: normalized units per pixel of vertical drag
/// (full 0..1 sweep over ~250 px).
const KNOB_SENS: f32 = 1.0 / 250.0;
/// Fine (Ctrl) drag sensitivity — ~10× finer.
const KNOB_FINE_SENS: f32 = KNOB_SENS / 10.0;

/// The suite's standard parameter control. Dispatches on the param's shape:
/// bool → toggle; everything else → rotary knob (detented for stepped params).
/// This is what [`labeled_slider`] and [`labeled_knob`] delegate to, so every existing
/// call site becomes a knob with no edit.
pub fn param_widget<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    if param.step_count() == Some(1) {
        toggle_control(ui, label, param, setter);
    } else {
        knob_control(ui, label, param, setter);
    }
}

/// A labeled parameter control. Historically a slider; now the suite rotary knob.
/// Kept as the canonical call site so the widget can evolve without touching plugins.
pub fn labeled_slider<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    param_widget(ui, label, param, setter);
}

/// Compact labeled knob (identical to [`labeled_slider`]; retained for call-site parity).
pub fn labeled_knob<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    param_widget(ui, label, param, setter);
}

/// Diameter of the knob face in logical points.
const KNOB_DIAMETER: f32 = 46.0;

fn knob_control<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    ui.vertical(|ui| {
        ui.set_width(KNOB_DIAMETER + 18.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(label).color(TEXT_DIM).small());
            knob_face(ui, param, setter);
            value_text(ui, label, param, setter);
        });
    });
}

/// The circular knob: allocate, handle input (drag/fine/reset/scroll with correct
/// begin/end_set_parameter discipline), then paint arc + ticks + needle.
fn knob_face<P: Param>(ui: &mut egui::Ui, param: &P, setter: &ParamSetter) {
    let desired = Vec2::splat(KNOB_DIAMETER);
    let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    let id = response.id;
    let start_id = id.with("start-norm");
    let accum_id = id.with("accum");

    // --- Input ---
    if response.drag_started() {
        setter.begin_set_parameter(param);
        ui.memory_mut(|m| {
            m.data
                .insert_temp(start_id, param.modulated_normalized_value());
            m.data.insert_temp(accum_id, 0.0f32);
        });
    }
    if response.dragged() {
        // egui y grows downward, so dragging up is a negative delta → increase.
        let dy = -response.drag_delta().y;
        let fine = ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
        let sens = if fine { KNOB_FINE_SENS } else { KNOB_SENS };
        let mut accum: f32 = ui.memory(|m| m.data.get_temp(accum_id).unwrap_or(0.0));
        accum += dy * sens;
        ui.memory_mut(|m| m.data.insert_temp(accum_id, accum));
        let start: f32 = ui.memory(|m| {
            m.data
                .get_temp(start_id)
                .unwrap_or_else(|| param.modulated_normalized_value())
        });
        setter.set_parameter_normalized(param, (start + accum).clamp(0.0, 1.0));
        response.mark_changed();
    }
    if response.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if response.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, param.default_plain_value());
        setter.end_set_parameter(param);
        response.mark_changed();
    }
    // Scroll wheel steps: one detent for stepped params, a small fixed step otherwise.
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let step = param
                .step_count()
                .map(|n| 1.0 / n.max(1) as f32)
                .unwrap_or(0.02);
            let cur = param.modulated_normalized_value();
            let nv = (cur + scroll.signum() * step).clamp(0.0, 1.0);
            setter.begin_set_parameter(param);
            setter.set_parameter_normalized(param, nv);
            setter.end_set_parameter(param);
        }
    }

    // --- Paint ---
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let center = rect.center();
        let radius = KNOB_DIAMETER * 0.5 - 3.0;
        let t = param.modulated_normalized_value().clamp(0.0, 1.0);

        // 270° sweep with the gap at the bottom (min → bottom-left, max → bottom-right).
        let a0 = 135.0_f32.to_radians();
        let a1 = 405.0_f32.to_radians();
        let ang = a0 + t * (a1 - a0);
        let pt = |a: f32, r: f32| center + Vec2::new(a.cos(), a.sin()) * r;

        // Body.
        painter.circle_filled(center, radius, PANEL);
        painter.circle_stroke(center, radius, egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 43, 48)));

        // Background track arc.
        painter.add(egui::Shape::line(
            arc_points(center, radius, a0, a1, 40),
            egui::Stroke::new(2.0, egui::Color32::from_rgb(48, 51, 57)),
        ));
        // Filled value arc.
        painter.add(egui::Shape::line(
            arc_points(center, radius, a0, ang, 40),
            egui::Stroke::new(2.5, ACCENT),
        ));
        // Ticks at 0/25/50/75/100 %.
        for k in 0..=4 {
            let a = a0 + (k as f32 / 4.0) * (a1 - a0);
            painter.line_segment(
                [pt(a, radius - 2.0), pt(a, radius + 2.0)],
                egui::Stroke::new(1.0, TEXT_DIM),
            );
        }
        // Needle.
        painter.line_segment(
            [pt(ang, radius * 0.28), pt(ang, radius * 0.92)],
            egui::Stroke::new(2.0, ACCENT),
        );
        painter.circle_filled(center, radius * 0.16, egui::Color32::from_rgb(30, 32, 37));
    }
}

fn arc_points(center: egui::Pos2, radius: f32, a0: f32, a1: f32, segments: usize) -> Vec<egui::Pos2> {
    let segments = segments.max(2);
    (0..=segments)
        .map(|i| {
            let a = a0 + (i as f32 / segments as f32) * (a1 - a0);
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// The live value below the knob, and the real click-to-type editor.
/// Plain label when idle (no phantom caret); clicking swaps in a focused `TextEdit`.
fn value_text<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    // A stable per-widget id derived from the enclosing ui's id + this knob's label.
    let editing_id = ui.make_persistent_id(("knob-editing", label));
    let buf_id = editing_id.with("buf");
    let focus_id = editing_id.with("te");

    let editing: bool = ui.memory(|m| m.data.get_temp(editing_id).unwrap_or(false));

    if editing {
        let mut buf: String = ui
            .memory(|m| m.data.get_temp(buf_id))
            .unwrap_or_else(|| param.to_string());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .id(focus_id)
                .desired_width(KNOB_DIAMETER + 12.0)
                .font(egui::TextStyle::Monospace),
        );
        // Grab focus on the first editing frame.
        let focused_once_id = editing_id.with("focused");
        let focused_once: bool = ui.memory(|m| m.data.get_temp(focused_once_id).unwrap_or(false));
        if !focused_once {
            resp.request_focus();
            ui.memory_mut(|m| m.data.insert_temp(focused_once_id, true));
        }

        let commit = |s: &str| {
            if let Some(nv) = param.string_to_normalized_value(s) {
                setter.begin_set_parameter(param);
                setter.set_parameter_normalized(param, nv);
                setter.end_set_parameter(param);
            }
        };

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            end_editing(ui, editing_id, buf_id, focused_once_id);
        } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            commit(&buf);
            end_editing(ui, editing_id, buf_id, focused_once_id);
        } else if resp.lost_focus() {
            // Clicking away commits.
            commit(&buf);
            end_editing(ui, editing_id, buf_id, focused_once_id);
        } else {
            ui.memory_mut(|m| m.data.insert_temp(buf_id, buf));
        }
    } else {
        let resp = ui.add(
            egui::Label::new(egui::RichText::new(param.to_string()).color(TEXT).small())
                .sense(Sense::click()),
        );
        if resp.clicked() {
            ui.memory_mut(|m| {
                m.data.insert_temp(editing_id, true);
                m.data.insert_temp(buf_id, param.to_string());
                m.data.insert_temp(editing_id.with("focused"), false);
            });
        }
    }
}

fn end_editing(ui: &egui::Ui, editing_id: egui::Id, buf_id: egui::Id, focused_id: egui::Id) {
    ui.memory_mut(|m| {
        m.data.insert_temp(editing_id, false);
        m.data.remove::<String>(buf_id);
        m.data.insert_temp(focused_id, false);
    });
}

/// Bool params render as a labeled toggle pill rather than a knob (VOXFIT and any other
/// `BoolParam` funnel through the same helper).
fn toggle_control<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    ui.vertical(|ui| {
        ui.set_width(KNOB_DIAMETER + 18.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(label).color(TEXT_DIM).small());
            let on = param.modulated_normalized_value() > 0.5;
            let desired = Vec2::new(38.0, 20.0);
            let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click());
            if response.clicked() {
                setter.begin_set_parameter(param);
                setter.set_parameter_normalized(param, if on { 0.0 } else { 1.0 });
                setter.end_set_parameter(param);
                response.mark_changed();
            }
            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let radius = rect.height() * 0.5;
                let track = if on { ACCENT.linear_multiply(0.6) } else { PANEL };
                painter.rect_filled(rect, radius, track);
                painter.rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 51, 57)),
                    egui::StrokeKind::Middle,
                );
                let knob_x = if on { rect.right() - radius } else { rect.left() + radius };
                painter.circle_filled(
                    egui::pos2(knob_x, rect.center().y),
                    radius - 2.0,
                    if on { ACCENT } else { TEXT_DIM },
                );
            }
            ui.label(egui::RichText::new(param.to_string()).color(TEXT).small());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::prelude::{BoolParam, FloatParam, FloatRange, IntParam, IntRange, Param};

    // --- Click-to-type commit path: string_to_value round-trips on representative
    //     params. This is exactly the path value_text() drives on Enter/blur, so it
    //     proves a typed value lands on the same normalized value the widget shows. ---

    fn roundtrip_normalized<P: Param>(param: &P, normalized: f32) {
        // Emulate the widget: snap the requested normalized value, read its display
        // string, then parse it back the way `value_text` does on commit.
        let a = param.preview_normalized(param.preview_plain(normalized));
        let display = param.normalized_value_to_string(a, true);
        let parsed = param
            .string_to_normalized_value(&display)
            .expect("representative param must parse its own display string");
        let b = param.preview_normalized(param.preview_plain(parsed));
        // Round-trip is stable at the (snapped) plain-value level; display rounding ok.
        assert!(
            (a - b).abs() < 1e-3,
            "round-trip drift: '{display}' -> {parsed} (norm {a} vs {b})"
        );
    }

    #[test]
    fn float_gain_param_typed_value_roundtrips() {
        let p = FloatParam::new(
            "Gain",
            0.0,
            FloatRange::Linear {
                min: -60.0,
                max: 24.0,
            },
        )
        .with_unit(" dB");
        for &n in &[0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            roundtrip_normalized(&p, n);
        }
    }

    #[test]
    fn int_param_typed_value_roundtrips() {
        let p = IntParam::new("Voices", 4, IntRange::Linear { min: 1, max: 16 });
        for &n in &[0.0_f32, 0.33, 0.5, 0.8, 1.0] {
            roundtrip_normalized(&p, n);
        }
    }

    #[test]
    fn bool_param_dispatches_to_toggle_and_roundtrips() {
        let p = BoolParam::new("Freeze", false);
        // param_widget routes step_count()==Some(1) to the toggle; confirm the shape.
        assert_eq!(p.step_count(), Some(1));
        roundtrip_normalized(&p, 0.0);
        roundtrip_normalized(&p, 1.0);
    }

    #[test]
    fn snap_pulls_onto_stops_within_band() {
        // Near a stop → snaps exactly.
        assert_eq!(snap_scale(1.02), 1.0);
        assert_eq!(snap_scale(0.73), 0.75);
        assert_eq!(snap_scale(1.27), 1.25);
        assert_eq!(snap_scale(1.48), 1.5);
    }

    #[test]
    fn snap_leaves_continuous_between_stops() {
        // Comfortably between 1.0 and 1.25 (outside either band) → untouched.
        let v = snap_scale(1.13);
        assert!((v - 1.13).abs() < 1e-6, "got {v}");
    }

    #[test]
    fn snap_clamps_to_range() {
        assert_eq!(snap_scale(9.0), SCALE_MAX);
        assert_eq!(snap_scale(0.01), SCALE_MIN);
    }

    #[test]
    fn scale_for_size_maps_window_px_to_ppp_at_each_snap() {
        let base = 560.0;
        // Window sized to exactly base*snap must yield that snap's zoom.
        for &s in &SCALE_SNAPS {
            let got = scale_for_size(base * s, base);
            assert!((got - s).abs() < 1e-6, "snap {s}: got {got}");
        }
    }

    #[test]
    fn scale_for_size_at_base_is_unity() {
        assert_eq!(scale_for_size(600.0, 600.0), 1.0);
    }

    #[test]
    fn scale_for_size_handles_degenerate_base() {
        assert_eq!(scale_for_size(600.0, 0.0), 1.0);
    }
}
