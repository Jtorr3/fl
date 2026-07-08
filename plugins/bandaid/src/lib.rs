//! BANDAID — multiband transient designer (Qeynos suite, Phase 3).
//!
//! An LR4 3-band split feeds a per-band transient detector (fast 1 ms − slow 50 ms envelope
//! difference); the positive region is the attack, the negative region the sustain/tail. Each
//! band applies an attack-region gain and a sustain-region gain (±12 dB, 5 ms-smoothed) and is
//! recombined as a **parallel delta** — `out = x + Σ (g_b − 1)·band_b` — so all-neutral gains
//! null against the dry input exactly (PRD §4 done-bar). Per-band solo auditions one shaped
//! band. Zero latency. See [`dsp`] for the core (shared verbatim with the tests).

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

use dsp::{BandaidCore, Settings};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/BANDAID.md");

/// Plain-number string→value parser for the dB / scale params stored as raw values (strips a
/// trailing unit like " dB"). Mirrors CARVE's `num_s2v`.
fn num_s2v() -> Arc<dyn Fn(&str) -> Option<f32> + Send + Sync> {
    Arc::new(|s: &str| {
        let s = s.trim();
        let mut end = 0;
        for (i, c) in s.char_indices() {
            if c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E') {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        s[..end].parse::<f32>().ok()
    })
}

/// A per-band ±12 dB shaping knob (attack or sustain), stored as dB directly.
fn gain_param(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -12.0, max: 12.0 })
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
        .with_string_to_value(num_s2v())
}

pub struct Bandaid {
    params: Arc<BandaidParams>,
    core: [BandaidCore; 2],
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct BandaidParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // --- crossovers ---
    #[id = "xlow"]
    pub xover_low: FloatParam,
    #[id = "xhigh"]
    pub xover_high: FloatParam,

    // --- per-band attack / sustain (dB, ±12) ---
    #[id = "latk"]
    pub low_attack: FloatParam,
    #[id = "lsus"]
    pub low_sustain: FloatParam,
    #[id = "matk"]
    pub mid_attack: FloatParam,
    #[id = "msus"]
    pub mid_sustain: FloatParam,
    #[id = "hatk"]
    pub high_attack: FloatParam,
    #[id = "hsus"]
    pub high_sustain: FloatParam,

    // --- per-band solo / listen ---
    #[id = "lsolo"]
    pub low_solo: BoolParam,
    #[id = "msolo"]
    pub mid_solo: BoolParam,
    #[id = "hsolo"]
    pub high_solo: BoolParam,

    // --- global ---
    #[id = "det"]
    pub detector: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for BandaidParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(600, 500),

            xover_low: FloatParam::new(
                "Xover Low",
                d.xover_low,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 800.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(0))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            xover_high: FloatParam::new(
                "Xover High",
                d.xover_high,
                FloatRange::Skewed {
                    min: 800.0,
                    max: 8000.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(0))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),

            low_attack: gain_param("Low Attack"),
            low_sustain: gain_param("Low Sustain"),
            mid_attack: gain_param("Mid Attack"),
            mid_sustain: gain_param("Mid Sustain"),
            high_attack: gain_param("High Attack"),
            high_sustain: gain_param("High Sustain"),

            low_solo: BoolParam::new("Low Solo", false),
            mid_solo: BoolParam::new("Mid Solo", false),
            high_solo: BoolParam::new("High Solo", false),

