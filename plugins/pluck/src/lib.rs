//! PLUCK — Karplus-Strong strummer (Qeynos suite, Phase 2b; Strum clone).
//!
//! An **effect with MIDI input**: the audio input is the exciter (an onset fires a
//! staggered STRUM across six Karplus-Strong strings, the input's timbre coloring each
//! pluck; a continuous-drive mode feeds the strings constantly), while held MIDI notes,
//! a chord table, or a chromagram key-detect tune the strings. A small embedded modal
//! **body IR** colors the wet path. Zero reported latency; `mix = 0` nulls against dry.
//!
//! See [`dsp`] for the DSP core, shared verbatim with the offline harness / done-bars.

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

use dsp::{
    Chord, PluckCore, Settings, StrumDir, TuningSource, MAX_STRINGS,
};
use suite_core::presets::{load_all, Preset};

/// Usage manual embedded from docs, rendered in-GUI by the '?' button (BUILT-IN-MANUALS).
pub const MANUAL_DOC: &str = include_str!("../../../docs/PLUCK.md");

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SourceParam {
    #[id = "chord"]
    #[name = "Chord"]
    Chord,
    #[id = "midi"]
    #[name = "MIDI Held"]
    Midi,
    #[id = "key"]
    #[name = "Key Detect"]
    KeyDetect,
}
impl SourceParam {
    fn to_dsp(self) -> TuningSource {
        match self {
            SourceParam::Chord => TuningSource::Chord,
            SourceParam::Midi => TuningSource::Midi,
            SourceParam::KeyDetect => TuningSource::KeyDetect,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChordParam {
    #[id = "min"]
    #[name = "Minor"]
    Minor,
    #[id = "min7"]
    #[name = "Minor 7"]
    Minor7,
    #[id = "sus2"]
    #[name = "Sus2"]
    Sus2,
    #[id = "min9"]
    #[name = "Minor 9"]
    Minor9,
    #[id = "pow5"]
    #[name = "5th Stack"]
    Power5,
    #[id = "sus4"]
    #[name = "Sus4"]
    Sus4,
}
impl ChordParam {
    fn to_dsp(self) -> Chord {
        match self {
            ChordParam::Minor => Chord::Minor,
            ChordParam::Minor7 => Chord::Minor7,
            ChordParam::Sus2 => Chord::Sus2,
            ChordParam::Minor9 => Chord::Minor9,
            ChordParam::Power5 => Chord::Power5,
            ChordParam::Sus4 => Chord::Sus4,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DirParam {
    #[id = "up"]
    #[name = "Up"]
    Up,
    #[id = "down"]
    #[name = "Down"]
    Down,
    #[id = "alt"]
    #[name = "Alternate"]
    Alternate,
}
impl DirParam {
    fn to_dsp(self) -> StrumDir {
        match self {
            DirParam::Up => StrumDir::Up,
            DirParam::Down => StrumDir::Down,
            DirParam::Alternate => StrumDir::Alternate,
        }
    }
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Params)]
pub struct PluckParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "source"]
    pub source: EnumParam<SourceParam>,
    #[id = "root"]
    pub root: IntParam,
    #[id = "chord"]
    pub chord: EnumParam<ChordParam>,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "damp"]
    pub damp: FloatParam,
    #[id = "strum"]
    pub strum: FloatParam,
    #[id = "dir"]
    pub dir: EnumParam<DirParam>,
    #[id = "exgain"]
    pub exgain: FloatParam,
    #[id = "cont"]
    pub cont: BoolParam,
    #[id = "velbright"]
    pub velbright: FloatParam,
    #[id = "body"]
    pub body: FloatParam,
    #[id = "spread"]
    pub spread: FloatParam,
    #[id = "stereoalt"]
    pub stereoalt: FloatParam,
    #[id = "wetsolo"]
    pub wetsolo: BoolParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "out"]
    pub out: FloatParam,

    /// NERVE listen layer: persisted per-param modulation routes (edited in the MOD section).
    #[persist = "mod"]
    pub mod_routes: Arc<RwLock<ModRoutes>>,
}

impl Default for PluckParams {
    fn default() -> Self {
        let pct = || {
            (
                formatters::v2s_f32_percentage(0),
                formatters::s2v_f32_percentage(),
            )
        };
        let (p_v2s, p_s2v) = pct();
        let (d_v2s, d_s2v) = pct();
        let (vb_v2s, vb_s2v) = pct();
        let (b_v2s, b_s2v) = pct();
        let (sa_v2s, sa_s2v) = pct();
        let (m_v2s, m_s2v) = pct();
        Self {
            editor_state: EguiState::from_size(560, 560),
            source: EnumParam::new("Source", SourceParam::Chord),
            root: IntParam::new("Root", 0, IntRange::Linear { min: 0, max: 11 })
                .with_value_to_string(Arc::new(|v| {
                    NOTE_NAMES[(v.rem_euclid(12)) as usize].to_string()
                }))
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim();
                    NOTE_NAMES
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(s))
                        .map(|i| i as i32)
                })),
            chord: EnumParam::new("Chord", ChordParam::Minor),
            decay: FloatParam::new("Decay", 0.6, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(p_v2s)
                .with_string_to_value(p_s2v),
            damp: FloatParam::new("Damp", 0.4, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(d_v2s)
                .with_string_to_value(d_s2v),
            strum: FloatParam::new(
                "Strum Time",
                25.0,
                FloatRange::Linear { min: 5.0, max: 80.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            dir: EnumParam::new("Direction", DirParam::Up),
            exgain: FloatParam::new(
                "Exciter Gain",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            cont: BoolParam::new("Continuous", false),
            velbright: FloatParam::new(
                "Vel→Bright",
                0.4,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(vb_v2s)
            .with_string_to_value(vb_s2v),
            body: FloatParam::new("Body", 0.4, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(b_v2s)
                .with_string_to_value(b_s2v),
            spread: FloatParam::new(
                "Spread",
                6.0,
                FloatRange::Linear { min: 0.0, max: 50.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" ct")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            stereoalt: FloatParam::new(
                "Stereo Alt",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(sa_v2s)
            .with_string_to_value(sa_s2v),
            wetsolo: BoolParam::new("Wet Solo", false),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(m_v2s)
                .with_string_to_value(m_s2v),
            out: FloatParam::new(
                "Out",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl PluckParams {
    /// Build a DSP [`Settings`] from the params + the currently-held MIDI notes.
    fn snapshot(&self, held: &[bool; 128]) -> Settings {
        let mut held_hz = [f32::NAN; MAX_STRINGS];
        let mut count = 0usize;
        for (note, &down) in held.iter().enumerate() {
            if down {
                if count < MAX_STRINGS {
                    held_hz[count] = dsp::midi_to_freq(note as f32);
                }
                count += 1;
            }
        }
        Settings {
            source: self.source.value().to_dsp(),
            root_pc: self.root.value(),
            chord: self.chord.value().to_dsp(),
            decay: self.decay.value(),
            damp: self.damp.value(),
            strum_ms: self.strum.value(),
            dir: self.dir.value().to_dsp(),
            exciter_gain: self.exgain.value(),
            continuous: self.cont.value(),
            vel_bright: self.velbright.value(),
            body: self.body.value(),
            spread_cents: self.spread.value(),
            stereo_alt: self.stereoalt.value(),
            wet_solo: self.wetsolo.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
            held: held_hz,
            held_count: count.min(MAX_STRINGS),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct Pluck {
    params: Arc<PluckParams>,
    core: PluckCore,
    held: [bool; 128],
    factory_presets: Arc<Vec<Preset>>,
    // Per-string activity envelope published to the GUI.
    activity: Arc<Vec<AtomicF32>>,
    spectrum: SpectrumPublisher,
}

impl Default for Pluck {
    fn default() -> Self {
        let mut activity = Vec::with_capacity(MAX_STRINGS);
        for _ in 0..MAX_STRINGS {
            activity.push(AtomicF32::new(0.0));
        }
        Self {
            params: Arc::new(PluckParams::default()),
            core: PluckCore::new(48_000.0),
            held: [false; 128],
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            activity: Arc::new(activity),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

fn apply_preset(params: &PluckParams, setter: &ParamSetter, p: &Preset) {
    let set_f = |fp: &FloatParam, v: f32| {
        setter.begin_set_parameter(fp);
        setter.set_parameter(fp, v);
        setter.end_set_parameter(fp);
    };
    if let Some(v) = p.get("source") {
        let e = SourceParam::from_index(v as usize);
        setter.begin_set_parameter(&params.source);
        setter.set_parameter(&params.source, e);
        setter.end_set_parameter(&params.source);
    }
    if let Some(v) = p.get("root") {
        setter.begin_set_parameter(&params.root);
        setter.set_parameter(&params.root, v as i32);
        setter.end_set_parameter(&params.root);
    }
    if let Some(v) = p.get("chord") {
        let e = ChordParam::from_index(v as usize);
        setter.begin_set_parameter(&params.chord);
        setter.set_parameter(&params.chord, e);
        setter.end_set_parameter(&params.chord);
    }
    if let Some(v) = p.get("dir") {
        let e = DirParam::from_index(v as usize);
        setter.begin_set_parameter(&params.dir);
        setter.set_parameter(&params.dir, e);
        setter.end_set_parameter(&params.dir);
    }
    if let Some(v) = p.get("cont") {
        setter.begin_set_parameter(&params.cont);
        setter.set_parameter(&params.cont, v > 0.5);
        setter.end_set_parameter(&params.cont);
    }
    if let Some(v) = p.get("wetsolo") {
        setter.begin_set_parameter(&params.wetsolo);
        setter.set_parameter(&params.wetsolo, v > 0.5);
        setter.end_set_parameter(&params.wetsolo);
    }
    if let Some(v) = p.get("decay") {
        set_f(&params.decay, v);
    }
    if let Some(v) = p.get("damp") {
        set_f(&params.damp, v);
    }
    if let Some(v) = p.get("strum") {
        set_f(&params.strum, v);
    }
    if let Some(v) = p.get("exgain") {
        set_f(&params.exgain, v);
    }
    if let Some(v) = p.get("velbright") {
        set_f(&params.velbright, v);
    }
    if let Some(v) = p.get("body") {
        set_f(&params.body, v);
    }
    if let Some(v) = p.get("spread") {
        set_f(&params.spread, v);
    }
    if let Some(v) = p.get("stereoalt") {
        set_f(&params.stereoalt, v);
    }
    if let Some(v) = p.get("mix") {
        set_f(&params.mix, v);
    }
    if let Some(v) = p.get("out") {
        set_f(&params.out, v);
    }
}

impl SourceParam {
    fn from_index(i: usize) -> SourceParam {
        match i {
            1 => SourceParam::Midi,
            2 => SourceParam::KeyDetect,
            _ => SourceParam::Chord,
        }
    }
}
impl ChordParam {
    fn from_index(i: usize) -> ChordParam {
        match i {
            1 => ChordParam::Minor7,
            2 => ChordParam::Sus2,
            3 => ChordParam::Minor9,
            4 => ChordParam::Power5,
            5 => ChordParam::Sus4,
            _ => ChordParam::Minor,
        }
    }
}
impl DirParam {
    fn from_index(i: usize) -> DirParam {
        match i {
            1 => DirParam::Down,
            2 => DirParam::Alternate,
            _ => DirParam::Up,
        }
    }
}

impl Plugin for Pluck {
    const NAME: &'static str = "Qeynos PLUCK";
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

    // Effect WITH MIDI input — held notes tune the strings.
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
        let activity = self.activity.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-pluck-window", Vec2::new(560.0, 560.0))
                    .min_size(Vec2::new(460.0, 460.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · PLUCK").color(suite_core::ui::ACCENT),
                        );
                        suite_core::ui::manual_button(ui, "pluck", "PLUCK", MANUAL_DOC);
                        ui.label(
                            egui::RichText::new(
                                "Karplus-Strong strummer — audio excites six tuned strings",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("pluck", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[("decay", "DECAY"), ("damp", "DAMP"), ("mix", "MIX"), ("out", "OUT")],
                        );
                        ui.separator();

                        // String-activity display: six bars, housed in the CONSOLE v2 CRT bay
                        // (glass + scanlines when console is on; plain panel in THEME-OFF).
                        // The bars themselves keep functioning identically inside the glass.
                        suite_core::ui::crt_frame(ui, "pluck-crt", 58.0, |ui| {
                            string_activity(ui, &activity);
                        });
                        ui.add_space(6.0);

                        egui::Grid::new("pluck-tuning")
                            .num_columns(4)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "SOURCE", &params.source, setter);
                                row(ui, "ROOT", &params.root, setter);
                                row(ui, "CHORD", &params.chord, setter);
                                row(ui, "SPREAD", &params.spread, setter);
                                ui.end_row();
                                row(ui, "DECAY", &params.decay, setter);
                                row(ui, "DAMP", &params.damp, setter);
                                row(ui, "STRUM", &params.strum, setter);
                                row(ui, "DIR", &params.dir, setter);
                                ui.end_row();
                                row(ui, "EX GAIN", &params.exgain, setter);
                                row(ui, "CONT", &params.cont, setter);
                                row(ui, "VEL→BRT", &params.velbright, setter);
                                row(ui, "BODY", &params.body, setter);
                                ui.end_row();
                                row(ui, "ST ALT", &params.stereoalt, setter);
                                row(ui, "WET SOLO", &params.wetsolo, setter);
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
        self.core.set_sample_rate(buffer_config.sample_rate);
        // Zero processing latency (the strings/body are causal; wet-only latency, dry is direct).
        context.set_latency_samples(0);
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "PLUCK");
        true
    }

    fn reset(&mut self) {
        self.core.reset();
        self.held = [false; 128];
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // Drain MIDI events → update the held-note set (tuning is block-rate).
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, .. } => self.held[note as usize] = true,
                NoteEvent::NoteOff { note, .. } => self.held[note as usize] = false,
                NoteEvent::Choke { note, .. } => self.held[note as usize] = false,
                _ => {}
            }
        }

        let mut s = self.params.snapshot(&self.held);
        if let Ok(routes) = self.params.mod_routes.try_read() {
            if !routes.routes.is_empty() {
                let bus = suite_core::bus::bus();
                s.decay = routes.modulated_float("decay", &self.params.decay, bus);
                s.damp = routes.modulated_float("damp", &self.params.damp, bus);
                s.mix = routes.modulated_float("mix", &self.params.mix, bus);
                s.out_db = routes.modulated_float("out", &self.params.out, bus);
            }
        }
        self.core.configure(&s);

        let num_samples = buffer.samples();
        let main = buffer.as_slice();
        let num_ch = main.len();
        if num_ch == 0 {
            return ProcessStatus::Normal;
        }

        for n in 0..num_samples {
            let l_in = main[0][n];
            let r_in = if num_ch > 1 { main[1][n] } else { l_in };
            let (ol, or) = self.core.process_sample(l_in, r_in, &s);
            main[0][n] = ol;
            if num_ch > 1 {
                main[1][n] = or;
            }
        }

        if self.params.editor_state.is_open() {
            let env = self.core.string_env();
            for i in 0..MAX_STRINGS {
                self.activity[i].store(env[i], Ordering::Relaxed);
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

        // KeepAlive: sympathetic/continuous modes and ringing tails outlast the input.
        ProcessStatus::KeepAlive
    }
}

impl Drop for Pluck {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

/// Six vertical activity bars showing per-string energy (cheap; optional GUI extra).
fn string_activity(ui: &mut egui::Ui, activity: &[AtomicF32]) {
    // Honor the CRT-motion pref + ~8 fps idle guarantee (guardrails #2/#6).
    suite_core::ui::scope_repaint(ui.ctx());
    // CONSOLE re-skin: on the CRT glass the opaque panel backing is dropped (glass shows
    // through) and the energy bars glow phosphor amber; THEME-OFF keeps the panel + accent.
    let console = suite_core::ui::console_on(ui.ctx());
    let n = activity.len();
    let avail = ui.available_width().min(360.0);
    let size = Vec2::new(avail, 42.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    if !console {
        painter.rect_filled(rect, 3.0, suite_core::ui::PANEL);
    }
    let bar_col = if console { suite_core::ui::PHOSPHOR } else { suite_core::ui::ACCENT };
    let gap = 4.0;
    let bw = (rect.width() - gap * (n as f32 + 1.0)) / n as f32;
    for i in 0..n {
        let e = activity[i].load(Ordering::Relaxed).clamp(0.0, 1.0);
        let h = e.sqrt() * (rect.height() - 6.0);
        let x0 = rect.left() + gap + i as f32 * (bw + gap);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, rect.bottom() - 3.0 - h),
            egui::pos2(x0 + bw, rect.bottom() - 3.0),
        );
        painter.rect_filled(bar, 2.0, bar_col);
    }
}

impl ClapPlugin for Pluck {
    const CLAP_ID: &'static str = "com.qeynos.pluck";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Karplus-Strong strummer — audio-excited tuned strings, chord/MIDI/key-detect, modal body");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Synthesizer,
    ];
}

impl Vst3Plugin for Pluck {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosPluckKS001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Synth];
}

nih_export_clap!(Pluck);
nih_export_vst3!(Pluck);
