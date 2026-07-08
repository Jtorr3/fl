//! OUROBOROS — recursive feedback processor (Qeynos suite, Phase 2a; Recurse clone).
//!
//! A feedback delay (1 ms–2 s, free or BPM-synced) whose loop runs through a **reorderable
//! chain of three effect slots** — each selectable from granular pitch shift, SVF filter
//! (LP/HP/BP), Hilbert frequency shifter, waveshaper saturator, reversed-granule playback, or
//! bit crush — then an **in-loop soft limiter** (`tanh` at unity) and a **DC blocker**, before
//! the output tap feeds back at up to **110 %**. Every repeat is re-processed, so the sound
//! mutates as it recirculates; past unity feedback the loop self-oscillates but stays bounded.
//! **Freeze** mutes the input and forces 100 % feedback (click-free) to hold an infinite tail.
//!
//! The delay line *is* the effect (not fixed latency), so OUROBOROS reports **zero latency**;
//! the delay read is fractional + smoothed so time changes glide click-free. See [`dsp`] for
//! the DSP core, shared verbatim with the offline harness / done-bar tests.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

use dsp::{Settings, SlotOrder, SlotSettings, SlotType, SyncDivision, OuroCore};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SlotTypeParam {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "pitch"]
    #[name = "Pitch Shift"]
    Pitch,
    #[id = "lp"]
    #[name = "Filter LP"]
    FilterLp,
    #[id = "hp"]
    #[name = "Filter HP"]
    FilterHp,
    #[id = "bp"]
    #[name = "Filter BP"]
    FilterBp,
    #[id = "shift"]
    #[name = "Freq Shift"]
    FreqShift,
    #[id = "sat"]
    #[name = "Saturate"]
    Saturate,
    #[id = "rev"]
    #[name = "Reverse"]
    Reverse,
    #[id = "crush"]
    #[name = "Bit Crush"]
    BitCrush,
}