            detector: FloatParam::new(
                "Detector",
                d.det_scale,
                FloatRange::Skewed {
                    min: 0.5,
                    max: 2.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(num_s2v()),
            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_string_to_value(num_s2v()),

            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl BandaidParams {
    fn snapshot(&self) -> Settings {
        Settings {
            xover_low: self.xover_low.value(),
            xover_high: self.xover_high.value(),
            attack_db: [
                self.low_attack.value(),
                self.mid_attack.value(),
                self.high_attack.value(),
            ],
            sustain_db: [
                self.low_sustain.value(),
                self.mid_sustain.value(),
                self.high_sustain.value(),
            ],
            solo: [
                self.low_solo.value(),
                self.mid_solo.value(),
                self.high_solo.value(),
            ],
            det_scale: self.detector.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Bandaid {
    fn default() -> Self {
        Self {
            params: Arc::new(BandaidParams::default()),
            core: [BandaidCore::new(48_000.0), BandaidCore::new(48_000.0)],
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset through the host (automation/undo see every scalar). Per-band solo
/// is live audition state and is never touched by a preset.
fn apply_preset(params: &BandaidParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.xover_low, s.xover_low);
    set_f(&params.xover_high, s.xover_high);
    set_f(&params.low_attack, s.attack_db[0]);
    set_f(&params.mid_attack, s.attack_db[1]);
    set_f(&params.high_attack, s.attack_db[2]);
    set_f(&params.low_sustain, s.sustain_db[0]);
    set_f(&params.mid_sustain, s.sustain_db[1]);
    set_f(&params.high_sustain, s.sustain_db[2]);
    set_f(&params.detector, s.det_scale);
    set_f(&params.mix, s.mix);
    set_f(&params.out, s.out_db);
}

impl Plugin for Bandaid {
    const NAME: &'static str = "Qeynos BANDAID";
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
                suite_core::ui::ScaledWindow::new("qeynos-bandaid-window", Vec2::new(600.0, 500.0))
                    .min_size(Vec2::new(500.0, 420.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · BANDAID").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "bandaid", "BANDAID", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new(
                                "multiband transient designer — LR4 3-band attack / sustain shaping",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("bandaid", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("latk", "LOW ATK"), ("hatk", "HI ATK"), ("mix", "MIX")],
                        );
                        ui.separator();

                        // Global row: crossovers + detector + mix + out.
                        egui::Grid::new("bandaid-global")
                            .num_columns(5)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "XOVER LOW", &params.xover_low, setter);
                                row(ui, "XOVER HIGH", &params.xover_high, setter);
                                row(ui, "DETECTOR", &params.detector, setter);
                                row(ui, "MIX", &params.mix, setter);
                                row(ui, "OUT", &params.out, setter);
                                ui.end_row();
                            });
                        ui.add_space(6.0);

                        // Per-band groups: ATTACK / SUSTAIN / SOLO.
                        band_group(ui, setter, "LOW", &params.low_attack, &params.low_sustain, &params.low_solo);
                        band_group(ui, setter, "MID", &params.mid_attack, &params.mid_sustain, &params.mid_solo);
                        band_group(ui, setter, "HIGH", &params.high_attack, &params.high_sustain, &params.high_solo);
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
        self.core = [
            BandaidCore::new(buffer_config.sample_rate),
            BandaidCore::new(buffer_config.sample_rate),
        ];
        // Zero latency — minimum-phase LR4, dry path never delayed.
        context.set_latency_samples(self.core[0].latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "BANDAID");
        true
    }

    fn reset(&mut self) {
        for c in self.core.iter_mut() {
            c.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // Settings (+ NERVE listen layer for low/high attack + mix).
        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.attack_db[0] = routes.modulated_float("latk", &self.params.low_attack, bus);
                s.attack_db[2] = routes.modulated_float("hatk", &self.params.high_attack, bus);
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
            }
        }
        for c in self.core.iter_mut() {
            c.configure(&s);
        }

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_ch = main.len();
        if num_ch == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l = main[0][n];
            main[0][n] = self.core[0].process_sample(l);
            if num_ch > 1 {
                let r = main[1][n];
                main[1][n] = self.core[1].process_sample(r);
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

impl Drop for Bandaid {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

/// One band's control group: a label + attack / sustain knobs + a solo toggle.
fn band_group(
    ui: &mut egui::Ui,
    setter: &ParamSetter,
    label: &str,
    attack: &FloatParam,
    sustain: &FloatParam,
    solo: &BoolParam,
) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .color(suite_core::ui::ACCENT)
                    .strong(),
            );
            ui.add_space(8.0);
            suite_core::ui::labeled_slider(ui, "ATTACK", attack, setter);
            suite_core::ui::labeled_slider(ui, "SUSTAIN", sustain, setter);
            solo_button(ui, setter, solo);
        });
    });
}

/// A solo/listen toggle wired to a `BoolParam` through the setter.
fn solo_button(ui: &mut egui::Ui, setter: &ParamSetter, param: &BoolParam) {
    let on = param.value();
    let text = egui::RichText::new("SOLO").strong().color(if on {
        suite_core::ui::BG
    } else {
        suite_core::ui::TEXT_DIM
    });
    let mut btn = egui::Button::new(text).min_size(Vec2::new(54.0, 22.0));
    if on {
        btn = btn.fill(suite_core::ui::ACCENT);
    }
    if ui.add(btn).clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !on);
        setter.end_set_parameter(param);
    }
}

impl ClapPlugin for Bandaid {
    const CLAP_ID: &'static str = "com.qeynos.bandaid";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Multiband transient designer — LR4 3-band split, per-band attack/sustain shaping via a \
         fast/slow envelope difference, per-band solo, parallel-delta reconstruction (neutral = \
         exact null), zero latency",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Custom("transient"),
    ];
}

impl Vst3Plugin for Bandaid {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosBANDAIDtr1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(Bandaid);
nih_export_vst3!(Bandaid);

#[cfg(test)]
mod tests;
