//! OVERSEER — mastering system (Qeynos suite, Phase 1). ONE library exporting TWO
//! plugins (PRD §3 tier 1):
//!
//! - **Qeynos OVERSEER Node** — a per-track channel strip: input meter → 4-band EQ →
//!   feed-forward compressor (RMS, soft knee) → tanh saturation → M/S width → trim →
//!   output meter. Each instance registers a slot on the same-DLL [`bus`], publishing
//!   its label, meters (peak/RMS/LUFS-M), and a mirror of its key params.
//! - **Qeynos OVERSEER Master** — the bus processor: 4-band EQ → 3-band multiband
//!   compressor (LR4 splits on TPT SVFs) → 2 ms lookahead limiter (4x-oversampled
//!   true-peak-approximate metering, reported latency) → BS.1770 LUFS meter
//!   (`suite_core::loudness`: momentary/short/integrated with gating + reset). Its GUI
//!   shows a live grid of every Node slot and can write param **overrides** into them;
//!   Node GUIs badge held params, and a local touch steals control back (write-wins
//!   timestamps, block granularity, atomics only — no locks across `process`).
//!
//! Both plugins are exported from this one cdylib via `nih_export_clap!`/`nih_export_vst3!`
//! (multi-plugin factories, verified in the pinned nih-plug rev), producing a single
//! `overseer.clap`/`.vst3` bundle containing both. Caveat (PRD §3): FL loads same-bitness
//! plugins in-process by default; a user-ticked "Make bridged" would isolate the two into
//! separate processes and break the link (tier-2 shared-memory bus is the planned fallback).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

pub mod bus;
pub mod dynamics;
pub mod enrich;
pub mod eq;
pub mod master;
pub mod node;
pub mod presets;

use bus::{Slot, NUM_OVERRIDES, OVR_DRIVE, OVR_NAMES, OVR_RATIO, OVR_THRESHOLD, OVR_TRIM, OVR_WIDTH};
use enrich::{
    apply_assist, context_defaults, suggest_from_features, theme_assist_targets, type_bank_category,
    LearnPersist, TypeParam,
};
use eq::EqSettings;
use master::{BandComp, MasterCore, MasterMeters, MasterSettings};
use node::{load_f32, NodeCore, NodeMeters, NodeSettings};
use suite_core::classify::{classify, infer_theme, InstrumentType, MixAnalysis, NodeReport, SessionTheme};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
/// One doc covers both editors (Node + Master); each editor's '?' opens it with its own slug.
pub const MANUAL_DOC: &str = include_str!("../../../docs/OVERSEER.md");

// ---------------------------------------------------------------------------
// Shared param constructors
// ---------------------------------------------------------------------------

fn hz_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 20.0,
            max: 20_000.0,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

fn gain_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: -24.0, max: 24.0 })
        .with_unit(" dB")
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn q_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 0.1,
            max: 10.0,
            factor: FloatRange::skew_factor(-1.0),
        },
    )
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn threshold_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: -60.0, max: 0.0 })
        .with_unit(" dB")
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn ratio_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 1.0,
            max: 20.0,
            factor: FloatRange::skew_factor(-1.2),
        },
    )
    .with_unit(":1")
    .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn ms_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
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

fn mix_param() -> FloatParam {
    FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn set_f(setter: &ParamSetter, param: &FloatParam, v: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, v);
    setter.end_set_parameter(param);
}

fn fmt_db(v: f32) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "-inf".to_string()
    }
}

// ===========================================================================
// NODE
// ===========================================================================

