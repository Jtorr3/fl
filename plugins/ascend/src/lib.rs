//! ASCEND — tension generator (Qeynos suite, taste-tailored Phase 3). A MIDI/transport instrument.
//!
//! ASCEND reads the host transport and counts down to the next N-bar boundary (8/16/32 or a custom
//! bar count). One master **tension envelope** (curve-morphed exp↔linear↔log) climbs across that
//! countdown and drives four things at once on its sources — filtered noise (white↔pink) plus a
//! tonal root+fifth oscillator stack: an SVF **filter sweep** (start→end cutoff, opening up), a
//! **pitch rise** of 0–24 semitones on the tonal stack, a **width bloom** (narrow→wide), and a
//! **volume swell** (quiet→full). At the target bar it fires an embedded **impact** (IMPACT's own
//! synth-kick math at a low pitch) and **auto-cuts** the sources to silence, re-arming for the next
//! boundary. **Downlifter** mode reverses the envelope (full at the boundary, falling away). With
//! the transport stopped a manual **TRIGGER** (or a MIDI note) runs the same envelope over a
//! time-based length, and with **key-track** on the root follows the played note.
//!
//! DSP core lives in [`dsp`] (pure Rust, shared with the offline harness tests).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use suite_core::bus::PluginKind;
use suite_core::modlisten::ModRoutes;
use suite_core::spectrum::SpectrumPublisher;

pub mod dsp;
pub mod presets;

use dsp::{AscendEngine, Settings, SyncTarget, TransportFrame};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/ASCEND.md");

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Audio-thread → GUI shared state (lock-free). Only the countdown readout for now.
#[derive(Default)]
pub struct AscendShared {
    /// Bars remaining until the next boundary (f32 bits).
    bars_remaining: AtomicU32,
    /// 1 when the transport is playing, 0 when free-run/idle.
    playing: AtomicU32,
}

impl AscendShared {
    fn store_bars(&self, v: f32) {
        self.bars_remaining.store(v.to_bits(), Ordering::Relaxed);
    }
    fn load_bars(&self) -> f32 {
        f32::from_bits(self.bars_remaining.load(Ordering::Relaxed))
    }
    fn store_playing(&self, p: bool) {
        self.playing.store(p as u32, Ordering::Relaxed);
    }
    fn load_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed) != 0
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Ascend {
    params: Arc<AscendParams>,
    engine: AscendEngine,
    factory_presets: Arc<Vec<Preset>>,
    shared: Arc<AscendShared>,
    sample_rate: f32,
    /// Rising-edge detector for the momentary TRIGGER button.
    last_trigger: bool,
    /// Last note-on note number (for key-track root).
    last_note: Option<u8>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct AscendParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "key"] pub key: IntParam,
    #[id = "octave"] pub octave: IntParam,
    #[id = "sync"] pub sync: IntParam,
    #[id = "bars"] pub bars: IntParam,
    #[id = "curve"] pub curve: FloatParam,
    #[id = "balance"] pub balance: FloatParam,
    #[id = "color"] pub color: FloatParam,
    #[id = "wave"] pub wave: FloatParam,
    #[id = "fstart"] pub fstart: FloatParam,
    #[id = "fend"] pub fend: FloatParam,
    #[id = "rise"] pub rise: FloatParam,
    #[id = "width"] pub width: FloatParam,
    #[id = "impact"] pub impact: BoolParam,
    #[id = "implevel"] pub implevel: FloatParam,
    #[id = "autocut"] pub autocut: BoolParam,
    #[id = "downlifter"] pub downlifter: BoolParam,
    #[id = "freelen"] pub freelen: FloatParam,
    #[id = "level"] pub level: FloatParam,
    #[id = "keytrack"] pub keytrack: BoolParam,
    #[id = "trigger"] pub trigger: BoolParam,

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

fn hz(name: &'static str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) },
    )
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_rounded(0))
}

