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

use nih_plug::prelude::{Param, ParamSetter, Params};
use nih_plug_egui::egui::{self, Sense, Vec2};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::EguiState;
use std::collections::BTreeMap;

use crate::presets::{self, Preset};

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

// ===========================================================================
// CONSOLE v2 palette (PEDAL-UI — LOCKED "console inside a pedal")
// ===========================================================================
//
// Amber is the suite identity (SPECS PEDAL-UI). These extra tones drive the
// hardware-pedal enclosure + the recessed amber-CRT telemetry bay. Contrast
// guardrail (#5): PHOSPHOR (#ffb000) on GLASS/near-black is ~13:1 (body text ≥
// 4.5:1); PHOSPHOR_DIM (#b07a10) on the same glass is ~5.4:1 (dim/label ≥ 3:1).

/// CRT phosphor amber (#ffb000-family) — the terminal body text in the CRT bay.
pub const PHOSPHOR: egui::Color32 = egui::Color32::from_rgb(255, 176, 0);
/// Dim phosphor for labels/rules inside the CRT (still ≥ 3:1 on the glass).
pub const PHOSPHOR_DIM: egui::Color32 = egui::Color32::from_rgb(176, 122, 16);
/// Recessed CRT glass: near-black with a faint warm bronze cast.
pub const GLASS_BG: egui::Color32 = egui::Color32::from_rgb(12, 9, 6);
/// Pedal enclosure body (dark machined metal).
pub const ENCLOSURE: egui::Color32 = egui::Color32::from_rgb(28, 29, 33);
/// Enclosure top-highlight (subtle vector "light from above").
pub const ENCLOSURE_HI: egui::Color32 = egui::Color32::from_rgb(44, 46, 52);
/// Enclosure lower body / recess shadow.
pub const ENCLOSURE_LO: egui::Color32 = egui::Color32::from_rgb(18, 19, 22);
/// Corner-screw head fill.
pub const SCREW: egui::Color32 = egui::Color32::from_rgb(52, 54, 60);

// --- Per-plugin theme preferences (persisted; guardrails #2 + the THEME-OFF fallback) ---
//
// Two bools per plugin: `console` (default ON — CONSOLE v2 vs the plain minimal-dark
// fallback) and `crt_motion` (default ON — scanline/cursor motion; the OFF-able switch
// guardrail #2 demands). They persist suite-wide in ONE JSON file
// `[MyDocuments]/Qeynos/ui_prefs.json` keyed by plugin slug — so NO plugin Params struct
// changes (this is a re-skin, not a re-plumb). Reads/writes are GUI-thread only and reuse
// `presets::documents_dir()`; a missing/broken file just yields defaults (never panics).
//
// System/host reduced-motion is NOT reliably detectable under baseview/nih_plug_egui, so
// guardrail #2's "respect reduced-motion when detectable" reduces to the explicit per-plugin
// `crt_motion` toggle here (documented in docs/UI.md, same spirit as the UI-CORE-FIX
// resize-API limitation note in DEFERRED.md).

/// Persisted per-plugin theme state. `Default` is the shipped look: CONSOLE on, motion on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemePrefs {
    pub console: bool,
    pub crt_motion: bool,
}

impl Default for ThemePrefs {
    fn default() -> Self {
        Self {
            console: true,
            crt_motion: true,
        }
    }
}

fn ui_prefs_path() -> Option<std::path::PathBuf> {
    presets::documents_dir().map(|d| d.join("Qeynos").join("ui_prefs.json"))
}

/// Load the whole slug -> {console, crt_motion} map. Missing file / parse error ⇒ empty
/// (every plugin then falls back to `ThemePrefs::default()`). Never errors, never panics.
fn load_ui_prefs() -> BTreeMap<String, (bool, bool)> {
    let Some(path) = ui_prefs_path() else {
        return BTreeMap::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    // Stored as { "grit": { "console": true, "crt_motion": false }, ... }.
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    if let Some(obj) = val.as_object() {
        for (slug, entry) in obj {
            let console = entry
                .get("console")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let motion = entry
                .get("crt_motion")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            out.insert(slug.clone(), (console, motion));
        }
    }
    out
}

/// Read one plugin's prefs (defaults if absent). GUI-thread only.
pub fn theme_prefs(slug: &str) -> ThemePrefs {
    load_ui_prefs()
        .get(slug)
        .map(|&(console, crt_motion)| ThemePrefs { console, crt_motion })
        .unwrap_or_default()
}

/// Persist one plugin's prefs (read-modify-write the shared map; last writer wins, which is
/// fine for a UI preference). Best-effort — IO failure is swallowed (the in-session egui-memory
/// copy still reflects the toggle so the UI stays responsive).
pub fn save_theme_prefs(slug: &str, prefs: ThemePrefs) {
    let Some(path) = ui_prefs_path() else {
        return;
    };
    let mut map = load_ui_prefs();
    map.insert(slug.to_string(), (prefs.console, prefs.crt_motion));
    let obj: serde_json::Map<String, serde_json::Value> = map
        .into_iter()
        .map(|(slug, (console, motion))| {
            (
                slug,
                serde_json::json!({ "console": console, "crt_motion": motion }),
            )
        })
        .collect();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&serde_json::Value::Object(obj)) {
        let _ = std::fs::write(&path, text);
    }
}

// --- Per-frame theme channel ---------------------------------------------------------------
//
// The theme is a paint-only re-skin: `knob_face`, `toggle_control` and `crt_frame` must know
// whether CONSOLE is on WITHOUT any change to their call signatures (so plugins that only call
// `labeled_slider` get re-skinned with ZERO edits). `ScaledWindow::show` resolves the effective
// prefs once per frame and stashes them in egui memory under these global keys; the paint
// helpers read them. One egui context == one plugin editor, so a single global key is safe.

fn console_key() -> egui::Id {
    egui::Id::new("qeynos-theme::console-on")
}
fn motion_key() -> egui::Id {
    egui::Id::new("qeynos-theme::crt-motion")
}

/// Is CONSOLE v2 active in this context this frame? Defaults to `true` if unset (so any stray
/// widget matches the shipped look); `ScaledWindow` sets it authoritatively each frame.
pub fn console_on(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.data.get_temp(console_key()).unwrap_or(true))
}

