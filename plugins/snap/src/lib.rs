//! SNAP — snare / clap generator (Qeynos suite, taste-tailored Phase 2). A MIDI instrument.
//!
//! A mono-ish, last-note-priority voice built on IMPACT's architecture: a note-on drives a
//! phase-continuous sine/tri BODY with a fast pitch env (shell knock), a white-noise RATTLE
//! through a 3-formant band-pass bank (snare wires), and a CLAP engine of N humanized noise
//! bursts + one longer tail — crossfaded by a continuous MODE blend (Snare ↔ Hybrid ↔ Clap).
//! A snap-scaled transient click sits on top; the sum is driven through the suite waveshaper
//! (2× oversampled), shaped by the master amp envelope, decorrelated per channel for a
//! mono-compatible stereo WIDTH, and soft-clipped. The DECAY macro scales every envelope
//! together. Retriggers are phase-continuous with a 1.5 ms declick ramp. Key-track (off by
//! default) sets the body fundamental from the MIDI note.
//!
//! DSP core lives in [`dsp`] (pure Rust, shared with the offline harness tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::{Arc, RwLock};
use suite_core::modlisten::ModRoutes;

pub mod dsp;
pub mod presets;

use dsp::{Settings, SnapVoice, DECAY_REF_MS, KEYTRACK_REF_NOTE, MAX_TAPS};
use suite_core::bus::PluginKind;
use suite_core::presets::{load_all, Preset};
use suite_core::spectrum::SpectrumPublisher;

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Snap {
    params: Arc<SnapParams>,
    voice: SnapVoice,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct SnapParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "mode"] pub mode: FloatParam,
    #[id = "tune"] pub tune: FloatParam,
    #[id = "balance"] pub balance: FloatParam,
    #[id = "snap"] pub snap: FloatParam,
    #[id = "decay"] pub decay: FloatParam,
    #[id = "taps"] pub taps: IntParam,
    #[id = "spread"] pub spread: FloatParam,
    #[id = "humanize"] pub humanize: FloatParam,
    #[id = "tone"] pub tone: FloatParam,
    #[id = "drive"] pub drive: FloatParam,
    #[id = "width"] pub width: FloatParam,
    #[id = "level"] pub level: FloatParam,
    #[id = "keytrack"] pub keytrack: BoolParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl Default for SnapParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 600),
            mode: FloatParam::new("Mode", d.mode, FloatRange::Linear { min: 0.0, max: 1.0 })
                // Always show the parseable percentage (so text_to_value round-trips), with a
                // Snare/Hybrid/Clap hint suffix. string_to_value reads the leading number.
                .with_value_to_string(Arc::new(|v| {
                    let label = if v < 0.2 {
                        "Snare"
                    } else if v > 0.8 {
                        "Clap"
                    } else {
                        "Hybrid"
                    };
                    format!("{:.0}% {label}", v * 100.0)
                }))
                .with_string_to_value(Arc::new(|s| {
                    let num: String = s
                        .trim()
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    num.parse::<f32>().ok().map(|v| (v / 100.0).clamp(0.0, 1.0))
                })),
            tune: FloatParam::new(
                "Tune",
                d.tune,
                FloatRange::Skewed { min: 100.0, max: 400.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            balance: pct("Body/Noise", d.balance),
            snap: pct("Snap", d.snap),
            decay: FloatParam::new(
                "Decay",
                d.decay_ms,
                FloatRange::Skewed { min: 40.0, max: 1200.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            taps: IntParam::new("Taps", d.taps as i32, IntRange::Linear { min: 3, max: MAX_TAPS as i32 })
                .with_value_to_string(Arc::new(|v| format!("{v}")))
                .with_string_to_value(Arc::new(|s| s.trim().parse::<i32>().ok())),
            spread: FloatParam::new(
                "Spread",
                d.spread_ms,
                FloatRange::Linear { min: 8.0, max: 30.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            humanize: pct("Humanize", d.humanize),
            tone: pct("Tone", d.tone),
            drive: pct("Drive", d.drive),
            width: pct("Width", d.width),
            level: FloatParam::new("Level", d.level_db, FloatRange::Linear { min: -24.0, max: 6.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            keytrack: BoolParam::new("Key Track", false),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl SnapParams {
    /// Snapshot the current parameter values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            mode: self.mode.value(),
            tune: self.tune.value(),
            balance: self.balance.value(),
            snap: self.snap.value(),
            decay_ms: self.decay.value(),
            taps: self.taps.value() as usize,
            spread_ms: self.spread.value(),
            humanize: self.humanize.value(),
            tone: self.tone.value(),
            drive: self.drive.value(),
            width: self.width.value(),
            level_db: self.level.value(),
        }
    }
}

impl Default for Snap {
    fn default() -> Self {
        Self {
            params: Arc::new(SnapParams::default()),
            voice: SnapVoice::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset through the host (so automation/undo see the change).
fn apply_preset(params: &SnapParams, setter: &ParamSetter, p: &Preset) {
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

    set_f(&params.mode, g("mode", d.mode));
    set_f(&params.tune, g("tune", d.tune));
    set_f(&params.balance, g("balance", d.balance));
    set_f(&params.snap, g("snap", d.snap));
    set_f(&params.decay, g("decay", d.decay_ms));
    set_i(&params.taps, g("taps", d.taps as f32) as i32);
    set_f(&params.spread, g("spread", d.spread_ms));
    set_f(&params.humanize, g("humanize", d.humanize));
    set_f(&params.tone, g("tone", d.tone));
    set_f(&params.drive, g("drive", d.drive));
    set_f(&params.width, g("width", d.width));
    set_f(&params.level, g("level", d.level_db));
    set_b(&params.keytrack, g("keytrack", 0.0) >= 0.5);
}

impl Plugin for Snap {
    const NAME: &'static str = "Qeynos SNAP";
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
                suite_core::ui::ScaledWindow::new("qeynos-snap-window", Vec2::new(560.0, 600.0))
                    .min_size(Vec2::new(480.0, 500.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · SNAP").color(suite_core::ui::ACCENT));
                        ui.label(
                            egui::RichText::new("snare / clap generator")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("snap", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("drive", "DRIVE"), ("tone", "TONE"), ("decay", "DECAY"), ("level", "LEVEL")],
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new("ENGINE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("snap-engine").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "MODE", &params.mode, setter);
                                row(ui, "TUNE", &params.tune, setter);
                                ui.end_row();
                                row(ui, "BODY/NOISE", &params.balance, setter);
                                row(ui, "SNAP", &params.snap, setter);
                                ui.end_row();
                                row(ui, "DECAY", &params.decay, setter);
                                row(ui, "TONE", &params.tone, setter);
                                ui.end_row();
                            });
                            ui.horizontal(|ui| {
                                let mut kt = params.keytrack.value();
                                if ui.checkbox(&mut kt, "Key Track (note → body tune)").changed() {
                                    setter.begin_set_parameter(&params.keytrack);
                                    setter.set_parameter(&params.keytrack, kt);
                                    setter.end_set_parameter(&params.keytrack);
                                }
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("CLAP ENGINE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("snap-clap").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "TAPS", &params.taps, setter);
                                row(ui, "SPREAD", &params.spread, setter);
                                ui.end_row();
                                row(ui, "HUMANIZE", &params.humanize, setter);
                                ui.end_row();
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("OUTPUT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("snap-output").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "DRIVE", &params.drive, setter);
                                row(ui, "WIDTH", &params.width, setter);
                                ui.end_row();
                                row(ui, "LEVEL", &params.level, setter);
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
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.voice = SnapVoice::new(buffer_config.sample_rate);
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "SNAP");
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
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.drive = routes.modulated_float("drive", &self.params.drive, bus);
                s.tone = routes.modulated_float("tone", &self.params.tone, bus);
                s.decay_ms = routes.modulated_float("decay", &self.params.decay, bus);
                s.level_db = routes.modulated_float("level", &self.params.level, bus);
            }
        }
        self.voice.configure(&s);
        let keytrack = self.params.keytrack.value();
        let macro_len = s.decay_ms.max(1.0) / DECAY_REF_MS;

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
                        // Reference note reproduces the `tune` knob; other notes transpose it.
                        let semis = note as i32 - KEYTRACK_REF_NOTE as i32;
                        Some(s.tune * 2.0f32.powf(semis as f32 / 12.0))
                    } else {
                        None
                    };
                    self.voice.note_on(velocity, key_hz, macro_len);
                }
                next_event = context.next_event();
            }

            let (l, r) = self.voice.process_sample();
            if num_ch >= 2 {
                out[0][n] = l;
                out[1][n] = r;
            } else {
                out[0][n] = 0.5 * (l + r);
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

        ProcessStatus::KeepAlive
    }
}

impl Drop for Snap {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for Snap {
    const CLAP_ID: &'static str = "com.qeynos.snap";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Snare/clap generator — body knock, noise formant rattle, humanized clap engine, mode blend");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Drum,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for Snap {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosSNAPsnr1cl";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Drum];
}

nih_export_clap!(Snap);
nih_export_vst3!(Snap);

#[cfg(test)]
mod render_tests;