#[derive(Params)]
pub struct NodeParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    /// Instance label ("KICK" etc.) — persisted host state, not an automatable param.
    #[persist = "label"]
    pub label: RwLock<String>,

    /// OVERSEER-ENRICH: instrument type. `Auto` (default) follows the classifier; a concrete
    /// type pins it and applies context-tuned defaults.
    #[id = "insttype"]
    pub inst_type: EnumParam<TypeParam>,

    /// OVERSEER-ENRICH: persisted LEARN state (locked type + ghost suggestions).
    #[persist = "learn"]
    pub learn: RwLock<LearnPersist>,

    #[id = "lowfreq"]
    pub low_freq: FloatParam,
    #[id = "lowgain"]
    pub low_gain: FloatParam,
    #[id = "b1freq"]
    pub b1_freq: FloatParam,
    #[id = "b1gain"]
    pub b1_gain: FloatParam,
    #[id = "b1q"]
    pub b1_q: FloatParam,
    #[id = "b2freq"]
    pub b2_freq: FloatParam,
    #[id = "b2gain"]
    pub b2_gain: FloatParam,
    #[id = "b2q"]
    pub b2_q: FloatParam,
    #[id = "hifreq"]
    pub high_freq: FloatParam,
    #[id = "higain"]
    pub high_gain: FloatParam,

    #[id = "thresh"]
    pub threshold: FloatParam,
    #[id = "ratio"]
    pub ratio: FloatParam,
    #[id = "knee"]
    pub knee: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "makeup"]
    pub makeup: FloatParam,

    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "trim"]
    pub trim: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for NodeParams {
    fn default() -> Self {
        let d = NodeSettings::default();
        Self {
            editor_state: EguiState::from_size(560, 700),
            label: RwLock::new("NODE".to_string()),
            inst_type: EnumParam::new("Instrument Type", TypeParam::Auto),
            learn: RwLock::new(LearnPersist::default()),

            low_freq: hz_param("Low Freq", d.eq.low_freq),
            low_gain: gain_param("Low Gain", d.eq.low_gain),
            b1_freq: hz_param("Bell 1 Freq", d.eq.b1_freq),
            b1_gain: gain_param("Bell 1 Gain", d.eq.b1_gain),
            b1_q: q_param("Bell 1 Q", d.eq.b1_q),
            b2_freq: hz_param("Bell 2 Freq", d.eq.b2_freq),
            b2_gain: gain_param("Bell 2 Gain", d.eq.b2_gain),
            b2_q: q_param("Bell 2 Q", d.eq.b2_q),
            high_freq: hz_param("High Freq", d.eq.high_freq),
            high_gain: gain_param("High Gain", d.eq.high_gain),

            threshold: threshold_param("Threshold", d.comp_threshold),
            ratio: ratio_param("Ratio", d.comp_ratio),
            knee: FloatParam::new("Knee", d.comp_knee, FloatRange::Linear { min: 0.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            attack: ms_param("Attack", d.comp_attack, 0.1, 100.0),
            release: ms_param("Release", d.comp_release, 10.0, 1000.0),
            makeup: gain_param("Makeup", d.comp_makeup),

            drive: FloatParam::new("Drive", d.drive_db, FloatRange::Linear { min: 0.0, max: 24.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            width: FloatParam::new("Width", d.width, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            trim: gain_param("Trim", d.trim_db),
            mix: mix_param(),
        }
    }
}

impl NodeParams {
    fn snapshot(&self) -> NodeSettings {
        NodeSettings {
            eq: EqSettings {
                low_freq: self.low_freq.value(),
                low_gain: self.low_gain.value(),
                b1_freq: self.b1_freq.value(),
                b1_gain: self.b1_gain.value(),
                b1_q: self.b1_q.value(),
                b2_freq: self.b2_freq.value(),
                b2_gain: self.b2_gain.value(),
                b2_q: self.b2_q.value(),
                high_freq: self.high_freq.value(),
                high_gain: self.high_gain.value(),
            },
            comp_threshold: self.threshold.value(),
            comp_ratio: self.ratio.value(),
            comp_knee: self.knee.value(),
            comp_attack: self.attack.value(),
            comp_release: self.release.value(),
            comp_makeup: self.makeup.value(),
            drive_db: self.drive.value(),
            width: self.width.value(),
            trim_db: self.trim.value(),
            mix: self.mix.value(),
        }
    }
}

fn apply_node_preset(params: &NodeParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::node_settings_from_preset(p);
    apply_node_settings(params, setter, &s);
}

/// Apply a [`NodeSettings`] to the live params through the host (shared by factory presets
/// and OVERSEER-ENRICH context defaults).
fn apply_node_settings(params: &NodeParams, setter: &ParamSetter, s: &NodeSettings) {
    set_f(setter, &params.low_freq, s.eq.low_freq);
    set_f(setter, &params.low_gain, s.eq.low_gain);
    set_f(setter, &params.b1_freq, s.eq.b1_freq);
    set_f(setter, &params.b1_gain, s.eq.b1_gain);
    set_f(setter, &params.b1_q, s.eq.b1_q);
    set_f(setter, &params.b2_freq, s.eq.b2_freq);
    set_f(setter, &params.b2_gain, s.eq.b2_gain);
    set_f(setter, &params.b2_q, s.eq.b2_q);
    set_f(setter, &params.high_freq, s.eq.high_freq);
    set_f(setter, &params.high_gain, s.eq.high_gain);
    set_f(setter, &params.threshold, s.comp_threshold);
    set_f(setter, &params.ratio, s.comp_ratio);
    set_f(setter, &params.knee, s.comp_knee);
    set_f(setter, &params.attack, s.comp_attack);
    set_f(setter, &params.release, s.comp_release);
    set_f(setter, &params.makeup, s.comp_makeup);
    set_f(setter, &params.drive, s.drive_db);
    set_f(setter, &params.width, s.width);
    set_f(setter, &params.trim, s.trim_db);
    set_f(setter, &params.mix, s.mix);
}

/// OVERSEER-ENRICH Node UI: instrument-type dropdown + guessed-type badge, LEARN button
/// (progress + commit → lock + ghost suggestions), and the APPLY-suggestion row. Returns the
/// current *effective* instrument type so the caller can filter the preset bank by it.
fn node_enrich_ui(
    ui: &mut egui::Ui,
    params: &NodeParams,
    setter: &ParamSetter,
    slot: &Arc<Slot>,
    meters: &Arc<NodeMeters>,
) -> InstrumentType {
    use suite_core::ui::{ACCENT, TEXT_DIM};

    let feats = slot.features();
    let (auto_ty, auto_conf) = classify(&feats);
    let pinned = params.inst_type.value().to_instrument();
    let learned = if slot.learn_locked() {
        Some(slot.learned_type())
    } else {
        None
    };
    let (eff_ty, eff_conf, src) = if let Some(t) = pinned {
        (t, 1.0, "PINNED")
    } else if let Some(t) = learned {
        (t, 1.0, "LEARNED")
    } else {
        (auto_ty, auto_conf, "AUTO")
    };

    // Poll for a finished LEARN capture (the audio thread bumps learn_gen on commit).
    let gen_id = egui::Id::new(("ov-node-learngen", slot.id));
    let cur_gen = slot.learn_gen();
    let last_gen: Option<u32> = ui.ctx().memory(|m| m.data.get_temp(gen_id));
    if let Some(lg) = last_gen {
        if lg != cur_gen {
            let cap = slot.learn_result();
            let (lty, _lc) = classify(&cap);
            let commit_ty = if lty == InstrumentType::Generic {
                auto_ty
            } else {
                lty
            };
            slot.set_learn_lock(Some(commit_ty));
            let sug = suggest_from_features(&cap);
            if let Ok(mut lp) = params.learn.write() {
                lp.locked = true;
                lp.ty = commit_ty.index();
                lp.suggestion = Some(sug);
            }
            apply_node_settings(params, setter, &context_defaults(commit_ty));
        }
    }
    ui.ctx().memory_mut(|m| m.data.insert_temp(gen_id, cur_gen));

    // Type dropdown + guessed-type badge.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("TYPE").color(TEXT_DIM).small());
        let cur = params.inst_type.value();
        egui::ComboBox::from_id_salt(("node-type", slot.id))
            .selected_text(format!("{cur:?}"))
            .width(90.0)
            .show_ui(ui, |ui| {
                for (i, name) in TypeParam::variants().iter().enumerate() {
                    let variant = TypeParam::from_index(i);
                    if ui.selectable_label(cur == variant, *name).clicked() {
                        setter.begin_set_parameter(&params.inst_type);
                        setter.set_parameter(&params.inst_type, variant);
                        setter.end_set_parameter(&params.inst_type);
                        if let Some(t) = variant.to_instrument() {
                            apply_node_settings(params, setter, &context_defaults(t));
                        }
                    }
                }
            });
        let (cr, cg, cb) = eff_ty.color_rgb();
        ui.label(
            egui::RichText::new(format!(" {} ", eff_ty.label()))
                .background_color(egui::Color32::from_rgb(cr, cg, cb))
                .color(egui::Color32::BLACK)
                .small(),
        );
        ui.label(egui::RichText::new(src).color(TEXT_DIM).small());
        if src == "AUTO" {
            ui.label(
                egui::RichText::new(format!("{:.0}%", eff_conf * 100.0))
                    .color(ACCENT)
                    .small(),
            );
        }
    });

    // LEARN button + progress.
    ui.horizontal(|ui| {
        if slot.capturing() {
            let p = slot.capture_prog();
            ui.add(
                egui::ProgressBar::new(p)
                    .desired_width(150.0)
                    .text(format!("LEARNING {:.0}%", p * 100.0)),
            );
        } else if ui
            .button("LEARN")
            .on_hover_text("Play the most representative ~8 s; commits type + suggestions")
            .clicked()
        {
            let sr = load_f32(&meters.sr).max(1.0);
            slot.request_learn((8.0 * sr) as usize);
        }
        if slot.learn_locked() && ui.button("Clear Learn").clicked() {
            slot.set_learn_lock(None);
            if let Ok(mut lp) = params.learn.write() {
                lp.locked = false;
                lp.suggestion = None;
            }
        }
    });

    // Ghost suggestions + APPLY.
    let sug = params.learn.read().ok().and_then(|lp| lp.suggestion);
    if let Some(s) = sug {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SUGGEST").color(TEXT_DIM).small());
            ui.label(
                egui::RichText::new(format!(
                    "Low {:+.1} dB · Thresh {:.1} dB · Ratio {:.1}:1",
                    s.low_gain, s.threshold, s.ratio
                ))
                .color(ACCENT)
                .small(),
            );
            if ui
                .button("APPLY")
                .on_hover_text("Apply the ghost suggestions to the strip")
                .clicked()
            {
                set_f(setter, &params.low_gain, s.low_gain);
                set_f(setter, &params.threshold, s.threshold);
                set_f(setter, &params.ratio, s.ratio);
            }
        });
    }

    eff_ty
}

pub struct OverseerNode {
    params: Arc<NodeParams>,
    slot: Arc<Slot>,
    meters: Arc<NodeMeters>,
    core: NodeCore,
    factory_presets: Arc<Vec<Preset>>,
    /// Last-seen local values of the 5 overridable params — a change means the user/host
    /// touched them locally, which steals control back from a Master override.
    last_local: [f32; NUM_OVERRIDES],
}

impl Default for OverseerNode {
    fn default() -> Self {
        let params = Arc::new(NodeParams::default());
        let label = params.label.read().map(|s| s.clone()).unwrap_or_default();
        let slot = bus::bus().register(&label);
        let meters = Arc::new(NodeMeters::default());
        let core = NodeCore::new(48_000.0, slot.clone(), meters.clone());
        Self {
            params,
            slot,
            meters,
            core,
            factory_presets: Arc::new(load_all(presets::NODE_PRESET_JSON)),
            last_local: [f32::NAN; NUM_OVERRIDES],
        }
    }
}

impl Plugin for OverseerNode {
    const NAME: &'static str = "Qeynos OVERSEER Node";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        names: PortNames {
            layout: Some("Stereo"),
            ..PortNames::const_default()
        },
        ..AudioIOLayout::const_default()
    }];

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
        let slot = self.slot.clone();
        let meters = self.meters.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-overseer-node-window", Vec2::new(560.0, 620.0))
                    .min_size(Vec2::new(500.0, 520.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.heading(
                                egui::RichText::new("QEYNOS · OVERSEER NODE")
                                    .color(suite_core::ui::ACCENT),
                            );
                            suite_core::ui::manual_button(ui, "overseer-node", "OVERSEER NODE", MANUAL_DOC);
                            if slot.override_held() {
                                ui.label(
                                    egui::RichText::new(" MASTER OVERRIDE ")
                                        .background_color(suite_core::ui::ACCENT)
                                        .color(egui::Color32::BLACK)
                                        .small(),
                                );
                            }
                        });
                        ui.label(
                            egui::RichText::new("channel strip on the overseer bus")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("LABEL")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            let mut label =
                                params.label.read().map(|s| s.clone()).unwrap_or_default();
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut label).desired_width(120.0),
                            );
                            if resp.changed() {
                                if let Ok(mut g) = params.label.write() {
                                    *g = label.clone();
                                }
                                slot.set_label(&label);
                            }
                        });

                        // OVERSEER-ENRICH: instrument type, LEARN, ghost suggestions.
                        let eff_ty = node_enrich_ui(ui, &params, setter, &slot, &meters);

                        // Preset bar: factory + user presets, filtered by the current type's
                        // bank; save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("overseer-node", presets.as_slice())
                            .filter(Some(type_bank_category(eff_ty)))
                            .show(ui, &*params, setter, |setter, p| {
                                apply_node_preset(&params, setter, p)
                            });

                        // Meters row.
                        ui.horizontal(|ui| {
                            let inp = load_f32(&meters.in_peak);
                            let outp = load_f32(&meters.out_peak);
                            let lufs = load_f32(&meters.lufs_m);
                            let gr = load_f32(&meters.gr_db);
                            ui.label(
                                egui::RichText::new(format!(
                                    "IN {} dB   OUT {} dB   LUFS-M {}   GR {} dB",
                                    fmt_db(inp),
                                    fmt_db(outp),
                                    fmt_db(lufs),
                                    fmt_db(gr)
                                ))
                                .color(suite_core::ui::ACCENT)
                                .small(),
                            );
                        });
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let held = |i: usize| slot.is_override_active(i) && slot.override_held();
                            let badge = |ui: &mut egui::Ui, on: bool| {
                                if on {
                                    ui.label(
                                        egui::RichText::new("OVR")
                                            .background_color(suite_core::ui::ACCENT)
                                            .color(egui::Color32::BLACK)
                                            .small(),
                                    );
                                }
                            };

                            ui.label(
                                egui::RichText::new("EQ — low shelf · 2 bells · high shelf")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("node-eq")
                                .num_columns(3)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "LOW FREQ", &params.low_freq, setter);
                                    row(ui, "LOW GAIN", &params.low_gain, setter);
                                    ui.end_row();
                                    row(ui, "BELL1 FREQ", &params.b1_freq, setter);
                                    row(ui, "BELL1 GAIN", &params.b1_gain, setter);
                                    row(ui, "BELL1 Q", &params.b1_q, setter);
                                    ui.end_row();
                                    row(ui, "BELL2 FREQ", &params.b2_freq, setter);
                                    row(ui, "BELL2 GAIN", &params.b2_gain, setter);
                                    row(ui, "BELL2 Q", &params.b2_q, setter);
                                    ui.end_row();
                                    row(ui, "HIGH FREQ", &params.high_freq, setter);
                                    row(ui, "HIGH GAIN", &params.high_gain, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("COMPRESSOR — RMS · soft knee")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("node-comp")
                                .num_columns(3)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "THRESHOLD", &params.threshold, setter);
                                    badge(ui, held(OVR_THRESHOLD));
                                    ui.end_row();
                                    row(ui, "RATIO", &params.ratio, setter);
                                    badge(ui, held(OVR_RATIO));
                                    ui.end_row();
                                    row(ui, "KNEE", &params.knee, setter);
                                    row(ui, "MAKEUP", &params.makeup, setter);
                                    ui.end_row();
                                    row(ui, "ATTACK", &params.attack, setter);
                                    row(ui, "RELEASE", &params.release, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("COLOR / OUTPUT")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("node-out")
                                .num_columns(3)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "DRIVE", &params.drive, setter);
                                    badge(ui, held(OVR_DRIVE));
                                    ui.end_row();
                                    row(ui, "WIDTH", &params.width, setter);
                                    badge(ui, held(OVR_WIDTH));
                                    ui.end_row();
                                    row(ui, "TRIM", &params.trim, setter);
                                    badge(ui, held(OVR_TRIM));
                                    ui.end_row();
                                    row(ui, "MIX", &params.mix, setter);
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
        self.core = NodeCore::new(buffer_config.sample_rate, self.slot.clone(), self.meters.clone());
        // Report the saturation oversampler group delay the dry path is compensated by.
        context.set_latency_samples(self.core.latency_samples());
        // Persisted label may have been restored after `Default` — sync it to the slot.
        if let Ok(label) = self.params.label.read() {
            self.slot.set_label(&label);
        }
        // Restore a persisted LEARN lock into the shared slot (OVERSEER-ENRICH).
        if let Ok(lp) = self.params.learn.read() {
            if lp.locked {
                self.slot
                    .set_learn_lock(Some(InstrumentType::from_index(lp.ty)));
            } else {
                self.slot.set_learn_lock(None);
            }
        }
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
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let s = self.params.snapshot();

        // OVERSEER-ENRICH: publish the effective type override to the bus (pinned param wins,
        // then a LEARN lock, else AUTO → the Master/GUI classify from features).
        let ty_override = match self.params.inst_type.value().to_instrument() {
            Some(t) => Some(t),
            None if self.slot.learn_locked() => Some(self.slot.learned_type()),
            None => None,
        };
        self.slot.set_override_type(ty_override);

        // Local-touch detection: if any of the 5 overridable local params moved since the
        // last block (GUI drag or host automation), steal control back from the Master.
        let local = [
            s.comp_threshold,
            s.comp_ratio,
            s.drive_db,
            s.width,
            s.trim_db,
        ];
        let mut touched = false;
        for i in 0..NUM_OVERRIDES {
            if self.last_local[i].is_finite() && (local[i] - self.last_local[i]).abs() > 1.0e-6 {
                touched = true;
            }
        }
        self.last_local = local;
        if touched {
            self.slot.note_local_touch();
        }

        self.core.configure(&s);
        let main = buffer.as_slice();
        if main.len() >= 2 {
            let (l, r) = main.split_at_mut(1);
            self.core.process_block(l[0], r[0]);
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for OverseerNode {
    const CLAP_ID: &'static str = "com.qeynos.overseer.node";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Channel strip on the OVERSEER bus — EQ, compressor, saturation, width; remote-controllable from OVERSEER Master");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mixing,
    ];
}

