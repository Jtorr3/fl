//! CARVE — spectral ducker (Qeynos suite, Phase 3; Trackspacer clone).
//!
//! A sidechain STFT drives a per-~1/3-octave-band gain reduction on the main signal's matching
//! bands: the more energy the sidechain has in a band, the deeper the main is cut there. This
//! *carves* a frequency-matched pocket for the sidechain instead of ducking the whole broadband
//! level. Two STFT roles (2048 / hop 512 / Hann) run in lockstep — one mono sidechain analysis
//! STFT computes the per-band gains at the frame boundary, one main STFT per channel applies
//! them — then the carved bins are resynthesised (iSTFT/OLA). Reports 2048-sample latency and a
//! latency-matched dry path for the mix null.
//!
//! - **Soft-knee** gain reduction from SC band energy vs a threshold, **attack/release**
//!   smoothed per band-group at hop rate, **tilt** (bias depth toward lows/highs), **max depth**
//!   (0–24 dB) and a **sensitivity** curve (knee width + excess span).
//! - **Δ-listen** monitors the carved residual (what's removed); **SC-listen** passes the
//!   sidechain through. A small per-band reduction meter reads the live envelope.
//!
//! The DSP math lives in [`dsp`] (pure Rust, shared with the offline harness tests) atop
//! `suite_core::stft`.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use suite_core::bus::PluginKind;
use suite_core::modlisten::ModRoutes;
use suite_core::spectrum::SpectrumPublisher;

pub mod dsp;
pub mod presets;

#[cfg(test)]
mod tests;

use dsp::{CarveCore, ListenMode, Settings, N_DISPLAY};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/CARVE.md");

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

/// Monitoring / output mode (the automatable param mirror of [`dsp::ListenMode`]).
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ListenParam {
    #[name = "Off"]
    Off,
    #[name = "Sidechain"]
    Sidechain,
    #[name = "Delta"]
    Delta,
}

impl ListenParam {
    fn to_mode(self) -> ListenMode {
        match self {
            ListenParam::Off => ListenMode::Off,
            ListenParam::Sidechain => ListenMode::Sidechain,
            ListenParam::Delta => ListenMode::Delta,
        }
    }
    fn from_mode(m: ListenMode) -> Self {
        match m {
            ListenMode::Off => ListenParam::Off,
            ListenMode::Sidechain => ListenParam::Sidechain,
            ListenMode::Delta => ListenParam::Delta,
        }
    }
}

pub struct Carve {
    params: Arc<CarveParams>,
    core: CarveCore,
    factory_presets: Arc<Vec<Preset>>,
    /// Per-display-band reduction (0..1) published to the GUI meter.
    meter: Arc<Vec<AtomicF32>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct CarveParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "amount"] pub amount: FloatParam,
    #[id = "maxdepth"] pub maxdepth: FloatParam,
    #[id = "thresh"] pub thresh: FloatParam,
    #[id = "tilt"] pub tilt: FloatParam,
    #[id = "attack"] pub attack: FloatParam,
    #[id = "release"] pub release: FloatParam,
    #[id = "sens"] pub sens: FloatParam,
    #[id = "listen"] pub listen: EnumParam<ListenParam>,
    #[id = "mix"] pub mix: FloatParam,
    #[id = "out"] pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

/// Parse the leading (optionally signed / decimal) number out of a display string like
/// "-45.0 dB" or "12 ms", ignoring any trailing unit — the round-trip inverse of the
/// `v2s_f32_rounded` formatters (needed for clap-validator's param-conversions check).
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

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn ms(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-1.0) },
    )
    .with_unit(" ms")
    .with_value_to_string(formatters::v2s_f32_rounded(1))
    .with_string_to_value(num_s2v())
}

