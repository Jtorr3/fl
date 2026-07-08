//! SWARM — mass granulator (Qeynos suite, Phase 2a; Glow clone).
//!
//! A **10-second stereo capture buffer** feeds a **grain scheduler** (poisson or tempo-grid) that
//! spawns up to **128 concurrent grains**. Each grain is randomised at spawn — position spray,
//! pitch scatter (±24 st, free or semitone-quantised), size 10–500 ms, Tukey window, equal-power
//! pan within the width, reverse probability — and reads an interpolated window of the buffer.
//! The grain sum optionally drives a **+12 st shimmer** feedback send (in-loop `tanh` limiter + DC
//! blocker) that re-enters the buffer to bloom. **Freeze** locks the write head for infinite,
//! evolving textures.
//!
//! Like OUROBOROS, a granulator is a **time-smearing effect** (not a fixed-latency FIR stage), so
//! SWARM reports **zero latency** and nulls at `mix = 0` against the dry input. See [`dsp`] for the
//! DSP core, shared verbatim with the offline harness / done-bar tests.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

use dsp::{Settings, SwarmCore, SyncDivision};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing division enum (mapped onto the pure-DSP enum).
// ---------------------------------------------------------------------------

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
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Swarm {
    params: Arc<SwarmParams>,
    core: SwarmCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct SwarmParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "density"]
    pub density: FloatParam,
    #[id = "size"]
    pub size: FloatParam,
    #[id = "spray"]
    pub spray: FloatParam,
    #[id = "scatter"]
    pub scatter: FloatParam,
    #[id = "quantize"]
    pub quantize: BoolParam,
    #[id = "reverse"]
    pub reverse: FloatParam,
    #[id = "shimmer"]
    pub shimmer: FloatParam,
    #[id = "freeze"]
    pub freeze: BoolParam,
    #[id = "freezemix"]
    pub freeze_mix: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "division"]
    pub division: EnumParam<DivisionParam>,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,
}

