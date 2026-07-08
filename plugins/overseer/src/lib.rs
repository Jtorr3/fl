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
pub mod eq;
pub mod master;
pub mod node;
pub mod presets;

use bus::{Slot, NUM_OVERRIDES, OVR_DRIVE, OVR_NAMES, OVR_RATIO, OVR_THRESHOLD, OVR_TRIM, OVR_WIDTH};
use eq::EqSettings;
use master::{BandComp, MasterCore, MasterMeters, MasterSettings};
use node::{load_f32, NodeCore, NodeMeters, NodeSettings};
use suite_core::presets::{load_all, Preset};

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
            editor_state: EguiState::from_size(560, 620),
            label: RwLock::new("NODE".to_string()),

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
                            ui.label(
                                egui::RichText::new("PRESET")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::ComboBox::from_id_salt("node-preset")
                                .selected_text("select…")
                                .show_ui(ui, |ui| {
                                    for p in presets.iter() {
                                        if ui.selectable_label(false, &p.name).clicked() {
                                            apply_node_preset(&params, setter, p);
                                        }
                                    }
                                });
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
            editor_state: EguiState::from_size(760, 680),

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

pub struct OverseerMaster {
    params: Arc<MasterParams>,
    meters: Arc<MasterMeters>,
    core: MasterCore,
    factory_presets: Arc<Vec<Preset>>,
    /// GUI → audio: reset the integrated-LUFS meter on the next block.
    lufs_reset: Arc<AtomicBool>,
}

impl Default for OverseerMaster {
    fn default() -> Self {
        let meters = Arc::new(MasterMeters::default());
        Self {
            params: Arc::new(MasterParams::default()),
            core: MasterCore::new(48_000.0, meters.clone()),
            meters,
            factory_presets: Arc::new(load_all(presets::MASTER_PRESET_JSON)),
            lufs_reset: Arc::new(AtomicBool::new(false)),
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
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-overseer-master-window", Vec2::new(760.0, 680.0))
                    .min_size(Vec2::new(680.0, 560.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · OVERSEER MASTER")
                                .color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(
                                "mastering bus — EQ · 3-band comp · lookahead limiter · LUFS",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("PRESET")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::ComboBox::from_id_salt("master-preset")
                                .selected_text("select…")
                                .show_ui(ui, |ui| {
                                    for p in presets.iter() {
                                        if ui.selectable_label(false, &p.name).clicked() {
                                            apply_master_preset(&params, setter, p);
                                        }
                                    }
                                });
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
        self.core = MasterCore::new(buffer_config.sample_rate, self.meters.clone());
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
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        if self.lufs_reset.swap(false, Ordering::Relaxed) {
            self.core.reset_lufs();
        }
        let s = self.params.snapshot();
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

    fn new_master() -> MasterCore {
        MasterCore::new(SR, Arc::new(MasterMeters::default()))
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
}
