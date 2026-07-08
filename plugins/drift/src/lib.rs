//! DRIFT — infinity filter (Qeynos suite, Phase 2a; Sweep clone).
//!
//! A Shepard-tone illusion in the filter domain: `N` peak (bell) filters spaced evenly
//! across a `[range_lo, range_hi]` log-frequency window all glide together up or down at
//! `Rate` (free Hz or BPM-synced to the host tempo), wrapping at the range edges. Each
//! filter's boost follows a raised-cosine window over its log-frequency position, so filters
//! fade in silently at the bottom and out at the top — the ear hears an endless rise or fall.
//!
//! Pure minimum-phase IIR (TPT SVF bells, time-varying-safe): zero latency, dry/wet always
//! aligned. The DSP math lives in [`dsp`], shared verbatim with the offline harness tests.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::{Arc, RwLock};
use suite_core::bus::PluginKind;
use suite_core::modlisten::ModRoutes;
use suite_core::spectrum::SpectrumPublisher;

pub mod dsp;
pub mod presets;

use dsp::{Direction, DriftCore, Settings, SyncDivision};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/DRIFT.md");

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DirectionParam {
    #[id = "up"]
    #[name = "Up"]
    Up,
    #[id = "down"]
    #[name = "Down"]
    Down,
}

impl DirectionParam {
    fn to_dsp(self) -> Direction {
        match self {
            DirectionParam::Up => Direction::Up,
            DirectionParam::Down => Direction::Down,
        }
    }
    fn from_index(i: usize) -> DirectionParam {
        match i {
            1 => DirectionParam::Down,
            _ => DirectionParam::Up,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DivisionParam {
    #[id = "b4"]
    #[name = "4 Bars"]
    FourBars,
    #[id = "b2"]
    #[name = "2 Bars"]
    TwoBars,
    #[id = "b1"]
    #[name = "1 Bar"]
    OneBar,
    #[id = "d2"]
    #[name = "1/2"]
    Half,
    #[id = "d4"]
    #[name = "1/4"]
    Quarter,
    #[id = "d8"]
    #[name = "1/8"]
    Eighth,
    #[id = "d16"]
    #[name = "1/16"]
    Sixteenth,
}

impl DivisionParam {
    fn to_dsp(self) -> SyncDivision {
        match self {
            DivisionParam::FourBars => SyncDivision::FourBars,
            DivisionParam::TwoBars => SyncDivision::TwoBars,
            DivisionParam::OneBar => SyncDivision::OneBar,
            DivisionParam::Half => SyncDivision::Half,
            DivisionParam::Quarter => SyncDivision::Quarter,
            DivisionParam::Eighth => SyncDivision::Eighth,
            DivisionParam::Sixteenth => SyncDivision::Sixteenth,
        }
    }
    fn from_index(i: usize) -> DivisionParam {
        match i {
            0 => DivisionParam::FourBars,
            1 => DivisionParam::TwoBars,
            2 => DivisionParam::OneBar,
            3 => DivisionParam::Half,
            4 => DivisionParam::Quarter,
            5 => DivisionParam::Eighth,
            _ => DivisionParam::Sixteenth,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Drift {
    params: Arc<DriftParams>,
    core: DriftCore,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct DriftParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "division"]
    pub division: EnumParam<DivisionParam>,
    #[id = "direction"]
    pub direction: EnumParam<DirectionParam>,
    #[id = "resonance"]
    pub resonance: FloatParam,
    #[id = "rangelo"]
    pub range_lo: FloatParam,
    #[id = "rangehi"]
    pub range_hi: FloatParam,
    #[id = "peaks"]
    pub peaks: IntParam,
    #[id = "stereo"]
    pub stereo_offset: FloatParam,
    #[id = "depth"]
    pub depth: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

fn hz(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

impl Default for DriftParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 560),
            rate: FloatParam::new(
                "Rate",
                d.rate_hz,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 10.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(3)),
            sync: BoolParam::new("Sync", d.sync),
            division: EnumParam::new("Division", DivisionParam::OneBar),
            direction: EnumParam::new("Direction", DirectionParam::Up),
            resonance: FloatParam::new(
                "Resonance",
                d.resonance,
                FloatRange::Skewed {
                    min: 0.3,
                    max: 12.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            range_lo: hz("Range Lo", d.range_lo, 20.0, 2000.0),
            range_hi: hz("Range Hi", d.range_hi, 200.0, 20_000.0),
            peaks: IntParam::new("Peaks", d.peaks as i32, IntRange::Linear { min: 2, max: 8 })
                .with_value_to_string(Arc::new(|v| v.to_string()))
                .with_string_to_value(Arc::new(|s| s.trim().parse::<i32>().ok())),
            stereo_offset: FloatParam::new(
                "Stereo Offset",
                d.stereo_offset,
                FloatRange::Linear { min: 0.0, max: 0.5 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            depth: FloatParam::new("Depth", d.depth_db, FloatRange::Linear { min: 0.0, max: 36.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl DriftParams {
    /// Snapshot the current param values into a DSP [`Settings`]. `tempo_bpm` comes from the
    /// host transport (falls back to 120 when unavailable). Per-sample smoothing of the
    /// glide-critical values happens inside [`DriftCore`].
    fn snapshot(&self, tempo_bpm: f32) -> Settings {
        Settings {
            rate_hz: self.rate.value(),
            sync: self.sync.value(),
            division: self.division.value().to_dsp(),
            tempo_bpm,
            direction: self.direction.value().to_dsp(),
            resonance: self.resonance.value(),
            range_lo: self.range_lo.value(),
            range_hi: self.range_hi.value(),
            peaks: (self.peaks.value() as usize).clamp(2, dsp::MAX_PEAKS),
            stereo_offset: self.stereo_offset.value(),
            depth_db: self.depth.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Drift {
    fn default() -> Self {
        Self {
            params: Arc::new(DriftParams::default()),
            core: DriftCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &DriftParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    setter.begin_set_parameter(&params.sync);
    setter.set_parameter(&params.sync, g("sync", 0.0) >= 0.5);
    setter.end_set_parameter(&params.sync);

    setter.begin_set_parameter(&params.division);
    setter.set_parameter(
        &params.division,
        DivisionParam::from_index(g("division", 2.0) as usize),
    );
    setter.end_set_parameter(&params.division);

    setter.begin_set_parameter(&params.direction);
    setter.set_parameter(
        &params.direction,
        DirectionParam::from_index(g("direction", 0.0) as usize),
    );
    setter.end_set_parameter(&params.direction);

    setter.begin_set_parameter(&params.peaks);
    setter.set_parameter(&params.peaks, (g("peaks", 6.0) as i32).clamp(2, 8));
    setter.end_set_parameter(&params.peaks);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.rate, g("rate", 0.1));
    set_f(&params.resonance, g("resonance", 3.0));
    set_f(&params.range_lo, g("range_lo", 50.0));
    set_f(&params.range_hi, g("range_hi", 3200.0));
    set_f(&params.stereo_offset, g("stereo_offset", 0.25));
    set_f(&params.depth, g("depth", 12.0));
    set_f(&params.mix, g("mix", 1.0));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Drift {
    const NAME: &'static str = "Qeynos DRIFT";
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
                suite_core::ui::ScaledWindow::new("qeynos-drift-window", Vec2::new(560.0, 560.0))
                    .min_size(Vec2::new(480.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · DRIFT").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "drift", "DRIFT", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("infinity filter — endless Shepard sweep")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("drift", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("depth", "DEPTH"), ("resonance", "RESONANCE"), ("mix", "MIX"), ("out", "OUT")],
                        );
                        ui.separator();

                        // CONSOLE v2 CRT telemetry bay — honest live state (same values shown
                        // on the knobs below; GUI-thread param reads only, process() untouched).
                        // THEME-OFF degrades to a plain readout panel.
                        suite_core::ui::crt_lines(
                            ui,
                            "drift-crt",
                            "DRIFT · INFINITY FILTER",
                            &[
                                ("SWEEP", format!("{} · {}", params.rate, params.direction)),
                                (
                                    "SYNC",
                                    format!(
                                        "{} · div {}",
                                        if params.sync.value() { "on" } else { "free" },
                                        params.division,
                                    ),
                                ),
                                ("BANK", format!("{} peaks · res {}", params.peaks, params.resonance)),
                                ("RANGE", format!("{} .. {}", params.range_lo, params.range_hi)),
                                (
                                    "OUT",
                                    format!("depth {} · mix {} · {}", params.depth, params.mix, params.out),
                                ),
                            ],
                        );
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("GLIDE")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("drift-glide")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "RATE", &params.rate, setter);
                                    row(ui, "DIRECTION", &params.direction, setter);
                                    ui.end_row();
                                    row(ui, "DIVISION", &params.division, setter);
                                    ui.end_row();
                                });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("SYNC")
                                        .color(suite_core::ui::TEXT_DIM)
                                        .small(),
                                );
                                let mut sy = params.sync.value();
                                if ui.checkbox(&mut sy, "BPM sync").changed() {
                                    setter.begin_set_parameter(&params.sync);
                                    setter.set_parameter(&params.sync, sy);
                                    setter.end_set_parameter(&params.sync);
                                }
                            });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("FILTER BANK")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("drift-bank")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "PEAKS", &params.peaks, setter);
                                    row(ui, "RESONANCE", &params.resonance, setter);
                                    ui.end_row();
                                    row(ui, "RANGE LO", &params.range_lo, setter);
                                    row(ui, "RANGE HI", &params.range_hi, setter);
                                    ui.end_row();
                                    row(ui, "DEPTH", &params.depth, setter);
                                    row(ui, "STEREO OFFSET", &params.stereo_offset, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("OUTPUT")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("drift-out")
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
        self.core = DriftCore::new(buffer_config.sample_rate);
        // Pure minimum-phase IIR — zero latency.
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum
            .init(buffer_config.sample_rate, PluginKind::Generic, "DRIFT");
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
        let mut s = self.params.snapshot(tempo);
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.depth_db = routes.modulated_float("depth", &self.params.depth, bus);
                s.resonance = routes.modulated_float("resonance", &self.params.resonance, bus);
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
                s.out_db = routes.modulated_float("out", &self.params.out, bus);
            }
        }
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

impl Drop for Drift {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for Drift {
    const CLAP_ID: &'static str = "com.qeynos.drift";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Infinity filter — endless Shepard-tone filter sweep (N octave-spaced peaks)");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Filter,
    ];
}

impl Vst3Plugin for Drift {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosDRIFTinf01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_clap!(Drift);
nih_export_vst3!(Drift);

#[cfg(test)]
mod render_tests {
    use crate::dsp::DriftCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(crate::MANUAL_DOC, &crate::DriftParams::default());
    }

    /// SOUND-PASS audition render (permanent infra, `#[ignore]`d in normal runs).
    /// Renders every factory preset AND `Settings::default()` over a genre-right musical
    /// source (a sustained minor pad — DRIFT's home turf) into
    /// renders/_audition/DRIFT/<QVS_AUDITION_DIR or "before">/<preset>.wav, plus a hot
    /// 1 kHz sine aliasing probe through the most resonant preset. Analyzed offline by
    /// tools/audition.py (click/seam, aliasing, true-peak, tonal balance).
    #[test]
    #[ignore]
    fn audition_render_musical_sources() {
        use crate::dsp::Settings;

        let sr = 48_000.0f32;
        let subdir = std::env::var("QVS_AUDITION_DIR").unwrap_or_else(|_| "before".into());

        // Main: 4 s sustained minor pad at 110 Hz — the broadband, sustained source DRIFT
        // is designed for, so the Shepard motion (and any wrap seam) is fully exposed.
        let pad = testsig::synth_pad(110.0, 4.0, sr);

        // Render every factory preset plus the default state (labelled "default").
        let presets = load_all(PRESET_JSON);
        let mut jobs: Vec<(String, crate::dsp::Settings)> = presets
            .iter()
            .map(|p| {
                let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");
                (fname, settings_from_preset(p))
            })
            .collect();
        jobs.push(("default".into(), Settings::default()));

        for (fname, s) in &jobs {
            let mut core = DriftCore::new(sr);
            let mut out = pad.clone();
            core.process_mono(&mut out, s);
            let path = render_path("_audition/DRIFT", &format!("{subdir}/{fname}"));
            write_wav(&path, &out, sr as u32).expect("write audition render");
        }

        // Aliasing probe: a hot 1 kHz sine (96k samples = 2 s) through the most resonant
        // preset ("Resonant Screamer", Q 12 / depth 20 dB). Read inharmonic residual with
        // --sine-probe 1000.
        if let Some(p) = presets.iter().find(|p| p.name == "Resonant Screamer") {
            let s = settings_from_preset(p);
            let mut out = testsig::sine(1000.0, 0.5, 96_000, sr);
            let mut core = DriftCore::new(sr);
            core.process_mono(&mut out, &s);
            let path = render_path("_audition/DRIFT", &format!("{subdir}/_alias_probe_1k"));
            write_wav(&path, &out, sr as u32).expect("write alias probe");
        }
    }

    /// Render each factory preset over pink noise and a full-band chirp, write the WAVs into
    /// renders/DRIFT/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.4, (sr * 4.0) as usize, 4242);
        let chirp = testsig::log_chirp(30.0, 12_000.0, 0.4, (sr * 4.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");

            let mut core = DriftCore::new(sr);
            let mut out = pink.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("DRIFT", &format!("{fname}_pink"));
            write_wav(&path, &out, sr as u32).expect("write pink render");

            let mut core = DriftCore::new(sr);
            let mut out = chirp.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("DRIFT", &format!("{fname}_chirp"));
            write_wav(&path, &out, sr as u32).expect("write chirp render");
        }
    }
}
