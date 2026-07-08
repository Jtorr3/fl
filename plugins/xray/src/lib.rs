//! X-RAY — the shared cross-plugin spectrum analyzer (Qeynos suite, Phase 3).
//!
//! X-RAY is a tier-2 **bus consumer**: every Qeynos audio plugin taps its own output into
//! a 32-band spectrum and publishes it to the shared bus ([`suite_core::spectrum`] /
//! [`suite_core::bus`]); X-RAY reads **every live slot** with `Bus::snapshot_live` and
//! overlays them as colored curves in one window, so you can see the whole session's
//! spectral balance at once without a meter on every track.
//!
//! Audio is a **bit-exact passthrough** (X-RAY is an inline probe — it can optionally
//! publish its *own* input spectrum too, kind [`PluginKind::Xray`]). The only params are
//! `Publish` (on/off for its own spectrum), `Freeze` (hold the display) and `Out` (a trim,
//! bit-exact at 0 dB).
//!
//! GUI: a log-frequency / dB analyzer panel drawing one polyline per live instance, plus a
//! legend (label · bus id · peak/RMS) where hovering a row highlights that instance
//! (others dim) and clicking solo-dims it.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::{Arc, Mutex};

use suite_core::bus::{PluginKind, SlotSnapshot};
use suite_core::spectrum::{band_center_hz, SpectrumPublisher, F_HIGH, F_LOW, NUM_BANDS};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/XRAY.md");

/// Linear-gain out trim from a dB value. `out_gain(0.0) == 1.0` exactly, so an untrimmed
/// passthrough is bit-exact.
#[inline]
pub fn out_gain(out_db: f32) -> f32 {
    if out_db == 0.0 {
        1.0
    } else {
        10.0_f32.powf(out_db / 20.0)
    }
}

// ---------------------------------------------------------------------------
// View state (persists across GUI frames: freeze snapshot + click-solo target)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct XrayView {
    /// Held snapshot while Freeze is on (captured once on the freezing frame).
    frozen: Option<Vec<SlotSnapshot>>,
    /// Click-solo target (instance id); when set, other instances draw dim.
    soloed: Option<u64>,
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Params)]
pub struct XrayParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// Publish X-RAY's own input spectrum to the bus (so it appears as a source too).
    #[id = "publish"]
    pub publish: BoolParam,
    /// Freeze the display (stop reading new bus snapshots).
    #[id = "freeze"]
    pub freeze: BoolParam,
    /// Output trim. Bit-exact passthrough at 0 dB.
    #[id = "out"]
    pub out: FloatParam,
}

impl Default for XrayParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(820, 560),
            publish: BoolParam::new("Publish", true),
            freeze: BoolParam::new("Freeze", false),
            out: FloatParam::new("Out", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct Xray {
    params: Arc<XrayParams>,
    /// Publishes X-RAY's own input spectrum (kind Xray) when `publish` is on.
    spectrum: SpectrumPublisher,
    view: Arc<Mutex<XrayView>>,
}

impl Default for Xray {
    fn default() -> Self {
        Self {
            params: Arc::new(XrayParams::default()),
            spectrum: SpectrumPublisher::new(),
            view: Arc::new(Mutex::new(XrayView::default())),
        }
    }
}

impl Plugin for Xray {
    const NAME: &'static str = "Qeynos X-RAY";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
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
        let view = self.view.clone();
        let egui_state = self.params.editor_state.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-xray-window", Vec2::new(820.0, 560.0))
                    .min_size(Vec2::new(620.0, 420.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        editor_ui(ui, &params, setter, &view);
                    });
                egui_ctx.request_repaint();
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.spectrum
            .init(buffer_config.sample_rate, PluginKind::Xray, "X-RAY");
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let publish = self.params.publish.value();
        let gain = out_gain(self.params.out.value());

        // Passthrough (bit-exact at 0 dB) + tap the input spectrum before trimming.
        for mut frame in buffer.iter_samples() {
            let n = frame.len().max(1) as f32;
            let mut mono = 0.0;
            for s in frame.iter_mut() {
                mono += *s;
                *s *= gain;
            }
            if publish {
                self.spectrum.feed(mono / n);
            }
        }

        if publish {
            self.spectrum.publish();
        } else {
            // Stop appearing as a bus source when publishing is off.
            self.spectrum.release();
        }

        ProcessStatus::Normal
    }
}

