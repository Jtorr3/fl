//! PATINA — analog lo-fi character (Qeynos suite, Phase 3).
//!
//! A tape/vinyl character chain: wow/flutter (fractional-delay pitch wobble), tape saturation
//! (2x oversampled), a head-bump low shelf, an azimuth HF phase skew, random dropouts, and a
//! noise layer (hiss + hum + crackle) keyed to the input envelope — all scaled together by an
//! **AGE** macro. Every section is an exact identity at amount 0, so age 0 + all sections 0
//! nulls against the latency-matched dry (PRD §4 done-bar). Reports `dsp::LATENCY` samples of
//! latency (base wow delay + saturation oversampler delay). See [`dsp`] for the core (shared
//! verbatim with the tests).

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

use dsp::{PatinaCore, Settings};
use suite_core::presets::{load_all, Preset};

/// Plain-number string→value parser (strips a trailing unit like " dB" / " Hz").
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

/// A 0..1 "amount" knob rendered as a percentage.
fn pct_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

pub struct Patina {
    params: Arc<PatinaParams>,
    core: PatinaCore,
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct PatinaParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // --- wow / flutter ---
    #[id = "wow"]
    pub wow: FloatParam,
    #[id = "wowrate"]
    pub wow_rate: FloatParam,
    #[id = "flut"]
    pub flutter: FloatParam,

    // --- saturation + tone ---
    #[id = "sat"]
    pub sat: FloatParam,
    #[id = "bump"]
    pub bump: FloatParam,
    #[id = "bumpf"]
    pub bump_freq: FloatParam,
    #[id = "azim"]
    pub azimuth: FloatParam,

    // --- dropouts ---
    #[id = "droprate"]
    pub dropout_rate: FloatParam,
    #[id = "dropdep"]
    pub dropout_depth: FloatParam,

    // --- noise ---
    #[id = "hiss"]
    pub hiss: FloatParam,
    #[id = "hum"]
    pub hum: FloatParam,
    #[id = "crackle"]
    pub crackle: FloatParam,
    #[id = "hum60"]
    pub hum_60: BoolParam,
    #[id = "key"]
    pub key_amount: FloatParam,

    // --- macros / output ---
    #[id = "age"]
    pub age: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for PatinaParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(620, 560),

