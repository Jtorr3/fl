//! SEANCE — ethereal vocal machine (Qeynos suite; Cynthoni/Sewerslvt-style ghost vocals).
//!
//! Signal chain (see [`dsp`]): formant-preserving pitch/formant shift → BPM-synced chopper →
//! shimmer FDN verb (+12 st in the feedback loop) → wash (LP + wow) → drowned-vocal ducker
//! (wet swells when the dry vocal pauses) → 3-macro layer (GHOST/DROWN/CHOP) → mix.
//!
//! The formant-preserving engine is `suite_core::shift::ShiftEngine` — built here and reused
//! by VOXKEY/VOXFIT. The DSP core lives in [`dsp`] (pure Rust, shared with the harness tests).

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

use dsp::{RawControls, SeanceCore, CHOP_DIVISIONS, CHOP_PATTERNS};
use suite_core::bus::PluginKind;
use suite_core::presets::{load_all, Preset};
use suite_core::spectrum::SpectrumPublisher;

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/SEANCE.md");

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Seance {
    params: Arc<SeanceParams>,
    core: SeanceCore,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct SeanceParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "pitch"] pub pitch: FloatParam,
    #[id = "formant"] pub formant: FloatParam,
    #[id = "preserve"] pub preserve: BoolParam,

    #[id = "choppat"] pub chop_pattern: IntParam,
    #[id = "choprate"] pub chop_rate: IntParam,
    #[id = "chopdepth"] pub chop_depth: FloatParam,

    #[id = "vsize"] pub verb_size: FloatParam,
    #[id = "vdecay"] pub verb_decay: FloatParam,
    #[id = "vshimmer"] pub verb_shimmer: FloatParam,
    #[id = "vwet"] pub verb_wet: FloatParam,

    #[id = "wash"] pub wash: FloatParam,

    #[id = "duckdepth"] pub duck_depth: FloatParam,
    #[id = "duckrel"] pub duck_release: FloatParam,

    #[id = "ghost"] pub ghost: FloatParam,
    #[id = "drown"] pub drown: FloatParam,
    #[id = "chopmac"] pub chop_macro: FloatParam,

    #[id = "mix"] pub mix: FloatParam,
    #[id = "out"] pub out_trim: FloatParam,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn semitones(name: &'static str, default: f32, range: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: -range, max: range })
        .with_unit(" st")
        .with_value_to_string(formatters::v2s_f32_rounded(2))
        .with_string_to_value(Arc::new(|s| {
            s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
        }))
}

fn ms(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" ms")
        .with_value_to_string(formatters::v2s_f32_rounded(0))
        .with_string_to_value(Arc::new(|s| {
            s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
        }))
}

fn db(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
        .with_string_to_value(Arc::new(|s| {
            s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
        }))
}