impl SlotTypeParam {
    fn to_dsp(self) -> SlotType {
        match self {
            SlotTypeParam::Off => SlotType::Off,
            SlotTypeParam::Pitch => SlotType::Pitch,
            SlotTypeParam::FilterLp => SlotType::FilterLp,
            SlotTypeParam::FilterHp => SlotType::FilterHp,
            SlotTypeParam::FilterBp => SlotType::FilterBp,
            SlotTypeParam::FreqShift => SlotType::FreqShift,
            SlotTypeParam::Saturate => SlotType::Saturate,
            SlotTypeParam::Reverse => SlotType::Reverse,
            SlotTypeParam::BitCrush => SlotType::BitCrush,
        }
    }
    fn from_index(i: usize) -> SlotTypeParam {
        match i {
            0 => SlotTypeParam::Off,
            1 => SlotTypeParam::Pitch,
            2 => SlotTypeParam::FilterLp,
            3 => SlotTypeParam::FilterHp,
            4 => SlotTypeParam::FilterBp,
            5 => SlotTypeParam::FreqShift,
            6 => SlotTypeParam::Saturate,
            7 => SlotTypeParam::Reverse,
            _ => SlotTypeParam::BitCrush,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum OrderParam {
    #[id = "abc"]
    #[name = "A → B → C"]
    Abc,
    #[id = "acb"]
    #[name = "A → C → B"]
    Acb,
    #[id = "bac"]
    #[name = "B → A → C"]
    Bac,
    #[id = "bca"]
    #[name = "B → C → A"]
    Bca,
    #[id = "cab"]
    #[name = "C → A → B"]
    Cab,
    #[id = "cba"]
    #[name = "C → B → A"]
    Cba,
}

impl OrderParam {
    fn to_dsp(self) -> SlotOrder {
        match self {
            OrderParam::Abc => SlotOrder::Abc,
            OrderParam::Acb => SlotOrder::Acb,
            OrderParam::Bac => SlotOrder::Bac,
            OrderParam::Bca => SlotOrder::Bca,
            OrderParam::Cab => SlotOrder::Cab,
            OrderParam::Cba => SlotOrder::Cba,
        }
    }
    fn from_index(i: usize) -> OrderParam {
        match i {
            0 => OrderParam::Abc,
            1 => OrderParam::Acb,
            2 => OrderParam::Bac,
            3 => OrderParam::Bca,
            4 => OrderParam::Cab,
            _ => OrderParam::Cba,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DivisionParam {
    #[id = "d16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "d8"]
    #[name = "1/8"]
    Eighth,
    #[id = "d8d"]
    #[name = "1/8·"]
    DottedEighth,
    #[id = "d4"]
    #[name = "1/4"]
    Quarter,
    #[id = "d4d"]
    #[name = "1/4·"]
    DottedQuarter,
    #[id = "d2"]
    #[name = "1/2"]
    Half,
    #[id = "bar"]
    #[name = "1 Bar"]
    Bar,
}

impl DivisionParam {
    fn to_dsp(self) -> SyncDivision {
        match self {
            DivisionParam::Sixteenth => SyncDivision::Sixteenth,
            DivisionParam::Eighth => SyncDivision::Eighth,
            DivisionParam::DottedEighth => SyncDivision::DottedEighth,
            DivisionParam::Quarter => SyncDivision::Quarter,
            DivisionParam::DottedQuarter => SyncDivision::DottedQuarter,
            DivisionParam::Half => SyncDivision::Half,
            DivisionParam::Bar => SyncDivision::Bar,
        }
    }
    fn from_index(i: usize) -> DivisionParam {
        match i {
            0 => DivisionParam::Sixteenth,
            1 => DivisionParam::Eighth,
            2 => DivisionParam::DottedEighth,
            3 => DivisionParam::Quarter,
            4 => DivisionParam::DottedQuarter,
            5 => DivisionParam::Half,
            _ => DivisionParam::Bar,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-slot param group
// ---------------------------------------------------------------------------

#[derive(Params)]
pub struct SlotParams {
    #[id = "type"]
    pub kind: EnumParam<SlotTypeParam>,
    #[id = "amt"]
    pub amount: FloatParam,
    #[id = "param"]
    pub param: FloatParam,
}

impl SlotParams {
    fn new(name: &str, default_kind: SlotTypeParam) -> Self {
        Self {
            kind: EnumParam::new(format!("{name} Type"), default_kind),
            amount: FloatParam::new(
                format!("{name} Amount"),
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            param: FloatParam::new(
                format!("{name} Param"),
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
    fn snapshot(&self) -> SlotSettings {
        SlotSettings {
            kind: self.kind.value().to_dsp(),
            amount: self.amount.value(),
            param: self.param.value(),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Ouroboros {
    params: Arc<OuroParams>,
    core: OuroCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct OuroParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "delay"]
    pub delay: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "division"]
    pub division: EnumParam<DivisionParam>,
    #[id = "feedback"]
    pub feedback: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "freeze"]
    pub freeze: BoolParam,
    #[id = "freezemix"]
    pub freeze_mix: FloatParam,
    #[id = "order"]
    pub order: EnumParam<OrderParam>,

    #[nested(id_prefix = "sa", group = "Slot A")]
    pub slot_a: SlotParams,
    #[nested(id_prefix = "sb", group = "Slot B")]
    pub slot_b: SlotParams,
    #[nested(id_prefix = "sc", group = "Slot C")]
    pub slot_c: SlotParams,

    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,
}

impl Default for OuroParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(600, 680),
            delay: FloatParam::new(
                "Delay",
                d.delay_ms,
                FloatRange::Skewed {
                    min: 1.0,
                    max: dsp::MAX_DELAY_MS,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            sync: BoolParam::new("Sync", d.sync),
            division: EnumParam::new("Division", DivisionParam::Quarter),
            feedback: FloatParam::new(
                "Feedback",
                d.feedback,
                FloatRange::Linear { min: 0.0, max: 1.1 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            decay: FloatParam::new("Decay", d.decay_scale, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            freeze: BoolParam::new("Freeze", d.freeze),
            freeze_mix: FloatParam::new("Freeze Mix", d.freeze_mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            order: EnumParam::new("Order", OrderParam::Abc),
            slot_a: SlotParams::new("Slot A", SlotTypeParam::Off),
            slot_b: SlotParams::new("Slot B", SlotTypeParam::Off),
            slot_c: SlotParams::new("Slot C", SlotTypeParam::Off),
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

impl OuroParams {
    /// Snapshot the live parameters into a DSP [`Settings`]. `tempo_bpm` comes from the host.
    fn snapshot(&self, tempo_bpm: f32) -> Settings {
        Settings {
            delay_ms: self.delay.value(),
            sync: self.sync.value(),
            division: self.division.value().to_dsp(),
            tempo_bpm,
            feedback: self.feedback.value(),
            decay_scale: self.decay.value(),
            freeze: self.freeze.value(),
            freeze_mix: self.freeze_mix.value(),
            order: self.order.value().to_dsp(),
            slots: [
                self.slot_a.snapshot(),
                self.slot_b.snapshot(),
                self.slot_c.snapshot(),
            ],
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Ouroboros {
    fn default() -> Self {
        Self {
            params: Arc::new(OuroParams::default()),
            core: OuroCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &OuroParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    setter.begin_set_parameter(&params.sync);
    setter.set_parameter(&params.sync, g("sync", 0.0) >= 0.5);
    setter.end_set_parameter(&params.sync);
    setter.begin_set_parameter(&params.freeze);
    setter.set_parameter(&params.freeze, g("freeze", 0.0) >= 0.5);
    setter.end_set_parameter(&params.freeze);
    setter.begin_set_parameter(&params.division);
    setter.set_parameter(&params.division, DivisionParam::from_index(g("division", 3.0) as usize));
    setter.end_set_parameter(&params.division);
    setter.begin_set_parameter(&params.order);
    setter.set_parameter(&params.order, OrderParam::from_index(g("order", 0.0) as usize));
    setter.end_set_parameter(&params.order);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.delay, g("delay", 300.0));
    set_f(&params.feedback, g("feedback", 0.5));
    set_f(&params.decay, g("decay", 1.0));
    set_f(&params.mix, g("mix", 0.5));
    set_f(&params.out, g("out", 0.0));

    let set_slot = |slot: &SlotParams, pfx: &str| {
        setter.begin_set_parameter(&slot.kind);
        setter.set_parameter(
            &slot.kind,
            SlotTypeParam::from_index(g(&format!("{pfx}_type"), 0.0) as usize),
        );
        setter.end_set_parameter(&slot.kind);
        set_f(&slot.amount, g(&format!("{pfx}_amt"), 0.5));
        set_f(&slot.param, g(&format!("{pfx}_param"), 0.5));
    };
    set_slot(&params.slot_a, "a");
    set_slot(&params.slot_b, "b");
    set_slot(&params.slot_c, "c");
}

impl Plugin for Ouroboros {
    const NAME: &'static str = "Qeynos OUROBOROS";
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
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-ouroboros-window", Vec2::new(600.0, 680.0))
                    .min_size(Vec2::new(520.0, 560.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · OUROBOROS").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new("recursive feedback processor — delay loop, 3-slot chain, freeze")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset selector.
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("PRESET")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::ComboBox::from_id_salt("ouro-preset")
                                .selected_text("select…")
                                .show_ui(ui, |ui| {
                                    for p in presets.iter() {
                                        if ui.selectable_label(false, &p.name).clicked() {
                                            apply_preset(&params, setter, p);
                                        }
                                    }
                                });
                        });
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("LOOP")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("ouro-loop")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "DELAY", &params.delay, setter);
                                    row(ui, "DIVISION", &params.division, setter);
                                    ui.end_row();
                                    row(ui, "FEEDBACK", &params.feedback, setter);
                                    row(ui, "DECAY", &params.decay, setter);
                                    ui.end_row();
                                });
                            ui.horizontal(|ui| {
                                let mut sy = params.sync.value();
                                if ui.checkbox(&mut sy, "SYNC").changed() {
                                    setter.begin_set_parameter(&params.sync);
                                    setter.set_parameter(&params.sync, sy);
                                    setter.end_set_parameter(&params.sync);
                                }
                                ui.add_space(12.0);
                                let mut fz = params.freeze.value();
                                if ui.checkbox(&mut fz, "FREEZE").changed() {
                                    setter.begin_set_parameter(&params.freeze);
                                    setter.set_parameter(&params.freeze, fz);
                                    setter.end_set_parameter(&params.freeze);
                                }
                                ui.add_space(12.0);
                                row(ui, "FREEZE MIX", &params.freeze_mix, setter);
                            });
                            ui.separator();

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("SLOT CHAIN")
                                        .color(suite_core::ui::TEXT_DIM)
                                        .small(),
                                );
                                ui.add_space(8.0);
                                row(ui, "ORDER", &params.order, setter);
                            });
                            let slot_ui = |ui: &mut egui::Ui, title: &str, slot: &SlotParams| {
                                ui.label(
                                    egui::RichText::new(title)
                                        .color(suite_core::ui::ACCENT)
                                        .small(),
                                );
                                egui::Grid::new(format!("ouro-{title}"))
                                    .num_columns(3)
                                    .spacing([12.0, 6.0])
                                    .show(ui, |ui| {
                                        row(ui, "TYPE", &slot.kind, setter);
                                        row(ui, "AMOUNT", &slot.amount, setter);
                                        row(ui, "PARAM", &slot.param, setter);
                                        ui.end_row();
                                    });
                            };
                            slot_ui(ui, "SLOT A", &params.slot_a);
                            slot_ui(ui, "SLOT B", &params.slot_b);
                            slot_ui(ui, "SLOT C", &params.slot_c);
                            ui.separator();

                            ui.label(
                                egui::RichText::new("OUTPUT")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("ouro-out")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "MIX", &params.mix, setter);
                                    row(ui, "OUT", &params.out, setter);
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
        self.core = OuroCore::new(buffer_config.sample_rate);
        // The delay line IS the effect — zero reported processing latency.
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
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop — the
        // feedback loop and IIR filters can otherwise leak denormals into a CPU spike.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let tempo = context.transport().tempo.unwrap_or(120.0) as f32;
        let s = self.params.snapshot(tempo);
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

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Ouroboros {
    const CLAP_ID: &'static str = "com.qeynos.ouroboros";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Recursive feedback processor — a delay loop through a reorderable 3-slot effect chain \
         (pitch/filter/freq-shift/saturate/reverse/crush) with an in-loop limiter, DC blocker, \
         110% feedback self-oscillation, and freeze",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Delay,
        ClapFeature::Custom("feedback"),
    ];
}

impl Vst3Plugin for Ouroboros {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosOUROBOROS1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

nih_export_clap!(Ouroboros);
nih_export_vst3!(Ouroboros);

#[cfg(test)]
mod render_tests {
    use crate::dsp::OuroCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    /// Render each factory preset over pink noise and a full-band chirp, write the WAVs into
    /// renders/OUROBOROS/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.5, (sr * 4.0) as usize, 4242);
        let chirp = testsig::log_chirp(40.0, 12_000.0, 0.5, (sr * 4.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");

            let mut core = OuroCore::new(sr);
            let mut out = pink.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("OUROBOROS", &format!("{fname}_pink"));
            write_wav(&path, &out, sr as u32).expect("write pink render");

            let mut core = OuroCore::new(sr);
            let mut out = chirp.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("OUROBOROS", &format!("{fname}_chirp"));
            write_wav(&path, &out, sr as u32).expect("write chirp render");
        }
    }
}
