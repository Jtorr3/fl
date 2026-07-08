//! VOXKEY — vocal retuner (Qeynos VOX suite).
//!
//! Pitch-tracks a mono vocal (`suite_core::pitch::PitchTracker`), snaps the detected pitch to
//! the nearest tone of a Root+Scale (or a held MIDI note in MIDI-override mode), and retunes
//! it with the shared formant-preserving PV engine (`suite_core::shift::ShiftEngine`, two for
//! stereo, envelope-preserve ON). Retune speed glides the correction (0 = hard-snap autotune),
//! Amount scales it, Humanize adds slow cents drift, a Formant Offset moves the formants, and a
//! Confidence Gate holds the correction at 1.0 on breaths/silence. See [`dsp`] for the core.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Vec2},
    EguiState,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

pub mod dsp;
pub mod presets;

#[cfg(test)]
mod tests;

use dsp::{hz_to_note_name, Controls, VoxCore, ROOT_NAMES, SCALES, SCALE_NAMES};
use suite_core::bus::PluginKind;
use suite_core::presets::{load_all, Preset};
use suite_core::spectrum::SpectrumPublisher;

// ---------------------------------------------------------------------------
// GUI read-out meter (audio thread → editor)
// ---------------------------------------------------------------------------

/// Lock-free read-out of the detected/target pitch for the GUI (audio thread writes, GUI reads).
pub struct VoxMeter {
    detected: AtomicU32,
    target: AtomicU32,
    conf: AtomicU32,
    active: AtomicBool,
}

impl VoxMeter {
    fn new() -> Self {
        Self {
            detected: AtomicU32::new(0),
            target: AtomicU32::new(0),
            conf: AtomicU32::new(0),
            active: AtomicBool::new(false),
        }
    }
    #[inline]
    fn store(&self, detected: f32, target: f32, conf: f32, active: bool) {
        self.detected.store(detected.to_bits(), Ordering::Relaxed);
        self.target.store(target.to_bits(), Ordering::Relaxed);
        self.conf.store(conf.to_bits(), Ordering::Relaxed);
        self.active.store(active, Ordering::Relaxed);
    }
    fn load(&self) -> (f32, f32, f32, bool) {
        (
            f32::from_bits(self.detected.load(Ordering::Relaxed)),
            f32::from_bits(self.target.load(Ordering::Relaxed)),
            f32::from_bits(self.conf.load(Ordering::Relaxed)),
            self.active.load(Ordering::Relaxed),
        )
    }
}

// ---------------------------------------------------------------------------
// Held-note tracking (last-note priority, alloc-free fixed stack)
// ---------------------------------------------------------------------------

const MAX_HELD: usize = 16;

struct HeldNotes {
    stack: [u8; MAX_HELD],
    len: usize,
}
impl HeldNotes {
    fn new() -> Self {
        Self { stack: [0; MAX_HELD], len: 0 }
    }
    fn push(&mut self, note: u8) {
        // Remove any existing instance, then push on top (most-recent priority).
        self.remove(note);
        if self.len < MAX_HELD {
            self.stack[self.len] = note;
            self.len += 1;
        } else {
            // Drop the oldest.
            for i in 1..MAX_HELD {
                self.stack[i - 1] = self.stack[i];
            }
            self.stack[MAX_HELD - 1] = note;
        }
    }
    fn remove(&mut self, note: u8) {
        if let Some(idx) = self.stack[..self.len].iter().position(|&n| n == note) {
            for i in idx..self.len - 1 {
                self.stack[i] = self.stack[i + 1];
            }
            self.len -= 1;
        }
    }
    fn clear(&mut self) {
        self.len = 0;
    }
    fn top(&self) -> Option<u8> {
        if self.len > 0 {
            Some(self.stack[self.len - 1])
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin + params
// ---------------------------------------------------------------------------

pub struct VoxKey {
    params: Arc<VoxKeyParams>,
    core: VoxCore,
    meter: Arc<VoxMeter>,
    held: HeldNotes,
    factory_presets: Arc<Vec<Preset>>,
    spectrum: SpectrumPublisher,
}

#[derive(Params)]
pub struct VoxKeyParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "root"] pub root: IntParam,
    #[id = "scale"] pub scale: IntParam,
    #[id = "retune"] pub retune: FloatParam,
    #[id = "amount"] pub amount: FloatParam,
    #[id = "humanize"] pub humanize: FloatParam,
    #[id = "formant"] pub formant: FloatParam,
    #[id = "gate"] pub gate: FloatParam,
    #[id = "midimode"] pub midi_mode: BoolParam,
    #[id = "mix"] pub mix: FloatParam,
    #[id = "out"] pub out_trim: FloatParam,
}

fn pct(name: &'static str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit(" %")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn first_number(s: &str) -> Option<f32> {
    s.split_whitespace().next().and_then(|t| t.parse::<f32>().ok())
}

fn root_from_str(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<i32>() {
        return Some(v.clamp(0, 11));
    }
    ROOT_NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(t))
        .map(|i| i as i32)
}

