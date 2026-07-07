//! suite-core — shared DSP, test signals, offline render harness, and egui theme
//! for the Qeynos audio suite. Everything here is API-agnostic pure Rust except the
//! `ui` module, which is gated behind the `gui` feature and depends on nih_plug_egui.

pub mod dsp;
pub mod harness;
pub mod loudness;
pub mod pitch;
pub mod presets;
pub mod stft;
pub mod testsig;

#[cfg(feature = "gui")]
pub mod ui;

/// Canonical sample rate used by the offline test harness and generated signals.
pub const TEST_SR: f32 = 48_000.0;

/// Linear amplitude -> dBFS. Returns -inf sentinel for non-positive input.
#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    if x <= 1.0e-12 {
        -f32::INFINITY
    } else {
        20.0 * x.log10()
    }
}

/// dB -> linear amplitude.
#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