impl Vst3Plugin for OverseerNode {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosOVERSEERnd";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

// ===========================================================================
// MASTER
// ===========================================================================

#[derive(Params)]
pub struct MasterParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// OVERSEER-ENRICH: assist strength (0 = display only, default 30%). Scales theme-derived
    /// nudges to the master EQ tilt / MB comp character / limiter drive.
    #[id = "assist"]
    pub assist: FloatParam,
    /// OVERSEER-ENRICH: SUGGEST-ONLY — keep the theme advisory (no audio nudges).
    #[id = "suggestonly"]
    pub suggest_only: BoolParam,
    /// OVERSEER-ENRICH: persisted theme lock (a LEARN locks the inferred theme).
    #[persist = "theme"]
    pub theme_lock: RwLock<enrich::ThemeLock>,

    #[id = "lowfreq"]
    pub low_freq: FloatParam,
    #[id = "lowgain"]
    pub low_gain: FloatParam,
    #[id = "b1freq"]
    pub b1_freq: FloatParam,
    #[id = "b1gain"]
    pub b1_gain: FloatParam,
    #[id = "b1q"]
    pub b1_q: FloatParam,
    #[id = "b2freq"]
    pub b2_freq: FloatParam,
    #[id = "b2gain"]
    pub b2_gain: FloatParam,
    #[id = "b2q"]
    pub b2_q: FloatParam,
    #[id = "hifreq"]
    pub high_freq: FloatParam,
    #[id = "higain"]
    pub high_gain: FloatParam,

    #[id = "xolow"]
    pub xo_low: FloatParam,
    #[id = "xohigh"]
    pub xo_high: FloatParam,

    #[id = "c1thresh"]
    pub c1_threshold: FloatParam,
    #[id = "c1ratio"]
    pub c1_ratio: FloatParam,
    #[id = "c1makeup"]
    pub c1_makeup: FloatParam,
    #[id = "c2thresh"]
    pub c2_threshold: FloatParam,
    #[id = "c2ratio"]
    pub c2_ratio: FloatParam,
    #[id = "c2makeup"]
    pub c2_makeup: FloatParam,
    #[id = "c3thresh"]
    pub c3_threshold: FloatParam,
    #[id = "c3ratio"]
    pub c3_ratio: FloatParam,
    #[id = "c3makeup"]
    pub c3_makeup: FloatParam,

    #[id = "knee"]
    pub knee: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,