impl Default for CarveParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(600, 560),

            amount: pct("Amount", d.amount).with_smoother(SmoothingStyle::Linear(20.0)),
            maxdepth: FloatParam::new(
                "Max Depth",
                d.max_depth_db,
                FloatRange::Linear { min: 0.0, max: dsp::MAX_DEPTH_LIMIT },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(num_s2v()),
            thresh: FloatParam::new(
                "Threshold",
                d.threshold_db,
                FloatRange::Linear { min: -90.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(num_s2v()),
            tilt: FloatParam::new("Tilt", d.tilt, FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2))
                .with_string_to_value(num_s2v()),
            attack: ms("Attack", d.attack_ms, 1.0, 50.0),
            release: ms("Release", d.release_ms, 20.0, 500.0),
            sens: pct("Sensitivity", d.sens),
            listen: EnumParam::new("Listen", ListenParam::Off),

            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new(
                "Out",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-24.0),
                    max: util::db_to_gain(24.0),
                    factor: FloatRange::gain_skew_factor(-24.0, 24.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl CarveParams {
    /// Snapshot the current parameter values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            amount: self.amount.value(),
            max_depth_db: self.maxdepth.value(),
            threshold_db: self.thresh.value(),
            tilt: self.tilt.value(),
            attack_ms: self.attack.value(),
            release_ms: self.release.value(),
            sens: self.sens.value(),
            listen: self.listen.value().to_mode(),
            mix: self.mix.value(),
            out_gain: self.out.value(),
        }
    }
}

impl Default for Carve {
    fn default() -> Self {
        let mut meter = Vec::with_capacity(N_DISPLAY);
        for _ in 0..N_DISPLAY {
            meter.push(AtomicF32::new(0.0));
        }
        Self {
            params: Arc::new(CarveParams::default()),
            core: CarveCore::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            meter: Arc::new(meter),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (so automation/undo see it).
fn apply_preset(params: &CarveParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
    let set_f = |param: &FloatParam, v: f32| {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    };
    set_f(&params.amount, s.amount);
    set_f(&params.maxdepth, s.max_depth_db);
    set_f(&params.thresh, s.threshold_db);
    set_f(&params.tilt, s.tilt);
    set_f(&params.attack, s.attack_ms);
    set_f(&params.release, s.release_ms);
    set_f(&params.sens, s.sens);
    set_f(&params.mix, s.mix);
    set_f(&params.out, s.out_gain);
    let lp = ListenParam::from_mode(s.listen);
    setter.begin_set_parameter(&params.listen);
    setter.set_parameter(&params.listen, lp);
    setter.end_set_parameter(&params.listen);
}

impl Plugin for Carve {
    const NAME: &'static str = "Qeynos CARVE";
    const VENDOR: &'static str = "Qeynos";
    const URL: &'static str = "https://github.com/Jtorr3/fl";
    const EMAIL: &'static str = "jason@qeynosholdings.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { layout: Some("Stereo"), ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
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
        let meter = self.meter.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-carve-window", Vec2::new(600.0, 560.0))
                    .min_size(Vec2::new(500.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · CARVE").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "carve", "CARVE", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("spectral ducker — sidechain carves matching frequencies")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("carve", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("amount", "AMOUNT"), ("maxdepth", "MAX DEPTH"), ("sens", "SENS"), ("mix", "MIX")],
                        );
                        ui.separator();

                        // Live per-band reduction meter, hosted in the CONSOLE v2 CRT
                        // telemetry bay (display-only meter; in THEME-OFF it degrades to a
                        // plain panel with the original colors).
                        section(ui, "REDUCTION");
                        suite_core::ui::crt_frame(ui, "carve-crt", 62.0, |ui| {
                            reduction_meter(ui, &meter);
                        });
                        ui.add_space(6.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            use suite_core::ui::labeled_slider as row;

                            section(ui, "DUCK");
                            egui::Grid::new("carve-duck").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "AMOUNT", &params.amount, setter);
                                row(ui, "MAX DEPTH", &params.maxdepth, setter);
                                ui.end_row();
                                row(ui, "THRESHOLD", &params.thresh, setter);
                                row(ui, "SENSITIVITY", &params.sens, setter);
                                ui.end_row();
                                row(ui, "TILT", &params.tilt, setter);
                                ui.end_row();
                            });

                            section(ui, "ENVELOPE");
                            egui::Grid::new("carve-env").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "ATTACK", &params.attack, setter);
                                row(ui, "RELEASE", &params.release, setter);
                                ui.end_row();
                            });

                            section(ui, "MONITOR · OUTPUT");
                            egui::Grid::new("carve-out").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "LISTEN", &params.listen, setter);
                                ui.end_row();
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
        self.core = CarveCore::new(buffer_config.sample_rate);
        context.set_latency_samples(self.core.latency() as u32);
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "CARVE");
        true
    }

    fn reset(&mut self) {
        self.core.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        let mut s = self.params.snapshot();
        // NERVE listen layer. amount/maxdepth/sens feed the core through `configure`, so route
        // them straight into the snapshot. MIX is applied per-sample from its own smoother below
        // (the core is passed `mix` explicitly — it never reads Settings.mix), so route it as a
        // block-rate PLAIN offset added to the smoothed value; before the fix the modulated
        // Settings.mix was inert because the per-sample path used the unmodulated smoother.
        let mut mix_delta = 0.0f32;
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.amount = routes.modulated_float("amount", &self.params.amount, bus);
                s.max_depth_db = routes.modulated_float("maxdepth", &self.params.maxdepth, bus);
                s.sens = routes.modulated_float("sens", &self.params.sens, bus);
                mix_delta =
                    routes.modulated_float("mix", &self.params.mix, bus) - self.params.mix.value();
            }
        }
        self.core.configure(&s);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_main = main.len();
        if num_main == 0 {
            return ProcessStatus::Normal;
        }

        // Sidechain: mono-sum the first aux port's channels (0 if absent).
        let sc_slice: Option<&[&mut [f32]]> = if aux.inputs.is_empty() {
            None
        } else {
            Some(aux.inputs[0].as_slice_immutable())
        };

        for n in 0..num_samples {
            let l = main[0][n];
            let r = if num_main > 1 { main[1][n] } else { l };

            let sc = match sc_slice {
                Some(chs) if !chs.is_empty() => {
                    let mut acc = 0.0f32;
                    for ch in chs.iter() {
                        acc += ch[n];
                    }
                    acc / chs.len() as f32
                }
                _ => 0.0,
            };

            let mix = dsp::apply_mod_delta(self.params.mix.smoothed.next(), mix_delta, 0.0, 1.0);
            let out_gain = self.params.out.smoothed.next();
            let (out_l, out_r) = self.core.process_sample(l, r, sc, mix, out_gain);
            main[0][n] = out_l;
            if num_main > 1 {
                main[1][n] = out_r;
            }
        }

        // Publish the live reduction meter for the GUI.
        if self.params.editor_state.is_open() {
            let d = self.core.display_reductions();
            for (i, v) in d.iter().enumerate() {
                self.meter[i].store(*v, Ordering::Relaxed);
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

impl Drop for Carve {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(title).color(suite_core::ui::ACCENT).small());
    ui.separator();
}

/// Horizontal per-band reduction meter: taller bar = deeper cut in that frequency band.
fn reduction_meter(ui: &mut egui::Ui, meter: &[AtomicF32]) {
    // Honor the CRT-motion pref + ~8 fps idle guarantee (guardrails #2/#6).
    suite_core::ui::scope_repaint(ui.ctx());
    let n = meter.len().max(1);
    let avail = ui.available_width().min(560.0);
    let size = Vec2::new(avail, 46.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    // CONSOLE re-skins the meter toward the amber phosphor palette; THEME-OFF keeps the
    // original panel + amber bars. The bar HEIGHTS carry the meaning (per-band cut depth).
    let console = suite_core::ui::console_on(ui.ctx());
    let painter = ui.painter_at(rect);
    if !console {
        painter.rect_filled(rect, 3.0, suite_core::ui::PANEL);
    }
    let bar_col = if console {
        suite_core::ui::PHOSPHOR
    } else {
        suite_core::ui::ACCENT
    };
    let gap = 3.0;
    let bw = (rect.width() - gap * (n as f32 + 1.0)) / n as f32;
    for i in 0..n {
        let e = meter[i].load(Ordering::Relaxed).clamp(0.0, 1.0);
        let h = e * (rect.height() - 6.0);
        let x0 = rect.left() + gap + i as f32 * (bw + gap);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, rect.bottom() - 3.0 - h),
            egui::pos2(x0 + bw, rect.bottom() - 3.0),
        );
        painter.rect_filled(bar, 2.0, bar_col);
    }
}

