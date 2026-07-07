# DEFERRED — features consciously descoped by the loop (PRD §1.5 valves)

Each entry: item | feature | why | how to pick it back up.

## GRIT — Mode C (spectral STFT per-bin drive)
- **Deferred 2026-07-07.** Shipped GRIT with Modes A (Env→Drive) and B
  (Waveshape-by-SC dynamic bias); Mode C was descoped before consuming the attempt
  budget as a §1.5 judgment call.
- **Why:** Mode C requires a streaming STFT (analysis/synthesis windowing, overlap-add,
  per-bin SC-magnitude-scaled drive, latency reporting) that must be *allocation-free
  inside `process`* under nih-plug's `assert_process_allocs`, and must pass pluginval
  strictness-8 across block sizes 64..1024 at 44.1/48/96 kHz. That is a large,
  high-risk surface for one mode of three; Modes A+B already satisfy the entire GRIT
  done-bar (THD rises during SC pulses; auto-gain holds post-RMS within ±1 dB of pre)
  and give a complete, shippable plugin. The 4x-oversampling and preset/harness
  infrastructure Mode C would need is already in place (`suite_core::dsp::Oversampler4x`,
  `suite_core::presets`).
- **How to resume:** add a preallocated real-FFT STFT to `suite-core` (frame 512,
  hop 128, Hann, COLA-normalized), preallocate all scratch in `GritCore` via
  `initialize`, report `set_latency_samples(frame)`, and add a `Mode::Spectral` variant
  to `dsp::Mode` + `ModeParam`. Per bin: `mag' = mag * (1 + drive·sc_bin_env)` shaped
  through a bounded nonlinearity, phase preserved. Add a THD-vs-SC render test in the
  spectral mode. Re-run `build.ps1 grit` and revalidate.