fn scale_from_str(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Ok(v) = t.parse::<i32>() {
        return Some(v.clamp(0, SCALES.len() as i32 - 1));
    }
    SCALE_NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(t))
        .map(|i| i as i32)
}

impl Default for VoxKeyParams {
    fn default() -> Self {
        let d = Controls::default();
        Self {
            editor_state: EguiState::from_size(520, 640),

            root: IntParam::new("Root", d.root as i32, IntRange::Linear { min: 0, max: 11 })
                .with_value_to_string(Arc::new(|v| {
                    ROOT_NAMES.get(v as usize).copied().unwrap_or("C").to_string()
                }))
                .with_string_to_value(Arc::new(root_from_str)),

            scale: IntParam::new(
                "Scale",
                d.scale as i32,
                IntRange::Linear { min: 0, max: SCALES.len() as i32 - 1 },
            )
            .with_value_to_string(Arc::new(|v| {
                SCALE_NAMES.get(v as usize).copied().unwrap_or("Major").to_string()
            }))
            .with_string_to_value(Arc::new(scale_from_str)),

            retune: FloatParam::new(
                "Retune Speed",
                d.retune_ms,
                FloatRange::Linear { min: 0.0, max: 400.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0))
            .with_string_to_value(Arc::new(first_number)),

            amount: pct("Amount", d.amount),

            humanize: FloatParam::new(
                "Humanize",
                d.humanize_cents,
                FloatRange::Linear { min: 0.0, max: 50.0 },
            )
            .with_unit(" ct")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(Arc::new(first_number)),

            formant: FloatParam::new(
                "Formant Offset",
                d.formant_st,
                FloatRange::Linear { min: -12.0, max: 12.0 },
            )
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(first_number)),

            gate: FloatParam::new(
                "Confidence Gate",
                d.conf_gate,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(first_number)),

            midi_mode: BoolParam::new("MIDI Mode", d.midi_mode),

            mix: pct("Mix", d.mix),

            out_trim: FloatParam::new("Out", 0.0, FloatRange::Linear { min: -24.0, max: 12.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1))
                .with_string_to_value(Arc::new(first_number)),
        }
    }
}

impl VoxKeyParams {
    /// Snapshot the params into [`Controls`].
    fn controls(&self) -> Controls {
        Controls {
            root: self.root.value() as usize,
            scale: self.scale.value() as usize,
            retune_ms: self.retune.value(),
            amount: self.amount.value(),
            humanize_cents: self.humanize.value(),
            formant_st: self.formant.value(),
            conf_gate: self.gate.value(),
            midi_mode: self.midi_mode.value(),
            mix: self.mix.value(),
            out_db: self.out_trim.value(),
        }
    }
}

impl Default for VoxKey {
    fn default() -> Self {
        Self {
            params: Arc::new(VoxKeyParams::default()),
            core: VoxCore::new(48_000.0),
            meter: Arc::new(VoxMeter::new()),
            held: HeldNotes::new(),
            factory_presets: Arc::new(load_all(presets::PRESET_JSON)),
            spectrum: SpectrumPublisher::new(),
        }
    }
}

/// Apply a factory preset to the live parameters through the host (automation/undo aware).
fn apply_preset(params: &VoxKeyParams, setter: &ParamSetter, p: &Preset) {
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
    let c = presets::controls_from_preset(p);
    set_i(&params.root, c.root as i32);
    set_i(&params.scale, c.scale as i32);
    set_f(&params.retune, c.retune_ms);
    set_f(&params.amount, c.amount);
    set_f(&params.humanize, c.humanize_cents);
    set_f(&params.formant, c.formant_st);
    set_f(&params.gate, c.conf_gate);
    set_b(&params.midi_mode, c.midi_mode);
    set_f(&params.mix, c.mix);
    set_f(&params.out_trim, c.out_db);
}

