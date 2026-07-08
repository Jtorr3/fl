//! HALT — performance buffer FX (Qeynos suite, Phase 3).
//!
//! A 4-bar circular buffer records continuously; four momentary modes replay it live:
//! **tape-stop** (rate 1→0 over a synced/free duration with a curve), **stutter** (loop the
//! last 1/4..1/64 with per-repeat decay + pitch step, retrigger-quantized), **reverse** (read
//! backward from the trigger point), and **half-speed** (rate 0.5). Each mode is a host-
//! automatable button AND MIDI-note triggerable (C1..D#1). Multiple held → last-pressed wins.
//! Every engage / disengage / mode-change / loop-wrap is a 5 ms equal-power crossfade. Inactive
//! (or `mix = 0`) is a bit-exact passthrough. See [`dsp`] for the core (shared with the tests).

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

use dsp::{HaltCore, QuantDiv, Settings, StutterDiv, TapeRelease, TapeSync, NUM_MODES};
use suite_core::presets::{load_all, Preset};

/// Lowest MIDI note that triggers a mode (C1 in FL's C5=60 convention). Notes base..base+3
/// map to tape-stop / stutter / reverse / half-speed (within the C1..E1 region SPECS calls out).
const MODE_BASE_NOTE: u8 = 36;

// ---------------------------------------------------------------------------
// Param-facing enums (mapped onto the pure-DSP enums)
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum StutterDivParam {
    #[id = "d4"]
    #[name = "1/4"]
    Quarter,
    #[id = "d8"]
    #[name = "1/8"]
    Eighth,
    #[id = "d16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "d32"]
    #[name = "1/32"]
    ThirtySecond,
    #[id = "d64"]
    #[name = "1/64"]
    SixtyFourth,
}