    #[id = "ceiling"]
    pub ceiling: FloatParam,
    #[id = "limrel"]
    pub lim_release: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for MasterParams {
    fn default() -> Self {
        let d = MasterSettings::default();
        Self {
            editor_state: EguiState::from_size(760, 760),

            assist: FloatParam::new("Assist", 0.30, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            suggest_only: BoolParam::new("Suggest Only", false),
            theme_lock: RwLock::new(enrich::ThemeLock::default()),

            low_freq: hz_param("Low Freq", d.eq.low_freq),
            low_gain: gain_param("Low Gain", d.eq.low_gain),
            b1_freq: hz_param("Bell 1 Freq", d.eq.b1_freq),
            b1_gain: gain_param("Bell 1 Gain", d.eq.b1_gain),
            b1_q: q_param("Bell 1 Q", d.eq.b1_q),
            b2_freq: hz_param("Bell 2 Freq", d.eq.b2_freq),
            b2_gain: gain_param("Bell 2 Gain", d.eq.b2_gain),
            b2_q: q_param("Bell 2 Q", d.eq.b2_q),
            high_freq: hz_param("High Freq", d.eq.high_freq),
            high_gain: gain_param("High Gain", d.eq.high_gain),

            xo_low: hz_param("XO Low", d.xo_low),
            xo_high: hz_param("XO High", d.xo_high),

            c1_threshold: threshold_param("Low Threshold", d.bands[0].threshold),
            c1_ratio: ratio_param("Low Ratio", d.bands[0].ratio),
            c1_makeup: gain_param("Low Makeup", d.bands[0].makeup),
            c2_threshold: threshold_param("Mid Threshold", d.bands[1].threshold),
            c2_ratio: ratio_param("Mid Ratio", d.bands[1].ratio),
            c2_makeup: gain_param("Mid Makeup", d.bands[1].makeup),
            c3_threshold: threshold_param("High Threshold", d.bands[2].threshold),
            c3_ratio: ratio_param("High Ratio", d.bands[2].ratio),
            c3_makeup: gain_param("High Makeup", d.bands[2].makeup),

            knee: FloatParam::new("Knee", d.comp_knee, FloatRange::Linear { min: 0.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            attack: ms_param("Attack", d.comp_attack, 0.1, 100.0),
            release: ms_param("Release", d.comp_release, 10.0, 1000.0),

            ceiling: FloatParam::new(
                "Ceiling",
                d.ceiling_db,
                FloatRange::Linear { min: -12.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            lim_release: ms_param("Lim Release", d.limiter_release, 10.0, 1000.0),
            mix: mix_param(),
        }
    }
}

impl MasterParams {
    fn snapshot(&self) -> MasterSettings {
        MasterSettings {
            eq: EqSettings {
                low_freq: self.low_freq.value(),
                low_gain: self.low_gain.value(),
                b1_freq: self.b1_freq.value(),
                b1_gain: self.b1_gain.value(),
                b1_q: self.b1_q.value(),
                b2_freq: self.b2_freq.value(),
                b2_gain: self.b2_gain.value(),
                b2_q: self.b2_q.value(),
                high_freq: self.high_freq.value(),
                high_gain: self.high_gain.value(),
            },
            xo_low: self.xo_low.value(),
            xo_high: self.xo_high.value(),
            bands: [
                BandComp {
                    threshold: self.c1_threshold.value(),
                    ratio: self.c1_ratio.value(),
                    makeup: self.c1_makeup.value(),
                },
                BandComp {
                    threshold: self.c2_threshold.value(),
                    ratio: self.c2_ratio.value(),
                    makeup: self.c2_makeup.value(),
                },
                BandComp {
                    threshold: self.c3_threshold.value(),
                    ratio: self.c3_ratio.value(),
                    makeup: self.c3_makeup.value(),
                },
            ],
            comp_knee: self.knee.value(),
            comp_attack: self.attack.value(),
            comp_release: self.release.value(),
            ceiling_db: self.ceiling.value(),
            limiter_release: self.lim_release.value(),
            mix: self.mix.value(),
        }
    }
}

fn apply_master_preset(params: &MasterParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::master_settings_from_preset(p);
    set_f(setter, &params.low_freq, s.eq.low_freq);
    set_f(setter, &params.low_gain, s.eq.low_gain);
    set_f(setter, &params.b1_freq, s.eq.b1_freq);
    set_f(setter, &params.b1_gain, s.eq.b1_gain);
    set_f(setter, &params.b1_q, s.eq.b1_q);
    set_f(setter, &params.b2_freq, s.eq.b2_freq);
    set_f(setter, &params.b2_gain, s.eq.b2_gain);
    set_f(setter, &params.b2_q, s.eq.b2_q);
    set_f(setter, &params.high_freq, s.eq.high_freq);
    set_f(setter, &params.high_gain, s.eq.high_gain);
    set_f(setter, &params.xo_low, s.xo_low);
    set_f(setter, &params.xo_high, s.xo_high);
    set_f(setter, &params.c1_threshold, s.bands[0].threshold);
    set_f(setter, &params.c1_ratio, s.bands[0].ratio);
    set_f(setter, &params.c1_makeup, s.bands[0].makeup);
    set_f(setter, &params.c2_threshold, s.bands[1].threshold);
    set_f(setter, &params.c2_ratio, s.bands[1].ratio);
    set_f(setter, &params.c2_makeup, s.bands[1].makeup);
    set_f(setter, &params.c3_threshold, s.bands[2].threshold);
    set_f(setter, &params.c3_ratio, s.bands[2].ratio);
    set_f(setter, &params.c3_makeup, s.bands[2].makeup);
    set_f(setter, &params.knee, s.comp_knee);
    set_f(setter, &params.attack, s.comp_attack);
    set_f(setter, &params.release, s.comp_release);
    set_f(setter, &params.ceiling, s.ceiling_db);
    set_f(setter, &params.lim_release, s.limiter_release);
    set_f(setter, &params.mix, s.mix);
}

/// OVERSEER-ENRICH Master UI: aggregate live Node reports + the master mix analysis into a
/// THEME, publish it + the assist targets for the audio thread, and draw the theme readout,
/// ASSIST knob, SUGGEST-ONLY toggle, LEARN button and summary card. Returns the active theme
/// so the caller can filter the Master preset bank by it.
fn master_enrich_ui(
    ui: &mut egui::Ui,
    params: &MasterParams,
    setter: &ParamSetter,
    shared: &Arc<master::MasterShared>,
) -> SessionTheme {
    use suite_core::ui::{labeled_slider, ACCENT, TEXT_DIM};

    // Gather the live Node reports (GUI thread → locking + allocation are fine here).
    let slots = bus::bus().live_slots();
    let mut reports: Vec<NodeReport> = Vec::with_capacity(slots.len());
    for s in slots.iter() {
        let (ty, _c) = s.resolved_type();
        reports.push(NodeReport {
            ty,
            features: s.features(),
        });
    }
    let mfeat = shared.features();
    let onset_density = reports
        .iter()
        .map(|r| r.features.onset_rate)
        .fold(0.0f32, |a, v| a + v)
        .max(mfeat.onset_rate);
    let mix = MixAnalysis {
        tempo_bpm: shared.tempo(),
        tilt: mfeat.tilt,
        onset_density,
        dynamic_range_db: 20.0 * mfeat.crest.max(1.0).log10(),
    };

    let locked = params
        .theme_lock
        .read()
        .ok()
        .filter(|t| t.locked)
        .map(|t| SessionTheme::from_index(t.theme));
    let (theme, conf) = match locked {
        Some(t) => (t, 1.0),
        None => infer_theme(&reports, &mix),
    };
    // Publish theme + assist targets for the audio thread.
    shared.set_theme(theme, conf);
    shared.set_assist(&theme_assist_targets(theme));

    // Poll a finished Master LEARN → lock the theme.
    let gid = egui::Id::new("ov-master-learngen");
    let cur_gen = shared.learn_gen();
    let last: Option<u32> = ui.ctx().memory(|m| m.data.get_temp(gid));
    if let Some(lg) = last {
        if lg != cur_gen {
            if let Ok(mut tl) = params.theme_lock.write() {
                tl.locked = true;
                tl.theme = theme.index();
            }
        }
    }
    ui.ctx().memory_mut(|m| m.data.insert_temp(gid, cur_gen));

    // Theme readout row.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("THEME").color(TEXT_DIM).small());
        ui.label(egui::RichText::new(theme.label()).color(ACCENT).strong());
        ui.label(
            egui::RichText::new(format!("{:.0}%", conf * 100.0))
                .color(TEXT_DIM)
                .small(),
        );
        if locked.is_some() {
            ui.label(
                egui::RichText::new(" LOCKED ")
                    .background_color(ACCENT)
                    .color(egui::Color32::BLACK)
                    .small(),
            );
            if ui.button("Clear").clicked() {
                if let Ok(mut tl) = params.theme_lock.write() {
                    tl.locked = false;
                }
            }
        }
    });

    // Assist controls.
    ui.horizontal(|ui| {
        labeled_slider(ui, "ASSIST", &params.assist, setter);
        let mut so = params.suggest_only.value();
        if ui.checkbox(&mut so, "SUGGEST ONLY").changed() {
            setter.begin_set_parameter(&params.suggest_only);
            setter.set_parameter(&params.suggest_only, so);
            setter.end_set_parameter(&params.suggest_only);
        }
        if shared.capturing() {
            let p = shared.capture_prog();
            ui.add(
                egui::ProgressBar::new(p)
                    .desired_width(140.0)
                    .text(format!("LEARNING {:.0}%", p * 100.0)),
            );
        } else if ui
            .button("LEARN THEME")
            .on_hover_text("Play the fullest ~12 s; locks the theme + assist targets")
            .clicked()
        {
            let sr = shared.sr().max(1.0);
            shared.request_learn((12.0 * sr) as usize);
        }
    });

    // Summary card: theme, per-track types, the assist moves.
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(
            egui::RichText::new(format!("SESSION: {}", theme.label()))
                .color(ACCENT)
                .small(),
        );
        if reports.is_empty() {
            ui.label(
                egui::RichText::new("no live nodes")
                    .color(TEXT_DIM)
                    .small(),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                for (s, rep) in slots.iter().zip(reports.iter()) {
                    let (cr, cg, cb) = rep.ty.color_rgb();
                    ui.label(
                        egui::RichText::new(format!(" {}·{} ", s.label(), rep.ty.label()))
                            .background_color(egui::Color32::from_rgb(cr, cg, cb))
                            .color(egui::Color32::BLACK)
                            .small(),
                    );
                }
            });
        }
        let t = theme_assist_targets(theme);
        ui.label(
            egui::RichText::new(format!(
                "moves: EQ tilt {:+.1} dB · low {:+.1} dB · comp {} · lim drive {:+.1} dB",
                t.eq_tilt_db,
                t.eq_low_db,
                if t.comp_character > 0.0 {
                    "glue"
                } else if t.comp_character < 0.0 {
                    "punch"
                } else {
                    "flat"
                },
                t.limiter_drive_db,
            ))
            .color(TEXT_DIM)
            .small(),
        );
    });

    theme
}

pub struct OverseerMaster {
    params: Arc<MasterParams>,
    meters: Arc<MasterMeters>,
    core: MasterCore,
    factory_presets: Arc<Vec<Preset>>,
    /// GUI → audio: reset the integrated-LUFS meter on the next block.
    lufs_reset: Arc<AtomicBool>,
    /// OVERSEER-ENRICH: theme/assist state shared between the core and the editor.
    shared: Arc<master::MasterShared>,
}

impl Default for OverseerMaster {
    fn default() -> Self {
        let meters = Arc::new(MasterMeters::default());
        let shared = Arc::new(master::MasterShared::default());
        Self {
            params: Arc::new(MasterParams::default()),
            core: MasterCore::new(48_000.0, meters.clone(), shared.clone()),
            meters,
            factory_presets: Arc::new(load_all(presets::MASTER_PRESET_JSON)),
            lufs_reset: Arc::new(AtomicBool::new(false)),
            shared,
        }
    }
}

impl Plugin for OverseerMaster {
    const NAME: &'static str = "Qeynos OVERSEER Master";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        names: PortNames {
            layout: Some("Stereo"),
            ..PortNames::const_default()
        },
        ..AudioIOLayout::const_default()
    }];

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
        let meters = self.meters.clone();
        let lufs_reset = self.lufs_reset.clone();
        let shared = self.shared.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-overseer-master-window", Vec2::new(760.0, 760.0))
                    .min_size(Vec2::new(680.0, 560.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · OVERSEER MASTER")
                                .color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "overseer-master", "OVERSEER MASTER", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new(
                                "mastering bus — EQ · 3-band comp · lookahead limiter · LUFS",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        // OVERSEER-ENRICH: theme inference, assist, LEARN, summary card.
                        let theme = master_enrich_ui(ui, &params, setter, &shared);

                        // Preset bar: factory + user presets, filtered by the session theme;
                        // save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("overseer-master", presets.as_slice())
                            .filter(Some(theme.bank_category()))
                            .show(ui, &*params, setter, |setter, p| {
                                apply_master_preset(&params, setter, p)
                            });
                        ui.horizontal(|ui| {
                            if ui.button("RESET LUFS").clicked() {
                                lufs_reset.store(true, Ordering::Relaxed);
                            }
                        });

                        // Meters row.
                        ui.horizontal(|ui| {
                            let tp = load_f32(&meters.true_peak);
                            let m = load_f32(&meters.lufs_m);
                            let s3 = load_f32(&meters.lufs_s);
                            let i = load_f32(&meters.lufs_i);
                            let lgr = load_f32(&meters.limiter_gr);
                            ui.label(
                                egui::RichText::new(format!(
                                    "TP≈ {} dB   LUFS M {} / S {} / I {}   LIM GR {} dB",
                                    fmt_db(tp),
                                    fmt_db(m),
                                    fmt_db(s3),
                                    fmt_db(i),
                                    fmt_db(lgr)
                                ))
                                .color(suite_core::ui::ACCENT)
                                .small(),
                            );
                        });
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("EQ — low shelf · 2 bells · high shelf")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("master-eq")
                                .num_columns(3)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "LOW FREQ", &params.low_freq, setter);
                                    row(ui, "LOW GAIN", &params.low_gain, setter);
                                    ui.end_row();
                                    row(ui, "BELL1 FREQ", &params.b1_freq, setter);
                                    row(ui, "BELL1 GAIN", &params.b1_gain, setter);
                                    row(ui, "BELL1 Q", &params.b1_q, setter);
                                    ui.end_row();
                                    row(ui, "BELL2 FREQ", &params.b2_freq, setter);
                                    row(ui, "BELL2 GAIN", &params.b2_gain, setter);
                                    row(ui, "BELL2 Q", &params.b2_q, setter);
                                    ui.end_row();
                                    row(ui, "HIGH FREQ", &params.high_freq, setter);
                                    row(ui, "HIGH GAIN", &params.high_gain, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new(
                                    "MULTIBAND COMP — LR4 splits, per-band GR shown",
                                )
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                            );
                            egui::Grid::new("master-mb")
                                .num_columns(4)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "XO LOW", &params.xo_low, setter);
                                    row(ui, "XO HIGH", &params.xo_high, setter);
                                    ui.end_row();
                                    row(ui, "LOW THRESH", &params.c1_threshold, setter);
                                    row(ui, "LOW RATIO", &params.c1_ratio, setter);
                                    row(ui, "LOW MAKEUP", &params.c1_makeup, setter);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "GR {}",
                                            fmt_db(load_f32(&meters.band_gr[0]))
                                        ))
                                        .color(suite_core::ui::ACCENT)
                                        .small(),
                                    );
                                    ui.end_row();
                                    row(ui, "MID THRESH", &params.c2_threshold, setter);
                                    row(ui, "MID RATIO", &params.c2_ratio, setter);
                                    row(ui, "MID MAKEUP", &params.c2_makeup, setter);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "GR {}",
                                            fmt_db(load_f32(&meters.band_gr[1]))
                                        ))
                                        .color(suite_core::ui::ACCENT)
                                        .small(),
                                    );
                                    ui.end_row();
                                    row(ui, "HIGH THRESH", &params.c3_threshold, setter);
                                    row(ui, "HIGH RATIO", &params.c3_ratio, setter);
                                    row(ui, "HIGH MAKEUP", &params.c3_makeup, setter);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "GR {}",
                                            fmt_db(load_f32(&meters.band_gr[2]))
                                        ))
                                        .color(suite_core::ui::ACCENT)
                                        .small(),
                                    );
                                    ui.end_row();
                                    row(ui, "KNEE", &params.knee, setter);
                                    row(ui, "ATTACK", &params.attack, setter);
                                    row(ui, "RELEASE", &params.release, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("LIMITER / OUTPUT — 2 ms lookahead")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("master-lim")
                                .num_columns(3)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "CEILING", &params.ceiling, setter);
                                    row(ui, "LIM RELEASE", &params.lim_release, setter);
                                    row(ui, "MIX", &params.mix, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            // ---- Live Node grid (the bus view) --------------
                            ui.label(
                                egui::RichText::new(
                                    "NODES — live OVERSEER Node instances (drag = override, × = release)",
                                )
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                            );
                            let slots = bus::bus().live_slots();
                            if slots.is_empty() {
                                ui.label(
                                    egui::RichText::new(
                                        "no live nodes — add \"Qeynos OVERSEER Node\" on tracks",
                                    )
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                                );
                            }
                            // Slider ranges per overridable param.
                            const RANGES: [(f32, f32); NUM_OVERRIDES] = [
                                (-60.0, 0.0), // THRESH
                                (1.0, 20.0),  // RATIO
                                (0.0, 24.0),  // DRIVE
                                (0.0, 2.0),   // WIDTH
                                (-24.0, 24.0), // TRIM
                            ];
                            for slot in slots.iter() {
                                let (peak, rms, lufs) = slot.meters();
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        // OVERSEER-ENRICH: type-colored badge per Node.
                                        let (rty, _rc) = slot.resolved_type();
                                        let (cr, cg, cb) = rty.color_rgb();
                                        ui.label(
                                            egui::RichText::new(format!(" {} ", rty.label()))
                                                .background_color(egui::Color32::from_rgb(cr, cg, cb))
                                                .color(egui::Color32::BLACK)
                                                .small(),
                                        );
                                        ui.label(
                                            egui::RichText::new(slot.label())
                                                .color(suite_core::ui::ACCENT)
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "PK {}  RMS {}  LUFS-M {}",
                                                fmt_db(peak),
                                                fmt_db(rms),
                                                fmt_db(lufs)
                                            ))
                                            .color(suite_core::ui::TEXT_DIM)
                                            .small(),
                                        );
                                    });
                                    egui::Grid::new(format!("slot-{}", slot.id))
                                        .num_columns(NUM_OVERRIDES)
                                        .spacing([10.0, 4.0])
                                        .show(ui, |ui| {
                                            for i in 0..NUM_OVERRIDES {
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(OVR_NAMES[i])
                                                            .color(suite_core::ui::TEXT_DIM)
                                                            .small(),
                                                    );
                                                    let mut v = if slot.is_override_active(i) {
                                                        slot.override_value(i)
                                                    } else {
                                                        slot.mirror(i)
                                                    };
                                                    let (lo, hi) = RANGES[i];
                                                    ui.horizontal(|ui| {
                                                        let resp = ui.add(
                                                            egui::Slider::new(&mut v, lo..=hi)
                                                                .show_value(true),
                                                        );
                                                        if resp.changed() {
                                                            slot.write_override(i, v);
                                                        }
                                                        if slot.is_override_active(i)
                                                            && ui.small_button("×").clicked()
                                                        {
                                                            slot.clear_override(i);
                                                        }
                                                    });
                                                });
                                            }
                                            ui.end_row();
                                        });
                                });
                            }
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
        self.core = MasterCore::new(
            buffer_config.sample_rate,
            self.meters.clone(),
            self.shared.clone(),
        );
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

        if self.lufs_reset.swap(false, Ordering::Relaxed) {
            self.core.reset_lufs();
        }
        // Publish transport tempo for theme inference.
        let tempo = context.transport().tempo.unwrap_or(0.0) as f32;
        self.shared.set_tempo(tempo);

        // OVERSEER-ENRICH: compute the assist targets HERE, on the audio thread, every block —
        // so ASSIST works whether or not the Master editor is open (it was previously computed
        // only in the editor tick, leaving the audio thread on stale/absent targets). The
        // editor now only DISPLAYS these. `theme_lock` is read non-blockingly (try_read).
        let locked = self
            .params
            .theme_lock
            .try_read()
            .ok()
            .filter(|t| t.locked)
            .map(|t| SessionTheme::from_index(t.theme));
        self.core.update_assist(tempo, locked);

        let base = self.params.snapshot();
        // Scale the theme-derived nudges by the assist strength (0 = display only; SUGGEST-ONLY
        // forces 0). apply_assist is BIT-EXACT identity at 0, so assist=0 changes nothing in the
        // audio path (the done-bar null test).
        let strength = if self.params.suggest_only.value() {
            0.0
        } else {
            self.params.assist.value()
        };
        let s = apply_assist(&base, &self.core.assist_targets(), strength);
        self.core.configure(&s);
        let main = buffer.as_slice();
        if main.len() >= 2 {
            let (l, r) = main.split_at_mut(1);
            self.core.process_block(l[0], r[0]);
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for OverseerMaster {
    const CLAP_ID: &'static str = "com.qeynos.overseer.master";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Mastering bus — EQ, 3-band multiband compressor, lookahead limiter, BS.1770 LUFS meter; remote-controls OVERSEER Node instances",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mastering,
        ClapFeature::Limiter,
    ];
}

