//! SMUDGE — spectral chaos (Qeynos suite, Phase 2a; Smear clone).
//!
//! A streaming STFT (2048 / hop 512 / Hann) feeds four per-frame spectral ops applied in the
//! FIXED order 1→4, each with its own amount param that is EXACTLY bypassed at 0:
//!   1. **Scramble** — permute bins within ±N-bin neighbourhoods, redrawn on a settable rate.
//!   2. **Spectral delay** — per-~1/3-octave-band frame delays (tilt curve) with feedback.
//!   3. **Blur** — per-bin temporal magnitude averaging (τ per band) + phase-vocoder advance.
//!   4. **Smear/stretch** — bin-index remap ×0.5–2 (energy-normalised).
//! A **chaos** macro slow-S&H-modulates the op params. Reports 2048-sample latency.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared with the offline harness tests) atop
//! `suite_core::stft`.

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

use dsp::{Settings, SmudgeCore};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Smudge {
    params: Arc<SmudgeParams>,
    core: SmudgeCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct SmudgeParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // op 1: scramble
    #[id = "scramble"] pub scramble: FloatParam,
    #[id = "srange"] pub srange: FloatParam,
    #[id = "srate"] pub srate: IntParam,

    // op 2: spectral delay
    #[id = "delay"] pub delay: FloatParam,
    #[id = "dtilt"] pub dtilt: FloatParam,
    #[id = "dfb"] pub dfb: FloatParam,

    // op 3: blur
    #[id = "blur"] pub blur: FloatParam,
    #[id = "btau"] pub btau: FloatParam,
    #[id = "btilt"] pub btilt: FloatParam,

    // op 4: smear / stretch
    #[id = "stretch"] pub stretch: FloatParam,
    #[id = "sfactor"] pub sfactor: FloatParam,

    // chaos macro
    #[id = "crate"] pub crate_: IntParam,
    #[id = "cdepth"] pub cdepth: FloatParam,

    #[id = "mix"] pub mix: FloatParam,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn bipolar(name: &'static str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
        .with_value_to_string(formatters::v2s_f32_rounded(2))
}

fn frames_param(name: &'static str, default: i32, max: i32) -> IntParam {
    IntParam::new(name, default, IntRange::Linear { min: 1, max })
        .with_unit(" fr")
        .with_value_to_string(Arc::new(|v| v.to_string()))
        .with_string_to_value(Arc::new(|s| {
            s.split_whitespace().next().and_then(|t| t.parse::<i32>().ok())
        }))
}

