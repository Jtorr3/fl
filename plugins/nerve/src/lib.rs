//! NERVE — the suite modulation bus source (Qeynos suite, Phase 3).
//!
//! NERVE generates 8 modulation streams (4 LFOs with an 8-shape bank incl. S&H, 2 env
//! followers on its own input, 2 random sample-and-hold, 4 hand macros summed into the LFO
//! streams) and **publishes them to the tier-2 cross-DLL/cross-process bus**
//! ([`suite_core::bus`]) at block rate. Any other Qeynos plugin's params can then "listen"
//! to a stream via the shared [`suite_core::modlisten`] layer (its MOD section).
//!
//! NERVE passes audio through **bit-exact** (it is a modulation tap, transparent, zero
//! latency) so it can sit inline on any track it wants to follow with an env follower.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared with the offline harness tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

pub mod dsp;
pub mod presets;

use dsp::{Division, NerveCore, Settings, Shape, NUM_ENV, NUM_LFO, NUM_MACRO, NUM_SH};
use suite_core::bus::{self, PluginKind, NUM_MOD_SIGNALS};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/NERVE.md");

// ---------------------------------------------------------------------------
// Param-facing enums
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ShapeParam {
    Sine,
    Triangle,
    #[name = "Saw Up"]
    SawUp,
    #[name = "Saw Down"]
    SawDown,
    Square,
    #[name = "S&H"]
    SampleHold,
    #[name = "Smooth Rnd"]
    SmoothRandom,
    #[name = "Exp Pulse"]
    ExpPulse,
}
impl ShapeParam {
    fn to_dsp(self) -> Shape {
        match self {
            ShapeParam::Sine => Shape::Sine,
            ShapeParam::Triangle => Shape::Triangle,
            ShapeParam::SawUp => Shape::SawUp,
            ShapeParam::SawDown => Shape::SawDown,
            ShapeParam::Square => Shape::Square,
            ShapeParam::SampleHold => Shape::StepRandom,
            ShapeParam::SmoothRandom => Shape::SmoothRandom,
            ShapeParam::ExpPulse => Shape::ExpPulse,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DivParam {
    #[name = "4 Bars"]
    Bars4,
    #[name = "2 Bars"]
    Bars2,
    #[name = "1 Bar"]
    Bar1,
    #[name = "1/2"]
    Half,
    #[name = "1/4"]
    Quarter,
    #[name = "1/8"]
    Eighth,
    #[name = "1/16"]
    Sixteenth,
}
impl DivParam {
    fn to_dsp(self) -> Division {
        match self {
            DivParam::Bars4 => Division::Bars4,
            DivParam::Bars2 => Division::Bars2,
            DivParam::Bar1 => Division::Bar1,
            DivParam::Half => Division::Half,
            DivParam::Quarter => Division::Quarter,
            DivParam::Eighth => Division::Eighth,
            DivParam::Sixteenth => Division::Sixteenth,
        }
    }
}

/// Apply a factory [`Preset`] — NERVE presets are param-id keyed with plain values, so the
/// generic apply path handles them with no per-key mapping.
fn apply_preset(params: &NerveParams, setter: &ParamSetter, p: &Preset) {
    suite_core::ui::apply_values(params, setter, &p.values);
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Params)]
pub struct NerveParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// User label shown to listeners (published to the bus slot).
    #[persist = "label"]
    label: Arc<RwLock<String>>,

    // ---- LFO A ----
    #[id = "lfo1_rate"]
    pub lfo1_rate: FloatParam,
    #[id = "lfo1_sync"]
    pub lfo1_sync: BoolParam,
    #[id = "lfo1_div"]
    pub lfo1_div: EnumParam<DivParam>,
    #[id = "lfo1_shape"]
    pub lfo1_shape: EnumParam<ShapeParam>,
    #[id = "lfo1_depth"]
    pub lfo1_depth: FloatParam,
    // ---- LFO B ----
    #[id = "lfo2_rate"]
    pub lfo2_rate: FloatParam,
    #[id = "lfo2_sync"]
    pub lfo2_sync: BoolParam,
    #[id = "lfo2_div"]
    pub lfo2_div: EnumParam<DivParam>,
    #[id = "lfo2_shape"]
    pub lfo2_shape: EnumParam<ShapeParam>,
    #[id = "lfo2_depth"]
    pub lfo2_depth: FloatParam,
    // ---- LFO C ----
    #[id = "lfo3_rate"]
    pub lfo3_rate: FloatParam,
    #[id = "lfo3_sync"]
    pub lfo3_sync: BoolParam,
    #[id = "lfo3_div"]
    pub lfo3_div: EnumParam<DivParam>,
    #[id = "lfo3_shape"]
    pub lfo3_shape: EnumParam<ShapeParam>,
    #[id = "lfo3_depth"]
    pub lfo3_depth: FloatParam,
    // ---- LFO D ----
    #[id = "lfo4_rate"]
    pub lfo4_rate: FloatParam,
    #[id = "lfo4_sync"]
    pub lfo4_sync: BoolParam,
    #[id = "lfo4_div"]
    pub lfo4_div: EnumParam<DivParam>,
    #[id = "lfo4_shape"]
    pub lfo4_shape: EnumParam<ShapeParam>,
    #[id = "lfo4_depth"]
    pub lfo4_depth: FloatParam,

    // ---- Macros (bipolar hand controllers, summed into streams 1..4) ----
    #[id = "macro1"]
    pub macro1: FloatParam,
    #[id = "macro2"]
    pub macro2: FloatParam,
    #[id = "macro3"]
    pub macro3: FloatParam,
    #[id = "macro4"]
    pub macro4: FloatParam,

    // ---- Env followers ----
    #[id = "env1_atk"]
    pub env1_atk: FloatParam,
    #[id = "env1_rel"]
    pub env1_rel: FloatParam,
    #[id = "env1_depth"]
    pub env1_depth: FloatParam,
    #[id = "env2_atk"]
    pub env2_atk: FloatParam,
    #[id = "env2_rel"]
    pub env2_rel: FloatParam,
    #[id = "env2_depth"]
    pub env2_depth: FloatParam,

    // ---- Random S&H ----
    #[id = "sh1_rate"]
    pub sh1_rate: FloatParam,
    #[id = "sh1_slew"]
    pub sh1_slew: FloatParam,
    #[id = "sh1_depth"]
    pub sh1_depth: FloatParam,
    #[id = "sh2_rate"]
    pub sh2_rate: FloatParam,
    #[id = "sh2_slew"]
    pub sh2_slew: FloatParam,
    #[id = "sh2_depth"]
    pub sh2_depth: FloatParam,
}

fn rate_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 0.01,
            max: 20.0,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn unit_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn bipolar_param(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
        .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn time_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-1.5),
        },
    )
    .with_unit(" ms")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