impl Plugin for VoxKey {
    const NAME: &'static str = "Qeynos VOXKEY";
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
        let meter = self.meter.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| suite_core::ui::apply_theme(ctx),
            move |egui_ctx, setter, _state| {
                suite_core::ui::apply_theme(egui_ctx);
                suite_core::ui::ScaledWindow::new("qeynos-voxkey-window", Vec2::new(520.0, 640.0))
                    .min_size(Vec2::new(460.0, 520.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        use suite_core::ui::labeled_slider as row;
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("QEYNOS · VOXKEY").color(suite_core::ui::ACCENT));
                        ui.label(
                            egui::RichText::new("vocal retuner — scale / MIDI pitch correction")
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.add_space(6.0);

                        // Preset bar: factory + user presets, save/save-as/delete, dirty dot.
                        suite_core::ui::PresetBar::new("voxkey", presets.as_slice()).show(
                            ui,
                            &*params,
                            setter,
                            |setter, p| apply_preset(&params, setter, p),
                        );

                        // Live detected → target read-out.
                        let (detected, target, conf, active) = meter.load();
                        ui.horizontal(|ui| {
                            let det = if detected > 0.0 {
                                format!("{}  ({:.0} Hz)", hz_to_note_name(detected), detected)
                            } else {
                                "—".to_string()
                            };
                            let tgt = if active && target > 0.0 {
                                format!("{}  ({:.0} Hz)", hz_to_note_name(target), target)
                            } else {
                                "— (hold / bypass)".to_string()
                            };
                            ui.label(egui::RichText::new("IN").color(suite_core::ui::TEXT_DIM).small());
                            ui.label(egui::RichText::new(det).color(suite_core::ui::TEXT).strong());
                            ui.label(egui::RichText::new("→").color(suite_core::ui::TEXT_DIM));
                            ui.label(egui::RichText::new("TGT").color(suite_core::ui::TEXT_DIM).small());
                            ui.label(egui::RichText::new(tgt).color(suite_core::ui::ACCENT).strong());
                        });
                        ui.label(
                            egui::RichText::new(format!("confidence {:.2}", conf))
                                .color(suite_core::ui::TEXT_DIM)
                                .small(),
                        );
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(egui::RichText::new("KEY").color(suite_core::ui::ACCENT).small());
                            egui::Grid::new("voxkey-key").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "ROOT", &params.root, setter);
                                row(ui, "SCALE", &params.scale, setter);
                                ui.end_row();
                                row(ui, "MIDI MODE", &params.midi_mode, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("RETUNE").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("voxkey-retune").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "SPEED", &params.retune, setter);
                                row(ui, "AMOUNT", &params.amount, setter);
                                ui.end_row();
                                row(ui, "HUMANIZE", &params.humanize, setter);
                                row(ui, "GATE", &params.gate, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("FORMANT").color(suite_core::ui::TEXT_DIM).small());
                            egui::Grid::new("voxkey-formant").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                                row(ui, "OFFSET", &params.formant, setter);
                                ui.end_row();
                            });

                            ui.add_space(4.0);
                            ui.separator();
                            egui::Grid::new("voxkey-out").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
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
        self.core = VoxCore::new(buffer_config.sample_rate);
        context.set_latency_samples(self.core.latency_samples());
        self.spectrum.init(buffer_config.sample_rate, PluginKind::Generic, "VOXKEY");
        true
    }

    fn reset(&mut self) {
        self.core.reset();
        self.held.clear();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let _ftz = suite_core::dsp::ScopedFtz::enable();

        // Drain MIDI events for this block (block-accurate held-note tracking is enough for
        // retune targeting). Update the held-note stack; the top note is the retune target.
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, .. } => self.held.push(note),
                NoteEvent::NoteOff { note, .. } => self.held.remove(note),
                NoteEvent::Choke { note, .. } => self.held.remove(note),
                _ => {}
            }
        }
        let held_hz = self.held.top().map(util::midi_note_to_freq);

        let controls = self.params.controls();
        let s = controls.resolve(held_hz);
        self.core.configure(&s);

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

        // Publish the read-out for the GUI.
        self.meter.store(
            self.core.detected_hz(),
            self.core.target_hz(),
            self.core.confidence(),
            self.core.active(),
        );

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

impl Drop for VoxKey {
    fn drop(&mut self) {
        self.spectrum.release();
    }
}

impl ClapPlugin for VoxKey {
    const CLAP_ID: &'static str = "com.qeynos.voxkey";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Vocal retuner — pitch-tracked scale/MIDI retune with formant-preserving pitch shift");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::PitchShifter,
        ClapFeature::Custom("vocal"),
    ];
}

impl Vst3Plugin for VoxKey {
    const VST3_CLASS_ID: [u8; 16] = *b"QeynosVOXKEYvox1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift];
}

nih_export_clap!(VoxKey);
nih_export_vst3!(VoxKey);
