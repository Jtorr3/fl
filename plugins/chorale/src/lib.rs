//! CHORALE — resonator bank (Qeynos suite, Phase 3; the last Phase 3 plugin).
//!
//! A bank of 12–24 waveguide (extended-Karplus-Strong) resonators, continuously excited by
//! the audio input, tuned to a **held MIDI chord**, a **selected scale/chord on a root**
//! (spread across octaves), or a **chromagram key-detect** (confidence-gated, falling back to
//! the scale). Each resonator's continuous input gain is optionally weighted by the input's
//! band energy at its pitch (via `suite_core::spectrum::SpectrumTap`) so the bank "sings"
//! sympathetically. Zero reported latency; `mix = 0` nulls against the dry input.
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

use dsp::{ChoraleCore, Scale, Settings, TuningSource, MAX_RESONATORS};
use suite_core::presets::{load_all, Preset};

// ---------------------------------------------------------------------------
// Param-facing enums (nih-plug `Enum`), mapped onto the pure-DSP enums.
// ---------------------------------------------------------------------------

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SourceParam {
    #[id = "scale"]
    #[name = "Scale/Chord"]
    Scale,
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
            SourceParam::Scale => TuningSource::Scale,
            SourceParam::Midi => TuningSource::Midi,
            SourceParam::KeyDetect => TuningSource::KeyDetect,
        }
    }
    fn from_index(i: usize) -> SourceParam {
        match i {
            1 => SourceParam::Midi,
            2 => SourceParam::KeyDetect,
            _ => SourceParam::Scale,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ScaleParam {
    #[id = "mtriad"]
    #[name = "Minor Triad"]
    MinorTriad,
    #[id = "mjtriad"]
    #[name = "Major Triad"]
    MajorTriad,
    #[id = "min7"]
    #[name = "Minor 7"]
    Minor7,
    #[id = "maj7"]
    #[name = "Major 7"]
    Major7,
    #[id = "sus2"]
    #[name = "Sus2"]
    Sus2,
    #[id = "sus4"]
    #[name = "Sus4"]
    Sus4,
    #[id = "pow5"]
    #[name = "5th Stack"]
    Power5,
    #[id = "mpent"]
    #[name = "Minor Pentatonic"]
    MinorPentatonic,
    #[id = "mjpent"]
    #[name = "Major Pentatonic"]
    MajorPentatonic,
    #[id = "phryg"]
    #[name = "Phrygian"]
    Phrygian,
    #[id = "dorian"]
    #[name = "Dorian"]
    Dorian,
    #[id = "oct"]
    #[name = "Octaves"]
    Octaves,
}
impl ScaleParam {
    fn to_dsp(self) -> Scale {
        match self {
            ScaleParam::MinorTriad => Scale::MinorTriad,
            ScaleParam::MajorTriad => Scale::MajorTriad,
            ScaleParam::Minor7 => Scale::Minor7,
            ScaleParam::Major7 => Scale::Major7,
            ScaleParam::Sus2 => Scale::Sus2,
            ScaleParam::Sus4 => Scale::Sus4,
            ScaleParam::Power5 => Scale::Power5,
            ScaleParam::MinorPentatonic => Scale::MinorPentatonic,
            ScaleParam::MajorPentatonic => Scale::MajorPentatonic,
            ScaleParam::Phrygian => Scale::Phrygian,
            ScaleParam::Dorian => Scale::Dorian,
            ScaleParam::Octaves => Scale::Octaves,
        }
    }
    fn from_index(i: usize) -> ScaleParam {
        match i {
            1 => ScaleParam::MajorTriad,
            2 => ScaleParam::Minor7,
            3 => ScaleParam::Major7,
            4 => ScaleParam::Sus2,
            5 => ScaleParam::Sus4,
            6 => ScaleParam::Power5,
            7 => ScaleParam::MinorPentatonic,
            8 => ScaleParam::MajorPentatonic,
            9 => ScaleParam::Phrygian,
            10 => ScaleParam::Dorian,
            11 => ScaleParam::Octaves,
            _ => ScaleParam::MinorTriad,
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
pub struct ChoraleParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "source"]
    pub source: EnumParam<SourceParam>,
    #[id = "root"]
    pub root: IntParam,
    #[id = "scale"]
    pub scale: EnumParam<ScaleParam>,
    #[id = "count"]
    pub count: IntParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "damp"]
    pub damp: FloatParam,
    #[id = "spread"]
    pub spread: FloatParam,
    #[id = "symp"]
    pub symp: FloatParam,
    #[id = "excite"]
    pub excite: FloatParam,
    #[id = "stereo"]
    pub stereo: FloatParam,
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

impl Default for ChoraleParams {
    fn default() -> Self {
        let pct = || {
            (
                formatters::v2s_f32_percentage(0),
                formatters::s2v_f32_percentage(),
            )
        };
        let (d_v2s, d_s2v) = pct();
        let (dm_v2s, dm_s2v) = pct();
        let (sy_v2s, sy_s2v) = pct();
        let (st_v2s, st_s2v) = pct();
        let (m_v2s, m_s2v) = pct();
        Self {
            editor_state: EguiState::from_size(560, 580),
            source: EnumParam::new("Source", SourceParam::Scale),
            root: IntParam::new("Root", 9, IntRange::Linear { min: 0, max: 11 })
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
            scale: EnumParam::new("Scale/Chord", ScaleParam::MinorTriad),
            count: IntParam::new("Resonators", 16, IntRange::Linear { min: 12, max: 24 }),
            decay: FloatParam::new("Decay", 0.85, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(d_v2s)
                .with_string_to_value(d_s2v),
            damp: FloatParam::new("Damp", 0.4, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(dm_v2s)
                .with_string_to_value(dm_s2v),
            spread: FloatParam::new("Spread", 6.0, FloatRange::Linear { min: 0.0, max: 50.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" ct")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            symp: FloatParam::new("Sympathetic", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(sy_v2s)
                .with_string_to_value(sy_s2v),
            excite: FloatParam::new("Excite", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            stereo: FloatParam::new("Stereo", 0.6, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(st_v2s)
                .with_string_to_value(st_s2v),
            wetsolo: BoolParam::new("Wet Solo", false),
            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(m_v2s)
                .with_string_to_value(m_s2v),
            out: FloatParam::new("Out", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mod_routes: Arc::new(RwLock::new(ModRoutes::new())),
        }
    }
}

impl ChoraleParams {
    /// Build a DSP [`Settings`] from the params + the currently-held MIDI notes.
    fn snapshot(&self, held: &[bool; 128]) -> Settings {
        let mut held_hz = [f32::NAN; MAX_RESONATORS];
        let mut count = 0usize;
        for (note, &down) in held.iter().enumerate() {
            if down {
                if count < MAX_RESONATORS {
                    held_hz[count] = dsp::midi_to_freq(note as f32);
                }
                count += 1;
            }
        }
        Settings {
            source: self.source.value().to_dsp(),
            root_pc: self.root.value(),
            scale: self.scale.value().to_dsp(),
            count: (self.count.value() as usize).clamp(12, MAX_RESONATORS),
            decay: self.decay.value(),
            damp: self.damp.value(),
            spread_cents: self.spread.value(),
            sympathetic: self.symp.value(),
            excite: self.excite.value(),
            stereo: self.stereo.value(),
            wet_solo: self.wetsolo.value(),
            mix: self.mix.value(),
            out_db: self.out.value(),
            held: held_hz,
            held_count: count.min(MAX_RESONATORS),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct Chorale {
    params: Arc<ChoraleParams>,
    core: ChoraleCore,
    held: [bool; 128],
    factory_presets: Arc<Vec<Preset>>,
    /// Per-resonator activity envelope published to the GUI.
    activity: Arc<Vec<AtomicF32>>,
    spectrum: SpectrumPublisher,
}

impl Default for Chorale {
    fn default() -> Self {
        let mut activity = Vec::with_capacity(MAX_RESONATORS);
        for _ in 0..MAX_RESONATORS {
            activity.push(AtomicF32::new(0.0));
        }
        Self {
            params: Arc::new(ChoraleParams::default()),
            core: ChoraleCore::new(48_000.0),
            held: [false; 128],
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            activity: Arc::new(activity),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

fn apply_preset(params: &ChoraleParams, setter: &ParamSetter, p: &Preset) {
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
    if let Some(v) = p.get("scale") {
        let e = ScaleParam::from_index(v as usize);
        setter.begin_set_parameter(&params.scale);
        setter.set_parameter(&params.scale, e);
        setter.end_set_parameter(&params.scale);
    }
    if let Some(v) = p.get("count") {
        setter.begin_set_parameter(&params.count);
        setter.set_parameter(&params.count, v as i32);
        setter.end_set_parameter(&params.count);
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
    if let Some(v) = p.get("spread") {
        set_f(&params.spread, v);
    }
    if let Some(v) = p.get("sympathetic") {
        set_f(&params.symp, v);
    }
    if let Some(v) = p.get("excite") {
        set_f(&params.excite, v);
    }
    if let Some(v) = p.get("stereo") {
        set_f(&params.stereo, v);
    }
    if let Some(v) = p.get("mix") {
        set_f(&params.mix, v);
    }
    if let Some(v) = p.get("out") {
        set_f(&params.out, v);
    }
}

impl Plugin for Chorale {
    const NAME: &'static str = "Qeynos CHORALE";
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

    // Effect WITH MIDI input — held notes tune the bank.
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
                suite_core::ui::ScaledWindow::new("qeynos-chorale-window", Vec2::new(560.0, 580.0))
                    .min_size(Vec2::new(460.0, 480.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(
                            egui::RichText::new("QEYNOS · CHORALE").color(suite_core::ui::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Resonator bank — audio excites a bank of tuned waveguides",
                            )
                            .color(suite_core::ui::TEXT_DIM)
                            .small(),
                        );
                        ui.add_space(6.0);

                        suite_core::ui::PresetBar::new("chorale", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );
                        suite_core::ui::mod_section(
                            ui,
                            &params.mod_routes,
                            &[
                                ("decay", "DECAY"),
                                ("damp", "DAMP"),
                                ("symp", "SYMP"),
                                ("mix", "MIX"),
                            ],
                        );
                        ui.separator();

                        // Resonator-activity display.
                        res_activity(ui, &activity);
                        ui.add_space(6.0);

                        egui::Grid::new("chorale-tuning")
                            .num_columns(4)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "SOURCE", &params.source, setter);
                                row(ui, "ROOT", &params.root, setter);
                                row(ui, "SCALE", &params.scale, setter);
                                row(ui, "COUNT", &params.count, setter);
                                ui.end_row();
                                row(ui, "DECAY", &params.decay, setter);
                                row(ui, "DAMP", &params.damp, setter);
                                row(ui, "SPREAD", &params.spread, setter);
                                row(ui, "SYMP", &params.symp, setter);
                                ui.end_row();
                                row(ui, "EXCITE", &params.excite, setter);
                                row(ui, "STEREO", &params.stereo, setter);
                                row(ui, "WET SOLO", &params.wetsolo, setter);
                                row(ui, "MIX", &params.mix, setter);
                                ui.end_row();
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
        // Zero processing latency (the resonators are causal; wet-only, dry is direct).
        context.set_latency_samples(0);
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "CHORALE");
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
                s.sympathetic = routes.modulated_float("symp", &self.params.symp, bus);
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
            let env = self.core.res_env();
            for i in 0..MAX_RESONATORS {
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

        // KeepAlive: the sympathetic ringing tails outlast the input.
        ProcessStatus::KeepAlive
    }
}

impl Drop for Chorale {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

/// Resonator-activity bars showing per-resonator energy (cheap; optional GUI extra).
fn res_activity(ui: &mut egui::Ui, activity: &[AtomicF32]) {
    ui.ctx().request_repaint();
    let n = activity.len();
    let avail = ui.available_width().min(420.0);
    let size = Vec2::new(avail, 42.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, suite_core::ui::PANEL);
    let gap = 2.0;
    let bw = (rect.width() - gap * (n as f32 + 1.0)) / n as f32;
    for i in 0..n {
        let e = activity[i].load(Ordering::Relaxed).clamp(0.0, 1.0);
        let h = e.sqrt() * (rect.height() - 6.0);
        let x0 = rect.left() + gap + i as f32 * (bw + gap);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, rect.bottom() - 3.0 - h),
            egui::pos2(x0 + bw, rect.bottom() - 3.0),
        );
        painter.rect_filled(bar, 1.0, suite_core::ui::ACCENT);
    }
}

impl ClapPlugin for Chorale {
    const CLAP_ID: &'static str = "com.qeynos.chorale";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Resonator bank — audio-excited waveguide resonators tuned to scale/MIDI/key-detect, sympathetic weighting");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Synthesizer,
    ];
}

impl Vst3Plugin for Chorale {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosChorale001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Synth];
}

nih_export_clap!(Chorale);
nih_export_vst3!(Chorale);
