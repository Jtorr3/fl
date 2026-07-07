//! IMPACT — kick drum synth (Qeynos suite, Phase 1). A MIDI instrument.
//!
//! A mono, last-note-priority kick voice: a note-on drives an exponential pitch envelope
//! (`f_start → f_end`, curve-morphed) into a phase-continuous sine/triangle body oscillator,
//! layered with a band-passed white-noise click + one of three embedded PCM transients
//! (synthesized offline in `build.rs`) and a sub oscillator tuned to `f_end × ratio`. The mix
//! is saturated through the suite waveshaper bank, shaped by an exponential amp envelope, and
//! clipped. The LENGTH macro scales amp decay and pitch τ together; key-track sets `f_end` from
//! the MIDI note (A1 = 55 Hz). Retriggers are phase-continuous with a 1.5 ms declick ramp.
//!
//! DSP core lives in [`dsp`] (pure Rust, shared with the offline harness tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    resizable_window::ResizableWindow,
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

use dsp::{DriveShape, KickVoice, Settings};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Impact {
    params: Arc<ImpactParams>,
    voice: KickVoice,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct ImpactParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "fstart"] pub fstart: FloatParam,
    #[id = "fend"] pub fend: FloatParam,
    #[id = "pdecay"] pub pdecay: FloatParam,
    #[id = "pcurve"] pub pcurve: FloatParam,
    #[id = "length"] pub length: FloatParam,
    #[id = "adecay"] pub adecay: FloatParam,
    #[id = "acurve"] pub acurve: FloatParam,
    #[id = "tone"] pub tone: FloatParam,
    #[id = "drive"] pub drive: FloatParam,
    #[id = "shape"] pub shape: IntParam,
    #[id = "clip"] pub clip_soft: BoolParam,
    #[id = "clicklvl"] pub clicklvl: FloatParam,
    #[id = "clickdecay"] pub clickdecay: FloatParam,
    #[id = "clickfreq"] pub clickfreq: FloatParam,
    #[id = "trans"] pub trans: IntParam,
    #[id = "translvl"] pub translvl: FloatParam,
    #[id = "sublvl"] pub sublvl: FloatParam,
    #[id = "subratio"] pub subratio: FloatParam,
    #[id = "keytrack"] pub keytrack: BoolParam,
    #[id = "outgain"] pub outgain: FloatParam,
}

fn hz(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) },
    )
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_rounded(0))
}