/// Is CRT motion (scanlines drift / cursor blink) allowed this frame?
pub fn crt_motion_on(ctx: &egui::Context) -> bool {
    console_on(ctx) && ctx.memory(|m| m.data.get_temp(motion_key()).unwrap_or(true))
}

fn set_theme_channel(ctx: &egui::Context, prefs: ThemePrefs) {
    ctx.memory_mut(|m| {
        m.data.insert_temp(console_key(), prefs.console);
        m.data.insert_temp(motion_key(), prefs.crt_motion);
    });
}

/// Derive the plugin slug from a window id (`"qeynos-grit-window"` -> `"grit"`,
/// `"qeynos-overseer-node-window"` -> `"overseer-node"`). The slug is the prefs key and the
/// preset folder slug — one stable identity per editor.
fn slug_from_window_id(id: &str) -> String {
    id.strip_prefix("qeynos-")
        .unwrap_or(id)
        .strip_suffix("-window")
        .unwrap_or(id)
        .to_string()
}

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

        // Resolve this plugin's theme prefs (cached in egui memory to avoid per-frame disk
        // IO) and publish them on the per-frame channel so the paint helpers below re-skin
        // without any change to their call signatures. THEME-OFF ⇒ everything falls back to
        // the plain minimal-dark look wholesale (the one code-path switch).
        let slug = slug_from_window_id(&self.id);
        let prefs = cached_prefs(ctx, &slug);
        set_theme_channel(ctx, prefs);

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
        let slug_for_menu = slug.clone();
        ResizableWindow::new(self.id)
            .min_size(self.base)
            .show(ctx, egui_state, |ui| {
                // Paint the hardware-pedal enclosure behind the content (CONSOLE only). Cheap
                // vector ops, no assets. Content draws on top, so nothing is obscured.
                if prefs.console {
                    paint_enclosure(ui);
                }
                let r = add_contents(ui);
                size_menu(ui, &id_for_menu, &slug_for_menu, scale, &override_id, prefs);
                r
            })
            .inner
    }
}

/// Read this plugin's theme prefs, caching them in egui memory so the disk file is touched
/// once per session (or after a toggle) rather than every frame.
fn cached_prefs(ctx: &egui::Context, slug: &str) -> ThemePrefs {
    let id = egui::Id::new(("qeynos-theme-prefs", slug));
    if let Some(p) = ctx.memory(|m| m.data.get_temp::<ThemePrefs>(id)) {
        return p;
    }
    let p = theme_prefs(slug);
    ctx.memory_mut(|m| m.data.insert_temp(id, p));
    p
}

/// Update the cached + on-disk prefs after a toggle in the size menu.
fn store_prefs(ctx: &egui::Context, slug: &str, prefs: ThemePrefs) {
    let id = egui::Id::new(("qeynos-theme-prefs", slug));
    ctx.memory_mut(|m| m.data.insert_temp(id, prefs));
    save_theme_prefs(slug, prefs);
}

/// The pedal enclosure: dark machined body, an amber brand strip, a top highlight + bottom
/// shadow (cheap "light from above"), an inner bevel, and four corner screws. Pure egui
/// painting — no textures/assets (stays self-contained per SPECS). ~14 primitives/frame.
fn paint_enclosure(ui: &egui::Ui) {
    let rect = ui.max_rect();
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let round = 10.0_f32;

    // Machined body.
    painter.rect_filled(rect, round, ENCLOSURE);
    // Amber brand strip along the very top edge (2 px; content's leading add_space clears it).
    let strip = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + 2.0));
    painter.rect_filled(strip, round, ACCENT);
    // Top highlight and bottom shadow bands.
    let hi = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.min.y + 2.0),
        egui::pos2(rect.max.x, rect.min.y + 5.0),
    );
    painter.rect_filled(hi, 0.0, ENCLOSURE_HI);
    let lo = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.max.y - 4.0),
        rect.max,
    );
    painter.rect_filled(lo, round, ENCLOSURE_LO);
    // Inner bevel.
    painter.rect_stroke(
        rect,
        round,
        egui::Stroke::new(1.0, ENCLOSURE_LO),
        egui::StrokeKind::Inside,
    );
    // Four corner screws (subtle, low-contrast; top-right sits under the ?/NN% buttons which
    // draw above in a Foreground Area).
    let inset = 9.0;
    let screws = [
        (rect.left() + inset, rect.top() + inset),
        (rect.right() - inset, rect.top() + inset),
        (rect.left() + inset, rect.bottom() - inset),
        (rect.right() - inset, rect.bottom() - inset),
    ];
    for (cx, cy) in screws {
        let c = egui::pos2(cx, cy);
        painter.circle_filled(c, 3.5, SCREW);
        painter.circle_stroke(c, 3.5, egui::Stroke::new(1.0, ENCLOSURE_LO));
        painter.line_segment(
            [egui::pos2(cx - 2.2, cy - 2.2), egui::pos2(cx + 2.2, cy + 2.2)],
            egui::Stroke::new(1.0, ENCLOSURE_LO),
        );
    }
}

