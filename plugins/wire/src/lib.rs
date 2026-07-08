//! WIRE — codec degradation (Qeynos suite, Phase 2a; Codec clone).
//!
//! An Opus round-trip abused as an effect: audio is resampled to 48 k, bandwidth-limited and
//! "crunched" (bit-depth + sample-rate reduction), encoded with a pure-Rust Opus encoder at a
//! chosen bitrate/mode, subjected to simulated packet loss (dropped frames with a click-free
//! zero-fill concealment), decoded, and fed through a re-encoding **regen** feedback loop for a
//! tape-style generation-loss effect — then width/mix/out. See [`dsp`] for the DSP core and the
//! codec-plan rationale (Plan A = `opus-rs`, run at a fixed 48 k internal rate).
//!
//! The codec allocates internally and one 20 ms frame costs ~0.3 % of the RT budget, so it runs
//! in the audio thread wrapped in `nih_plug::util::permit_alloc` (the workspace enables
//! `assert_process_allocs`). Latency (frame buffering + codec delay + SRC) is reported and the
//! dry path is delay-compensated inside [`dsp::WireCore`].

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

use dsp::{BandwidthSel, Mode, Settings, WireCore};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/WIRE.md");

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ModeParam {
    #[id = "voice"]
    #[name = "Voice"]
    Voice,
    #[id = "music"]
    #[name = "Music"]
    Music,
}