impl ClapPlugin for Carve {
    const CLAP_ID: &'static str = "com.qeynos.carve";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Spectral ducker — a sidechain carves its matching frequencies out of the main signal");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Compressor,
    ];
}

impl Vst3Plugin for Carve {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosCARVEduck1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(Carve);
nih_export_vst3!(Carve);

#[cfg(test)]
mod render_tests {
    use crate::dsp::CarveCore;
    use crate::presets::{settings_from_preset, PRESET_JSON};
    use suite_core::harness::{assert_universal, render_path, write_wav};
    use suite_core::presets::load_all;

    /// Render each factory preset with a full-spectrum pink main and a band-limited (500 Hz–2 kHz)
    /// sidechain pulse train, write to renders/CARVE/, and assert the universal properties.
    #[test]
    fn every_preset_renders_and_passes_universal() {
        let sr = 48_000.0f32;
        let n = (sr * 2.5) as usize;

        // Main: steady full-spectrum pink noise.
        let main = suite_core::testsig::pink_noise(0.5, n, 9182);
        // Sidechain: band-limited noise, gated into ~4 Hz pulses so the duck opens/closes.
        let sc = crate::tests::band_limited_pulses(500.0, 2000.0, 0.5, 4.0, n, sr, 271);

        let presets = load_all(PRESET_JSON);
        assert!(presets.len() >= 6);
        for p in &presets {
            let s = settings_from_preset(p);
            let mut core = CarveCore::new(sr);
            let mut out = main.clone();
            core.process_mono(&mut out, &sc, &s);
            assert_universal(&out);
            let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
            let path = render_path("CARVE", &fname);
            write_wav(&path, &out, sr as u32).expect("write render");
        }
    }