fn ms(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) },
    )
    .with_unit(" ms")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl Default for ImpactParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 560),
            fstart: hz("Pitch Start", d.f_start, 30.0, 2000.0),
            fend: hz("Pitch End", d.f_end, 20.0, 400.0),
            pdecay: ms("Pitch Decay", d.pitch_decay_ms, 1.0, 500.0),
            pcurve: FloatParam::new("Pitch Curve", d.pitch_curve, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            length: FloatParam::new(
                "Length",
                d.length,
                FloatRange::Skewed { min: 0.1, max: 4.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            adecay: ms("Amp Decay", d.amp_decay_ms, 20.0, 3000.0),
            acurve: FloatParam::new("Amp Curve", d.amp_curve, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            tone: pct("Tone", d.tone),
            drive: pct("Drive", d.drive),
            shape: IntParam::new("Drive Shape", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        0 => "Tube",
                        1 => "Tape",
                        2 => "Fold",
                        _ => "Hard",
                    }
                    .to_string()
                })),
            clip_soft: BoolParam::new("Soft Clip", d.clip_soft),
            clicklvl: pct("Click Level", d.click_level),
            clickdecay: ms("Click Decay", d.click_decay_ms, 5.0, 50.0),
            clickfreq: hz("Click Freq", d.click_freq, 1000.0, 8000.0),
            trans: IntParam::new("Transient", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        0 => "Off",
                        1 => "Tick",
                        2 => "Snap",
                        _ => "Knock",
                    }
                    .to_string()
                })),
            translvl: pct("Transient Level", d.transient_level),
            sublvl: pct("Sub Level", d.sub_level),
            subratio: FloatParam::new(
                "Sub Ratio",
                d.sub_ratio,
                FloatRange::Linear { min: 0.25, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            keytrack: BoolParam::new("Key Track", false),
            outgain: FloatParam::new("Out", d.out_gain_db, FloatRange::Linear { min: -24.0, max: 6.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

impl ImpactParams {
    /// Snapshot the current parameter values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            f_start: self.fstart.value(),
            f_end: self.fend.value(),
            pitch_decay_ms: self.pdecay.value(),
            pitch_curve: self.pcurve.value(),
            length: self.length.value(),
            amp_decay_ms: self.adecay.value(),
            amp_curve: self.acurve.value(),
            tone: self.tone.value(),
            drive: self.drive.value(),
            shape: DriveShape::from_index(self.shape.value() as usize),
            clip_soft: self.clip_soft.value(),
            click_level: self.clicklvl.value(),
            click_decay_ms: self.clickdecay.value(),
            click_freq: self.clickfreq.value(),
            transient: self.trans.value() as usize,
            transient_level: self.translvl.value(),
            sub_level: self.sublvl.value(),
            sub_ratio: self.subratio.value(),
            out_gain_db: self.outgain.value(),
        }
    }
}

impl Default for Impact {
    fn default() -> Self {
        Self {
            params: Arc::new(ImpactParams::default()),
            voice: KickVoice::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset through the host (so automation/undo see the change).
fn apply_preset(params: &ImpactParams, setter: &ParamSetter, p: &Preset) {
    let d = Settings::default();
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_i = |param: &IntParam, v: i32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_b = |param: &BoolParam, v: bool| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    set_f(&params.fstart, g("fstart", d.f_start));
    set_f(&params.fend, g("fend", d.f_end));
    set_f(&params.pdecay, g("pdecay", d.pitch_decay_ms));
    set_f(&params.pcurve, g("pcurve", d.pitch_curve));
    set_f(&params.length, g("length", d.length));
    set_f(&params.adecay, g("adecay", d.amp_decay_ms));
    set_f(&params.acurve, g("acurve", d.amp_curve));
    set_f(&params.tone, g("tone", d.tone));
    set_f(&params.drive, g("drive", d.drive));
    set_i(&params.shape, g("shape", 0.0) as i32);
    set_b(&params.clip_soft, g("clip", 1.0) >= 0.5);
    set_f(&params.clicklvl, g("clicklvl", d.click_level));
    set_f(&params.clickdecay, g("clickdecay", d.click_decay_ms));
    set_f(&params.clickfreq, g("clickfreq", d.click_freq));
    set_i(&params.trans, g("trans", 0.0) as i32);
    set_f(&params.translvl, g("translvl", d.transient_level));
    set_f(&params.sublvl, g("sublvl", d.sub_level));
    set_f(&params.subratio, g("subratio", d.sub_ratio));
    set_b(&params.keytrack, g("keytrack", 0.0) >= 0.5);
    set_f(&params.outgain, g("outgain", d.out_gain_db));
}

impl Plugin for Impact {
    const NAME: &'static str = "Qeynos IMPACT";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Instrument: no main input, stereo (or mono) output.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            names: PortNames { layout: Some("Stereo"), ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            names: PortNames { layout: Some("Mono"), ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
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
                ResizableWindow::new("qeynos-impact-window")
                    .min_size(Vec2::new(480.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · IMPACT").color(suite_core::ui::ACCENT));
                        ui.label(
                            egui::RichText::new("kick drum synth")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset selector
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("PRESET").color(suite_core::ui::TEXT_DIM).small());
                            egui::ComboBox::from_id_salt("impact-preset")
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
                            ui.label(egui::RichText::new("PITCH").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("impact-pitch").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "START", &params.fstart, setter);
                                row(ui, "END", &params.fend, setter);
                                ui.end_row();
                                row(ui, "DECAY", &params.pdecay, setter);
                                row(ui, "CURVE", &params.pcurve, setter);
                                ui.end_row();
                            });
                            ui.horizontal(|ui| {
                                let mut kt = params.keytrack.value();
                                if ui.checkbox(&mut kt, "Key Track (note → pitch end)").changed() {
                                    setter.begin_set_parameter(&params.keytrack);
                                    setter.set_parameter(&params.keytrack, kt);
                                    setter.end_set_parameter(&params.keytrack);
                                }
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("AMP / LENGTH").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("impact-amp").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "LENGTH", &params.length, setter);
                                row(ui, "AMP DECAY", &params.adecay, setter);
                                ui.end_row();
                                row(ui, "AMP CURVE", &params.acurve, setter);
                                row(ui, "TONE", &params.tone, setter);
                                ui.end_row();
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("CLICK / TRANSIENT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("impact-click").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "CLICK LVL", &params.clicklvl, setter);
                                row(ui, "CLICK DECAY", &params.clickdecay, setter);
                                ui.end_row();
                                row(ui, "CLICK FREQ", &params.clickfreq, setter);
                                row(ui, "TRANSIENT", &params.trans, setter);
                                ui.end_row();
                                row(ui, "TRANS LVL", &params.translvl, setter);
                                ui.end_row();
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("SUB / DRIVE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("impact-sub").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "SUB LVL", &params.sublvl, setter);
                                row(ui, "SUB RATIO", &params.subratio, setter);
                                ui.end_row();
                                row(ui, "DRIVE", &params.drive, setter);
                                row(ui, "SHAPE", &params.shape, setter);
                                ui.end_row();
                                row(ui, "OUT", &params.outgain, setter);
                                ui.end_row();
                            });
                            ui.horizontal(|ui| {
                                let mut soft = params.clip_soft.value();
                                if ui.checkbox(&mut soft, "Soft clip output").changed() {
                                    setter.begin_set_parameter(&params.clip_soft);
                                    setter.set_parameter(&params.clip_soft, soft);
                                    setter.end_set_parameter(&params.clip_soft);
                                }
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
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.voice = KickVoice::new(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.voice.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let s = self.params.snapshot();
        self.voice.configure(&s);
        let keytrack = self.params.keytrack.value();

        let num_samples = buffer.samples();
        let out = buffer.as_slice();
        let num_ch = out.len();
        if num_ch == 0 {
            return ProcessStatus::KeepAlive;
        }

        let mut next_event = context.next_event();
        for n in 0..num_samples {
            while let Some(event) = next_event {
                if event.timing() > n as u32 {
                    break;
                }
                if let NoteEvent::NoteOn { note, velocity, .. } = event {
                    let key_hz = if keytrack {
                        Some(util::midi_note_to_freq(note))
                    } else {
                        None
                    };
                    self.voice.note_on(velocity, key_hz);
                }
                next_event = context.next_event();
            }

            let y = self.voice.process_sample();
            for ch in 0..num_ch {
                out[ch][n] = y;
            }
        }

        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for Impact {
    const CLAP_ID: &'static str = "com.qeynos.impact";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Kick drum synth — pitch/amp envelopes, click + embedded PCM transients, sub, drive");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Drum,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for Impact {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosIMPACTkik1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Drum];
}

nih_export_clap!(Impact);
nih_export_vst3!(Impact);

#[cfg(test)]
mod render_tests;