/// A small "NN%" button anchored to the window's top-right corner. Opens a popup with
/// the snap stops; picking one locks the zoom (session state) until the window is
/// dragged. The effective size persists via `EguiState`, so scale survives reloads.
fn size_menu(
    ui: &egui::Ui,
    id: &str,
    slug: &str,
    current: f32,
    override_id: &egui::Id,
    prefs: ThemePrefs,
) {
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
                ui.separator();
                // THEME controls (guardrail #2 + the THEME-OFF fallback). Persisted per plugin.
                ui.label(egui::RichText::new("THEME").color(TEXT_DIM).small());
                let mut console = prefs.console;
                if ui.checkbox(&mut console, "Console skin").changed() {
                    store_prefs(
                        ui.ctx(),
                        slug,
                        ThemePrefs {
                            console,
                            crt_motion: prefs.crt_motion,
                        },
                    );
                }
                let mut motion = prefs.crt_motion;
                if ui
                    .add_enabled(
                        prefs.console,
                        egui::Checkbox::new(&mut motion, "CRT motion"),
                    )
                    .changed()
                {
                    store_prefs(
                        ui.ctx(),
                        slug,
                        ThemePrefs {
                            console: prefs.console,
                            crt_motion: motion,
                        },
                    );
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
    // CONSOLE re-skins the FACE ONLY (tick-ring collar + machined cap). Hit-testing, drag,
    // fine-drag, reset, scroll and click-to-type above are byte-identical in both themes
    // (guardrail #4). The value stays plain text under the knob via `value_text`.
    if ui.is_rect_visible(rect) {
        let console = console_on(ui.ctx());
        let painter = ui.painter();
        let center = rect.center();
        let radius = KNOB_DIAMETER * 0.5 - 3.0;
        let t = param.modulated_normalized_value().clamp(0.0, 1.0);

        // 270° sweep with the gap at the bottom (min → bottom-left, max → bottom-right).
        let a0 = 135.0_f32.to_radians();
        let a1 = 405.0_f32.to_radians();
        let ang = a0 + t * (a1 - a0);
        let pt = |a: f32, r: f32| center + Vec2::new(a.cos(), a.sin()) * r;

        if console {
            // Tick-ring collar just outside the cap (machined-knob look). Static ⇒ cheap.
            let ring_r = radius + 2.0;
            const RING_TICKS: usize = 21;
            for k in 0..=RING_TICKS {
                let a = a0 + (k as f32 / RING_TICKS as f32) * (a1 - a0);
                let major = k % 5 == 0;
                let (inner, w, col) = if major {
                    (ring_r - 4.0, 1.4, PHOSPHOR_DIM)
                } else {
                    (ring_r - 2.5, 1.0, egui::Color32::from_rgb(70, 62, 40))
                };
                painter.line_segment([pt(a, inner), pt(a, ring_r)], egui::Stroke::new(w, col));
            }
        }

        // Cap body.
        let body = if console {
            egui::Color32::from_rgb(30, 31, 35)
        } else {
            PANEL
        };
        painter.circle_filled(center, radius, body);
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 43, 48)),
        );

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
        if !console {
            // Plain theme: 5 coarse ticks on the cap edge.
            for k in 0..=4 {
                let a = a0 + (k as f32 / 4.0) * (a1 - a0);
                painter.line_segment(
                    [pt(a, radius - 2.0), pt(a, radius + 2.0)],
                    egui::Stroke::new(1.0, TEXT_DIM),
                );
            }
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
                let console = console_on(ui.ctx());
                let painter = ui.painter();
                let radius = rect.height() * 0.5;
                let track = if on {
                    ACCENT.linear_multiply(0.6)
                } else {
                    PANEL
                };
                painter.rect_filled(rect, radius, track);
                painter.rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 51, 57)),
                    egui::StrokeKind::Middle,
                );
                let knob_x = if on { rect.right() - radius } else { rect.left() + radius };
                let cap = egui::pos2(knob_x, rect.center().y);
                painter.circle_filled(cap, radius - 2.0, if on { ACCENT } else { TEXT_DIM });
                if console {
                    // Footswitch LED: a lit amber dot on the cap when engaged (a soft glow
                    // ring behind it), a dark socket when off. Reads as a stompbox switch.
                    if on {
                        painter.circle_filled(cap, radius - 2.0, PHOSPHOR.linear_multiply(0.35));
                        painter.circle_filled(cap, (radius - 2.0) * 0.5, PHOSPHOR);
                    } else {
                        painter.circle_filled(cap, (radius - 2.0) * 0.5, GLASS_BG);
                    }
                }
            }
            ui.label(egui::RichText::new(param.to_string()).color(TEXT).small());
        });
    });
}

// ===========================================================================
// CONSOLE v2 — CRT telemetry bay + section header (PEDAL-UI)
// ===========================================================================

/// An amber section header (small-caps label over a faint rule). CONSOLE draws it in
/// phosphor amber; THEME-OFF falls back to the plain dim label. Cosmetic only.
pub fn section_header(ui: &mut egui::Ui, text: &str) {
    let console = console_on(ui.ctx());
    let col = if console { PHOSPHOR_DIM } else { TEXT_DIM };
    ui.label(egui::RichText::new(text).color(col).small().strong());
}

/// The recessed **amber-CRT telemetry bay**: a bronze-black glass panel with faint static
/// scanlines and (when CRT motion is on) a blinking terminal cursor, hosting a plugin's live
/// telemetry/visualization in terminal style. `add_contents` paints INTO the glass — amber
/// monospace text, meters, scopes, spectra — clipped to the inner recess.
///
/// * CONSOLE on  → glass + scanlines (static) + cursor blink gated by the per-plugin CRT-motion
///   pref (guardrail #2). Motion requests only a slow (~8 fps) repaint so idle GUIs stay cheap
///   (guardrail #6).
/// * CONSOLE off → a plain faint panel, no glass/scanlines/cursor; the SAME content still renders
///   (guardrail #3: the CRT is additive instrumentation — nothing operable lives only inside it).
pub fn crt_frame<R>(
    ui: &mut egui::Ui,
    id_salt: &str,
    height: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let console = console_on(ui.ctx());
    let motion = crt_motion_on(ui.ctx());
    let width = ui.available_width();
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let inner = rect.shrink(8.0);

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if console {
            painter.rect_filled(rect, 6.0, GLASS_BG);
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 44, 12)),
                egui::StrokeKind::Inside,
            );
            // Faint static scanlines every 3 px (a texture, not motion — stays on with
            // motion off). ~height/3 cheap line segments.
            let sl = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 64);
            let mut y = inner.top() + 1.0;
            while y < inner.bottom() {
                painter.line_segment(
                    [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
                    egui::Stroke::new(1.0, sl),
                );
                y += 3.0;
            }
        } else {
            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(10, 11, 13));
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, PANEL),
                egui::StrokeKind::Inside,
            );
        }
    }

    // Content clipped to the inner glass.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    child.set_clip_rect(inner);
    let _salt = id_salt; // reserved for future per-bay state; keeps the API stable.
    let r = add_contents(&mut child);

    // Blinking cursor, motion only.
    if console && motion && ui.is_rect_visible(rect) {
        let ctx = ui.ctx();
        let t = ctx.input(|i| i.time);
        if (t * 1.6).fract() < 0.5 {
            let cur = egui::Rect::from_min_size(
                egui::pos2(inner.left(), inner.bottom() - 11.0),
                egui::vec2(7.0, 10.0),
            );
            ui.painter().rect_filled(cur, 0.0, PHOSPHOR);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
    r
}

/// Convenience over [`crt_frame`]: a titled block of amber terminal lines (`(label, value)`
/// pairs). This is the honest, cheap telemetry every plugin without a dedicated scope shows —
/// the SAME live values that appear on its knobs (guardrail #3), in monospace so columns align.
pub fn crt_lines(ui: &mut egui::Ui, id_salt: &str, title: &str, lines: &[(&str, String)]) {
    let height = 22.0 + lines.len() as f32 * 16.0 + 8.0;
    crt_frame(ui, id_salt, height, |ui| {
        let console = console_on(ui.ctx());
        let title_col = if console { PHOSPHOR } else { TEXT_DIM };
        ui.label(
            egui::RichText::new(title)
                .color(title_col)
                .monospace()
                .strong()
                .size(12.0),
        );
        ui.add_space(2.0);
        let body = if console { PHOSPHOR } else { TEXT };
        let dim = if console { PHOSPHOR_DIM } else { TEXT_DIM };
        for (label, value) in lines {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{label:<11}"))
                        .color(dim)
                        .monospace()
                        .size(12.0),
                );
                ui.label(
                    egui::RichText::new(value)
                        .color(body)
                        .monospace()
                        .size(12.0),
                );
            });
        }
    });
}