impl Default for NerveParams {
    fn default() -> Self {
        let mk_lfo_rate = || rate_param("Rate", 1.0);
        Self {
            editor_state: EguiState::from_size(760, 560),
            label: Arc::new(RwLock::new(String::from("NERVE"))),

            lfo1_rate: mk_lfo_rate(),
            lfo1_sync: BoolParam::new("Sync", false),
            lfo1_div: EnumParam::new("Div", DivParam::Bar1),
            lfo1_shape: EnumParam::new("Shape", ShapeParam::Sine),
            lfo1_depth: unit_param("Depth", 1.0),
            lfo2_rate: mk_lfo_rate(),
            lfo2_sync: BoolParam::new("Sync", false),
            lfo2_div: EnumParam::new("Div", DivParam::Bar1),
            lfo2_shape: EnumParam::new("Shape", ShapeParam::Triangle),
            lfo2_depth: unit_param("Depth", 0.0),
            lfo3_rate: mk_lfo_rate(),
            lfo3_sync: BoolParam::new("Sync", false),
            lfo3_div: EnumParam::new("Div", DivParam::Bar1),
            lfo3_shape: EnumParam::new("Shape", ShapeParam::SawUp),
            lfo3_depth: unit_param("Depth", 0.0),
            lfo4_rate: mk_lfo_rate(),
            lfo4_sync: BoolParam::new("Sync", false),
            lfo4_div: EnumParam::new("Div", DivParam::Bar1),
            lfo4_shape: EnumParam::new("Shape", ShapeParam::Square),
            lfo4_depth: unit_param("Depth", 0.0),

            macro1: bipolar_param("Macro 1"),
            macro2: bipolar_param("Macro 2"),
            macro3: bipolar_param("Macro 3"),
            macro4: bipolar_param("Macro 4"),

            env1_atk: time_param("Env1 Atk", 10.0, 0.1, 200.0),
            env1_rel: time_param("Env1 Rel", 150.0, 5.0, 1000.0),
            env1_depth: unit_param("Env1 Depth", 0.0),
            env2_atk: time_param("Env2 Atk", 5.0, 0.1, 200.0),
            env2_rel: time_param("Env2 Rel", 120.0, 5.0, 1000.0),
            env2_depth: unit_param("Env2 Depth", 0.0),

            sh1_rate: rate_param("S&H1 Rate", 4.0),
            sh1_slew: unit_param("S&H1 Slew", 0.0),
            sh1_depth: unit_param("S&H1 Depth", 0.0),
            sh2_rate: rate_param("S&H2 Rate", 6.0),
            sh2_slew: unit_param("S&H2 Slew", 0.0),
            sh2_depth: unit_param("S&H2 Depth", 0.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct Nerve {
    params: Arc<NerveParams>,
    core: NerveCore,
    /// This instance's bus identity — **session-scoped** (assigned on `initialize`, shared
    /// with the editor). NOT persisted: a persisted per-instance id would make two instances'
    /// CLAP states differ (breaking `state-reproducibility`). Listener routes are therefore
    /// session-live and re-pointed after a reload (see docs/NERVE.md).
    inst_id: Arc<AtomicU64>,
    /// Cached label for the (audio-thread) claim; the GUI updates the live bus label.
    label_cache: String,
    /// Claimed bus slot index (lazily acquired; `None` when the bus is unavailable/full).
    slot: Option<usize>,
    /// Published stream values for the GUI scopes.
    scopes: Arc<Vec<AtomicF32>>,
    factory_presets: Arc<Vec<Preset>>,
}

impl Default for Nerve {
    fn default() -> Self {
        Self {
            params: Arc::new(NerveParams::default()),
            core: NerveCore::new(48_000.0),
            inst_id: Arc::new(AtomicU64::new(0)),
            label_cache: String::from("NERVE"),
            slot: None,
            scopes: Arc::new((0..NUM_MOD_SIGNALS).map(|_| AtomicF32::new(0.0)).collect()),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

impl Nerve {
    #[inline]
    fn build_settings(&self, tempo: f64, beats: f64, playing: bool) -> Settings {
        let p = &self.params;
        let lfo = |rate: &FloatParam,
                   sync: &BoolParam,
                   div: &EnumParam<DivParam>,
                   shape: &EnumParam<ShapeParam>,
                   depth: &FloatParam| dsp::LfoSet {
            rate_hz: rate.value(),
            synced: sync.value(),
            div: div.value().to_dsp(),
            shape: shape.value().to_dsp(),
            depth: depth.value(),
        };
        Settings {
            lfo: [
                lfo(&p.lfo1_rate, &p.lfo1_sync, &p.lfo1_div, &p.lfo1_shape, &p.lfo1_depth),
                lfo(&p.lfo2_rate, &p.lfo2_sync, &p.lfo2_div, &p.lfo2_shape, &p.lfo2_depth),
                lfo(&p.lfo3_rate, &p.lfo3_sync, &p.lfo3_div, &p.lfo3_shape, &p.lfo3_depth),
                lfo(&p.lfo4_rate, &p.lfo4_sync, &p.lfo4_div, &p.lfo4_shape, &p.lfo4_depth),
            ],
            macros: [p.macro1.value(), p.macro2.value(), p.macro3.value(), p.macro4.value()],
            env: [
                dsp::EnvSet {
                    attack_ms: p.env1_atk.value(),
                    release_ms: p.env1_rel.value(),
                    depth: p.env1_depth.value(),
                },
                dsp::EnvSet {
                    attack_ms: p.env2_atk.value(),
                    release_ms: p.env2_rel.value(),
                    depth: p.env2_depth.value(),
                },
            ],
            sh: [
                dsp::ShSet {
                    rate_hz: p.sh1_rate.value(),
                    slew: p.sh1_slew.value(),
                    depth: p.sh1_depth.value(),
                },
                dsp::ShSet {
                    rate_hz: p.sh2_rate.value(),
                    slew: p.sh2_slew.value(),
                    depth: p.sh2_depth.value(),
                },
            ],
            tempo: tempo as f32,
            beats,
            playing,
        }
    }
}

impl Plugin for Nerve {
    const NAME: &'static str = "Qeynos NERVE";
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
        let scopes = self.scopes.clone();
        let presets = self.factory_presets.clone();
        let inst_id = self.inst_id.clone();
        let egui_state = self.params.editor_state.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-nerve-window", Vec2::new(760.0, 560.0))
                    .min_size(Vec2::new(560.0, 420.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        editor_ui(ui, &params, setter, &scopes, &presets, &inst_id);
                    });
                egui_ctx.request_repaint();
            },
        )
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.core.set_sample_rate(buffer_config.sample_rate);
        self.core.reset();
        // Assign the session bus id once (kept stable across re-activation so listener routes
        // don't break mid-session).
        if self.inst_id.load(Ordering::Relaxed) == 0 {
            self.inst_id
                .store(bus::new_instance_id(), Ordering::Relaxed);
        }
        self.label_cache = self
            .params
            .label
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "NERVE".to_string());
        // (Re)claim a fresh slot on (re)activation; a stale prior slot GCs on its own.
        self.slot = None;
        if let Some(b) = bus::bus() {
            self.slot = b.claim(
                self.inst_id.load(Ordering::Relaxed),
                PluginKind::Nerve,
                &self.label_cache,
            );
        }
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let t = context.transport();
        let set = self.build_settings(
            t.tempo.unwrap_or(120.0),
            t.pos_beats().unwrap_or(0.0),
            t.playing,
        );

        let num_samples = buffer.samples();
        // Env followers tap the (mono-summed) input; audio passes through untouched.
        for channel_samples in buffer.iter_samples() {
            let n = channel_samples.len().max(1) as f32;
            let mut mono = 0.0;
            for s in channel_samples {
                mono += *s;
            }
            self.core.feed_input(mono / n, &set);
        }

        let outs = self.core.advance(num_samples, &set);

        // Publish to the bus + heartbeat. Claim lazily if the bus appeared after init.
        if let Some(b) = bus::bus() {
            let id = self.inst_id.load(Ordering::Relaxed);
            if self.slot.is_none() && id != 0 {
                self.slot = b.claim(id, PluginKind::Nerve, &self.label_cache);
            }
            if let Some(idx) = self.slot {
                b.publish_mods(idx, &outs);
                b.beat(idx);
            }
        }

        if self.params.editor_state.is_open() {
            for (a, v) in self.scopes.iter().zip(outs.iter()) {
                a.store(*v, std::sync::atomic::Ordering::Relaxed);
            }
        }

        ProcessStatus::Normal
    }
}

impl Drop for Nerve {
    fn drop(&mut self) {
        if let (Some(idx), Some(b)) = (self.slot, bus::bus()) {
            b.release(idx, self.inst_id.load(Ordering::Relaxed));
        }
    }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

fn editor_ui(
    ui: &mut egui::Ui,
    params: &Arc<NerveParams>,
    setter: &ParamSetter,
    scopes: &Arc<Vec<AtomicF32>>,
    presets: &Arc<Vec<Preset>>,
    inst_id: &Arc<AtomicU64>,
) {
    use suite_core::ui::{labeled_knob, param_widget, ACCENT, TEXT_DIM};

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("QEYNOS · NERVE").color(ACCENT));
        suite_core::ui::manual_button(ui, "nerve", "NERVE", crate::MANUAL_DOC);
        ui.add_space(10.0);
        // Editable bus label — published live to the slot so listeners see a friendly name.
        let mut label = params.label.read().map(|g| g.clone()).unwrap_or_default();
        if ui.text_edit_singleline(&mut label).changed() {
            if let Ok(mut g) = params.label.write() {
                *g = label.clone();
            }
            if let Some(b) = bus::bus() {
                let id = inst_id.load(Ordering::Relaxed);
                if id != 0 {
                    if let Some(idx) = b.resolve_instance(id) {
                        b.set_label(idx, id, &label);
                    }
                }
            }
        }
        let id = inst_id.load(Ordering::Relaxed);
        ui.label(
            egui::RichText::new(format!("bus id #{}", id & 0xFFFF))
                .color(TEXT_DIM)
                .small(),
        );
    });

    suite_core::ui::PresetBar::new("nerve", presets.as_slice()).show(
        ui,
        &**params,
        setter,
        |setter, p| apply_preset(params, setter, p),
    );
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Live scopes for the 8 published streams — housed in the CONSOLE v2 CRT telemetry bay
        // (glass + scanlines when console is on; plain readout in THEME-OFF). Per-stream identity
        // is TEXTUAL (S1·LFO A … S8·S&H B), not colour-coded — every bar already shares one
        // accent — so moving the fill to phosphor amber on the glass loses no meaning. Height is
        // sized for the two-row wrap the min window width produces, so nothing clips. This is a
        // pure visual wrap: the bus publish/claim path in process() is untouched.
        let console = suite_core::ui::console_on(ui.ctx());
        let title_col = if console { suite_core::ui::PHOSPHOR } else { TEXT_DIM };
        let lbl_col = if console { suite_core::ui::PHOSPHOR_DIM } else { TEXT_DIM };
        let fill = if console { suite_core::ui::PHOSPHOR } else { ACCENT };
        suite_core::ui::crt_frame(ui, "nerve-crt", 118.0, |ui| {
            ui.label(egui::RichText::new("STREAMS").color(title_col).monospace().small().strong());
            ui.horizontal_wrapped(|ui| {
                let names = [
                    "S1·LFO A", "S2·LFO B", "S3·LFO C", "S4·LFO D", "S5·Env A", "S6·Env B",
                    "S7·S&H A", "S8·S&H B",
                ];
                for (i, name) in names.iter().enumerate() {
                    let v = scopes[i].load(std::sync::atomic::Ordering::Relaxed);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(*name).color(lbl_col).small());
                        // Bipolar streams map -1..1 -> 0..1; env streams already 0..1.
                        let norm = ((v + 1.0) * 0.5).clamp(0.0, 1.0);
                        ui.add(
                            egui::widgets::ProgressBar::new(norm)
                                .desired_width(70.0)
                                .fill(fill)
                                .text(format!("{v:+.2}")),
                        );
                    });
                }
            });
        });
        ui.separator();

        // LFO A..D.
        let lfos: [(&str, &FloatParam, &BoolParam, &EnumParam<DivParam>, &EnumParam<ShapeParam>, &FloatParam); NUM_LFO] = [
            ("LFO A", &params.lfo1_rate, &params.lfo1_sync, &params.lfo1_div, &params.lfo1_shape, &params.lfo1_depth),
            ("LFO B", &params.lfo2_rate, &params.lfo2_sync, &params.lfo2_div, &params.lfo2_shape, &params.lfo2_depth),
            ("LFO C", &params.lfo3_rate, &params.lfo3_sync, &params.lfo3_div, &params.lfo3_shape, &params.lfo3_depth),
            ("LFO D", &params.lfo4_rate, &params.lfo4_sync, &params.lfo4_div, &params.lfo4_shape, &params.lfo4_depth),
        ];
        for (name, rate, sync, div, shape, depth) in lfos {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name).color(ACCENT).small());
                labeled_knob(ui, "SHAPE", shape, setter);
                labeled_knob(ui, "RATE", rate, setter);
                param_widget(ui, "SYNC", sync, setter);
                labeled_knob(ui, "DIV", div, setter);
                labeled_knob(ui, "DEPTH", depth, setter);
            });
        }
        ui.separator();

        // Macros.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("MACROS").color(ACCENT).small());
            labeled_knob(ui, "M1", &params.macro1, setter);
            labeled_knob(ui, "M2", &params.macro2, setter);
            labeled_knob(ui, "M3", &params.macro3, setter);
            labeled_knob(ui, "M4", &params.macro4, setter);
        });
        ui.separator();

        // Env followers.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ENV A").color(ACCENT).small());
            labeled_knob(ui, "ATK", &params.env1_atk, setter);
            labeled_knob(ui, "REL", &params.env1_rel, setter);
            labeled_knob(ui, "DEPTH", &params.env1_depth, setter);
            ui.add_space(12.0);
            ui.label(egui::RichText::new("ENV B").color(ACCENT).small());
            labeled_knob(ui, "ATK", &params.env2_atk, setter);
            labeled_knob(ui, "REL", &params.env2_rel, setter);
            labeled_knob(ui, "DEPTH", &params.env2_depth, setter);
        });
        ui.separator();

        // S&H.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("S&H A").color(ACCENT).small());
            labeled_knob(ui, "RATE", &params.sh1_rate, setter);
            labeled_knob(ui, "SLEW", &params.sh1_slew, setter);
            labeled_knob(ui, "DEPTH", &params.sh1_depth, setter);
            ui.add_space(12.0);
            ui.label(egui::RichText::new("S&H B").color(ACCENT).small());
            labeled_knob(ui, "RATE", &params.sh2_rate, setter);
            labeled_knob(ui, "SLEW", &params.sh2_slew, setter);
            labeled_knob(ui, "DEPTH", &params.sh2_depth, setter);
        });

        let _ = (NUM_ENV, NUM_SH, NUM_MACRO); // documented arities
    });
}

impl ClapPlugin for Nerve {
    const CLAP_ID: &'static str = "com.qeynos.nerve";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Qeynos NERVE — suite modulation bus source (8 streams to the tier-2 bus)");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Nerve {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosNerve00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(Nerve);
nih_export_vst3!(Nerve);

#[cfg(test)]
mod tests;