    /// SOUND-PASS audition render (permanent infra, `#[ignore]`d in normal runs).
    /// Renders every factory preset AND `Settings::default()` over the genre-right
    /// KAS:ST scenario — main = detuned reese bass, sidechain = a 4-on-the-floor kick
    /// loop — into renders/_audition/CARVE/<QVS_AUDITION_DIR or "before">/<preset>.wav.
    /// The spectral duck should carve the kick's pocket out of the reese without
    /// pumping / warble. Analyzed offline by tools/audition.py.
    #[test]
    #[ignore]
    fn audition_render_musical_sources() {
        use crate::dsp::Settings;
        use suite_core::testsig;

        let sr = 48_000.0f32;
        let subdir = std::env::var("QVS_AUDITION_DIR").unwrap_or_else(|_| "before".into());

        // Main: 4 s of detuned reese bass at 50 Hz (dark, low, thick — the KAS:ST reese).
        let main_src = testsig::synth_reese(50.0, 4.0, sr);
        // Sidechain: 4-on-the-floor kick loop at 140 BPM, sized to cover the main length
        // (mirror GRIT's sidechain sizing).
        let bar_samples = (60.0 / 140.0 * sr).round() as usize * 4;
        let n_bars = (main_src.len() / bar_samples) + 2;
        let sc = testsig::synth_kick_loop(140.0, n_bars, sr);

        // Render every factory preset plus the default state (labelled "default").
        let presets = load_all(PRESET_JSON);
        let mut jobs: Vec<(String, Settings)> = presets
            .iter()
            .map(|p| {
                let fname = p.name.to_lowercase().replace([' ', '·', '-'], "_");
                (fname, settings_from_preset(p))
            })
            .collect();
        jobs.push(("default".into(), Settings::default()));

        // Dry-source baseline: the unprocessed reese, so the analyzer can separate
        // artifacts CARVE introduces from ones inherent to the raw saw-bass source.
        {
            let path = render_path("_audition/CARVE", &format!("{subdir}/_dry_reese"));
            write_wav(&path, &main_src, sr as u32).expect("write dry baseline");
        }

        for (fname, s) in &jobs {
            let mut core = CarveCore::new(sr);
            let mut out = main_src.clone();
            core.process_mono(&mut out, &sc, s);
            let path = render_path("_audition/CARVE", &format!("{subdir}/{fname}"));
            write_wav(&path, &out, sr as u32).expect("write audition render");
        }
    }
}