// ===========================================================================
// Preset bar (PRESET-SYSTEM, SPECS "POLISH phase")
// ===========================================================================
//
// One call site per editor replaces the old per-plugin preset ComboBox. It shows a
// FACTORY + USER dropdown, Save / Save As / Delete, and a dirty dot. All of it runs on
// the GUI thread: loads apply through the [`ParamSetter`] (host-visible, undoable),
// saves snapshot the live param values, and the disk IO goes through
// [`crate::presets`]. NOTHING here touches the audio thread.
//
// Factory and user presets are unified via a generic value model keyed by nih-plug
// PARAM IDS (`Params::param_map`): [`snapshot_params`] reads the live plain values and
// [`apply_values`] writes them back through the setter. User presets are saved in this
// param-id key space; factory presets keep their own pretty JSON keys and are applied
// through a small per-plugin callback. After EITHER kind of load the bar captures a
// fresh generic snapshot as the "loaded baseline", so the dirty dot works uniformly for
// both. OVERSEER-ENRICH builds its type-filtered banks on this same widget.

/// Snapshot every parameter's current plain value into a `param_id -> value` map. Uses
/// `unmodulated_plain_value()`, which returns a uniform `f32` for Float (plain), Int
/// (i32 as f32), Bool (0.0/1.0) and Enum (variant index as f32) — exactly the flat
/// numeric model presets are stored in. GUI-thread only (allocates a `Vec` + map).
pub fn snapshot_params(params: &dyn Params) -> BTreeMap<String, f32> {
    params
        .param_map()
        .into_iter()
        // SAFETY: `params` outlives this call; ParamPtr reads are valid for its lifetime.
        .map(|(id, ptr, _group)| (id, unsafe { ptr.unmodulated_plain_value() }))
        .collect()
}

/// Apply a `param_id -> plain value` map to the live parameters through the host
/// (begin/set/end bracketed per param, so the change is host-visible automation and
/// undoable). Ids absent from `params` are ignored; params absent from `values` are
/// left untouched. GUI-thread only. This is the exact inverse of [`snapshot_params`],
/// so `apply_values(snapshot_params(p))` restores `p` bit-for-bit.
pub fn apply_values(params: &dyn Params, setter: &ParamSetter, values: &BTreeMap<String, f32>) {
    for (id, ptr, _group) in params.param_map() {
        if let Some(&plain) = values.get(&id) {
            // SAFETY: ptr belongs to `params`, alive for this call; raw_context matches.
            unsafe {
                let nv = ptr.preview_normalized(plain);
                setter.raw_context.raw_begin_set_parameter(ptr);
                setter.raw_context.raw_set_parameter_normalized(ptr, nv);
                setter.raw_context.raw_end_set_parameter(ptr);
            }
        }
    }
}

/// Combined abs+rel tolerance for the dirty comparison. Params span dB (~±60), Hz
/// (~20k) and 0..1 mixes, so a pure absolute epsilon can't serve all of them.
fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4 * (1.0 + a.abs().max(b.abs()))
}

/// True if the live params diverge from `baseline` (the snapshot captured at load). An
/// empty baseline (nothing loaded yet) is never dirty.
fn is_dirty(current: &BTreeMap<String, f32>, baseline: &BTreeMap<String, f32>) -> bool {
    if baseline.is_empty() {
        return false;
    }
    baseline
        .iter()
        .any(|(k, &bv)| current.get(k).map_or(true, |&cv| !approx_eq(cv, bv)))
}

/// Per-editor transient state for the preset bar, kept in egui memory keyed by the bar
/// id (so the retrofit adds no fields to any plugin's editor state).
#[derive(Clone, Default)]
struct BarState {
    /// Param snapshot captured right after the last successful load. Empty = none.
    baseline: BTreeMap<String, f32>,
    /// Display name of the loaded preset (drives Save / Delete targeting).
    current_label: String,
    /// Delete + overwrite-Save are user-preset only.
    current_is_user: bool,
    save_as_open: bool,
    save_as_buf: String,
    /// Two-click delete confirm.
    delete_arm: bool,
    user_cache: Vec<Preset>,
    cache_loaded: bool,
    /// Last IO error, shown inline (never panics).
    error: String,
}

/// The suite preset bar. Retrofit is one call replacing the old ComboBox row:
///
/// ```ignore
/// suite_core::ui::PresetBar::new("grit", &factory_presets)
///     .show(ui, &*params, setter, |setter, preset| apply_preset(&params, setter, preset));
/// ```
///
/// `plugin_id` is BOTH the egui id salt and the on-disk folder slug
/// (`[MyDocuments]/Qeynos/Presets/<plugin_id>/`).
pub struct PresetBar<'a> {
    plugin_id: &'a str,
    factory: &'a [Preset],
    /// OVERSEER-ENRICH: when `Some(category)` the factory dropdown surfaces the presets
    /// tagged with that category ([`Preset::category`]) first under a category heading — the
    /// Node bar passes its current instrument type, the Master bar its inferred theme. The
    /// rest of the factory bank is still listed under an OTHER heading (nothing is hidden).
    filter: Option<String>,
}