impl Default for SmudgeParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(620, 520),

            scramble: pct("Scramble", d.scramble_amt),
            srange: pct("Scramble Range", d.scramble_range),
            srate: frames_param("Scramble Rate", d.scramble_rate as i32, 32),

            delay: pct("Delay", d.delay_amt),
            dtilt: bipolar("Delay Tilt"),
            dfb: FloatParam::new(
                "Delay Feedback",
                d.delay_feedback,
                FloatRange::Linear { min: 0.0, max: dsp::MAX_DELAY_FEEDBACK },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            blur: pct("Blur", d.blur_amt),
            btau: FloatParam::new(
                "Blur Time",
                d.blur_tau_ms,
                FloatRange::Skewed {
                    min: dsp::MIN_BLUR_TAU_MS,
                    max: dsp::MAX_BLUR_TAU_MS,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            btilt: bipolar("Blur Tilt"),

            stretch: pct("Stretch", d.stretch_amt),
            sfactor: FloatParam::new(
                "Stretch Factor",
                d.stretch_factor,
                FloatRange::Skewed {
                    min: dsp::MIN_STRETCH,
                    max: dsp::MAX_STRETCH,
                    factor: FloatRange::skew_factor(0.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            crate_: frames_param("Chaos Rate", d.chaos_rate as i32, 512),
            cdepth: pct("Chaos Depth", d.chaos_depth),

            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl SmudgeParams {
    /// Snapshot the current parameter values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            scramble_amt: self.scramble.value(),
            scramble_range: self.srange.value(),
            scramble_rate: self.srate.value().max(1) as u32,
            delay_amt: self.delay.value(),
            delay_tilt: self.dtilt.value(),
            delay_feedback: self.dfb.value(),
            blur_amt: self.blur.value(),
            blur_tau_ms: self.btau.value(),
            blur_tilt: self.btilt.value(),
            stretch_amt: self.stretch.value(),
            stretch_factor: self.sfactor.value(),
            chaos_rate: self.crate_.value().max(1) as u32,
            chaos_depth: self.cdepth.value(),
            mix: self.mix.value(),
        }
    }
}

impl Default for Smudge {
    fn default() -> Self {
        Self {
            params: Arc::new(SmudgeParams::default()),
            core: SmudgeCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &SmudgeParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
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
    set_f(&params.scramble, s.scramble_amt);
    set_f(&params.srange, s.scramble_range);
    set_i(&params.srate, s.scramble_rate as i32);
    set_f(&params.delay, s.delay_amt);
    set_f(&params.dtilt, s.delay_tilt);
    set_f(&params.dfb, s.delay_feedback);
    set_f(&params.blur, s.blur_amt);
    set_f(&params.btau, s.blur_tau_ms);
    set_f(&params.btilt, s.blur_tilt);
    set_f(&params.stretch, s.stretch_amt);
    set_f(&params.sfactor, s.stretch_factor);
    set_i(&params.crate_, s.chaos_rate as i32);
    set_f(&params.cdepth, s.chaos_depth);
    set_f(&params.mix, s.mix);
}

impl Plugin for Smudge {
    const NAME: &'static str = "Qeynos SMUDGE";
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
                suite_core::ui::ScaledWindow::new("qeynos-smudge-window", Vec2::new(620.0, 520.0))
                    .min_size(Vec2::new(520.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · SMUDGE").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new("spectral chaos — scramble · delay · blur · stretch")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset selector
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("PRESET")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::ComboBox::from_id_salt("smudge-preset")
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
                            use suite_core::ui::labeled_slider as row;

                            section(ui, "1 · SCRAMBLE");
                            egui::Grid::new("smudge-scramble").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "AMOUNT", &params.scramble, setter);
                                row(ui, "RANGE", &params.srange, setter);
                                ui.end_row();
                                row(ui, "RATE", &params.srate, setter);
                                ui.end_row();
                            });

                            section(ui, "2 · SPECTRAL DELAY");
                            egui::Grid::new("smudge-delay").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "AMOUNT", &params.delay, setter);
                                row(ui, "TILT", &params.dtilt, setter);
                                ui.end_row();
                                row(ui, "FEEDBACK", &params.dfb, setter);
                                ui.end_row();
                            });

                            section(ui, "3 · BLUR");
                            egui::Grid::new("smudge-blur").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "AMOUNT", &params.blur, setter);
                                row(ui, "TIME", &params.btau, setter);
                                ui.end_row();
                                row(ui, "TILT", &params.btilt, setter);
                                ui.end_row();
                            });

                            section(ui, "4 · SMEAR / STRETCH");
                            egui::Grid::new("smudge-stretch").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "AMOUNT", &params.stretch, setter);
                                row(ui, "FACTOR", &params.sfactor, setter);
                                ui.end_row();
                            });

                            section(ui, "CHAOS · OUTPUT");
                            egui::Grid::new("smudge-chaos").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "CHAOS RATE", &params.crate_, setter);
                                row(ui, "CHAOS DEPTH", &params.cdepth, setter);
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
        self.core = SmudgeCore::new(buffer_config.sample_rate);
        context.set_latency_samples(self.core.latency() as u32);
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
        self.core.configure(&s);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l = main[0][n];
            let r = if num_main > 1 { main[1][n] } else { l };
            let mix = self.params.mix.smoothed.next();
            let (out_l, out_r) = self.core.process_sample(l, r, mix);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        ProcessStatus::Normal
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).color(suite_core::ui::ACCENT).small());
    ui.separator();
}

impl ClapPlugin for Smudge {
    const CLAP_ID: &'static str = "com.qeynos.smudge";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Spectral chaos — STFT bin scramble, spectral delay, blur, and smear/stretch");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Glitch,
    ];
}

impl Vst3Plugin for Smudge {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosSMUDGEspc1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Smudge);
nih_export_vst3!(Smudge);

#[cfg(test)]
mod render_tests {
    use crate::dsp::SmudgeCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;

    /// Render each factory preset with a 2 s pink-noise burst then 1 s of silence (so spectral
    /// delay/blur tails are audible in the WAV), write to renders/SMUDGE/, assert universal.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let burst = (sr * 2.0) as usize;
        let tail = (sr * 1.0) as usize;
        let n = burst + tail;

        let mut input = suite_core::testsig::pink_noise(0.5, n, 1357);
        for v in input.iter_mut().skip(burst) {
            *v = 0.0;
        }

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let mut core = SmudgeCore::new(sr);
            let mut out = input.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
            let path = render_path("SMUDGE", &fname);
            write_wav(&path, &out, sr as u32).expect("write render");
        }
    }
}