impl Drop for Xray {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/// dB window shown on the analyzer's vertical axis.
const DB_TOP: f32 = 6.0;
const DB_BOT: f32 = -96.0;

/// Distinct per-instance color from a slot index (golden-angle hue walk).
fn slot_color(index: usize) -> egui::Color32 {
    let h = (index as f32 * 0.61803398875).fract(); // golden ratio hue rotation
    hsv(h, 0.72, 1.0)
}

/// Minimal HSV → Color32 (s,v,h in 0..1) so we don't depend on egui's ecolor surface.
fn hsv(h: f32, s: f32, v: f32) -> egui::Color32 {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Fractional x-position (0..1) of a frequency on the log axis.
#[inline]
fn freq_frac(f: f32) -> f32 {
    (f / F_LOW).ln() / (F_HIGH / F_LOW).ln()
}

/// Linear band level → normalized y (0 = bottom of the dB window, 1 = top).
#[inline]
fn level_norm(level: f32) -> f32 {
    let db = if level <= 1.0e-9 {
        DB_BOT
    } else {
        20.0 * level.log10()
    };
    ((db - DB_BOT) / (DB_TOP - DB_BOT)).clamp(0.0, 1.0)
}

fn editor_ui(
    ui: &mut egui::Ui,
    params: &Arc<XrayParams>,
    setter: &ParamSetter,
    view: &Arc<Mutex<XrayView>>,
) {
    use suite_core::ui::{
        console_on, crt_frame, labeled_knob, param_widget, ACCENT, PANEL, PHOSPHOR_DIM, TEXT,
        TEXT_DIM,
    };

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("QEYNOS · X-RAY").color(ACCENT));
        suite_core::ui::manual_button(ui, "xray", "X-RAY", MANUAL_DOC);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("suite spectrum overlay")
                .color(TEXT_DIM)
                .small(),
        );
    });

    // ---- Snapshot selection (freeze holds the last read) -------------------
    let freeze = params.freeze.value();
    let live_now: Vec<SlotSnapshot> = live_snapshot();
    let snaps: Vec<SlotSnapshot> = {
        let mut v = view.lock().unwrap_or_else(|p| p.into_inner());
        if freeze {
            if v.frozen.is_none() {
                v.frozen = Some(live_now.clone());
            }
            v.frozen.clone().unwrap_or_default()
        } else {
            v.frozen = None;
            live_now
        }
    };

    // Focus target: legend hover wins for this frame; else the persisted click-solo.
    let soloed = view.lock().map(|v| v.soloed).unwrap_or(None);
    let mut hovered: Option<u64> = None;

    // ---- Analyzer panel (recessed CRT bay) ---------------------------------
    // The multicolor per-slot curves (golden-angle hue walk) and the legend
    // hover/click solo-dim below are the FUNCTIONAL product and are unchanged;
    // CONSOLE only re-skins the chrome — glass background via crt_frame,
    // gridlines/axis text → PHOSPHOR_DIM. THEME-OFF restores the original look.
    let console = console_on(ui.ctx());
    let panel_h = (ui.available_height() - 150.0).clamp(200.0, 420.0);
    let rect = crt_frame(ui, "xray-crt", panel_h + 16.0, |ui| {
        let (rect, _resp) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), panel_h),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        // Original panel fill only when CONSOLE is off (else the glass shows through).
        if !console {
            painter.rect_filled(rect, 4.0, PANEL);
        }

        // Grid: decade freq lines + dB lines.
        let grid_col = if console {
            PHOSPHOR_DIM
        } else {
            egui::Color32::from_rgb(40, 43, 48)
        };
        let label_col = if console { PHOSPHOR_DIM } else { TEXT_DIM };
        for &f in &[100.0f32, 1_000.0, 10_000.0] {
            let x = rect.left() + freq_frac(f) * rect.width();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, grid_col),
            );
            let txt = if f >= 1_000.0 {
                format!("{:.0}k", f / 1_000.0)
            } else {
                format!("{f:.0}")
            };
            painter.text(
                egui::pos2(x + 2.0, rect.bottom() - 12.0),
                egui::Align2::LEFT_TOP,
                txt,
                egui::FontId::proportional(10.0),
                label_col,
            );
        }
        for &db in &[0.0f32, -24.0, -48.0, -72.0] {
            let yn = (db - DB_BOT) / (DB_TOP - DB_BOT);
            let y = rect.bottom() - yn * rect.height();
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_col),
            );
            painter.text(
                egui::pos2(rect.left() + 2.0, y - 11.0),
                egui::Align2::LEFT_TOP,
                format!("{db:.0}"),
                egui::FontId::proportional(10.0),
                label_col,
            );
        }
        rect
    });

    // Curves are drawn AFTER the legend so this frame's hover/solo can dim the right ones.

    // ---- Legend (built first so hover/solo can dim the right curves) -------
    // Reserve the legend into a scroll area below the panel.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("SOURCES").color(TEXT_DIM).small());
        ui.label(
            egui::RichText::new(format!("· {} live", snaps.len()))
                .color(TEXT_DIM)
                .small(),
        );
        if freeze {
            ui.label(egui::RichText::new("· FROZEN").color(ACCENT).small());
        }
    });

    let mut clicked_solo: Option<u64> = None;
    egui::ScrollArea::vertical()
        .max_height(96.0)
        .id_salt("xray-legend")
        .show(ui, |ui| {
            if snaps.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No live sources. Add Qeynos plugins (un-bridged) to any track — \
                         each publishes its spectrum here.",
                    )
                    .color(TEXT_DIM)
                    .small(),
                );
            }
            for snap in &snaps {
                let col = slot_color(snap.index);
                let peak_db = lin_db(snap.peak);
                let rms_db = lin_db(snap.rms);
                let name = if snap.label.is_empty() {
                    format!("#{}", snap.instance_id & 0xFFFF)
                } else {
                    snap.label.clone()
                };
                ui.horizontal(|ui| {
                    let (sw, _r) =
                        ui.allocate_exact_size(Vec2::new(12.0, 12.0), egui::Sense::hover());
                    ui.painter_at(sw).rect_filled(sw, 2.0, col);
                    let is_solo = soloed == Some(snap.instance_id);
                    let txt = format!(
                        "{name}  #{:<4}  pk {peak_db:>5.0}  rms {rms_db:>5.0} dB",
                        snap.instance_id & 0xFFFF
                    );
                    let resp = ui.selectable_label(
                        is_solo,
                        egui::RichText::new(txt).color(if is_solo { ACCENT } else { TEXT }).monospace(),
                    );
                    if resp.hovered() {
                        hovered = Some(snap.instance_id);
                    }
                    if resp.clicked() {
                        clicked_solo = Some(snap.instance_id);
                    }
                });
            }
        });

    // Apply a click: toggle solo target.
    if let Some(id) = clicked_solo {
        if let Ok(mut v) = view.lock() {
            v.soloed = if v.soloed == Some(id) { None } else { Some(id) };
        }
    }

    // Now draw the curves with the resolved focus (hover this frame, else solo).
    // Clipped to the panel rect (inside the CRT glass); colors are untouched.
    let focus = hovered.or(soloed);
    let painter = ui.painter_at(rect);
    for snap in &snaps {
        let base = slot_color(snap.index);
        let dim = focus.is_some() && focus != Some(snap.instance_id);
        let col = if dim { base.gamma_multiply(0.16) } else { base };
        let width = if dim { 1.0 } else { 1.8 };
        let mut pts: Vec<egui::Pos2> = Vec::with_capacity(NUM_BANDS);
        for i in 0..NUM_BANDS {
            let t = freq_frac(band_center_hz(i));
            let x = rect.left() + t * rect.width();
            let y = rect.bottom() - level_norm(snap.spectrum[i]) * rect.height();
            pts.push(egui::pos2(x, y));
        }
        painter.add(egui::Shape::line(pts, egui::Stroke::new(width, col)));
    }

    // ---- Controls ----------------------------------------------------------
    ui.separator();
    ui.horizontal(|ui| {
        param_widget(ui, "PUBLISH", &params.publish, setter);
        ui.add_space(12.0);
        param_widget(ui, "FREEZE", &params.freeze, setter);
        ui.add_space(12.0);
        labeled_knob(ui, "OUT", &params.out, setter);
        ui.add_space(12.0);
        if soloed.is_some() {
            if ui.button("clear solo").clicked() {
                if let Ok(mut v) = view.lock() {
                    v.soloed = None;
                }
            }
        }
    });
    ui.label(
        egui::RichText::new(
            "Hover a source to highlight it · click to solo-dim · Freeze holds the view",
        )
        .color(TEXT_DIM)
        .small(),
    );
}

/// Linear amplitude → dB for the legend (−inf floored to the display bottom).
#[inline]
fn lin_db(x: f32) -> f32 {
    if x <= 1.0e-9 {
        DB_BOT
    } else {
        20.0 * x.log10()
    }
}

/// Read every live slot from the process-default bus (empty if the bus can't be mapped).
fn live_snapshot() -> Vec<SlotSnapshot> {
    suite_core::bus::bus()
        .map(|b| b.snapshot_live())
        .unwrap_or_default()
}

impl ClapPlugin for Xray {
    const CLAP_ID: &'static str = "com.qeynos.xray";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Qeynos X-RAY — shared cross-plugin spectrum analyzer (reads the tier-2 bus)");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Analyzer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Xray {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosXray000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer, Vst3SubCategory::Tools];
}

nih_export_clap!(Xray);
nih_export_vst3!(Xray);

#[cfg(test)]
mod tests;
