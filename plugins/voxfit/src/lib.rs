//! VOXFIT — vocal character conformer (Qeynos VOX suite, final plugin).
//!
//! Makes a ripped or foreign acapella *sit* in a completely different production. A pitch-
//! independent formant shift (`suite_core::shift::ShiftEngine` in formant-only mode) reshapes the
//! timbre, then a de-esser (5–9 kHz), a dynamic harshness tamer (2–5 kHz bell cut), a tilt EQ
//! (complementary shelves at 1 kHz), a proximity low-mid shelf, and an air high shelf finish the
//! character. The **SIT** macro sweeps a curated combination tuned for dropping a bright pop vocal
//! into a dark mix. See [`dsp`] for the core.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

#[cfg(test)]
mod tests;

use dsp::{Controls, VoxFitCore};
use suite_core::bus::PluginKind;
use suite_core::presets::{load_all, Preset};
use suite_core::spectrum::SpectrumPublisher;

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/VOXFIT.md");

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct VoxFit {
    params: Arc<VoxFitParams>,
    core: VoxFitCore,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct VoxFitParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "formant"] pub formant: FloatParam,
    #[id = "dsthresh"] pub deess_thresh: FloatParam,
    #[id = "deess"] pub deess: FloatParam,
    #[id = "listen"] pub listen: BoolParam,
    #[id = "hsthresh"] pub harsh_thresh: FloatParam,
    #[id = "harsh"] pub harsh: FloatParam,
    #[id = "tilt"] pub tilt: FloatParam,
    #[id = "prox"] pub prox: FloatParam,
    #[id = "air"] pub air: FloatParam,
    #[id = "sit"] pub sit: FloatParam,
    #[id = "mix"] pub mix: FloatParam,
    #[id = "out"] pub out_trim: FloatParam,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn first_number(s: &str) -> Option<f32> {
    s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
}

fn db(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
        .with_string_to_value(Arc::new(first_number))
}

impl Default for VoxFitParams {
    fn default() -> Self {
        let d = Controls::default();
        Self {
            editor_state: EguiState::from_size(540, 660),

            formant: FloatParam::new(
                "Formant",
                d.formant_st,
                FloatRange::Linear { min: -5.0, max: 5.0 },
            )
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(first_number)),

            deess_thresh: db("De-Ess Thresh", d.deess_thresh_db, -60.0, 0.0),
            deess: pct("De-Ess", d.deess_amount),
            listen: BoolParam::new("De-Ess Listen", d.deess_listen),

            harsh_thresh: db("Harsh Thresh", d.harsh_thresh_db, -60.0, 0.0),
            harsh: pct("Harsh", d.harsh_amount),

            tilt: db("Tilt", d.tilt_db, -6.0, 6.0),
            prox: db("Proximity", d.prox_db, -6.0, 6.0),
            air: db("Air", d.air_db, -6.0, 6.0),

            sit: pct("Sit", d.sit),
            mix: pct("Mix", d.mix),

            out_trim: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 12.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .with_string_to_value(Arc::new(first_number)),
        }
    }
}

impl VoxFitParams {
    /// Snapshot the params into [`Controls`].
    fn controls(&self) -> Controls {
        Controls {
            formant_st: self.formant.value(),
            deess_thresh_db: self.deess_thresh.value(),
            deess_amount: self.deess.value(),
            deess_listen: self.listen.value(),
            harsh_thresh_db: self.harsh_thresh.value(),
            harsh_amount: self.harsh.value(),
            tilt_db: self.tilt.value(),
            prox_db: self.prox.value(),
            air_db: self.air.value(),
            sit: self.sit.value(),
            mix: self.mix.value(),
            out_db: self.out_trim.value(),
        }
    }
}

