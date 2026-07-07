# STATUS

CURRENT: IMPACT | STEP: 1 | ATTEMPTS: 0 | LAST-ACTION: start(IMPACT) — kick drum synth MIDI instrument. EMBER SHIPPED (full, [x]) prior.
PUSH-PENDING: no
DONE: BOOTSTRAP, GRIT, EMBER
DESCOPED: GRIT Mode C (spectral STFT) → DEFERRED.md

## LOG (append-only: date | item | outcome | how-to-test-in-FL)
2026-07-07 | PLANNING | PRD v2 hardened via 3-agent adversarial review; repo, specs, loop contract, allowlist committed | n/a
2026-07-07 | BOOTSTRAP | GO: _template passes clap-validator + pluginval on windows-gnu | rescan plugins in FL, load "Qeynos Template"
2026-07-07 | GRIT | SHIPPED (degraded, [x]*): sidechained distortion, Modes A (Env-Drive) + B (Waveshape); Mode C (spectral STFT) deferred to DEFERRED.md. 4x oversampling + presets module added to suite-core (all-crates revalidated: _template green). clap-validator 14/0, pluginval s8 PASS, CLAP installed. Done-bar met: THD rises during SC pulses, auto-gain holds post-RMS within ±1 dB of pre. 5 presets, renders in renders/GRIT/. | FL: Find more plugins → add "Qeynos GRIT", route a kick to its sidechain, load "Kick Bass Grit", confirm it pumps with the kick (SC Listen to audition the focus band)
2026-07-07 | EMBER | SHIPPED (full, [x]): spectral fader / temporal smoother. Added alloc-free streaming STFT engine to suite-core (`suite_core::stft`, realfft 3.5) — all crates revalidated green (_template, grit). EMBER: per-bin state machine (coef=1-exp(-T/τ), 8-band log-freq attack/decay curves, decay to 60s), phase-vocoder tails (tonal ring), 1/3-oct fitting envelope, freeze (τ→∞), gate, tail gain, latency-aligned dry/wet. Reports 2048-sample latency. Done-bar met on FIRST attempt (no Fable escalation): τ=10s noise tail +2s > -40 dBFS & frame-RMS monotone↓; freeze tail flat ±1 dB over 5s; mix=0 nulls vs latency-delayed dry < -80 dB. clap-validator PASS, pluginval s8 SUCCESS (44.1/48/96k, blocks 64..1024), CLAP installed. 5 presets, renders in renders/EMBER/. | FL: Find more plugins → add "Qeynos EMBER", load "Bloom Pad" on a pad/vocal (notes bloom & sustain past release); play a sustained note, tick Freeze, stop input → spectrum holds as a drone. Host reports +2048-sample latency (auto delay-comp).

## NOTES
- New suite-core API (EMBER, 2026-07-07): `stft::Stft` — streaming alloc-free STFT.
  `Stft::new(fft_size, hop)` (periodic Hann, COLA-normalized WOLA); `process(x, &mut cb)`
  where `cb: FnMut(&mut [Complex<f32>])` mutates the length-`num_bins()` complex spectrum
  per frame (DC/Nyquist imag auto-zeroed for a valid real inverse); returns one output
  sample delayed by `latency()` (== fft_size). Also `reset()`, `fft_size()`, `hop()`,
  `num_bins()` (= fft_size/2+1), `bin_freq(k, sr)`. `Complex` is re-exported from
  `suite_core::stft`. Backed by `realfft` (workspace dep, pure Rust, windows-gnu clean).
  SMUDGE/SEANCE/CARVE/DRIFT reuse this. To build a spectral effect: keep per-bin state,
  in the callback read mag=`b.norm()`/phase=`b.arg()`, rewrite `b = Complex::from_polar`.
  Report `set_latency_samples(stft.latency())` and delay the dry path by the same for a
  clean mix=0 null.
- New suite-core APIs (GRIT, 2026-07-07): `dsp::Oversampler2x` / `dsp::Oversampler4x`
  (polyphase halfband FIR, alloc-free `process(x, |v| f(v))`; reset()); `presets`
  module (`Preset{name, values}`, `Preset::parse`, `load_all(&[&str])` — flat embedded
  JSON via serde_json, now a suite-core dep). Any later plugin needing oversampling or
  factory presets should reuse these. suite-core API rule honored: workspace rebuilt,
  _template revalidated green.
- Fixed a latent pre-existing bug: `dsp::tests::env_follower_tracks_level` asserted RMS
  (0.354) but used fast-attack/slow-release times (which peak-track ~0.5). Made the test
  times symmetric so it measures level. (Never gated anything — build.ps1 tests per
  plugin crate, not `-p suite-core`.)
- Toolchain gap fixed: rustup's x86_64-pc-windows-gnu ships dlltool but NO assembler,
  so raw-dylib import-lib generation (windows-sys, parking_lot_core) fails with
  "dlltool could not create import library ... CreateProcess". Fix = portable
  MinGW-w64 binutils (winlibs 16.1.0 ucrt) extracted to tools/bin/mingw64 (gitignored)
  and prepended to PATH. build.ps1 does this automatically. Any fresh shell that builds
  cargo directly (not via build.ps1) MUST prepend tools\bin\mingw64\bin to PATH.
- nih-plug pinned rev: f36931f7af4646065488a9845d8f8c2f95252c23 (master @ 2026-07-07).
- clap-validator: 14 passed / 0 failed / 6 skipped / 1 warning (scan-time 363ms, cosmetic).
- pluginval strictness 8 (--skip-gui-tests): SUCCESS across 44.1/48/96k, blocks 64..1024.