            wow: pct_param("Wow", d.wow_depth),
            wow_rate: FloatParam::new(
                "Wow Rate",
                d.wow_rate,
                FloatRange::Skewed {
                    min: 0.25,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(num_s2v()),
            flutter: pct_param("Flutter", d.flutter),

            sat: pct_param("Saturation", d.sat_drive),
            bump: pct_param("Head Bump", d.bump_amount),
            bump_freq: FloatParam::new(
                "Bump Freq",
                d.bump_freq,
                FloatRange::Linear { min: 60.0, max: 120.0 },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0))
            .with_string_to_value(num_s2v()),
            azimuth: pct_param("Azimuth", d.azimuth),

            dropout_rate: pct_param("Dropout Rate", d.dropout_rate),
            dropout_depth: pct_param("Dropout Depth", d.dropout_depth),

            hiss: pct_param("Hiss", d.hiss),
            hum: pct_param("Hum", d.hum),
            crackle: pct_param("Crackle", d.crackle),
            hum_60: BoolParam::new("Hum 60 Hz", d.hum_60),
            key_amount: pct_param("Noise Key", d.key_amount),

            age: pct_param("Age", d.age),
            mix: pct_param("Mix", d.mix),
            out: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_string_to_value(num_s2v()),

            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl PatinaParams {
    fn snapshot(&self) -> Settings {
        Settings {
            wow_depth: self.wow.value(),
            wow_rate: self.wow_rate.value(),
            flutter: self.flutter.value(),
            sat_drive: self.sat.value(),
            bump_amount: self.bump.value(),
            bump_freq: self.bump_freq.value(),
            azimuth: self.azimuth.value(),
            dropout_rate: self.dropout_rate.value(),
            dropout_depth: self.dropout_depth.value(),
            hiss: self.hiss.value(),
            hum: self.hum.value(),
            crackle: self.crackle.value(),
            hum_60: self.hum_60.value(),
            key_amount: self.key_amount.value(),
            age: self.age.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Patina {
    fn default() -> Self {
        Self {
            params: Arc::new(PatinaParams::default()),
            core: PatinaCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset through the host (automation/undo see every scalar).
fn apply_preset(params: &PatinaParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.wow, s.wow_depth);
    set_f(&params.wow_rate, s.wow_rate);
    set_f(&params.flutter, s.flutter);
    set_f(&params.sat, s.sat_drive);
    set_f(&params.bump, s.bump_amount);
    set_f(&params.bump_freq, s.bump_freq);
    set_f(&params.azimuth, s.azimuth);
    set_f(&params.dropout_rate, s.dropout_rate);
    set_f(&params.dropout_depth, s.dropout_depth);
    set_f(&params.hiss, s.hiss);
    set_f(&params.hum, s.hum);
    set_f(&params.crackle, s.crackle);
    set_f(&params.key_amount, s.key_amount);
    set_f(&params.age, s.age);
    set_f(&params.mix, s.mix);
    set_f(&params.out, s.out_db);
    setter.begin_set_parameter(&params.hum_60);
    setter.set_parameter(&params.hum_60, s.hum_60);
    setter.end_set_parameter(&params.hum_60);
}

impl Plugin for Patina {
    const NAME: &'static str = "Qeynos PATINA";
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
                suite_core::ui::ScaledWindow::new("qeynos-patina-window", Vec2::new(620.0, 560.0))
                    .min_size(Vec2::new(520.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · PATINA").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(
                                "analog lo-fi character — wow/flutter · tape sat · dropouts · keyed noise",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("patina", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("wow", "WOW"), ("age", "AGE"), ("mix", "MIX")],
                        );
                        ui.separator();

                        // AGE macro + mix/out headline row.
                        egui::Grid::new("patina-macro")
                            .num_columns(3)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "AGE", &params.age, setter);
                                row(ui, "MIX", &params.mix, setter);
                                row(ui, "OUT", &params.out, setter);
                                ui.end_row();
                            });
                        ui.add_space(4.0);

                        section(ui, "WOW / FLUTTER", |ui| {
                            row(ui, "WOW", &params.wow, setter);
                            row(ui, "WOW RATE", &params.wow_rate, setter);
                            row(ui, "FLUTTER", &params.flutter, setter);
                        });
                        section(ui, "SATURATION / TONE", |ui| {
                            row(ui, "SAT", &params.sat, setter);
                            row(ui, "BUMP", &params.bump, setter);
                            row(ui, "BUMP FREQ", &params.bump_freq, setter);
                            row(ui, "AZIMUTH", &params.azimuth, setter);
                        });
                        section(ui, "DROPOUTS", |ui| {
                            row(ui, "RATE", &params.dropout_rate, setter);
                            row(ui, "DEPTH", &params.dropout_depth, setter);
                        });
                        section(ui, "NOISE", |ui| {
                            row(ui, "HISS", &params.hiss, setter);
                            row(ui, "HUM", &params.hum, setter);
                            row(ui, "CRACKLE", &params.crackle, setter);
                            row(ui, "KEY", &params.key_amount, setter);
                            hum_toggle(ui, setter, &params.hum_60);
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
        self.core = PatinaCore::new(buffer_config.sample_rate);
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
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // Settings (+ NERVE listen layer for wow / age / mix).
        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.wow_depth = routes.modulated_float("wow", &self.params.wow, bus);
                s.age = routes.modulated_float("age", &self.params.age, bus);
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
            }
        }
        self.core.configure(&s);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_ch = main.len();
        if num_ch == 0 {
            return ProcessStatus::Normal;
        }

        if num_ch == 1 {
            for n in 0..num_samples {
                main[0][n] = self.core.process_mono(main[0][n]);
            }
        } else {
            let (l, rest) = main.split_at_mut(1);
            let r = &mut rest[0];
            for n in 0..num_samples {
                let (ol, or) = self.core.process_stereo(l[0][n], r[n]);
                l[0][n] = ol;
                r[n] = or;
            }
        }

        ProcessStatus::Normal
    }
}

/// A titled group of knobs.
fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.label(
            egui::RichText::new(title)
                .color(suite_core::ui::ACCENT)
                .strong(),
        );
        ui.horizontal_wrapped(|ui| add(ui));
    });
}

/// 50/60 Hz hum toggle wired to a `BoolParam`.
fn hum_toggle(ui: &mut egui::Ui, setter: &ParamSetter, param: &BoolParam) {
    let on = param.value();
    let text = egui::RichText::new(if on { "HUM 60Hz" } else { "HUM 50Hz" })
        .strong()
        .color(if on {
            suite_core::ui::BG
        } else {
            suite_core::ui::TEXT_DIM
        });
    let mut btn = egui::Button::new(text).min_size(Vec2::new(80.0, 22.0));
    if on {
        btn = btn.fill(suite_core::ui::ACCENT);
    }
    if ui.add(btn).clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !on);
        setter.end_set_parameter(param);
    }
}

impl ClapPlugin for Patina {
    const CLAP_ID: &'static str = "com.qeynos.patina";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Analog lo-fi character — wow/flutter fractional-delay pitch wobble, tape saturation (2x \
         OS), head-bump low shelf, azimuth HF phase skew, random dropouts, and an input-keyed \
         noise layer (hiss/hum/crackle), all scaled by an AGE macro. Reports latency; neutral = \
         exact null",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
        ClapFeature::Custom("lofi"),
    ];
}

impl Vst3Plugin for Patina {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosPATINAlofi";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Patina);
nih_export_vst3!(Patina);

#[cfg(test)]
mod tests;