impl StutterDivParam {
    fn to_dsp(self) -> StutterDiv {
        match self {
            StutterDivParam::Quarter => StutterDiv::Quarter,
            StutterDivParam::Eighth => StutterDiv::Eighth,
            StutterDivParam::Sixteenth => StutterDiv::Sixteenth,
            StutterDivParam::ThirtySecond => StutterDiv::ThirtySecond,
            StutterDivParam::SixtyFourth => StutterDiv::SixtyFourth,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum TapeSyncParam {
    #[id = "free"]
    #[name = "Free"]
    Free,
    #[id = "beat"]
    #[name = "1 Beat"]
    Beat,
    #[id = "half"]
    #[name = "1/2 Bar"]
    Half,
    #[id = "bar"]
    #[name = "1 Bar"]
    Bar,
    #[id = "twobar"]
    #[name = "2 Bar"]
    TwoBar,
}

impl TapeSyncParam {
    fn to_dsp(self) -> TapeSync {
        match self {
            TapeSyncParam::Free => TapeSync::Free,
            TapeSyncParam::Beat => TapeSync::Beat,
            TapeSyncParam::Half => TapeSync::Half,
            TapeSyncParam::Bar => TapeSync::Bar,
            TapeSyncParam::TwoBar => TapeSync::TwoBar,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum QuantDivParam {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "q16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "q8"]
    #[name = "1/8"]
    Eighth,
    #[id = "q4"]
    #[name = "1/4"]
    Quarter,
}

impl QuantDivParam {
    fn to_dsp(self) -> QuantDiv {
        match self {
            QuantDivParam::Off => QuantDiv::Off,
            QuantDivParam::Sixteenth => QuantDiv::Sixteenth,
            QuantDivParam::Eighth => QuantDiv::Eighth,
            QuantDivParam::Quarter => QuantDiv::Quarter,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum TapeReleaseParam {
    #[id = "ramp"]
    #[name = "Ramp"]
    Ramp,
    #[id = "instant"]
    #[name = "Instant"]
    Instant,
}

impl TapeReleaseParam {
    fn to_dsp(self) -> TapeRelease {
        match self {
            TapeReleaseParam::Ramp => TapeRelease::Ramp,
            TapeReleaseParam::Instant => TapeRelease::Instant,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct Halt {
    params: Arc<HaltParams>,
    core: HaltCore,
    /// Held MIDI notes (block-rate), so note-triggered modes behave like the buttons.
    notes: [bool; 128],
    factory_presets: Arc<Vec<Preset>>,
}

#[derive(Params)]
pub struct HaltParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // --- momentary mode buttons (host-automatable bools; also MIDI-triggerable) ---
    #[id = "tapestop"]
    pub tape_stop: BoolParam,
    #[id = "stutter"]
    pub stutter: BoolParam,
    #[id = "reverse"]
    pub reverse: BoolParam,
    #[id = "halfspeed"]
    pub half_speed: BoolParam,

    // --- stutter ---
    #[id = "stutdiv"]
    pub stutter_div: EnumParam<StutterDivParam>,
    #[id = "decay"]
    pub stutter_decay: FloatParam,
    #[id = "pitchstep"]
    pub stutter_pitch: IntParam,

    // --- tape-stop ---
    #[id = "tapesync"]
    pub tape_sync: EnumParam<TapeSyncParam>,
    #[id = "tapefree"]
    pub tape_free: FloatParam,
    #[id = "tapecurve"]
    pub tape_curve: FloatParam,
    #[id = "taperel"]
    pub tape_release: EnumParam<TapeReleaseParam>,

    // --- global ---
    #[id = "quant"]
    pub quantize: EnumParam<QuantDivParam>,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for HaltParams {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            editor_state: EguiState::from_size(560, 520),

            tape_stop: BoolParam::new("Tape Stop", false),
            stutter: BoolParam::new("Stutter", false),
            reverse: BoolParam::new("Reverse", false),
            half_speed: BoolParam::new("Half Speed", false),

            stutter_div: EnumParam::new("Stutter Div", StutterDivParam::Eighth),
            stutter_decay: FloatParam::new(
                "Stutter Decay",
                d.stutter_decay,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            stutter_pitch: IntParam::new(
                "Pitch Step",
                d.stutter_pitch,
                IntRange::Linear { min: -12, max: 12 },
            )
            .with_unit(" st"),

            tape_sync: EnumParam::new("Stop Time", TapeSyncParam::Bar),
            tape_free: FloatParam::new(
                "Stop Free",
                d.tape_free_s,
                FloatRange::Skewed {
                    min: 0.05,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            tape_curve: FloatParam::new(
                "Stop Curve",
                d.tape_curve,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            tape_release: EnumParam::new("Release", TapeReleaseParam::Instant),

            quantize: EnumParam::new("Quantize", QuantDivParam::Off),
            mix: FloatParam::new("Mix", d.mix, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(5.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            out: FloatParam::new("Out", d.out_db, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_smoother(SmoothingStyle::Linear(5.0))
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),

            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl HaltParams {
    fn snapshot(&self) -> Settings {
        Settings {
            stutter_div: self.stutter_div.value().to_dsp(),
            stutter_decay: self.stutter_decay.value(),
            stutter_pitch: self.stutter_pitch.value(),
            tape_sync: self.tape_sync.value().to_dsp(),
            tape_free_s: self.tape_free.value(),
            tape_curve: self.tape_curve.value(),
            tape_release: self.tape_release.value().to_dsp(),
            quantize: self.quantize.value().to_dsp(),
            mix: self.mix.value(),
            out_db: self.out.value(),
        }
    }
}

impl Default for Halt {
    fn default() -> Self {
        Self {
            params: Arc::new(HaltParams::default()),
            core: HaltCore::new(48_000.0),
            notes: [false; 128],
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
        }
    }
}

/// Apply a factory preset through the host (automation/undo see every scalar). The mode buttons
/// are never touched by a preset — they are live performance state.
fn apply_preset(params: &HaltParams, setter: &ParamSetter, p: &Preset) {
    let s = presets::settings_from_preset(p);
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
    setter.begin_set_parameter(&params.stutter_div);
    setter.set_parameter(
        &params.stutter_div,
        match s.stutter_div {
            StutterDiv::Quarter => StutterDivParam::Quarter,
            StutterDiv::Eighth => StutterDivParam::Eighth,
            StutterDiv::Sixteenth => StutterDivParam::Sixteenth,
            StutterDiv::ThirtySecond => StutterDivParam::ThirtySecond,
            StutterDiv::SixtyFourth => StutterDivParam::SixtyFourth,
        },
    );
    setter.end_set_parameter(&params.stutter_div);
    setter.begin_set_parameter(&params.tape_sync);
    setter.set_parameter(
        &params.tape_sync,
        match s.tape_sync {
            TapeSync::Free => TapeSyncParam::Free,
            TapeSync::Beat => TapeSyncParam::Beat,
            TapeSync::Half => TapeSyncParam::Half,
            TapeSync::Bar => TapeSyncParam::Bar,
            TapeSync::TwoBar => TapeSyncParam::TwoBar,
        },
    );
    setter.end_set_parameter(&params.tape_sync);
    setter.begin_set_parameter(&params.tape_release);
    setter.set_parameter(
        &params.tape_release,
        match s.tape_release {
            TapeRelease::Ramp => TapeReleaseParam::Ramp,
            TapeRelease::Instant => TapeReleaseParam::Instant,
        },
    );
    setter.end_set_parameter(&params.tape_release);
    setter.begin_set_parameter(&params.quantize);
    setter.set_parameter(
        &params.quantize,
        match s.quantize {
            QuantDiv::Off => QuantDivParam::Off,
            QuantDiv::Sixteenth => QuantDivParam::Sixteenth,
            QuantDiv::Eighth => QuantDivParam::Eighth,
            QuantDiv::Quarter => QuantDivParam::Quarter,
        },
    );
    setter.end_set_parameter(&params.quantize);

    set_f(&params.stutter_decay, s.stutter_decay);
    set_i(&params.stutter_pitch, s.stutter_pitch);
    set_f(&params.tape_free, s.tape_free_s);
    set_f(&params.tape_curve, s.tape_curve);
    set_f(&params.mix, s.mix);
    set_f(&params.out, s.out_db);
}

impl Plugin for Halt {
    const NAME: &'static str = "Qeynos HALT";
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

    // MIDI notes trigger the four modes (C1..D#1).
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
                suite_core::ui::ScaledWindow::new("qeynos-halt-window", Vec2::new(560.0, 520.0))
                    .min_size(Vec2::new(460.0, 420.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · HALT").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(
                                "performance buffer — tape-stop / stutter / reverse / half-speed",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("halt", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("mix", "MIX"), ("out", "OUT"), ("decay", "DECAY")],
                        );
                        ui.separator();

                        // Big mode buttons (toggle the momentary bools; last-pressed wins).
                        ui.horizontal(|ui| {
                            mode_button(ui, setter, &params.tape_stop, "TAPE STOP");
                            mode_button(ui, setter, &params.stutter, "STUTTER");
                            mode_button(ui, setter, &params.reverse, "REVERSE");
                            mode_button(ui, setter, &params.half_speed, "HALF SPEED");
                        });
                        ui.add_space(8.0);

                        egui::Grid::new("halt-controls")
                            .num_columns(4)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "STUTTER DIV", &params.stutter_div, setter);
                                row(ui, "DECAY", &params.stutter_decay, setter);
                                row(ui, "PITCH STEP", &params.stutter_pitch, setter);
                                row(ui, "QUANTIZE", &params.quantize, setter);
                                ui.end_row();
                                row(ui, "STOP TIME", &params.tape_sync, setter);
                                row(ui, "STOP FREE", &params.tape_free, setter);
                                row(ui, "STOP CURVE", &params.tape_curve, setter);
                                row(ui, "RELEASE", &params.tape_release, setter);
                                ui.end_row();
                                row(ui, "MIX", &params.mix, setter);
                                row(ui, "OUT", &params.out, setter);
                                ui.end_row();
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
        self.core = HaltCore::new(buffer_config.sample_rate);
        // Zero latency — the dry path is never delayed.
        context.set_latency_samples(self.core.latency_samples());
        true
    }

    fn reset(&mut self) {
        self.core.reset();
        self.notes = [false; 128];
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // --- settings (+ NERVE listen layer for mix/out/decay) ---
        let mut s = self.params.snapshot();
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
                s.out_db = routes.modulated_float("out", &self.params.out, bus);
                s.stutter_decay = routes.modulated_float("decay", &self.params.stutter_decay, bus);
            }
        }
        self.core.configure(&s);

        // --- transport → shared TransportFrame ---
        let t = context.transport();
        let sr = self.core.sample_rate() as f64;
        let tempo = t.tempo.unwrap_or(120.0).max(1.0);
        let tsn = t.time_sig_numerator.unwrap_or(4).max(1) as f64;
        let tsd = t.time_sig_denominator.unwrap_or(4).max(1) as f64;
        let beats_per_bar = (tsn * 4.0 / tsd).max(1.0e-3);
        let bars_per_sample = (tempo / 60.0 / sr) / beats_per_bar;
        let ppq = t.pos_beats().unwrap_or(0.0);
        self.core.set_transport(&suite_core::testsig::TransportFrame {
            playing: t.playing,
            tempo,
            ppq_pos: ppq,
            bar_pos: ppq / beats_per_bar,
            bars_per_sample,
            beats_per_bar,
        });

        // --- MIDI: note-on/off → held bitmap (block-rate; modes are crossfaded) ---
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, .. } => self.notes[note as usize] = true,
                NoteEvent::NoteOff { note, .. } => self.notes[note as usize] = false,
                NoteEvent::Choke { note, .. } => self.notes[note as usize] = false,
                _ => {}
            }
        }

        // --- combine buttons + MIDI into the four held modes (last-pressed wins) ---
        let buttons = [
            self.params.tape_stop.value(),
            self.params.stutter.value(),
            self.params.reverse.value(),
            self.params.half_speed.value(),
        ];
        let mut held = [false; NUM_MODES];
        for i in 0..NUM_MODES {
            let note = MODE_BASE_NOTE as usize + i;
            held[i] = buttons[i] || (note < 128 && self.notes[note]);
        }
        self.core.set_held(&held);

        // --- per-sample process ---
        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_ch = main.len();
        if num_ch == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l_in = main[0][n];
            let r_in = if num_ch > 1 { main[1][n] } else { l_in };

            let (wl, wr) = self.core.process_sample(l_in, r_in);
            // Advance the smoothers every sample so automation stays sample-accurate.
            let mix = self.params.mix.smoothed.next();
            let out_gain = util::db_to_gain(self.params.out.smoothed.next());

            if self.core.is_idle() {
                // Bit-exact passthrough (no mode active, no crossfade in flight).
                main[0][n] = l_in;
                if num_ch > 1 {
                    main[1][n] = r_in;
                }
            } else {
                main[0][n] = ((1.0 - mix) * l_in + mix * wl) * out_gain;
                if num_ch > 1 {
                    main[1][n] = ((1.0 - mix) * r_in + mix * wr) * out_gain;
                }
            }
        }

        ProcessStatus::Normal
    }
}

/// A big momentary mode button: a highlighted toggle wired to a `BoolParam` through the setter.
fn mode_button(ui: &mut egui::Ui, setter: &ParamSetter, param: &BoolParam, label: &str) {
    let on = param.value();
    let text = egui::RichText::new(label).strong().color(if on {
        suite_core::ui::BG
    } else {
        suite_core::ui::TEXT
    });
    let mut btn = egui::Button::new(text).min_size(Vec2::new(118.0, 40.0));
    if on {
        btn = btn.fill(suite_core::ui::ACCENT);
    }
    if ui.add(btn).clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, !on);
        setter.end_set_parameter(param);
    }
}

impl ClapPlugin for Halt {
    const CLAP_ID: &'static str = "com.qeynos.halt";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Performance buffer FX — 4-bar circular buffer with momentary tape-stop, stutter, \
         reverse, and half-speed; transport-synced, MIDI-triggerable, 5 ms equal-power \
         crossfades, bit-exact passthrough when idle",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Glitch,
        ClapFeature::Custom("buffer-fx"),
    ];
}

impl Vst3Plugin for Halt {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosHALTbuffr1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Custom("Glitch")];
}

nih_export_clap!(Halt);
nih_export_vst3!(Halt);

#[cfg(test)]
mod tests;