impl Default for VoxFit {
    fn default() -> Self {
        Self {
            params: Arc::new(VoxFitParams::default()),
            core: VoxFitCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (automation/undo aware).
fn apply_preset(params: &VoxFitParams, setter: &ParamSetter, p: &Preset) {
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let set_b = |param: &BoolParam, v: bool| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    let c = presets::controls_from_preset(p);
    set_f(&params.formant, c.formant_st);
    set_f(&params.deess_thresh, c.deess_thresh_db);
    set_f(&params.deess, c.deess_amount);
    set_b(&params.listen, c.deess_listen);
    set_f(&params.harsh_thresh, c.harsh_thresh_db);
    set_f(&params.harsh, c.harsh_amount);
    set_f(&params.tilt, c.tilt_db);
    set_f(&params.prox, c.prox_db);
    set_f(&params.air, c.air_db);
    set_f(&params.sit, c.sit);
    set_f(&params.mix, c.mix);
    set_f(&params.out_trim, c.out_db);
}

impl Plugin for VoxFit {
    const NAME: &'static str = "Qeynos VOXFIT";
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
            main_output_channels: NonZeroU32::new(1),
            names: PortNames { layout: Some("Mono"), ..PortNames::const_default() },
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
                suite_core::ui::ScaledWindow::new("qeynos-voxfit-window", Vec2::new(540.0, 660.0))
                    .min_size(Vec2::new(480.0, 560.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · VOXFIT").color(suite_core::ui::ACCENT));
                        suite_core::ui::manual_button(ui, "voxfit", "VOXFIT", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("vocal character conformer — make a foreign acapella sit")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("voxfit", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        ui.separator();

                        // CONSOLE v2 CRT telemetry bay — honest live param state (the same
                        // values shown on the knobs below; GUI-thread reads only). THEME-OFF ⇒
                        // plain readout panel.
                        suite_core::ui::crt_lines(
                            ui,
                            "voxfit-crt",
                            "VOXFIT · VOX CONFORM",
                            &[
                                ("MACRO", format!("sit {} · form {}", params.sit, params.formant)),
                                ("DE-ESS", format!("thr {} · amt {}{}", params.deess_thresh, params.deess, if params.listen.value() { " · LISTEN" } else { "" })),
                                ("HARSH", format!("thr {} · amt {}", params.harsh_thresh, params.harsh)),
                                ("TONE", format!("tilt {} · prox {} · air {}", params.tilt, params.prox, params.air)),
                                ("OUT", format!("mix {} · {}", params.mix, params.out_trim)),
                            ],
                        );
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            // The macro up top.
                            ui.label(egui::RichText::new("MACRO").color(suite_core::ui::ACCENT).small());
                            egui::Grid::new("voxfit-macro").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "SIT", &params.sit, setter);
                                row(ui, "FORMANT", &params.formant, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("DE-ESS").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("voxfit-deess").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "THRESH", &params.deess_thresh, setter);
                                row(ui, "AMOUNT", &params.deess, setter);
                                ui.end_row();
                                row(ui, "LISTEN", &params.listen, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("HARSH TAMER").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("voxfit-harsh").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "THRESH", &params.harsh_thresh, setter);
                                row(ui, "AMOUNT", &params.harsh, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("TONE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("voxfit-tone").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "TILT", &params.tilt, setter);
                                row(ui, "PROXIMITY", &params.prox, setter);
                                ui.end_row();
                                row(ui, "AIR", &params.air, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.separator();
                            egui::Grid::new("voxfit-out").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "MIX", &params.mix, setter);
                                row(ui, "OUT", &params.out_trim, setter);
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
        self.core = VoxFitCore::new(buffer_config.sample_rate);
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "VOXFIT");
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

        let s = self.params.controls().resolve();
        let num_samples = buffer.samples();
        self.core.configure(&s, num_samples);

        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l = main[0][n];
            let r = if num_main > 1 { main[1][n] } else { l };
            let (out_l, out_r) = self.core.process_sample(l, r);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        // Publish this instance's output spectrum to the suite bus (X-RAY reads it).
        for mut xr_frame in buffer.iter_samples() {
            let xr_n = xr_frame.len().max(1) as f32;
            let mut xr_m = 0.0f32;
            for xr_s in xr_frame.iter_mut() {
                xr_m += *xr_s;
            }
            self.spectrum.feed(xr_m / xr_n);
        }
        self.spectrum.publish();
        ProcessStatus::Normal
    }
}

impl Drop for VoxFit {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for VoxFit {
    const CLAP_ID: &'static str = "com.qeynos.voxfit";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Vocal character conformer — pitch-independent formant shift, de-esser, harshness tamer, tilt/proximity/air EQ, SIT macro");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Equalizer,
        ClapFeature::Custom("vocal"),
    ];
}

impl Vst3Plugin for VoxFit {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosVOXFITvox2";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_clap!(VoxFit);
nih_export_vst3!(VoxFit);

#[cfg(test)]
mod manual_tests {
    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(
            crate::MANUAL_DOC,
            &crate::VoxFitParams::default(),
        );
    }
}