impl Default for SwarmParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(600, 660),
            density: FloatParam::new(
                "Density",
                d.density,
                FloatRange::Skewed {
                    min: dsp::MIN_DENSITY,
                    max: dsp::MAX_DENSITY,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" gr/s")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            size: FloatParam::new(
                "Size",
                d.size_ms,
                FloatRange::Skewed {
                    min: dsp::MIN_SIZE_MS,
                    max: dsp::MAX_SIZE_MS,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            spray: FloatParam::new(
                "Spray",
                d.spray_ms,
                FloatRange::Skewed {
                    min: 0.0,
                    max: dsp::MAX_SPRAY_MS,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            scatter: FloatParam::new(
                "Scatter",
                d.scatter_st,
                FloatRange::Linear {
                    min: 0.0,
                    max: dsp::MAX_SCATTER_ST,
                },
            )
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            quantize: BoolParam::new("Quantize", d.quantize),
            reverse: FloatParam::new("Reverse", d.reverse_prob, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            shimmer: FloatParam::new("Shimmer", d.shimmer, FloatRange::Linear { min: 0.0, max: 1.1 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            freeze: BoolParam::new("Freeze", d.freeze),
            freeze_mix: FloatParam::new("Freeze Mix", d.freeze_mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            sync: BoolParam::new("Sync", d.sync),
            division: EnumParam::new("Division", DivisionParam::Sixteenth),
            width: FloatParam::new("Width", d.width, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
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

impl SwarmParams {
    /// Snapshot the live parameters into a DSP [`Settings`]. `tempo_bpm` comes from the host.
    fn snapshot(&self, tempo_bpm: f32) -> Settings {
        Settings {
            density: self.density.value(),
            size_ms: self.size.value(),
            spray_ms: self.spray.value(),
            scatter_st: self.scatter.value(),
            quantize: self.quantize.value(),
            reverse_prob: self.reverse.value(),
            shimmer: self.shimmer.value(),
            freeze: self.freeze.value(),
            freeze_mix: self.freeze_mix.value(),
            sync: self.sync.value(),
            division: self.division.value().to_dsp(),
            tempo_bpm,
            width: self.width.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Swarm {
    fn default() -> Self {
        Self {
            params: Arc::new(SwarmParams::default()),
            core: SwarmCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &SwarmParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    setter.begin_set_parameter(&params.quantize);
    setter.set_parameter(&params.quantize, g("quantize", 0.0) >= 0.5);
    setter.end_set_parameter(&params.quantize);
    setter.begin_set_parameter(&params.freeze);
    setter.set_parameter(&params.freeze, g("freeze", 0.0) >= 0.5);
    setter.end_set_parameter(&params.freeze);
    setter.begin_set_parameter(&params.sync);
    setter.set_parameter(&params.sync, g("sync", 0.0) >= 0.5);
    setter.end_set_parameter(&params.sync);
    setter.begin_set_parameter(&params.division);
    setter.set_parameter(&params.division, DivisionParam::from_index(g("division", 0.0) as usize));
    setter.end_set_parameter(&params.division);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.density, g("density", 40.0));
    set_f(&params.size, g("size", 120.0));
    set_f(&params.spray, g("spray", 80.0));
    set_f(&params.scatter, g("scatter", 4.0));
    set_f(&params.reverse, g("reverse", 0.0));
    set_f(&params.shimmer, g("shimmer", 0.0));
    set_f(&params.width, g("width", 0.7));
    set_f(&params.mix, g("mix", 0.5));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Swarm {
    const NAME: &'static str = "Qeynos SWARM";
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
                suite_core::ui::ScaledWindow::new("qeynos-swarm-window", Vec2::new(600.0, 660.0))
                    .min_size(Vec2::new(520.0, 540.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · SWARM").color(suite_core::ui::ACCENT));
                        ui.label(
                            egui::RichText::new("mass granulator — 10 s buffer, ≤128 grains, shimmer, freeze")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset selector.
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("PRESET").color(suite_core::ui::TEXT_DIM).small());
                            egui::ComboBox::from_id_salt("swarm-preset")
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
                            ui.label(egui::RichText::new("CLOUD").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("swarm-cloud")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "DENSITY", &params.density, setter);
                                    row(ui, "SIZE", &params.size, setter);
                                    ui.end_row();
                                    row(ui, "SPRAY", &params.spray, setter);
                                    row(ui, "SCATTER", &params.scatter, setter);
                                    ui.end_row();
                                    row(ui, "REVERSE", &params.reverse, setter);
                                    row(ui, "SHIMMER", &params.shimmer, setter);
                                    ui.end_row();
                                });
                            ui.horizontal(|ui| {
                                let mut q = params.quantize.value();
                                if ui.checkbox(&mut q, "QUANTIZE").changed() {
                                    setter.begin_set_parameter(&params.quantize);
                                    setter.set_parameter(&params.quantize, q);
                                    setter.end_set_parameter(&params.quantize);
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
                                ui.label(egui::RichText::new("SCHEDULER").color(suite_core::ui::TEXT_DIM).small());
                                ui.add_space(8.0);
                                let mut sy = params.sync.value();
                                if ui.checkbox(&mut sy, "GRID SYNC").changed() {
                                    setter.begin_set_parameter(&params.sync);
                                    setter.set_parameter(&params.sync, sy);
                                    setter.end_set_parameter(&params.sync);
                                }
                                ui.add_space(12.0);
                                row(ui, "DIVISION", &params.division, setter);
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("OUTPUT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("swarm-out")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "WIDTH", &params.width, setter);
                                    row(ui, "MIX", &params.mix, setter);
                                    ui.end_row();
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
        // Preallocate the grain pool + 10 s capture buffer for this sample rate (off the audio
        // thread) so process() is allocation-free.
        self.core = SwarmCore::new(buffer_config.sample_rate);
        // A granulator is a time-smearing effect, not fixed latency ⇒ zero reported latency.
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

impl ClapPlugin for Swarm {
    const CLAP_ID: &'static str = "com.qeynos.swarm";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Mass granulator — a 10 s capture buffer sprayed into up to 128 concurrent grains \
         (poisson or tempo-grid scheduler) with pitch scatter, size, position spray, reverse, \
         equal-power pan, a +12 st shimmer feedback bloom, and freeze",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Custom("granular"),
    ];
}

impl Vst3Plugin for Swarm {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosSWARM00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Custom("Granular")];
}

nih_export_clap!(Swarm);
nih_export_vst3!(Swarm);

#[cfg(test)]
mod render_tests {
    use crate::dsp::SwarmCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, write_wav, render_path};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    /// Render each factory preset over pink noise and a full-band chirp, write the WAVs into
    /// renders/SWARM/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.5, (sr * 4.0) as usize, 4242);
        let chirp = testsig::log_chirp(40.0, 12_000.0, 0.5, (sr * 4.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");

            let mut core = SwarmCore::new(sr);
            let mut out = pink.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("SWARM", &format!("{fname}_pink"));
            write_wav(&path, &out, sr as u32).expect("write pink render");

            let mut core = SwarmCore::new(sr);
            let mut out = chirp.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("SWARM", &format!("{fname}_chirp"));
            write_wav(&path, &out, sr as u32).expect("write chirp render");
        }
    }
}
