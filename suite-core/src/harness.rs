//! Offline render harness (PRD §4). Runs a pure-DSP processor block-by-block over a
//! test signal, writes `renders/<plugin>/*.wav` via `hound`, and provides the
//! universal mechanical assertions.

use std::path::{Path, PathBuf};

use crate::lin_to_db;

/// A pure-DSP mono processor: transform a block of samples in place. Implemented for
/// any `FnMut(&mut [f32])` so closures work as processors.
pub trait Processor {
    fn process(&mut self, block: &mut [f32]);
}

impl<F: FnMut(&mut [f32])> Processor for F {
    #[inline]
    fn process(&mut self, block: &mut [f32]) {
        self(block)
    }
}

/// Render `input` through `proc` block-by-block and return the output buffer.
pub fn render_offline<P: Processor>(mut proc: P, input: &[f32], block_size: usize) -> Vec<f32> {
    let bs = block_size.max(1);
    let mut out = input.to_vec();
    for chunk in out.chunks_mut(bs) {
        proc.process(chunk);
    }
    out
}

/// Resolve `<repo>/renders/<plugin>/<name>.wav`. The repo root is located by walking
/// up from `CARGO_MANIFEST_DIR` until a `Cargo.toml` with `[workspace]` is found.
pub fn render_path(plugin: &str, name: &str) -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // suite-core/ -> repo root
    while dir.parent().is_some() {
        let candidate = dir.join("renders");
        if dir.join("Cargo.toml").exists()
            && std::fs::read_to_string(dir.join("Cargo.toml"))
                .map(|s| s.contains("[workspace]"))
                .unwrap_or(false)
        {
            return candidate.join(plugin).join(format!("{name}.wav"));
        }
        dir.pop();
    }
    PathBuf::from("renders").join(plugin).join(format!("{name}.wav"))
}

/// Write a mono f32 buffer to a 32-bit float WAV, creating parent dirs.
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    for &s in samples {
        writer
            .write_sample(s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}

/// Convenience: render, write to `renders/<plugin>/<name>.wav`, return the output.
pub fn render_and_write<P: Processor>(
    plugin: &str,
    name: &str,
    proc: P,
    input: &[f32],
    block_size: usize,
    sample_rate: u32,
) -> Vec<f32> {
    let out = render_offline(proc, input, block_size);
    let path = render_path(plugin, name);
    if let Err(e) = write_wav(&path, &out, sample_rate) {
        eprintln!("warning: failed to write render {}: {e}", path.display());
    }
    out
}

// ---------------------------------------------------------------------------
// Measurement + assertion helpers
// ---------------------------------------------------------------------------

pub fn has_nan_or_inf(x: &[f32]) -> bool {
    x.iter().any(|v| !v.is_finite())
}

pub fn peak_dbfs(x: &[f32]) -> f32 {
    let peak = x.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    lin_to_db(peak)
}

pub fn rms_dbfs(x: &[f32]) -> f32 {
    if x.is_empty() {
        return -f32::INFINITY;
    }
    let mean_sq = x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32;
    lin_to_db(mean_sq.sqrt())
}

/// A-B null test: RMS of the residual (a - b) in dBFS. Lower = better null.
pub fn null_residual_db(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return -f32::INFINITY;
    }
    let residual: Vec<f32> = (0..n).map(|i| a[i] - b[i]).collect();
    rms_dbfs(&residual)
}

/// Partial-mix (parallel) alignment assertion — the regression guard for HARD CHECKPOINT
/// 1's comb-filtering class of bug.
///
/// Given the output of a plugin fed a **unit impulse at mix = 0.5** with a neutral /
/// near-identity wet setting, the dry and wet paths must land on top of each other: the
/// output must show a SINGLE coherent peak, not two peaks separated by the wet path's
/// (uncompensated) oversampler group delay. Concretely: no sample farther than
/// `cluster` samples from the global-peak index may reach `frac`·peak. An uncompensated
/// dry path leaves a second peak of ~0.5·(full scale) at lag 0 (or at the wet delay),
/// which trips this assertion.
pub fn assert_single_coherent_peak(out: &[f32], cluster: usize, frac: f32) {
    assert!(!has_nan_or_inf(out), "signal contains NaN/inf");
    let (peak_idx, peak) = out
        .iter()
        .enumerate()
        .fold((0usize, 0.0f32), |(bi, bv), (i, &v)| {
            if v.abs() > bv {
                (i, v.abs())
            } else {
                (bi, bv)
            }
        });
    assert!(peak > 0.0, "no peak found (silent output)");
    let thresh = frac * peak;
    for (i, &v) in out.iter().enumerate() {
        let dist = i.abs_diff(peak_idx);
        if dist > cluster && v.abs() >= thresh {
            panic!(
                "second peak at sample {i} (|{:.4}| >= {:.4}) is {dist} samples from the \
                 main peak at {peak_idx} — dry/wet not aligned (comb filtering)",
                v.abs(),
                thresh
            );
        }
    }
}

/// Universal assertions applied to every render (PRD §4): finite, peak <= 0 dBFS,
/// RMS above the silence floor. Panics with a descriptive message on failure.
pub fn assert_universal(x: &[f32]) {
    assert!(!has_nan_or_inf(x), "signal contains NaN/inf");
    let peak = peak_dbfs(x);
    assert!(peak <= 0.0, "peak {peak:.2} dBFS exceeds 0 dBFS");
    let rms = rms_dbfs(x);
    assert!(rms > -60.0, "signal is silent: RMS {rms:.2} dBFS <= -60 dBFS");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsig;

    #[test]
    fn passthrough_nulls_against_input() {
        let sig = testsig::sine(1_000.0, 0.5, 48_000, 48_000.0);
        let out = render_offline(|_b: &mut [f32]| {}, &sig, 512);
        assert!(null_residual_db(&sig, &out) < -120.0);
        assert_universal(&out);
    }

    #[test]
    fn rms_and_peak_measure_correctly() {
        // Full-scale sine: peak ~0 dBFS, RMS ~-3 dBFS.
        let sig = testsig::sine(1_000.0, 1.0, 48_000, 48_000.0);
        assert!(peak_dbfs(&sig) > -0.1 && peak_dbfs(&sig) <= 0.0);
        assert!((rms_dbfs(&sig) - (-3.01)).abs() < 0.1);
    }
}
