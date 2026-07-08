//! EMBER — spectral fader / temporal smoother (Qeynos suite, Phase 1).
//!
//! A streaming STFT (2048 / hop 512 / Hann) feeds a per-bin state machine: each bin's
//! magnitude eases toward the input with an attack or decay time constant chosen from an
//! 8-breakpoint log-frequency curve. Decay τ reaches 60 s, so spectral tails keep ringing
//! after the input stops; **Freeze** holds the captured spectrum forever. Generated tails
//! stay tonal via a phase-vocoder phase advance. Fitting blends bins toward a ~1/3-octave
//! spectral envelope. Reports 2048-sample latency.
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

use dsp::{EmberCore, Settings, N_BANDS};
use suite_core::presets::{load_all, Preset};

/// Log-frequency center of factor band `idx` (0..N_BANDS-1), for GUI labels.
fn band_center_hz(idx: usize) -> f32 {
    let t = idx as f32 / (N_BANDS - 1) as f32;
    dsp::F_LO * (dsp::F_HI / dsp::F_LO).powf(t)
}

fn fmt_hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1}k", f / 1000.0)
    } else {
        format!("{:.0}", f)
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Ember {
    params: Arc<EmberParams>,
    core: EmberCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct EmberParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // 8 attack-band breakpoints (ms), low → high frequency.
    #[id = "atk0"] pub atk0: FloatParam,
    #[id = "atk1"] pub atk1: FloatParam,
    #[id = "atk2"] pub atk2: FloatParam,
    #[id = "atk3"] pub atk3: FloatParam,
    #[id = "atk4"] pub atk4: FloatParam,
    #[id = "atk5"] pub atk5: FloatParam,
    #[id = "atk6"] pub atk6: FloatParam,
    #[id = "atk7"] pub atk7: FloatParam,

    // 8 decay-band breakpoints (ms, up to 60 000), low → high frequency.
    #[id = "dec0"] pub dec0: FloatParam,
    #[id = "dec1"] pub dec1: FloatParam,
    #[id = "dec2"] pub dec2: FloatParam,
    #[id = "dec3"] pub dec3: FloatParam,
    #[id = "dec4"] pub dec4: FloatParam,
    #[id = "dec5"] pub dec5: FloatParam,
    #[id = "dec6"] pub dec6: FloatParam,
    #[id = "dec7"] pub dec7: FloatParam,

    #[id = "fitting"] pub fitting: FloatParam,
    #[id = "freeze"] pub freeze: BoolParam,
    #[id = "freezemix"] pub freeze_mix: FloatParam,
    #[id = "gate"] pub gate: FloatParam,
    #[id = "tailgain"] pub tailgain: FloatParam,
    #[id = "mix"] pub mix: FloatParam,
}

fn atk_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        20.0,
        FloatRange::Skewed { min: 1.0, max: 2000.0, factor: FloatRange::skew_factor(-2.0) },
    )
    .with_unit(" ms")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
}

fn dec_param(name: &'static str) -> FloatParam {
    FloatParam::new(
        name,
        800.0,
        // 5 ms .. 60 s, strongly log-skewed so the long-tail region is reachable.
        FloatRange::Skewed { min: 5.0, max: 60_000.0, factor: FloatRange::skew_factor(-2.5) },
    )
    .with_unit(" ms")
    .with_value_to_string(formatters::v2s_f32_rounded(0))
}