impl<'a> PresetBar<'a> {
    pub fn new(plugin_id: &'a str, factory: &'a [Preset]) -> Self {
        Self {
            plugin_id,
            factory,
            filter: None,
        }
    }

    /// Filter the factory bank by a category tag (instrument type / session theme). `None`
    /// (the default) shows one flat FACTORY list, exactly as before OVERSEER-ENRICH.
    pub fn filter(mut self, category: Option<impl Into<String>>) -> Self {
        self.filter = category.map(Into::into);
        self
    }

    /// Draw the bar. `params` drives the generic snapshot/apply + dirty dot;
    /// `apply_factory` applies one factory [`Preset`] through the setter (the plugin's
    /// existing per-preset mapping, since factory JSON uses pretty keys).
    pub fn show(
        self,
        ui: &mut egui::Ui,
        params: &dyn Params,
        setter: &ParamSetter,
        apply_factory: impl Fn(&ParamSetter, &Preset),
    ) {
        let state_id = egui::Id::new(("qeynos-preset-bar", self.plugin_id));
        let mut st: BarState = ui.memory(|m| m.data.get_temp(state_id).unwrap_or_default());

        if !st.cache_loaded {
            match presets::list_user(self.plugin_id) {
                Ok(list) => {
                    st.user_cache = list;
                    st.error.clear();
                }
                Err(e) => st.error = e,
            }
            st.cache_loaded = true;
        }

        let current = snapshot_params(params);
        let dirty = is_dirty(&current, &st.baseline);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("PRESET").color(TEXT_DIM).small());

            let selected = if st.current_label.is_empty() {
                "select…".to_string()
            } else {
                st.current_label.clone()
            };
            egui::ComboBox::from_id_salt((self.plugin_id, "preset-combo"))
                .selected_text(selected)
                .width(180.0)
                .show_ui(ui, |ui| {
                    // Factory bank. With a category filter (OVERSEER-ENRICH type/theme banks)
                    // the matching presets are surfaced first under a category heading, then
                    // the rest under OTHER; without a filter it is one flat FACTORY list.
                    let choose = |ui: &mut egui::Ui, st: &mut BarState, p: &Preset| {
                        if ui.selectable_label(false, &p.name).clicked() {
                            apply_factory(setter, p);
                            st.baseline = snapshot_params(params);
                            st.current_label = p.name.clone();
                            st.current_is_user = false;
                            st.delete_arm = false;
                            st.error.clear();
                        }
                    };
                    match self.filter.as_deref() {
                        Some(cat) => {
                            ui.label(egui::RichText::new(cat).color(ACCENT).small());
                            let mut matched = 0usize;
                            for p in self.factory.iter() {
                                if p.category.as_deref() == Some(cat) {
                                    choose(ui, &mut st, p);
                                    matched += 1;
                                }
                            }
                            if matched == 0 {
                                ui.label(
                                    egui::RichText::new("(no bank for this type)")
                                        .color(TEXT_DIM)
                                        .small(),
                                );
                            }
                            let has_other =
                                self.factory.iter().any(|p| p.category.as_deref() != Some(cat));
                            if has_other {
                                ui.separator();
                                ui.label(egui::RichText::new("OTHER").color(TEXT_DIM).small());
                                for p in self.factory.iter() {
                                    if p.category.as_deref() != Some(cat) {
                                        choose(ui, &mut st, p);
                                    }
                                }
                            }
                        }
                        None => {
                            // PRESET-EXPANSION: when the bank tags presets with
                            // categories, render one headed section per category (in
                            // first-appearance order) so the deep banks read as
                            // sections. Untagged presets fall under a plain FACTORY
                            // heading. A bank with no categories at all (e.g. _template)
                            // is still one flat FACTORY list — fully back-compatible.
                            let has_categories =
                                self.factory.iter().any(|p| p.category.is_some());
                            if !has_categories {
                                ui.label(egui::RichText::new("FACTORY").color(ACCENT).small());
                                for p in self.factory.iter() {
                                    choose(ui, &mut st, p);
                                }
                            } else {
                                // Distinct categories in first-appearance order.
                                let mut cats: Vec<&str> = Vec::new();
                                for p in self.factory.iter() {
                                    if let Some(c) = p.category.as_deref() {
                                        if !cats.contains(&c) {
                                            cats.push(c);
                                        }
                                    }
                                }
                                let mut first = true;
                                for cat in cats {
                                    if !first {
                                        ui.separator();
                                    }
                                    first = false;
                                    ui.label(egui::RichText::new(cat).color(ACCENT).small());
                                    for p in self.factory.iter() {
                                        if p.category.as_deref() == Some(cat) {
                                            choose(ui, &mut st, p);
                                        }
                                    }
                                }
                                // Any untagged stragglers under a plain FACTORY heading.
                                if self.factory.iter().any(|p| p.category.is_none()) {
                                    ui.separator();
                                    ui.label(egui::RichText::new("FACTORY").color(ACCENT).small());
                                    for p in self.factory.iter() {
                                        if p.category.is_none() {
                                            choose(ui, &mut st, p);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("USER").color(ACCENT).small());
                    if st.user_cache.is_empty() {
                        ui.label(egui::RichText::new("(none saved)").color(TEXT_DIM).small());
                    }
                    // Clone to avoid borrowing st across the apply mutation.
                    let user = st.user_cache.clone();
                    for p in user.iter() {
                        if ui.selectable_label(false, &p.name).clicked() {
                            apply_values(params, setter, &p.values);
                            st.baseline = snapshot_params(params);
                            st.current_label = p.name.clone();
                            st.current_is_user = true;
                            st.delete_arm = false;
                            st.error.clear();
                        }
                    }
                });

            // Dirty dot.
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            if dirty && ui.is_rect_visible(dot) {
                ui.painter().circle_filled(dot.center(), 3.5, ACCENT);
            }

            // Save = overwrite the loaded USER preset.
            let can_overwrite = st.current_is_user && !st.current_label.is_empty();
            if ui
                .add_enabled(can_overwrite, egui::Button::new("Save"))
                .on_hover_text("Overwrite the loaded user preset")
                .clicked()
            {
                match presets::save_user(self.plugin_id, &st.current_label, &current) {
                    Ok(_) => {
                        st.baseline = current.clone();
                        st.cache_loaded = false;
                        st.error.clear();
                    }
                    Err(e) => st.error = e,
                }
            }

            if ui.button("Save As").clicked() {
                st.save_as_open = !st.save_as_open;
                if st.save_as_open {
                    st.save_as_buf = st.current_label.clone();
                }
                st.delete_arm = false;
            }

            // Delete = user presets only, two-click confirm.
            let can_delete = st.current_is_user && !st.current_label.is_empty();
            let del_txt = if st.delete_arm { "Confirm?" } else { "Delete" };
            if ui
                .add_enabled(can_delete, egui::Button::new(del_txt))
                .clicked()
            {
                if st.delete_arm {
                    match presets::delete_user(self.plugin_id, &st.current_label) {
                        Ok(()) => {
                            st.current_label.clear();
                            st.baseline.clear();
                            st.current_is_user = false;
                            st.cache_loaded = false;
                            st.error.clear();
                        }
                        Err(e) => st.error = e,
                    }
                    st.delete_arm = false;
                } else {
                    st.delete_arm = true;
                }
            }
        });

        // Inline Save-As text field row.
        if st.save_as_open {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("NAME").color(TEXT_DIM).small());
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut st.save_as_buf)
                        .desired_width(180.0)
                        .hint_text("preset name"),
                );
                let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Save").clicked() || submit {
                    match presets::save_user(self.plugin_id, &st.save_as_buf, &current) {
                        Ok(_) => {
                            st.current_label = presets::sanitize_name(&st.save_as_buf);
                            st.current_is_user = true;
                            st.baseline = current.clone();
                            st.save_as_open = false;
                            st.cache_loaded = false;
                            st.error.clear();
                        }
                        Err(e) => st.error = e,
                    }
                }
                if ui.button("Cancel").clicked() {
                    st.save_as_open = false;
                }
            });
        }

        if !st.error.is_empty() {
            ui.label(
                egui::RichText::new(format!("⚠ {}", st.error))
                    .color(egui::Color32::from_rgb(220, 120, 90))
                    .small(),
            );
        }

        ui.memory_mut(|m| m.data.insert_temp(state_id, st));
    }
}