impl Default for AscendParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 640),
            key: IntParam::new("Key", d.key as i32, IntRange::Linear { min: 0, max: 11 })
                .with_value_to_string(Arc::new(|v| NOTE_NAMES[(v as usize) % 12].to_string()))
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim();
                    NOTE_NAMES
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(s))
                        .map(|i| i as i32)
                        .or_else(|| s.parse::<i32>().ok())
                })),
            octave: IntParam::new("Octave", d.octave, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(Arc::new(|v| format!("{v}")))
                .with_string_to_value(Arc::new(|s| s.trim().parse::<i32>().ok())),
            sync: IntParam::new("Sync Target", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        0 => "8 bars",
                        1 => "16 bars",
                        2 => "32 bars",
                        _ => "Custom",
                    }
                    .to_string()
                }))
                .with_string_to_value(Arc::new(|s| {
                    match s.trim().to_ascii_lowercase().as_str() {
                        "8 bars" | "8" | "0" => Some(0),
                        "16 bars" | "16" | "1" => Some(1),
                        "32 bars" | "32" | "2" => Some(2),
                        "custom" | "3" => Some(3),
                        _ => s.trim().parse::<i32>().ok(),
                    }
                })),
            bars: IntParam::new("Custom Bars", d.custom_bars as i32, IntRange::Linear { min: 1, max: 64 })
                .with_unit(" bar")
                .with_value_to_string(Arc::new(|v| format!("{v}")))
                .with_string_to_value(Arc::new(|s| {
                    // Accept the "N bar" display we render, or a bare integer.
                    let num: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
                    num.parse::<i32>().ok()
                })),
            curve: FloatParam::new("Curve", d.curve, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    let label = if v < 0.4 { "exp" } else if v > 0.6 { "log" } else { "lin" };
                    format!("{:.2} {label}", v)
                }))
                .with_string_to_value(Arc::new(|s| {
                    let num: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                    num.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
                })),
            balance: pct("Noise/Tone", d.balance),
            color: pct("Noise Color", d.color),
            wave: pct("Saw/Sine", d.wave),
            fstart: hz("Filter Start", d.filter_start_hz, 20.0, 18_000.0),
            fend: hz("Filter End", d.filter_end_hz, 20.0, 18_000.0),
            rise: FloatParam::new("Pitch Rise", d.rise_st, FloatRange::Linear { min: 0.0, max: 24.0 })
                .with_unit(" st")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            width: pct("Width Bloom", d.width),
            impact: BoolParam::new("Impact", d.impact_on),
            implevel: pct("Impact Level", d.impact_level),
            autocut: BoolParam::new("Auto-Cut", d.auto_cut),
            downlifter: BoolParam::new("Downlifter", d.downlifter),
            freelen: FloatParam::new(
                "Free Length",
                d.free_len_s,
                FloatRange::Skewed { min: 0.1, max: 30.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            level: FloatParam::new("Level", d.level_db, FloatRange::Linear { min: -24.0, max: 6.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            keytrack: BoolParam::new("Key Track", false),
            trigger: BoolParam::new("Trigger", false),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl AscendParams {
    /// Snapshot the current parameter values into a DSP [`Settings`].
    fn snapshot(&self) -> Settings {
        Settings {
            key: self.key.value() as usize,
            octave: self.octave.value(),
            sync: SyncTarget::from_index(self.sync.value() as usize),
            custom_bars: self.bars.value() as f32,
            curve: self.curve.value(),
            balance: self.balance.value(),
            color: self.color.value(),
            wave: self.wave.value(),
            filter_start_hz: self.fstart.value(),
            filter_end_hz: self.fend.value(),
            rise_st: self.rise.value(),
            width: self.width.value(),
            impact_on: self.impact.value(),
            impact_level: self.implevel.value(),
            auto_cut: self.autocut.value(),
            downlifter: self.downlifter.value(),
            free_len_s: self.freelen.value(),
            level_db: self.level.value(),
        }
    }
}

impl Default for Ascend {
    fn default() -> Self {
        Self {
            params: Arc::new(AscendParams::default()),
            engine: AscendEngine::new(48_000.0),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            shared: Arc::new(AscendShared::default()),
            sample_rate: 48_000.0,
            last_trigger: false,
            last_note: None,
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset through the host (so automation/undo see the change).
fn apply_preset(params: &AscendParams, setter: &ParamSetter, p: &Preset) {
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

    set_i(&params.key, g("key", d.key as f32) as i32);
    set_i(&params.octave, g("octave", d.octave as f32) as i32);
    set_i(&params.sync, g("sync", 0.0) as i32);
    set_i(&params.bars, g("bars", d.custom_bars) as i32);
    set_f(&params.curve, g("curve", d.curve));
    set_f(&params.balance, g("balance", d.balance));
    set_f(&params.color, g("color", d.color));
    set_f(&params.wave, g("wave", d.wave));
    set_f(&params.fstart, g("fstart", d.filter_start_hz));
    set_f(&params.fend, g("fend", d.filter_end_hz));
    set_f(&params.rise, g("rise", d.rise_st));
    set_f(&params.width, g("width", d.width));
    set_b(&params.impact, g("impact", 1.0) >= 0.5);
    set_f(&params.implevel, g("implevel", d.impact_level));
    set_b(&params.autocut, g("autocut", 1.0) >= 0.5);
    set_b(&params.downlifter, g("downlifter", 0.0) >= 0.5);
    set_f(&params.freelen, g("freelen", d.free_len_s));
    set_f(&params.level, g("level", d.level_db));
}

impl Plugin for Ascend {
    const NAME: &'static str = "Qeynos ASCEND";
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
        let shared = self.shared.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-ascend-window", Vec2::new(560.0, 640.0))
                    .min_size(Vec2::new(480.0, 520.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · ASCEND").color(suite_core::ui::ACCENT));
                        suite_core::ui::manual_button(ui, "ascend", "ASCEND", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new("tension generator")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(4.0);

                        // Countdown display (bars remaining).
                        let bars = shared.load_bars();
                        let playing = shared.load_playing();
                        let (txt, col) = if playing {
                            (format!("COUNTDOWN  {:.2} bars", bars), suite_core::ui::ACCENT)
                        } else {
                            ("TRANSPORT STOPPED — TRIGGER for free-run".to_string(), suite_core::ui::TEXT_DIM)
                        };
                        ui.label(egui::RichText::new(txt).color(col).monospace());
                        ui.add_space(4.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("ascend", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("level", "LEVEL"), ("width", "WIDTH"), ("rise", "RISE"), ("balance", "BALANCE")],
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new("TARGET").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("ascend-target").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "KEY", &params.key, setter);
                                row(ui, "OCTAVE", &params.octave, setter);
                                ui.end_row();
                                row(ui, "SYNC", &params.sync, setter);
                                row(ui, "BARS", &params.bars, setter);
                                ui.end_row();
                                row(ui, "CURVE", &params.curve, setter);
                                ui.end_row();
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("SOURCES").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("ascend-src").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "NOISE/TONE", &params.balance, setter);
                                row(ui, "COLOR", &params.color, setter);
                                ui.end_row();
                                row(ui, "SAW/SINE", &params.wave, setter);
                                ui.end_row();
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("TENSION SWEEP").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("ascend-sweep").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "FILT START", &params.fstart, setter);
                                row(ui, "FILT END", &params.fend, setter);
                                ui.end_row();
                                row(ui, "PITCH RISE", &params.rise, setter);
                                row(ui, "WIDTH", &params.width, setter);
                                ui.end_row();
                            });
                            ui.separator();

                            ui.label(egui::RichText::new("IMPACT / MODE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("ascend-impact").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "IMPACT", &params.impact, setter);
                                row(ui, "IMP LVL", &params.implevel, setter);
                                ui.end_row();
                                row(ui, "AUTO-CUT", &params.autocut, setter);
                                row(ui, "DOWNLIFT", &params.downlifter, setter);
                                ui.end_row();
                                row(ui, "FREE LEN", &params.freelen, setter);
                                row(ui, "LEVEL", &params.level, setter);
                                ui.end_row();
                                row(ui, "KEYTRACK", &params.keytrack, setter);
                                row(ui, "TRIGGER", &params.trigger, setter);
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
        self.sample_rate = buffer_config.sample_rate;
        self.engine = AscendEngine::new(buffer_config.sample_rate);
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "ASCEND");
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.last_trigger = false;
        self.last_note = None;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Denormal mitigation for the whole process scope (FTZ/DAZ), restored on drop.
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // --- Transport → bar position + advance rate ---
        let transport = context.transport();
        let sr = self.sample_rate.max(1.0) as f64;
        let tempo = transport.tempo.unwrap_or(120.0).max(1.0);
        let tsn = transport.time_sig_numerator.unwrap_or(4).max(1) as f64;
        let tsd = transport.time_sig_denominator.unwrap_or(4).max(1) as f64;
        let beats_per_bar = (tsn * 4.0 / tsd).max(1.0e-3);
        let bars_per_sample = (tempo / 60.0 / sr) / beats_per_bar;
        let bar_pos = transport.pos_beats().unwrap_or(0.0) / beats_per_bar;
        let playing = transport.playing;

        // --- Key-track root override (from the last held note) ---
        let keytrack = self.params.keytrack.value();
        let root_override = if keytrack {
            self.last_note.map(util::midi_note_to_freq)
        } else {
            None
        };
        self.engine.set_root_override(root_override);

        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.level_db = routes.modulated_float("level", &self.params.level, bus);
                s.width = routes.modulated_float("width", &self.params.width, bus);
                s.rise_st = routes.modulated_float("rise", &self.params.rise, bus);
                s.balance = routes.modulated_float("balance", &self.params.balance, bus);
            }
        }
        self.engine.configure(&s);
        self.engine.set_transport(TransportFrame { playing, bar_pos, bars_per_sample });

        // --- Momentary TRIGGER button: rising edge → free-run one-shot ---
        let trig = self.params.trigger.value();
        if trig && !self.last_trigger {
            self.engine.trigger_free();
        }
        self.last_trigger = trig;

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
                if let NoteEvent::NoteOn { note, .. } = event {
                    self.last_note = Some(note);
                    if keytrack {
                        self.engine.set_root_override(Some(util::midi_note_to_freq(note)));
                    }
                    // A note also triggers the envelope when the transport is stopped.
                    self.engine.trigger_free();
                }
                next_event = context.next_event();
            }

            let (l, r) = self.engine.process_sample();
            if num_ch >= 2 {
                out[0][n] = l;
                out[1][n] = r;
            } else {
                out[0][n] = 0.5 * (l + r);
            }
        }

        // Publish the countdown for the GUI.
        self.shared.store_bars(self.engine.bars_remaining());
        self.shared.store_playing(playing);

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

impl Drop for Ascend {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for Ascend {
    const CLAP_ID: &'static str = "com.qeynos.ascend";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Tension generator — transport-synced countdown drives one tension envelope (filter sweep, pitch rise, width bloom, volume swell) into an embedded impact + auto-cut; downlifter + free-run modes");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for Ascend {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosASCENDrsr1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(Ascend);
nih_export_vst3!(Ascend);

#[cfg(test)]
mod render_tests;

#[cfg(test)]
mod manual_tests {
    #[test]
    fn manual_covers_all_params_and_has_recipes() {
        suite_core::manual::assert_manual_covers_params(
            crate::MANUAL_DOC,
            &crate::AscendParams::default(),
        );
    }
}
