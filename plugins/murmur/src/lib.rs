//! MURMUR — stochastic reverb (Qeynos suite, Phase 2a; Hikari clone).
//!
//! An 8×8 Householder feedback delay network (`suite_core::fdn::Fdn8`) whose room is
//! **re-randomised on every onset**: a spectral-flux-style onset detector (or the manual
//! re-roll button) triggers a fresh random draw of delay lengths, diffusion coefficient, and
//! damping color into an idle second FDN instance, then a 50 ms equal-power crossfade swaps to
//! it — every hit is a different room, click-free. Freeze sends the feedback to (near-)infinity
//! and ducks the input so the current wash sustains forever.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared with the offline harness tests) atop the
//! reusable `suite_core::fdn` FDN core.

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

use dsp::{MurmurCore, Settings};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Murmur {
    params: Arc<MurmurParams>,
    core: MurmurCore,
    factory_presets: Arc<Vec<Preset>>,
    /// Last observed re-roll button value (edge-detect → trigger a re-roll).
    reroll_prev: bool,
}

#[derive(Params)]
pub struct MurmurParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "size"] pub size: FloatParam,
    #[id = "decay"] pub decay: FloatParam,
    #[id = "color"] pub color: FloatParam,
    #[id = "random"] pub randomness: FloatParam,
    #[id = "sens"] pub sensitivity: FloatParam,
    #[id = "freeze"] pub freeze: BoolParam,
    #[id = "freezemix"] pub freeze_mix: FloatParam,
    #[id = "reroll"] pub reroll: BoolParam,
    #[id = "width"] pub width: FloatParam,
    #[id = "mix"] pub mix: FloatParam,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl Default for MurmurParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 520),

            size: pct("Size", d.size),
            decay: FloatParam::new(
                "Decay",
                d.decay,
                FloatRange::Skewed {
                    min: 0.2,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(|s| {
                s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
            })),
            color: FloatParam::new("Color", d.color, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            randomness: pct("Randomness", d.randomness),
            sensitivity: pct("Sensitivity", d.sensitivity),
            freeze: BoolParam::new("Freeze", d.freeze),
            freeze_mix: pct("Freeze Mix", d.freeze_mix),
            reroll: BoolParam::new("Re-Roll", false),
            width: pct("Width", d.width),
            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl MurmurParams {
    /// Snapshot the current parameter values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            size: self.size.value(),
            decay: self.decay.value(),
            color: self.color.value(),
            randomness: self.randomness.value(),
            sensitivity: self.sensitivity.value(),
            freeze: self.freeze.value(),
            freeze_mix: self.freeze_mix.value(),
            width: self.width.value(),
            mix: self.mix.value(),
        }
    }
}

impl Default for Murmur {
    fn default() -> Self {
        Self {
            params: Arc::new(MurmurParams::default()),
            core: MurmurCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            reroll_prev: false,
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &MurmurParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.size, s.size);
    set_f(&params.decay, s.decay);
    set_f(&params.color, s.color);
    set_f(&params.randomness, s.randomness);
    set_f(&params.sensitivity, s.sensitivity);
    set_f(&params.width, s.width);
    set_f(&params.mix, s.mix);
}

impl Plugin for Murmur {
    const NAME: &'static str = "Qeynos MURMUR";
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
                suite_core::ui::ScaledWindow::new("qeynos-murmur-window", Vec2::new(560.0, 520.0))
                    .min_size(Vec2::new(480.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · MURMUR").color(suite_core::ui::ACCENT));
                        ui.label(
                            egui::RichText::new("stochastic reverb — a new random room on every onset")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("murmur", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new("ROOM").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("murmur-room")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "SIZE", &params.size, setter);
                                    row(ui, "DECAY", &params.decay, setter);
                                    ui.end_row();
                                    row(ui, "COLOR", &params.color, setter);
                                    row(ui, "RANDOMNESS", &params.randomness, setter);
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("TRIGGER").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("murmur-trig")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "SENSITIVITY", &params.sensitivity, setter);
                                    ui.end_row();
                                });
                            ui.horizontal(|ui| {
                                if ui.button("RE-ROLL").clicked() {
                                    // Flip the param — any change is an edge that re-rolls.
                                    let v = !params.reroll.value();
                                    setter.begin_set_parameter(&params.reroll);
                                    setter.set_parameter(&params.reroll, v);
                                    setter.end_set_parameter(&params.reroll);
                                }
                                ui.add_space(12.0);
                                let mut fz = params.freeze.value();
                                if ui.checkbox(&mut fz, "FREEZE").changed() {
                                    setter.begin_set_parameter(&params.freeze);
                                    setter.set_parameter(&params.freeze, fz);
                                    setter.end_set_parameter(&params.freeze);
                                }
                            });
                            egui::Grid::new("murmur-freezemix")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "FREEZE MIX", &params.freeze_mix, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(egui::RichText::new("OUTPUT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("murmur-out")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "WIDTH", &params.width, setter);
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
        // Preallocate the two FDNs (max-length delay lines) for this sample rate off the audio
        // thread so process() is allocation-free.
        self.core = MurmurCore::new(buffer_config.sample_rate);
        // A reverb is a time-smearing effect, not fixed latency ⇒ zero reported latency.
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

        let s = self.params.snapshot();
        self.core.configure(&s);

        // Manual re-roll: trigger on any change of the button value.
        let reroll_now = self.params.reroll.value();
        if reroll_now != self.reroll_prev {
            self.core.request_reroll();
            self.reroll_prev = reroll_now;
        }

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

impl ClapPlugin for Murmur {
    const CLAP_ID: &'static str = "com.qeynos.murmur";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Stochastic reverb — an 8×8 Householder FDN re-randomised into a new room on every onset");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Reverb,
    ];
}

impl Vst3Plugin for Murmur {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosMURMURrvb1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_clap!(Murmur);
nih_export_vst3!(Murmur);

#[cfg(test)]
mod render_tests {
    use crate::dsp::MurmurCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;

    /// Render each factory preset with two short impulses + a percussive noise burst then
    /// silence (so onset-triggered room swaps and the tails are audible), write to
    /// renders/MURMUR/, assert universal.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let n = (sr * 4.0) as usize;

        // Two impulses 2 s apart, plus a decaying noise transient at 1 s, to exercise onsets.
        let mut input = vec![0.0f32; n];
        input[0] = 0.9;
        input[(sr * 2.0) as usize] = 0.9;
        let mut env = 1.0f32;
        for i in 0..(sr as usize / 20) {
            let idx = (sr * 1.0) as usize + i;
            if idx < n {
                input[idx] += 0.5 * env * suite_core::testsig::white_noise(1.0, 1, idx as u32 + 7)[0];
                env *= 0.999;
            }
        }

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let mut core = MurmurCore::new(sr);
            let mut out = input.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
            let path = render_path("MURMUR", &fname);
            write_wav(&path, &out, sr as u32).expect("write render");
        }
    }
}