// ===========================================================================
// MOD section — per-param cross-plugin modulation routing (NERVE listen layer)
// ===========================================================================

/// A collapsible "MOD" section listing the plugin's modulatable params. For each target it
/// shows: source instance (live NERVE slots on the tier-2 [`crate::bus`]), which of the 8
/// signals, a depth (-1..1), and a shaping curve. Edits are written straight into the
/// shared, persisted [`crate::modlisten::ModRoutes`] (GUI thread; the audio thread only
/// `try_read`s it). Retrofit is one call in a plugin's editor closure:
///
/// ```ignore
/// suite_core::ui::mod_section(ui, &plugin.mod_routes, &[
///     ("drive", "DRIVE"), ("mix", "MIX"),
/// ]);
/// ```
pub fn mod_section(
    ui: &mut egui::Ui,
    routes: &std::sync::RwLock<crate::modlisten::ModRoutes>,
    targets: &[(&str, &str)],
) {
    use crate::bus;
    use crate::bus::NUM_MOD_SIGNALS;
    use crate::modlisten::Curve;

    // Live *mod-publishing* sources on the bus. NERVE is the only kind that writes the 8
    // mod signals; spectrum-only publishers (e.g. X-RAY) claim slots too but never populate
    // `mods`, so listing them would offer routes that are permanently zero. Filter to NERVE.
    let sources: Vec<(u64, String)> = bus::bus()
        .map(|b| {
            b.snapshot_live()
                .into_iter()
                .filter(|s| s.kind == bus::PluginKind::Nerve)
                .map(|s| {
                    let name = if s.label.is_empty() {
                        format!("#{}", s.instance_id & 0xFFFF)
                    } else {
                        s.label
                    };
                    (s.instance_id, name)
                })
                .collect()
        })
        .unwrap_or_default();

    egui::CollapsingHeader::new(egui::RichText::new("MOD").color(ACCENT))
        .id_salt("qeynos-mod-section")
        .show(ui, |ui| {
            if sources.is_empty() {
                ui.label(
                    egui::RichText::new("No bus sources — add a Qeynos NERVE (un-bridged).")
                        .color(TEXT_DIM)
                        .small(),
                );
            }
            let mut cfg = routes.write().unwrap_or_else(|p| p.into_inner());
            for (pid, label) in targets {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(*label).color(TEXT).small());

                    let cur = cfg.get(pid).cloned();
                    let cur_src = cur.as_ref().map(|r| r.source_instance).unwrap_or(0);
                    let cur_src_label = sources
                        .iter()
                        .find(|(id, _)| *id == cur_src)
                        .map(|(_, l)| l.clone())
                        .unwrap_or_else(|| "— none —".to_string());

                    egui::ComboBox::from_id_salt((*pid, "mod-src"))
                        .selected_text(cur_src_label)
                        .width(96.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(cur_src == 0, "— none —").clicked() {
                                cfg.clear(pid);
                            }
                            for (id, l) in &sources {
                                if ui.selectable_label(cur_src == *id, l).clicked() {
                                    cfg.entry(pid).source_instance = *id;
                                }
                            }
                        });

                    if let Some(r) = cfg.get(pid).filter(|r| r.source_instance != 0).cloned() {
                        egui::ComboBox::from_id_salt((*pid, "mod-sig"))
                            .selected_text(format!("S{}", r.source_index + 1))
                            .width(50.0)
                            .show_ui(ui, |ui| {
                                for i in 0..NUM_MOD_SIGNALS as u8 {
                                    if ui
                                        .selectable_label(r.source_index == i, format!("S{}", i + 1))
                                        .clicked()
                                    {
                                        cfg.entry(pid).source_index = i;
                                    }
                                }
                            });

                        let mut d = r.depth;
                        if ui
                            .add(
                                egui::DragValue::new(&mut d)
                                    .speed(0.01)
                                    .fixed_decimals(2)
                                    .prefix("d "),
                            )
                            .changed()
                        {
                            cfg.entry(pid).depth = d.clamp(-1.0, 1.0);
                        }

                        egui::ComboBox::from_id_salt((*pid, "mod-curve"))
                            .selected_text(r.curve.label())
                            .width(78.0)
                            .show_ui(ui, |ui| {
                                for c in Curve::ALL {
                                    if ui.selectable_label(r.curve == c, c.label()).clicked() {
                                        cfg.entry(pid).curve = c;
                                    }
                                }
                            });

                        if ui.small_button("✕").clicked() {
                            cfg.clear(pid);
                        }
                    }
                });
            }
        });
}

