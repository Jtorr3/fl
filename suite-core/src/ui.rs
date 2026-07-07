//! Shared egui theme for the suite: minimal dark, near-black background, one accent.
//! Gated behind the `gui` feature (depends on nih_plug_egui).

use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui;
use nih_plug_egui::widgets::ParamSlider;

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

/// A labeled parameter slider using the suite theme. `label` sits above the control.
pub fn labeled_slider<P: Param>(
    ui: &mut egui::Ui,
    label: &str,
    param: &P,
    setter: &ParamSetter,
) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).color(TEXT_DIM).small());
        ui.add(ParamSlider::for_param(param, setter));
    });
}

/// A compact "knob-style" labeled control. nih_plug_egui ships a slider widget rather
/// than a rotary; this keeps a consistent labeled call site so plugins can swap in a
/// rotary later without touching call sites.
pub fn labeled_knob<P: Param>(
    ui: &mut egui::Ui,
    label: &str,
    param: &P,
    setter: &ParamSetter,
) {
    ui.vertical_centered(|ui| {
        ui.add(ParamSlider::for_param(param, setter).without_value());
        ui.label(egui::RichText::new(label).color(TEXT_DIM).small());
    });
}
