//! Qeynos Template — a hello-gain plugin built on nih-plug + nih_plug_egui.
//!
//! Proves the whole pipeline for Phase 0: workspace builds on windows-gnu, an egui
//! window opens with the suite theme, one smoothed gain parameter automates, a peak
//! meter reads, and both CLAP and VST3 exports validate. Kept forever as the template
//! every later plugin is copied from.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::Arc;
use suite_core::presets::{load_all, Preset};

/// The plugin's usage manual, embedded from the docs file and rendered in-GUI by the '?'
/// button (BUILT-IN-MANUALS). Same source of truth as the GitHub-readable docs.
pub const MANUAL_DOC: &str = include_str!("../../../docs/_TEMPLATE.md");

/// Factory presets — the template keeps a couple so it also demonstrates the suite
/// preset bar (factory + user save/load) that every plugin adopts. Values are plain
/// (linear gain).
const PRESET_JSON: &[&str] = &[
    r#"{ "name": "Unity", "gain": 1.0 }"#,
    r#"{ "name": "Quiet -12", "gain": 0.25118864 }"#,
    r#"{ "name": "Loud +6", "gain": 1.9952623 }"#,
];

/// Apply a factory preset to the live params through the host (the per-plugin pretty-key
/// mapping; user presets go through the generic param-id path in `suite_core::ui`).
fn apply_preset(params: &TemplateParams, setter: &ParamSetter, p: &Preset) {
    if let Some(g) = p.get("gain") {
        setter.begin_set_parameter(&params.gain);
        setter.set_parameter(&params.gain, g);
        setter.end_set_parameter(&params.gain);
    }
}

/// Time for the peak meter to decay ~12 dB after silence.
const PEAK_METER_DECAY_MS: f64 = 150.0;

/// Pure-DSP gain stage, shared by `process` and the offline harness tests so the
/// tested math is exactly the shipped math.
#[derive(Clone, Copy)]
pub struct GainDsp {
    /// Linear gain factor.
    pub gain: f32,
}

impl GainDsp {
    pub fn from_db(db: f32) -> Self {
        Self {
            gain: util::db_to_gain(db),
        }
    }
}

impl suite_core::harness::Processor for GainDsp {
    #[inline]
    fn process(&mut self, block: &mut [f32]) {
        for s in block.iter_mut() {
            *s *= self.gain;
        }
    }
}

pub struct Template {
    params: Arc<TemplateParams>,
    peak_meter_decay_weight: f32,
    peak_meter: Arc<AtomicF32>,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct TemplateParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "gain"]
    pub gain: FloatParam,
}

impl Default for Template {
    fn default() -> Self {
        Self {
            params: Arc::new(TemplateParams::default()),
            peak_meter_decay_weight: 1.0,
            peak_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            factory_presets: Arc::new(load_all(PRESET_JSON)),
        }
    }
}

impl Default for TemplateParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(320, 236),
            // Smoothed gain, -60 .. +24 dB, default unity.
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(24.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 24.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Plugin for Template {
    const NAME: &'static str = "Qeynos Template";
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
        let peak_meter = self.peak_meter.clone();
        let presets = self.factory_presets.clone();
        let egui_state = self.params.editor_state.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| {
                suite_core::ui::apply_theme(ctx);
            },
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-template-window", Vec2::new(320.0, 236.0))
                    .min_size(Vec2::new(240.0, 160.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        ui.add_space(6.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · TEMPLATE")
                                .color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "_template", "TEMPLATE", MANUAL_DOC);
                        ui.add_space(8.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("_template", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        ui.separator();

                        suite_core::ui::labeled_slider(ui, "GAIN", &params.gain, setter);

                        ui.add_space(10.0);

                        let peak_db =
                            util::gain_to_db(peak_meter.load(std::sync::atomic::Ordering::Relaxed));
                        let peak_text = if peak_db > util::MINUS_INFINITY_DB {
                            format!("{peak_db:.1} dBFS")
                        } else {
                            String::from("-inf dBFS")
                        };
                        // CONSOLE v2 CRT telemetry bay (the reference crate demonstrates the
                        // pattern): live peak + gain in amber terminal style. THEME-OFF ⇒ plain.
                        suite_core::ui::crt_lines(
                            ui,
                            "template-crt",
                            "TEMPLATE",
                            &[
                                ("PEAK", peak_text),
                                ("GAIN", params.gain.to_string()),
                            ],
                        );
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
        self.peak_meter_decay_weight = 0.25f64
            .powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip())
            as f32;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        // Every Qeynos plugin copies this line — keep it at the top of `process`.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        for channel_samples in buffer.iter_samples() {
            let mut amplitude = 0.0;
            let num_samples = channel_samples.len();

            let gain = self.params.gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
                amplitude += *sample;
            }

            if self.params.editor_state.is_open() {
                amplitude = (amplitude / num_samples as f32).abs();
                let current = self.peak_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new = if amplitude > current {
                    amplitude
                } else {
                    current * self.peak_meter_decay_weight
                        + amplitude * (1.0 - self.peak_meter_decay_weight)
                };
                self.peak_meter
                    .store(new, std::sync::atomic::Ordering::Relaxed);
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Template {
    const CLAP_ID: &'static str = "com.qeynos.template";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Qeynos suite template — smoothed gain with peak meter");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Template {
    // Unique 16-byte class id for this plugin.
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosTemplate01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(Template);
nih_export_vst3!(Template);

#[cfg(test)]
mod tests {
    use super::GainDsp;
    use suite_core::harness::{null_residual_db, render_offline, rms_dbfs};
    use suite_core::testsig;

    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(
            super::MANUAL_DOC,
            &super::TemplateParams::default(),
        );
    }

    #[test]
    fn unity_gain_nulls_against_input() {
        let sig = testsig::sine(1_000.0, 0.5, 48_000, 48_000.0);
        let out = render_offline(GainDsp::from_db(0.0), &sig, 512);
        let residual = null_residual_db(&sig, &out);
        assert!(residual < -80.0, "0 dB gain residual was {residual:.2} dB (want < -80)");
    }

    #[test]
    fn minus_12_db_drops_rms_by_12() {
        let sig = testsig::sine(1_000.0, 0.5, 48_000, 48_000.0);
        let dry = rms_dbfs(&sig);
        let out = render_offline(GainDsp::from_db(-12.0), &sig, 512);
        let wet = rms_dbfs(&out);
        let drop = dry - wet;
        assert!(
            (drop - 12.0).abs() < 0.5,
            "expected ~12 dB drop, got {drop:.3} dB (dry {dry:.2}, wet {wet:.2})"
        );
    }
}