impl Default for EmberParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(620, 460),
            atk0: atk_param("Atk 1"),
            atk1: atk_param("Atk 2"),
            atk2: atk_param("Atk 3"),
            atk3: atk_param("Atk 4"),
            atk4: atk_param("Atk 5"),
            atk5: atk_param("Atk 6"),
            atk6: atk_param("Atk 7"),
            atk7: atk_param("Atk 8"),
            dec0: dec_param("Dec 1"),
            dec1: dec_param("Dec 2"),
            dec2: dec_param("Dec 3"),
            dec3: dec_param("Dec 4"),
            dec4: dec_param("Dec 5"),
            dec5: dec_param("Dec 6"),
            dec6: dec_param("Dec 7"),
            dec7: dec_param("Dec 8"),
            fitting: FloatParam::new("Fitting", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            freeze: BoolParam::new("Freeze", false),
            freeze_mix: FloatParam::new("Freeze Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            gate: FloatParam::new("Gate", -60.0, FloatRange::Linear { min: -90.0, max: 0.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            tailgain: FloatParam::new(
                "Tail Gain",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl EmberParams {
    fn atk_refs(&self) -> [&FloatParam; N_BANDS] {
        [
            &self.atk0, &self.atk1, &self.atk2, &self.atk3, &self.atk4, &self.atk5, &self.atk6,
            &self.atk7,
        ]
    }
    fn dec_refs(&self) -> [&FloatParam; N_BANDS] {
        [
            &self.dec0, &self.dec1, &self.dec2, &self.dec3, &self.dec4, &self.dec5, &self.dec6,
            &self.dec7,
        ]
    }

    /// Snapshot the current (un-smoothed) values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        let mut attack_ms = [0.0f32; N_BANDS];
        let mut decay_ms = [0.0f32; N_BANDS];
        for (i, p) in self.atk_refs().iter().enumerate() {
            attack_ms[i] = p.value();
        }
        for (i, p) in self.dec_refs().iter().enumerate() {
            decay_ms[i] = p.value();
        }
        Settings {
            attack_ms,
            decay_ms,
            fitting: self.fitting.value(),
            freeze: self.freeze.value(),
            freeze_mix: self.freeze_mix.value(),
            gate_db: self.gate.value(),
            tail_gain_db: self.tailgain.value(),
            mix: self.mix.value(),
        }
    }
}

impl Default for Ember {
    fn default() -> Self {
        Self {
            params: Arc::new(EmberParams::default()),
            core: EmberCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see
/// the change).
fn apply_preset(params: &EmberParams, setter: &ParamSetter, p: &Preset) {
    let d = Settings::default();
    let atk = presets::band_from_preset(p, "atk", "attack", &d.attack_ms);
    let dec = presets::band_from_preset(p, "dec", "decay", &d.decay_ms);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    for (i, param) in params.atk_refs().iter().enumerate() {
        set_f(param, atk[i]);
    }
    for (i, param) in params.dec_refs().iter().enumerate() {
        set_f(param, dec[i]);
    }
    set_f(&params.fitting, p.get("fitting").unwrap_or(d.fitting));
    set_f(&params.gate, p.get("gate").unwrap_or(d.gate_db));
    set_f(&params.tailgain, p.get("tailgain").unwrap_or(d.tail_gain_db));
    set_f(&params.mix, p.get("mix").unwrap_or(d.mix));

    let freeze = p.get("freeze").unwrap_or(0.0) >= 0.5;
    setter.begin_set_parameter(&params.freeze);
    setter.set_parameter(&params.freeze, freeze);
    setter.end_set_parameter(&params.freeze);
}

fn band_row(ui: &mut egui::Ui, label: &str, refs: &[&FloatParam; N_BANDS], setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(suite_core::ui::TEXT_DIM).small());
        for (i, param) in refs.iter().enumerate() {
            ui.vertical(|ui| {
                ui.add(
                    nih_plug_egui::widgets::ParamSlider::for_param(*param, setter)
                        .without_value(),
                );
                ui.label(
                    egui::RichText::new(fmt_hz(band_center_hz(i)))
                        .color(suite_core::ui::TEXT_DIM)
                        .small(),
                );
            });
        }
    });
}

impl Plugin for Ember {
    const NAME: &'static str = "Qeynos EMBER";
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
                suite_core::ui::ScaledWindow::new("qeynos-ember-window", Vec2::new(620.0, 460.0))
                    .min_size(Vec2::new(520.0, 400.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · EMBER").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new("spectral fader / temporal smoother")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("ember", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("FACTOR BANDS  (time constant per frequency)")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            band_row(ui, "ATTACK", &params.atk_refs(), setter);
                            band_row(ui, "DECAY ", &params.dec_refs(), setter);
                            ui.separator();

                            use suite_core::ui::labeled_slider as row;
                            egui::Grid::new("ember-macros")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "FITTING", &params.fitting, setter);
                                    row(ui, "GATE", &params.gate, setter);
                                    ui.end_row();
                                    row(ui, "TAIL GAIN", &params.tailgain, setter);
                                    row(ui, "MIX", &params.mix, setter);
                                    ui.end_row();
                                });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("FREEZE")
                                        .color(suite_core::ui::ACCENT)
                                        .small(),
                                );
                                let mut fz = params.freeze.value();
                                if ui.checkbox(&mut fz, "hold spectrum").changed() {
                                    setter.begin_set_parameter(&params.freeze);
                                    setter.set_parameter(&params.freeze, fz);
                                    setter.end_set_parameter(&params.freeze);
                                }
                            });
                            row(ui, "FREEZE MIX", &params.freeze_mix, setter);
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
        self.core = EmberCore::new(buffer_config.sample_rate);
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

impl ClapPlugin for Ember {
    const CLAP_ID: &'static str = "com.qeynos.ember";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Spectral fader / temporal smoother — per-bin STFT state machine with freeze");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Reverb,
    ];
}

impl Vst3Plugin for Ember {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosEMBERspct1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_clap!(Ember);
nih_export_vst3!(Ember);

#[cfg(test)]
mod render_tests {
    use crate::dsp::EmberCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;

    /// Render each factory preset with a 1 s pink-noise burst then 2 s of silence (so the
    /// spectral tail is audible in the WAV), write to renders/EMBER/, assert universal.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let burst = (sr * 1.0) as usize;
        let tail = (sr * 2.0) as usize;
        let n = burst + tail;

        let mut input = suite_core::testsig::pink_noise(0.5, n, 4242);
        for v in input.iter_mut().skip(burst) {
            *v = 0.0;
        }

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5);
        for p in &presets {
            let mut s = settings_from_preset(p);
            // Freeze is a live control: build the spectrum during the burst with freeze
            // off, then engage it for the tail (matches real usage and keeps the render
            // non-silent).
            let render_freeze = s.freeze;
            s.freeze = false;
            let mut core = EmberCore::new(sr);
            core.configure(&s);
            let mut out = vec![0.0f32; n];
            for i in 0..n {
                if i == burst && render_freeze {
                    s.freeze = true;
                    core.configure(&s);
                }
                let (y, _) = core.process_sample(input[i], input[i], s.mix);
                out[i] = y;
            }
            assert_universal(&out);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
            let path = render_path("EMBER", &fname);
            write_wav(&path, &out, sr as u32).expect("write render");
        }
    }
}