impl Default for SeanceParams {
    fn default() -> Self {
        let d = RawControls::default();
        Self {
            editor_state: EguiState::from_size(560, 720),

            pitch: semitones("Pitch", d.pitch_st, 12.0),
            formant: semitones("Formant", d.formant_st, 12.0),
            preserve: BoolParam::new("Formant Preserve", d.preserve),

            chop_pattern: IntParam::new(
                "Chop Pattern",
                0,
                IntRange::Linear { min: 0, max: CHOP_PATTERNS.len() as i32 - 1 },
            )
            .with_value_to_string(Arc::new(|v| {
                CHOP_PATTERNS.get(v as usize).copied().unwrap_or("Square").to_string()
            }))
            .with_string_to_value(Arc::new(|s| pattern_from_str(s))),

            chop_rate: IntParam::new(
                "Chop Rate",
                2,
                IntRange::Linear { min: 0, max: CHOP_DIVISIONS.len() as i32 - 1 },
            )
            .with_value_to_string(Arc::new(|v| {
                CHOP_DIVISIONS.get(v as usize).map(|d| d.0).unwrap_or("1/8").to_string()
            }))
            .with_string_to_value(Arc::new(|s| rate_from_str(s))),

            chop_depth: pct("Chop Depth", d.chop_depth),

            verb_size: pct("Verb Size", d.verb_size),
            verb_decay: FloatParam::new(
                "Verb Decay",
                d.verb_decay,
                FloatRange::Skewed { min: 0.3, max: 8.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(|s| {
                s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
            })),
            verb_shimmer: pct("Shimmer", d.verb_shimmer),
            verb_wet: pct("Verb Wet", d.verb_wet),

            wash: pct("Wash", d.wash),

            duck_depth: pct("Duck Depth", d.duck_depth),
            duck_release: ms("Duck Release", d.duck_release_ms, 40.0, 800.0),

            ghost: pct("Ghost", 0.0),
            drown: pct("Drown", 0.0),
            chop_macro: pct("Chop", 0.0),

            mix: pct("Mix", d.mix),
            out_trim: db("Out", 0.0, -24.0, 12.0),
        }
    }
}

fn pattern_from_str(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<i32>() {
        return Some(v.clamp(0, CHOP_PATTERNS.len() as i32 - 1));
    }
    CHOP_PATTERNS
        .iter()
        .position(|p| p.eq_ignore_ascii_case(t))
        .map(|i| i as i32)
}

fn rate_from_str(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<i32>() {
        return Some(v.clamp(0, CHOP_DIVISIONS.len() as i32 - 1));
    }
    CHOP_DIVISIONS
        .iter()
        .position(|d| d.0.eq_ignore_ascii_case(t))
        .map(|i| i as i32)
}

impl SeanceParams {
    /// Snapshot the current params into [`RawControls`] (macros resolved downstream).
    fn raw(&self, tempo_bpm: f32) -> RawControls {
        RawControls {
            pitch_st: self.pitch.value(),
            formant_st: self.formant.value(),
            preserve: self.preserve.value(),
            chop_pattern: self.chop_pattern.value() as usize,
            chop_rate: self.chop_rate.value() as usize,
            chop_depth: self.chop_depth.value(),
            verb_size: self.verb_size.value(),
            verb_decay: self.verb_decay.value(),
            verb_shimmer: self.verb_shimmer.value(),
            verb_wet: self.verb_wet.value(),
            wash: self.wash.value(),
            duck_depth: self.duck_depth.value(),
            duck_release_ms: self.duck_release.value(),
            ghost: self.ghost.value(),
            drown: self.drown.value(),
            chop_macro: self.chop_macro.value(),
            mix: self.mix.value(),
            out_gain: db_to_gain(self.out_trim.value()),
            tempo_bpm,
        }
    }
}

#[inline]
fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

impl Default for Seance {
    fn default() -> Self {
        Self {
            params: Arc::new(SeanceParams::default()),
            core: SeanceCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (automation/undo aware).
fn apply_preset(params: &SeanceParams, setter: &ParamSetter, p: &Preset) {
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
    let get = |k: &str, fb: f32| p.get(k).unwrap_or(fb);
    set_f(&params.pitch, get("pitch", 0.0));
    set_f(&params.formant, get("formant", 0.0));
    set_b(&params.preserve, get("preserve", 1.0) >= 0.5);
    set_i(&params.chop_pattern, get("pattern", 0.0).round() as i32);
    set_i(&params.chop_rate, get("rate", 2.0).round() as i32);
    set_f(&params.chop_depth, get("chopdepth", 0.0));
    set_f(&params.verb_size, get("size", 0.6));
    set_f(&params.verb_decay, get("decay", 2.2));
    set_f(&params.verb_shimmer, get("shimmer", 0.35));
    set_f(&params.verb_wet, get("wet", 0.35));
    set_f(&params.wash, get("wash", 0.3));
    set_f(&params.duck_depth, get("duckdepth", 0.4));
    set_f(&params.duck_release, get("duckrel", 260.0));
    set_f(&params.ghost, get("ghost", 0.0));
    set_f(&params.drown, get("drown", 0.0));
    set_f(&params.chop_macro, get("chopmacro", 0.0));
    set_f(&params.mix, get("mix", 0.5));
    // `out` is stored in dB; the FloatParam is also in dB.
    set_f(&params.out_trim, get("out", 0.0));
}

impl Plugin for Seance {
    const NAME: &'static str = "Qeynos SEANCE";
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
                suite_core::ui::ScaledWindow::new("qeynos-seance-window", Vec2::new(560.0, 720.0))
                    .min_size(Vec2::new(500.0, 560.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · SEANCE").color(suite_core::ui::ACCENT));
                        suite_core::ui::manual_button(ui, "seance", "SEANCE", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("ethereal vocal machine — ghost vocals")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("seance", presets.as_slice()).show(
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
                            "seance-crt",
                            "SEANCE · GHOST VOX",
                            &[
                                ("MACRO", format!("ghost {} · drown {} · chop {}", params.ghost, params.drown, params.chop_macro)),
                                ("SHIFT", format!("pitch {} · form {}", params.pitch, params.formant)),
                                ("CHOP", format!("{} · rate {} · dep {}", params.chop_pattern, params.chop_rate, params.chop_depth)),
                                ("VERB", format!("size {} · dec {} · wet {}", params.verb_size, params.verb_decay, params.verb_wet)),
                                ("OUT", format!("mix {} · {}", params.mix, params.out_trim)),
                            ],
                        );
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new("MACROS").color(suite_core::ui::ACCENT).small());
                            egui::Grid::new("seance-macros").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                                row(ui, "GHOST", &params.ghost, setter);
                                row(ui, "DROWN", &params.drown, setter);
                                row(ui, "CHOP", &params.chop_macro, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("SHIFT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("seance-shift").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "PITCH", &params.pitch, setter);
                                row(ui, "FORMANT", &params.formant, setter);
                                ui.end_row();
                                row(ui, "PRESERVE", &params.preserve, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("CHOP").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("seance-chop").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "PATTERN", &params.chop_pattern, setter);
                                row(ui, "RATE", &params.chop_rate, setter);
                                ui.end_row();
                                row(ui, "DEPTH", &params.chop_depth, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("SHIMMER VERB").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("seance-verb").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "SIZE", &params.verb_size, setter);
                                row(ui, "DECAY", &params.verb_decay, setter);
                                ui.end_row();
                                row(ui, "SHIMMER", &params.verb_shimmer, setter);
                                row(ui, "WET", &params.verb_wet, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("WASH / DUCK").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("seance-washduck").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "WASH", &params.wash, setter);
                                row(ui, "DUCK DEPTH", &params.duck_depth, setter);
                                ui.end_row();
                                row(ui, "DUCK REL", &params.duck_release, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.separator();
                            egui::Grid::new("seance-out").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
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
        self.core = SeanceCore::new(buffer_config.sample_rate);
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "SEANCE");
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
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let t = context.transport();
        let tempo = t.tempo.unwrap_or(120.0) as f32;
        let raw = self.params.raw(tempo);
        let s = raw.resolve();
        self.core.configure(&s);

        // Phase-lock the chopper to the transport grid: build a shared TransportFrame from the
        // host position (same plumbing as CLEAVE) and hand it to the core.
        {
            let sr = self.core.sample_rate();
            let tempo_f = t.tempo.unwrap_or(120.0).max(1.0);
            let tsn = t.time_sig_numerator.unwrap_or(4).max(1) as f64;
            let tsd = t.time_sig_denominator.unwrap_or(4).max(1) as f64;
            let beats_per_bar = (tsn * 4.0 / tsd).max(1.0e-3);
            let bars_per_sample = (tempo_f / 60.0 / sr as f64) / beats_per_bar;
            let ppq = t.pos_beats().unwrap_or(0.0);
            let frame = suite_core::testsig::TransportFrame {
                playing: t.playing,
                tempo: tempo_f,
                ppq_pos: ppq,
                bar_pos: ppq / beats_per_bar,
                bars_per_sample,
                beats_per_bar,
            };
            self.core.set_transport(&frame);
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

impl Drop for Seance {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for Seance {
    const CLAP_ID: &'static str = "com.qeynos.seance";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Ethereal vocal machine — formant-preserving shift, chopper, shimmer verb, drowned-vocal ducker");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Reverb,
        ClapFeature::PitchShifter,
        ClapFeature::Custom("vocal"),
    ];
}

impl Vst3Plugin for Seance {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosSEANCEvox1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb, Vst3SubCategory::PitchShift];
}

nih_export_clap!(Seance);
nih_export_vst3!(Seance);

#[cfg(test)]
mod manual_tests {
    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(
            crate::MANUAL_DOC,
            &crate::SeanceParams::default(),
        );
    }
}