impl Vst3Plugin for OverseerMaster {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosOVERSEERms";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

// One library, two plugins (PRD §3 tier 1).
nih_export_clap!(OverseerNode, OverseerMaster);
nih_export_vst3!(OverseerNode, OverseerMaster);

// ===========================================================================
// Done-bar + render tests (PRD §4)
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::harness::{assert_universal, null_residual_db, render_path, write_wav};
    use suite_core::loudness::{k_response, LUFS_OFFSET};
    use suite_core::testsig;

    const SR: f32 = 48_000.0;

    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(crate::MANUAL_DOC, &crate::NodeParams::default());
        suite_core::manual::assert_manual_covers_params(crate::MANUAL_DOC, &crate::MasterParams::default());
    }

    fn new_master() -> MasterCore {
        MasterCore::new(
            SR,
            Arc::new(MasterMeters::default()),
            Arc::new(master::MasterShared::default()),
        )
    }

    /// A neutral Master settings: flat EQ, ratio-1 comp, limiter parked at 0 dB ceiling.
    fn neutral_master() -> MasterSettings {
        let mut s = MasterSettings::default();
        s.eq = EqSettings::default();
        for b in s.bands.iter_mut() {
            b.threshold = 0.0;
            b.ratio = 1.0;
            b.makeup = 0.0;
        }
        s.ceiling_db = 0.0;
        s
    }

    fn process_stereo(core: &mut MasterCore, s: &MasterSettings, l: &mut [f32], r: &mut [f32]) {
        core.configure(s);
        for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            core.configure(s);
            core.process_block(cl, cr);
        }
    }

    // ---- OVERSEER done-bar (1): limiter holds the ceiling -----------------
    #[test]
    fn master_limiter_holds_ceiling_on_plus6_sine() {
        let mut s = neutral_master();
        s.ceiling_db = -1.0;
        s.limiter_release = 100.0;
        let mut core = new_master();
        // +6 dBFS sine = amplitude 2.0.
        let n = (SR * 2.0) as usize;
        let mut l = testsig::sine(220.0, 2.0, n, SR);
        let mut r = l.clone();
        process_stereo(&mut core, &s, &mut l, &mut r);
        // Skip the first 100 ms (attack settle), then measure the peak.
        let start = (SR * 0.1) as usize;
        let peak = l[start..]
            .iter()
            .chain(r[start..].iter())
            .fold(0.0f32, |m, &v| m.max(v.abs()));
        let peak_db = 20.0 * peak.log10();
        assert!(
            peak_db <= -1.0 + 0.1,
            "limiter output peak {peak_db:.3} dBFS exceeds ceiling -1 dBFS + 0.1 dB"
        );
        // And it should sit near the ceiling, not squash to nothing.
        assert!(peak_db > -2.5, "limiter over-attenuated: {peak_db:.3} dBFS");
        assert!(!l.iter().any(|v| !v.is_finite()));
    }

    /// Limiter artifact check: on a steady over-limit sine the post-settle gain envelope
    /// must not chatter (no clicks). We assert sample-to-sample output deltas stay within
    /// what the sine itself explains (no discontinuities).
    #[test]
    fn master_limiter_output_has_no_discontinuities() {
        let mut s = neutral_master();
        s.ceiling_db = -1.0;
        let mut core = new_master();
        let n = (SR * 1.0) as usize;
        let mut l = testsig::sine(220.0, 2.0, n, SR);
        let mut r = l.clone();
        process_stereo(&mut core, &s, &mut l, &mut r);
        let start = (SR * 0.2) as usize;
        // Max |dx| of a 220 Hz sine at amp a is a·2π·220/SR ≈ a·0.0288. Allow 2x margin.
        let bound = 0.9 * 2.0 * std::f32::consts::PI * 220.0 / SR * 2.0;
        for i in (start + 1)..n {
            let d = (l[i] - l[i - 1]).abs();
            assert!(
                d <= bound,
                "click at sample {i}: delta {d:.4} exceeds continuity bound {bound:.4}"
            );
        }
    }

    // ---- OVERSEER done-bar (2): LUFS meter self-consistency ---------------
    #[test]
    fn master_lufs_meter_matches_analytic_reference() {
        // -20 dBFS-RMS 997 Hz sine through a neutral Master. The meter must read within
        // ±0.5 LU of the analytic K-weighted value computed from our own filter response.
        let mut core = new_master();
        let s = neutral_master();
        let rms = 10f32.powf(-20.0 / 20.0);
        let amp = rms * 2.0f32.sqrt();
        let n = (SR * 3.0) as usize;
        let mut l = testsig::sine(997.0, amp, n, SR);
        let mut r = l.clone();
        process_stereo(&mut core, &s, &mut l, &mut r);
        let meter = core.meters.lufs_integrated();
        let mom = load_f32(&core.meters.lufs_m);

        // Analytic: stereo sum of two identical channels doubles power (+3.01 dB).
        let kmag = k_response(997.0, SR) as f64;
        let meansq = (rms as f64) * (rms as f64);
        let analytic = (LUFS_OFFSET + 10.0 * (2.0 * kmag * kmag * meansq).log10()) as f32;
        assert!(
            (mom - analytic).abs() < 0.5,
            "momentary {mom:.2} LUFS vs analytic {analytic:.2} (>±0.5 LU)"
        );
        assert!(
            (meter - analytic).abs() < 0.5,
            "integrated {meter:.2} LUFS vs analytic {analytic:.2} (>±0.5 LU)"
        );
    }

    #[test]
    fn master_lufs_meter_unweighted_reads_minus20() {
        // Test hook: K-weighting disabled → the meter is a plain mean-square level and a
        // -20 dBFS-RMS mono-identical stereo sine reads -20 + 3.01 (stereo sum) ≈ -16.99.
        // Feed one channel only to get exactly -20.0.
        let mut core = new_master();
        core.set_kweighting(false);
        let s = neutral_master();
        let rms = 10f32.powf(-20.0 / 20.0);
        let amp = rms * 2.0f32.sqrt();
        let n = (SR * 2.0) as usize;
        let mut l = testsig::sine(997.0, amp, n, SR);
        let mut r = vec![0.0f32; n];
        process_stereo(&mut core, &s, &mut l, &mut r);
        let mom = load_f32(&core.meters.lufs_m);
        assert!(
            (mom - (-20.0)).abs() < 0.1,
            "unweighted momentary {mom:.3} != -20.0 ±0.1"
        );
    }

    // ---- OVERSEER done-bar (3): bus round-trip -----------------------------
    #[test]
    fn bus_round_trip_override_reaches_node_next_block() {
        // Node + Master DSP structs in one process. Node registers; "Master" writes an
        // override into the Node's slot; the Node's effective param reflects it on the
        // next configure/process; a local touch steals it back.
        let meters = Arc::new(NodeMeters::default());
        let slot = bus::bus().register("KICK");
        let mut node = NodeCore::new(SR, slot.clone(), meters);
        let mut s = NodeSettings::default();
        s.comp_threshold = -18.0;

        // Block 1: no override → mirror shows the local value.
        let mut l = testsig::sine(100.0, 0.5, 512, SR);
        let mut r = l.clone();
        node.configure(&s);
        node.process_block(&mut l, &mut r);
        assert_eq!(slot.mirror(OVR_THRESHOLD), -18.0);

        // Master finds the slot on the bus and writes an override.
        let live = bus::bus().live_slots();
        let found = live.iter().find(|x| x.id == slot.id).expect("slot on bus");
        found.write_override(OVR_THRESHOLD, -30.0);

        // Block 2: Node's effective threshold reflects the override.
        node.configure(&s);
        node.process_block(&mut l, &mut r);
        assert_eq!(slot.mirror(OVR_THRESHOLD), -30.0, "override did not reach the node");
        assert!(slot.override_held());

        // Local touch steals back.
        slot.note_local_touch();
        node.configure(&s);
        assert_eq!(slot.mirror(OVR_THRESHOLD), -18.0, "local touch did not steal back");
        assert!(!slot.override_held());
    }

    #[test]
    fn node_slot_gc_after_drop() {
        let meters = Arc::new(NodeMeters::default());
        let slot = bus::bus().register("TEMP");
        let node = NodeCore::new(SR, slot.clone(), meters);
        let id = slot.id;
        drop(node);
        drop(slot);
        // Retry: the global BUS is shared across all tests in this binary, so a concurrent
        // test cloning the slot vec can transiently keep this slot alive for one GC pass.
        let mut gone = false;
        for _ in 0..10_000 {
            if !bus::bus().live_slots().iter().any(|x| x.id == id) {
                gone = true;
                break;
            }
            std::thread::yield_now();
        }
        assert!(gone, "dead node slot not GC'd");
    }

    // ---- Universal assertions + renders ------------------------------------
    #[test]
    fn node_mix_zero_nulls_latency_matched_dry() {
        let meters = Arc::new(NodeMeters::default());
        let slot = bus::bus().register("NULL");
        let mut node = NodeCore::new(SR, slot, meters);
        let mut s = NodeSettings::default();
        s.mix = 0.0;
        s.drive_db = 12.0;
        s.comp_threshold = -40.0;
        let dry = testsig::synth_kick_stub((SR * 1.0) as usize, SR);
        let mut l = dry.clone();
        let mut r = dry.clone();
        node.configure(&s);
        let lat = node.latency_samples() as usize;
        node.process_block(&mut l, &mut r);
        // At mix=0 the output is the dry path delayed by the saturation-oversampler latency.
        let m = dry.len() - lat;
        let delayed: Vec<f32> = dry[..m].to_vec();
        let out: Vec<f32> = l[lat..].to_vec();
        let resid = null_residual_db(&delayed, &out);
        assert!(resid < -80.0, "node mix=0 residual {resid:.1} dB >= -80");
    }

    /// OVERSEER MINOR (node.rs): DRIVE at its floor (0 dB) must bypass the saturation stage
    /// EXACTLY — a `tanh(x)/1` curve at 0 dB still colors, so the strip could never run clean.
    /// With a neutral strip (flat EQ, inactive comp, unity width/trim, full wet) the output at
    /// the DRIVE floor equals the input through the same latency path; with DRIVE up it colors.
    #[test]
    fn node_drive_floor_bypasses_saturation_exactly() {
        let meters = Arc::new(NodeMeters::default());
        let slot = bus::bus().register("SATBYP");
        let mut node = NodeCore::new(SR, slot, meters);

        let mut s = NodeSettings::default();
        s.eq = EqSettings::default(); // flat → identity
        s.comp_threshold = 60.0; // far above any signal → comp inactive (g = 1)
        s.comp_ratio = 1.0;
        s.comp_makeup = 0.0;
        s.drive_db = 0.0; // FLOOR → exact bypass
        s.width = 1.0;
        s.trim_db = 0.0;
        s.mix = 1.0;

        // A hot kick: the sat coloration (~2 dB peak compression) would show plainly if present.
        let dry = testsig::synth_kick_stub((SR * 1.0) as usize, SR);
        let mut l = dry.clone();
        let mut r = dry.clone();
        node.configure(&s);
        let lat = node.latency_samples() as usize;
        node.process_block(&mut l, &mut r);

        let m = dry.len() - lat;
        let delayed: Vec<f32> = dry[..m].to_vec();
        let out: Vec<f32> = l[lat..].to_vec();
        let resid = null_residual_db(&delayed, &out);
        // The sat bypass is bit-exact (a latency-matched DelayLine tap); the residual floor
        // here (~-98 dB) is the flat 4-biquad EQ's own ~ULP identity error on the wet path,
        // NOT the saturation stage. Either way it sits ~40 dB below any sat coloration.
        assert!(
            resid < -96.0,
            "DRIVE floor did not bypass saturation exactly: residual {resid:.1} dB (want < -96)"
        );

        // Sanity: DRIVE up DOES color (the stage is real, not disabled).
        let meters2 = Arc::new(NodeMeters::default());
        let slot2 = bus::bus().register("SATON");
        let mut node2 = NodeCore::new(SR, slot2, meters2);
        s.drive_db = 12.0;
        let mut l2 = dry.clone();
        let mut r2 = dry.clone();
        node2.configure(&s);
        node2.process_block(&mut l2, &mut r2);
        let out2: Vec<f32> = l2[lat..].to_vec();
        let resid2 = null_residual_db(&delayed, &out2);
        assert!(
            resid2 > -60.0,
            "DRIVE=12 dB should color vs the dry input (residual {resid2:.1} dB)"
        );
    }

    /// OVERSEER MINOR (lib.rs/master.rs): the Master computes ENRICH assist targets on the
    /// AUDIO thread — with NO editor open — so ASSIST is not silently dependent on the GUI. A
    /// locked theme resolves to its full-strength targets straight from `update_assist`.
    #[test]
    fn master_assist_targets_computed_on_audio_thread() {
        let mut core = new_master();
        // Default (no update yet) → neutral targets.
        assert_eq!(core.assist_targets(), crate::enrich::AssistTargets::default());
        // Audio-thread update with a locked theme (the editor never opened).
        core.update_assist(128.0, Some(SessionTheme::DarkTechno));
        assert_eq!(
            core.assist_targets(),
            theme_assist_targets(SessionTheme::DarkTechno),
            "audio-thread update_assist did not produce the locked theme's targets"
        );
        // And it published them to the shared state the editor reads for display.
        let (t, _c) = core.shared().theme();
        assert_eq!(t, SessionTheme::DarkTechno);
    }

    /// MAJOR 3 done-bar: a mid-buffer step change in trim / mix / EQ gain must be smoothed
    /// (params declare smoothers; the fix applies them). Assert the output has no
    /// sample-to-sample jump beyond a bound consistent with the smoothing — the previous
    /// snapshot-reads-`.value()` code applied the whole change in one sample (a click).
    #[test]
    fn node_param_step_is_smoothed_no_zipper() {
        let meters = Arc::new(NodeMeters::default());
        let slot = bus::bus().register("SMOOTH");
        let mut node = NodeCore::new(SR, slot, meters);

        // Neutral strip: flat EQ, unity comp, no drive.
        let mut s = NodeSettings::default();
        s.eq = EqSettings::default();
        s.comp_ratio = 1.0;
        s.comp_threshold = 0.0;
        s.comp_makeup = 0.0;
        s.drive_db = 0.0;
        s.width = 1.0;
        s.trim_db = 0.0;
        s.mix = 1.0;

        let n = 8192usize;
        let sig = testsig::sine(220.0, 0.5, n, SR);
        let half = n / 2;

        let mut l1 = sig[..half].to_vec();
        let mut r1 = l1.clone();
        node.configure(&s);
        node.process_block(&mut l1, &mut r1);

        // Step trim +6 dB, mix → 0.5, low-shelf gain +12 dB, all at the buffer midpoint.
        s.trim_db = 6.0;
        s.mix = 0.5;
        s.eq.low_gain = 12.0;
        let mut l2 = sig[half..].to_vec();
        let mut r2 = l2.clone();
        node.configure(&s);
        node.process_block(&mut l2, &mut r2);

        let mut out = l1;
        out.extend_from_slice(&l2);
        let mut max_delta = 0.0f32;
        for i in 1..out.len() {
            max_delta = max_delta.max((out[i] - out[i - 1]).abs());
        }
        // A 220 Hz sine peaking near ~1.0 after the +6 dB trim slews at most
        // ~1.0·2π·220/48000 ≈ 0.029/sample; the smoother adds only a tiny per-sample
        // increment. The unsmoothed step would jump by ~half the peak (>0.3).
        assert!(
            max_delta < 0.1,
            "zipper: max sample-to-sample delta {max_delta:.4} (smoothing not applied?)"
        );
    }

    #[test]
    fn master_mix_zero_nulls_latency_matched_dry() {
        let mut core = new_master();
        let mut s = MasterSettings::default();
        s.mix = 0.0;
        let dry = testsig::synth_kick_stub((SR * 1.0) as usize, SR);
        let mut l = dry.clone();
        let mut r = dry.clone();
        process_stereo(&mut core, &s, &mut l, &mut r);
        // Output is the dry path delayed by the limiter lookahead.
        let lat = core.latency_samples() as usize;
        let n = dry.len() - lat;
        let delayed: Vec<f32> = dry[..n].to_vec();
        let out: Vec<f32> = l[lat..].to_vec();
        let resid = null_residual_db(&delayed, &out);
        assert!(resid < -80.0, "master mix=0 residual {resid:.1} dB >= -80");
    }

    #[test]
    fn every_preset_renders_and_passes_universal() {
        let kick = testsig::synth_kick_stub((SR * 1.5) as usize, SR);
        let vocal = testsig::synth_vocal(180.0, (SR * 1.5) as usize, SR);
        // A crude "mix" for the master: kick + vocal + soft noise bed.
        let noise = testsig::pink_noise(0.1, kick.len(), 7);
        let mix: Vec<f32> = (0..kick.len())
            .map(|i| (kick[i] * 0.8 + vocal[i] * 0.5 + noise[i]).clamp(-1.0, 1.0) * 0.7)
            .collect();

        // Node presets over kick and vocal.
        for p in load_all(presets::NODE_PRESET_JSON).iter() {
            let s = presets::node_settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '&'], "_");
            for (sig, tag) in [(&kick, "kick"), (&vocal, "vocal")] {
                let meters = Arc::new(NodeMeters::default());
                let slot = bus::bus().register("RENDER");
                let mut node = NodeCore::new(SR, slot, meters);
                let mut out = (*sig).clone();
                node.process_mono(&mut out, &s);
                assert_universal(&out);
                write_wav(&render_path("OVERSEER", &format!("node_{fname}_{tag}")), &out, SR as u32)
                    .expect("write node render");
            }
        }

        // Master presets over the mix.
        for p in load_all(presets::MASTER_PRESET_JSON).iter() {
            let s = presets::master_settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '&'], "_");
            let mut core = new_master();
            let mut out = mix.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            write_wav(
                &render_path("OVERSEER", &format!("master_{fname}")),
                &out,
                SR as u32,
            )
            .expect("write master render");
        }
    }

    #[test]
    fn master_survives_extreme_settings() {
        // Fuzzer-extremes guard: degenerate crossovers, max ratios, hot input.
        let mut s = MasterSettings::default();
        s.xo_low = 20_000.0;
        s.xo_high = 20.0; // deliberately inverted — set_crossovers must sanitize
        for b in s.bands.iter_mut() {
            b.threshold = -60.0;
            b.ratio = 20.0;
            b.makeup = 24.0;
        }
        s.ceiling_db = -12.0;
        s.eq.low_gain = 24.0;
        s.eq.high_gain = 24.0;
        let mut core = new_master();
        let n = (SR * 0.5) as usize;
        let mut l = testsig::white_noise(1.0, n, 3);
        let mut r = testsig::white_noise(1.0, n, 4);
        process_stereo(&mut core, &s, &mut l, &mut r);
        assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()));
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak <= 10f32.powf(-12.0 / 20.0) + 1e-3, "ceiling violated: {peak}");
    }

    // ===================================================================
    // OVERSEER-ENRICH done-bars (plugin/bus level)
    // ===================================================================

    /// A four-on-the-floor kick train (default kick every 0.25 s).
    fn kick_train(secs: f32) -> Vec<f32> {
        let n = (SR * secs) as usize;
        let mut out = vec![0.0f32; n];
        let period = (SR * 0.25) as usize;
        let one = testsig::synth_kick_stub((SR * 0.24) as usize, SR);
        let mut t = 0;
        while t < n {
            for (i, &v) in one.iter().enumerate() {
                if t + i < n {
                    out[t + i] += v;
                }
            }
            t += period;
        }
        out
    }

    /// A slow, wide, sustained chord pad (decorrelated L/R).
    fn wide_pad(secs: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (SR * secs) as usize;
        let freqs = [220.0f32, 261.63, 329.63];
        let mut lp_l = suite_core::dsp::Svf::new();
        let mut lp_r = suite_core::dsp::Svf::new();
        lp_l.set(1200.0, 0.707, SR);
        lp_r.set(1200.0, 0.707, SR);
        let mut l = vec![0.0f32; n];
        let mut r = vec![0.0f32; n];
        for i in 0..n {
            let t = i as f32 / SR;
            let mut sl = 0.0f32;
            let mut sr_ = 0.0f32;
            for (k, &fr) in freqs.iter().enumerate() {
                sl += (2.0 * (fr * t).fract() - 1.0) * 0.2;
                sr_ += (2.0 * ((fr * 1.003) * t + 0.25 * (k as f32 + 1.0)).fract() - 1.0) * 0.2;
            }
            let env = (t / 0.5).min(1.0);
            l[i] = lp_l.process(sl * env).lp;
            r[i] = lp_r.process(sr_ * env).lp;
        }
        (l, r)
    }

    /// Stream a stereo signal through a fresh Node core (default settings) and return its slot
    /// with published features.
    fn stream_node(label: &str, l: &[f32], r: &[f32]) -> Arc<Slot> {
        let slot = bus::bus().register(label);
        let meters = Arc::new(NodeMeters::default());
        let mut core = NodeCore::new(SR, slot.clone(), meters);
        let s = NodeSettings::default();
        for (cl, cr) in l.chunks(512).zip(r.chunks(512)) {
            let mut a = cl.to_vec();
            let mut b = cr.to_vec();
            core.configure(&s);
            core.process_block(&mut a, &mut b);
        }
        slot
    }

    /// Done-bar: **assist at 0 changes NOTHING in the audio path** (null vs the pre-enrich
    /// render). A locked theme's assist targets present, scaled to zero → bit-identical output.
    #[test]
    fn assist_at_zero_is_audio_null_vs_base() {
        let base = MasterSettings::default();
        let targets = theme_assist_targets(SessionTheme::DarkTechno);
        let assist0 = apply_assist(&base, &targets, 0.0);

        let mix = kick_train(1.5);
        let (mut a_l, mut a_r) = (mix.clone(), mix.clone());
        let (mut b_l, mut b_r) = (mix.clone(), mix.clone());
        let mut core_a = new_master();
        process_stereo(&mut core_a, &base, &mut a_l, &mut a_r);
        let mut core_b = new_master();
        process_stereo(&mut core_b, &assist0, &mut b_l, &mut b_r);

        let resid = null_residual_db(&a_l, &b_l);
        assert!(resid < -120.0, "assist=0 not null vs pre-enrich base: {resid:.1} dB");
    }

    /// Done-bar: kick + rumble + pad Node streams through the Bus at 130 BPM → DARK-TECHNO.
    #[test]
    fn techno_session_over_bus_infers_dark_techno() {
        let n = (SR * 4.0) as usize;
        let kick = kick_train(4.0);
        let ks = stream_node("BUS-KICK", &kick, &kick);
        let rumble = testsig::sine(45.0, 0.5, n, SR);
        let rs = stream_node("BUS-RUMBLE", &rumble, &rumble);
        let (pl, pr) = wide_pad(4.0);
        let ps = stream_node("BUS-PAD", &pl, &pr);

        // Build reports from THESE slots (not live_slots(): the global bus is shared across
        // the whole test binary, so other tests' slots would pollute an enumeration).
        let reports = [
            NodeReport {
                ty: ks.resolved_type().0,
                features: ks.features(),
            },
            NodeReport {
                ty: rs.resolved_type().0,
                features: rs.features(),
            },
            NodeReport {
                ty: ps.resolved_type().0,
                features: ps.features(),
            },
        ];
        assert_eq!(ks.resolved_type().0, InstrumentType::Kick, "bus kick misclassified");
        let mix = MixAnalysis {
            tempo_bpm: 130.0,
            tilt: -0.3,
            onset_density: 4.0,
            dynamic_range_db: 10.0,
        };
        let (theme, conf) = infer_theme(&reports, &mix);
        assert_eq!(theme, SessionTheme::DarkTechno, "got {theme:?} @ {conf}");
        assert!(conf >= 0.4);
    }

    /// Done-bar: the Node LEARN window captures exactly N seconds through the bus and commits
    /// the type played DURING the window (KICK), even if a different fixture (VOCAL) follows.
    #[test]
    fn node_learn_over_bus_commits_window_type() {
        let slot = bus::bus().register("BUS-LEARN");
        let meters = Arc::new(NodeMeters::default());
        let mut core = NodeCore::new(SR, slot.clone(), meters);
        let s = NodeSettings::default();
        let n = (SR * 2.0) as usize;
        slot.request_learn(n);
        let gen0 = slot.learn_gen();

        let kick = kick_train(2.5);
        let mut fed = 0;
        for chunk in kick.chunks(512) {
            let mut a = chunk.to_vec();
            let mut b = chunk.to_vec();
            core.configure(&s);
            core.process_block(&mut a, &mut b);
            fed += chunk.len();
            if fed >= n + 1024 {
                break;
            }
        }
        assert_ne!(slot.learn_gen(), gen0, "LEARN did not finalise");
        assert_eq!(
            classify(&slot.learn_result()).0,
            InstrumentType::Kick,
            "committed type must match the fixture during the window"
        );
        let gen1 = slot.learn_gen();

        // Play VOCAL after commit — no new capture result may appear.
        let vocal = testsig::synth_vocal(180.0, (SR * 2.0) as usize, SR);
        for chunk in vocal.chunks(512) {
            let mut a = chunk.to_vec();
            let mut b = chunk.to_vec();
            core.configure(&s);
            core.process_block(&mut a, &mut b);
        }
        assert_eq!(slot.learn_gen(), gen1, "post-commit audio produced a new capture");
    }

    /// Done-bar: an old project (no type param, no learn persist saved) loads cleanly on AUTO.
    #[test]
    fn node_defaults_to_auto_and_unlocked() {
        let p = NodeParams::default();
        assert_eq!(p.inst_type.value(), TypeParam::Auto);
        assert!(!p.learn.read().unwrap().locked);
        assert!(p.learn.read().unwrap().suggestion.is_none());
        // Master theme lock also defaults to unlocked.
        let m = MasterParams::default();
        assert!(!m.theme_lock.read().unwrap().locked);
        assert_eq!(m.assist.value(), 0.30);
    }
}