// ===========================================================================
// Built-in manual — the '?' button + scrollable, closable panel (BUILT-IN-MANUALS)
// ===========================================================================

/// The suite-standard **manual button**: a small `?` anchored to the window's top-right
/// (just left of the size menu), and — while toggled open — a closable, scrollable panel
/// rendering the plugin's embedded `docs/<PLUGIN>.md` via [`crate::manual`].
///
/// Retrofit is one mechanical call in a plugin's editor closure — position-independent,
/// since it draws into its own top-anchored [`egui::Area`]:
///
/// ```ignore
/// const MANUAL_DOC: &str = include_str!("../../../docs/GRIT.md");
/// // ...inside the editor, anywhere in the ScaledWindow content:
/// suite_core::ui::manual_button(ui, "grit", "GRIT", MANUAL_DOC);
/// ```
///
/// `plugin_id` namespaces the open/close state (and must be unique per editor — OVERSEER
/// passes distinct ids for its Node and Master windows); `display_name` titles the panel.
pub fn manual_button(ui: &egui::Ui, plugin_id: &str, display_name: &str, doc: &str) {
    let ctx = ui.ctx().clone();
    let open_id = egui::Id::new(("qeynos-manual-open", plugin_id));

    // '?' button — its own foreground Area anchored top-right, sitting to the left of the
    // size menu's "NN%" button (which is at offset -6). Position-independent so the retrofit
    // call can go anywhere in the editor body.
    egui::Area::new(egui::Id::new(("qeynos-manual-btn", plugin_id)))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-46.0, 5.0))
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            let mut open: bool = ui.ctx().memory(|m| m.data.get_temp(open_id).unwrap_or(false));
            let btn = egui::Button::new(egui::RichText::new("?").color(ACCENT).strong())
                .min_size(Vec2::splat(18.0));
            if ui.add(btn).on_hover_text("Manual").clicked() {
                open = !open;
                ui.ctx().memory_mut(|m| m.data.insert_temp(open_id, open));
            }
        });

    let open: bool = ctx.memory(|m| m.data.get_temp(open_id).unwrap_or(false));
    if !open {
        return;
    }

    let manual = crate::manual::Manual::parse(doc);
    let mut still_open = true;
    egui::Window::new(egui::RichText::new(format!("{display_name} · MANUAL")).color(ACCENT))
        .id(egui::Id::new(("qeynos-manual-win", plugin_id)))
        .open(&mut still_open)
        .collapsible(false)
        .resizable(true)
        .default_width(470.0)
        .default_height(540.0)
        .show(&ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| render_manual(ui, &manual));
        });
    if !still_open {
        ctx.memory_mut(|m| m.data.insert_temp(open_id, false));
    }
}

/// Render the four canonical sections (in reading order) that are present. Signal Flow is
/// verbatim monospace (the ASCII diagram); the rest go through a light markdown-ish line
/// renderer. Missing sections are simply skipped.
fn render_manual(ui: &mut egui::Ui, manual: &crate::manual::Manual) {
    const ORDER: [&str; 4] = ["What It Is", "Signal Flow", "Controls", "Recipes"];
    let mut drew_any = false;
    for (i, title) in ORDER.iter().enumerate() {
        let Some(body) = manual.section(title) else {
            continue;
        };
        if body.trim().is_empty() {
            continue;
        }
        if i != 0 && drew_any {
            ui.add_space(10.0);
        }
        drew_any = true;
        ui.label(egui::RichText::new(*title).color(ACCENT).strong().size(15.0));
        ui.add_space(2.0);
        if title.eq_ignore_ascii_case("Signal Flow") {
            render_monospace_block(ui, body);
        } else {
            render_lines(ui, body);
        }
    }
    if !drew_any {
        ui.label(
            egui::RichText::new("(no manual content)")
                .color(TEXT_DIM)
                .italics(),
        );
    }
}

/// Signal-flow diagram: verbatim, non-wrapping monospace inside a faint panel so ASCII art
/// keeps its alignment. Horizontally scrollable if wider than the window.
fn render_monospace_block(ui: &mut egui::Ui, body: &str) {
    // Strip a leading/trailing ``` fence line if the docs wrapped the diagram in a code block.
    let inner: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(10, 11, 13))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("manual-flow")
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(inner).monospace().color(TEXT))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

/// Light line renderer for Controls/Recipes/What-It-Is: handles `### ` subheadings, `- `/
/// `* ` bullets, numbered steps, markdown `|` tables (kept monospace so columns align),
/// blank-line spacing, and inline `**bold**`/backtick stripping. Deliberately tiny — not a
/// full markdown engine.
fn render_lines(ui: &mut egui::Ui, body: &str) {
    let mut prev_was_table = false;
    for raw in body.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            ui.add_space(4.0);
            prev_was_table = false;
            continue;
        }

        // Markdown table row → monospace verbatim so pipes line up. Skip the |---|---| rule.
        if trimmed.starts_with('|') {
            if trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                prev_was_table = true;
                continue;
            }
            if !prev_was_table {
                ui.add_space(2.0);
            }
            prev_was_table = true;
            ui.add(
                egui::Label::new(
                    egui::RichText::new(clean_inline(trimmed)).monospace().color(TEXT),
                )
                .wrap_mode(egui::TextWrapMode::Extend),
            );
            continue;
        }
        prev_was_table = false;

        if let Some(h) = trimmed.strip_prefix("### ") {
            ui.add_space(3.0);
            ui.label(egui::RichText::new(clean_inline(h)).color(TEXT).strong());
            continue;
        }
        if let Some(b) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("•").color(ACCENT));
                ui.label(egui::RichText::new(clean_inline(b)).color(TEXT));
            });
            continue;
        }
        // Numbered list item ("1. ", "2. ", ...).
        if let Some((n, rest)) = split_numbered(trimmed) {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{n}.")).color(ACCENT));
                ui.label(egui::RichText::new(clean_inline(rest)).color(TEXT));
            });
            continue;
        }
        ui.label(egui::RichText::new(clean_inline(trimmed)).color(TEXT));
    }
}