impl ModeParam {
    fn to_dsp(self) -> Mode {
        match self {
            ModeParam::Voice => Mode::Voice,
            ModeParam::Music => Mode::Music,
        }
    }
    fn from_index(i: usize) -> ModeParam {
        match i {
            0 => ModeParam::Voice,
            _ => ModeParam::Music,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum BandwidthParam {
    #[id = "nb"]
    #[name = "Narrow"]
    Narrow,
    #[id = "mb"]
    #[name = "Medium"]
    Medium,
    #[id = "wb"]
    #[name = "Wide"]
    Wide,
    #[id = "swb"]
    #[name = "Super"]
    Superwide,
    #[id = "fb"]
    #[name = "Full"]
    Full,
}

impl BandwidthParam {
    fn to_dsp(self) -> BandwidthSel {
        match self {
            BandwidthParam::Narrow => BandwidthSel::Narrow,
            BandwidthParam::Medium => BandwidthSel::Medium,
            BandwidthParam::Wide => BandwidthSel::Wide,
            BandwidthParam::Superwide => BandwidthSel::Superwide,
            BandwidthParam::Full => BandwidthSel::Full,
        }
    }
    fn from_index(i: usize) -> BandwidthParam {
        match i {
            0 => BandwidthParam::Narrow,
            1 => BandwidthParam::Medium,
            2 => BandwidthParam::Wide,
            3 => BandwidthParam::Superwide,
            _ => BandwidthParam::Full,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Wire {
    params: Arc<WireParams>,
    core: WireCore,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct WireParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "bitrate"]
    pub bitrate: FloatParam,
    #[id = "mode"]
    pub mode: EnumParam<ModeParam>,
    #[id = "bandwidth"]
    pub bandwidth: EnumParam<BandwidthParam>,
    #[id = "fec"]
    pub fec: BoolParam,
    #[id = "loss"]
    pub loss: FloatParam,
    #[id = "crunch"]
    pub crunch: FloatParam,
    #[id = "regendelay"]
    pub regen_delay: FloatParam,
    #[id = "regenamt"]
    pub regen_amount: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for WireParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 600),
            bitrate: FloatParam::new(
                "Bitrate",
                d.bitrate_kbps,
                FloatRange::Skewed {
                    min: 6.0,
                    max: 128.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" kbps")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            mode: EnumParam::new("Mode", ModeParam::Music),
            bandwidth: EnumParam::new("Bandwidth", BandwidthParam::Full),
            fec: BoolParam::new("FEC", d.fec),
            loss: FloatParam::new("Packet Loss", d.loss_pct, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_rounded(0)),
            crunch: FloatParam::new("Crunch", d.crunch, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            regen_delay: FloatParam::new(
                "Regen Delay",
                d.regen_delay_ms,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            regen_amount: FloatParam::new(
                "Regen Amount",
                d.regen_amount,
                FloatRange::Linear { min: 0.0, max: 0.95 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            width: FloatParam::new("Width", d.width, FloatRange::Linear { min: 0.0, max: 2.0 })
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
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl WireParams {
    /// Snapshot the live parameters into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            bitrate_kbps: self.bitrate.value(),
            mode: self.mode.value().to_dsp(),
            bandwidth: self.bandwidth.value().to_dsp(),
            fec: self.fec.value(),
            loss_pct: self.loss.value(),
            crunch: self.crunch.value(),
            regen_delay_ms: self.regen_delay.value(),
            regen_amount: self.regen_amount.value(),
            width: self.width.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Wire {
    fn default() -> Self {
        Self {
            params: Arc::new(WireParams::default()),
            core: WireCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &WireParams, setter: &ParamSetter, p: &Preset) {
    let g = |k: &str, fallback: f32| p.get(k).unwrap_or(fallback);

    setter.begin_set_parameter(&params.mode);
    setter.set_parameter(&params.mode, ModeParam::from_index(g("mode", 1.0) as usize));
    setter.end_set_parameter(&params.mode);

    setter.begin_set_parameter(&params.bandwidth);
    setter.set_parameter(
        &params.bandwidth,
        BandwidthParam::from_index(g("bandwidth", 4.0) as usize),
    );
    setter.end_set_parameter(&params.bandwidth);

    setter.begin_set_parameter(&params.fec);
    setter.set_parameter(&params.fec, g("fec", 0.0) >= 0.5);
    setter.end_set_parameter(&params.fec);

    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.bitrate, g("bitrate", 32.0));
    set_f(&params.loss, g("loss", 0.0));
    set_f(&params.crunch, g("crunch", 0.0));
    set_f(&params.regen_delay, g("regen_delay", 120.0));
    set_f(&params.regen_amount, g("regen_amount", 0.0));
    set_f(&params.width, g("width", 1.0));
    set_f(&params.mix, g("mix", 1.0));
    set_f(&params.out, g("out", 0.0));
}

impl Plugin for Wire {
    const NAME: &'static str = "Qeynos WIRE";
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
                suite_core::ui::ScaledWindow::new("qeynos-wire-window", Vec2::new(560.0, 600.0))
                    .min_size(Vec2::new(480.0, 500.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · WIRE").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "wire", "WIRE", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("codec degradation — Opus round-trip, crunch & regen")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("wire", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("crunch", "CRUNCH"), ("regenamt", "REGEN"), ("mix", "MIX"), ("out", "OUT")],
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("CODEC")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("wire-codec")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "BITRATE", &params.bitrate, setter);
                                    row(ui, "MODE", &params.mode, setter);
                                    ui.end_row();
                                    row(ui, "BANDWIDTH", &params.bandwidth, setter);
                                    ui.end_row();
                                });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("FEC")
                                        .color(suite_core::ui::TEXT_DIM)
                                        .small(),
                                );
                                let mut fv = params.fec.value();
                                if ui.checkbox(&mut fv, "in-band FEC").changed() {
                                    setter.begin_set_parameter(&params.fec);
                                    setter.set_parameter(&params.fec, fv);
                                    setter.end_set_parameter(&params.fec);
                                }
                            });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("DEGRADE")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("wire-degrade")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "PACKET LOSS", &params.loss, setter);
                                    row(ui, "CRUNCH", &params.crunch, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("REGEN (generation loss)")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("wire-regen")
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    row(ui, "DELAY", &params.regen_delay, setter);
                                    row(ui, "AMOUNT", &params.regen_amount, setter);
                                    ui.end_row();
                                });
                            ui.separator();

                            ui.label(
                                egui::RichText::new("OUTPUT")
                                    .color(suite_core::ui::TEXT_DIM)
                                    .small(),
                            );
                            egui::Grid::new("wire-out")
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
        self.core = WireCore::new(buffer_config.sample_rate);
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "WIRE");
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

        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.crunch = routes.modulated_float("crunch", &self.params.crunch, bus);
                s.regen_amount = routes.modulated_float("regenamt", &self.params.regen_amount, bus);
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
                s.out_db = routes.modulated_float("out", &self.params.out, bus);
            }
        }
        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        // opus-rs allocates internally; wrap the whole frame/codec work in permit_alloc so the
        // wrapper's `assert_process_allocs` guard tolerates it. The codec cost is ~0.3 % RT.
        let core = &mut self.core;
        nih_plug::util::permit_alloc(|| {
            core.configure(&s);
            for n in 0..num_samples {
                let l_in = main[0][n];
                let r_in = if num_main > 1 { main[1][n] } else { l_in };
                let (o_l, o_r) = core.process_sample(l_in, r_in, &s);
                main[0][n] = o_l;
                if num_main > 1 {
                    main[1][n] = o_r;
                }
            }
        });

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

impl Drop for Wire {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for Wire {
    const CLAP_ID: &'static str = "com.qeynos.wire";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Codec degradation — Opus round-trip through a pure-Rust encoder/decoder, with crunch, packet-loss simulation, and a re-encoding generation-loss regen loop");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
        ClapFeature::Custom("lo-fi"),
    ];
}

impl Vst3Plugin for Wire {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosWIREcodec1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Wire);
nih_export_vst3!(Wire);

#[cfg(test)]
mod render_tests {
    use crate::dsp::WireCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;
    use suite_core::testsig;

    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(crate::MANUAL_DOC, &crate::WireParams::default());
    }

    /// Render each factory preset over pink noise and a full-band chirp, write the WAVs into
    /// renders/WIRE/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let pink = testsig::pink_noise(0.5, (sr * 3.0) as usize, 4242);
        let chirp = testsig::log_chirp(60.0, 12_000.0, 0.5, (sr * 3.0) as usize, sr);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 5);
        for p in &presets {
            let s = settings_from_preset(p);
            let fname = p.name.to_lowercase().replace([' ', '·', '-', '/'], "_");

            let mut core = WireCore::new(sr);
            let mut out = pink.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("WIRE", &format!("{fname}_pink"));
            write_wav(&path, &out, sr as u32).expect("write pink render");

            let mut core = WireCore::new(sr);
            let mut out = chirp.clone();
            core.process_mono(&mut out, &s);
            assert_universal(&out);
            let path = render_path("WIRE", &format!("{fname}_chirp"));
            write_wav(&path, &out, sr as u32).expect("write chirp render");
        }
    }
}