/// Split `"3. some text"` → `(3, "some text")`, else `None`.
fn split_numbered(s: &str) -> Option<(u32, &str)> {
    let dot = s.find(". ")?;
    let n: u32 = s[..dot].parse().ok()?;
    Some((n, &s[dot + 2..]))
}

/// Strip the markdown emphasis markers we don't render richly (`**`, `` ` ``) so the text
/// reads cleanly without a full inline parser.
fn clean_inline(s: &str) -> String {
    s.replace("**", "").replace('`', "")
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

    // --- Preset bar: the snapshot→apply round trip is what save→load restores. We
    //     can't drive a real ParamSetter without a host, but apply() reduces to
    //     `preview_normalized(plain)` then set; the value read back is
    //     `preview_plain(preview_normalized(plain))`. Proving that is identity for a
    //     snapshotted plain value proves the round trip is exact. ---

    fn plain_roundtrips_exact<P: Param>(param: &P, plain: P::Plain)
    where
        P::Plain: Copy,
    {
        let norm = param.preview_normalized(plain);
        let back = param.preview_plain(norm);
        // Compare via normalized (uniform f32) to avoid P::Plain arithmetic.
        assert!(
            (param.preview_normalized(back) - norm).abs() < 1e-6,
            "plain value did not survive normalize/denormalize round trip"
        );
    }

    #[test]
    fn float_plain_value_survives_snapshot_apply() {
        let p = FloatParam::new("Hz", 1000.0, FloatRange::Skewed {
            min: 20.0,
            max: 20_000.0,
            factor: FloatRange::skew_factor(-2.0),
        });
        for &v in &[20.0_f32, 100.0, 440.0, 12_345.0, 20_000.0] {
            plain_roundtrips_exact(&p, v);
        }
    }

    #[test]
    fn int_and_bool_plain_values_survive_snapshot_apply() {
        let ip = IntParam::new("N", 4, IntRange::Linear { min: 0, max: 16 });
        for v in 0..=16 {
            plain_roundtrips_exact(&ip, v);
        }
        let bp = BoolParam::new("On", false);
        plain_roundtrips_exact(&bp, false);
        plain_roundtrips_exact(&bp, true);
    }

    #[test]
    fn dirty_is_false_without_a_loaded_baseline() {
        let cur: BTreeMap<String, f32> = [("a".into(), 1.0)].into_iter().collect();
        assert!(!is_dirty(&cur, &BTreeMap::new()));
    }

    #[test]
    fn dirty_tracks_divergence_from_baseline() {
        let base: BTreeMap<String, f32> =
            [("drive".into(), 8.0), ("hz".into(), 12000.0)].into_iter().collect();
        // Identical → clean.
        assert!(!is_dirty(&base.clone(), &base));
        // One param nudged past tolerance → dirty.
        let mut moved = base.clone();
        moved.insert("drive".into(), 8.5);
        assert!(is_dirty(&moved, &base));
        // Within tolerance (Hz jitter well under the rel epsilon) → still clean.
        let mut jitter = base.clone();
        jitter.insert("hz".into(), 12000.0 + 0.5);
        assert!(!is_dirty(&jitter, &base));
    }

    #[test]
    fn approx_eq_scales_with_magnitude() {
        assert!(approx_eq(20_000.0, 20_000.5)); // 0.5 Hz on 20k: fine
        assert!(!approx_eq(0.0, 0.01)); // 0.01 on a 0..1 mix: dirty
    }

    // --- CONSOLE v2 theme foundation (PEDAL-UI) ---

    #[test]
    fn slug_derives_from_window_id() {
        assert_eq!(slug_from_window_id("qeynos-grit-window"), "grit");
        assert_eq!(
            slug_from_window_id("qeynos-overseer-node-window"),
            "overseer-node"
        );
        assert_eq!(
            slug_from_window_id("qeynos-overseer-master-window"),
            "overseer-master"
        );
        assert_eq!(slug_from_window_id("qeynos-template-window"), "template");
        // Degenerate/foreign ids pass through unchanged rather than panicking.
        assert_eq!(slug_from_window_id("weird"), "weird");
    }

    #[test]
    fn theme_prefs_default_is_the_shipped_console_look() {
        let d = ThemePrefs::default();
        assert!(d.console, "CONSOLE must default ON");
        assert!(d.crt_motion, "CRT motion must default ON");
    }

    #[test]
    fn ui_prefs_json_roundtrips_and_tolerates_missing_fields() {
        // The exact shape save_theme_prefs writes must parse back through load's reader.
        let json = serde_json::json!({
            "grit": { "console": false, "crt_motion": true },
            "ember": { "console": true },          // missing crt_motion -> defaults true
            "wire": { "crt_motion": false },       // missing console    -> defaults true
        })
        .to_string();
        let map: BTreeMap<String, (bool, bool)> = {
            // Mirror load_ui_prefs' parsing (can't hit the real file path in a unit test).
            let val: serde_json::Value = serde_json::from_str(&json).unwrap();
            let mut out = BTreeMap::new();
            for (slug, entry) in val.as_object().unwrap() {
                let console = entry.get("console").and_then(|v| v.as_bool()).unwrap_or(true);
                let motion = entry.get("crt_motion").and_then(|v| v.as_bool()).unwrap_or(true);
                out.insert(slug.clone(), (console, motion));
            }
            out
        };
        assert_eq!(map["grit"], (false, true));
        assert_eq!(map["ember"], (true, true));
        assert_eq!(map["wire"], (true, false));
    }
}
