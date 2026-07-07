# STATUS

CURRENT: (none) | STEP: - | ATTEMPTS: 0 | LAST-ACTION: OVERSEER SHIPPED (full, [x]) — one bundle exporting Node + Master, both green on clap-validator (26/0) + pluginval s8 (both plugins, incl. Fuzz parameters), installed. New suite-core API: loudness (BS.1770 K-weighting + gated LUFS). All crates revalidated via build.ps1 -All. One fix attempt total (inverted-clamp panic under param fuzz at 44.1k). No Fable escalation, no descope beyond spec-mandated Ozone hosting.
PUSH-PENDING: no
DONE: BOOTSTRAP, GRIT, EMBER, IMPACT, TRACER, OVERSEER
DESCOPED: GRIT Mode C (spectral STFT) → DEFERRED.md; OVERSEER Ozone hosting + tier-2 bus (by spec/design) → DEFERRED.md

## LOG (append-only: date | item | outcome | how-to-test-in-FL)
2026-07-07 | PLANNING | PRD v2 hardened via 3-agent adversarial review; repo, specs, loop contract, allowlist committed | n/a
2026-07-07 | BOOTSTRAP | GO: _template passes clap-validator + pluginval on windows-gnu | rescan plugins in FL, load "Qeynos Template"
2026-07-07 | GRIT | SHIPPED (degraded, [x]*): sidechained distortion, Modes A (Env-Drive) + B (Waveshape); Mode C (spectral STFT) deferred to DEFERRED.md. 4x oversampling + presets module added to suite-core (all-crates revalidated: _template green). clap-validator 14/0, pluginval s8 PASS, CLAP installed. Done-bar met: THD rises during SC pulses, auto-gain holds post-RMS within ±1 dB of pre. 5 presets, renders in renders/GRIT/. | FL: Find more plugins → add "Qeynos GRIT", route a kick to its sidechain, load "Kick Bass Grit", confirm it pumps with the kick (SC Listen to audition the focus band)
2026-07-07 | IMPACT | SHIPPED (full, [x]): kick drum synth (MIDI instrument). Mono last-note-priority voice: exponential pitch env f(t)=f_end+(f_start−f_end)e^(−t/τ_p) with curve morph → phase-continuous sine/tri body; band-passed noise click (SVF BP, own 5–50ms decay) + 3 embedded PCM transients (Tick/Snap/Knock, synthesized offline in build.rs as const arrays, windowed to zero) + sub osc (f_end×ratio); mix → waveshaper-bank drive (Tube/Tape/Fold/Hard, pre-amp-env) → exponential amp env (curve) → soft/hard clip. Length macro scales amp decay + pitch τ together; Key-track sets f_end from MIDI (A1=55Hz). Phase-continuous retrigger + 1.5ms declick ramp on amp env AND click/transient onset. New suite-core API `testsig::synth_kick`/`KickSpec` (IMPACT's own math, replaces the kick stub) — all crates revalidated green via build.ps1 -All (_template, grit, ember, impact). Done-bar met: STFT f0 starts within 10% of f_start & ends within 5% of f_end; mid-decay retrigger stays within declick bound vs no-retrigger. clap-validator 16/0 (was 15/1 — fixed IntParam text_to_value consistency by adding string_to_value to shape/trans), pluginval s8 SUCCESS, CLAP installed. 5 presets, renders in renders/IMPACT/. No Fable escalation, no descope. | FL: Find more plugins → add "Qeynos IMPACT" to a channel, play notes (each fires a kick); load "808 Long"/"House Punch"; enable Key Track to tune from the keyboard; fire rapid repeated notes to hear the declicked retrigger.
2026-07-07 | EMBER | SHIPPED (full, [x]): spectral fader / temporal smoother. Added alloc-free streaming STFT engine to suite-core (`suite_core::stft`, realfft 3.5) — all crates revalidated green (_template, grit). EMBER: per-bin state machine (coef=1-exp(-T/τ), 8-band log-freq attack/decay curves, decay to 60s), phase-vocoder tails (tonal ring), 1/3-oct fitting envelope, freeze (τ→∞), gate, tail gain, latency-aligned dry/wet. Reports 2048-sample latency. Done-bar met on FIRST attempt (no Fable escalation): τ=10s noise tail +2s > -40 dBFS & frame-RMS monotone↓; freeze tail flat ±1 dB over 5s; mix=0 nulls vs latency-delayed dry < -80 dB. clap-validator PASS, pluginval s8 SUCCESS (44.1/48/96k, blocks 64..1024), CLAP installed. 5 presets, renders in renders/EMBER/. | FL: Find more plugins → add "Qeynos EMBER", load "Bloom Pad" on a pad/vocal (notes bloom & sustain past release); play a sustained note, tick Freeze, stop input → spectrum holds as a drone. Host reports +2048-sample latency (auto delay-comp).

2026-07-07 | TRACER | SHIPPED (full, [x]): pitch-tracking multiband saturation. MPM pitch detector (new `suite_core::pitch`) on a mono-summed, anti-aliased, ~12 kHz-decimated stream (window 1024) → median-5 → ±35-cent hysteresis → Hz/ms slew → (f0, confidence). Time-varying LR4 crossover tree = cascaded 2nd-order Butterworth pairs built on the TPT SVF (stable under per-32-sample-block cutoff modulation), cutoffs = harmonic multiples of f0 (×1.5/×4/×8) × 2^SmartFreq, or per-crossover Fixed Hz; confidence < 0.6 freezes cutoffs; NaN/blow-up guard resets tree + 256-sample crossfade. Per band: drive → suite waveshaper bank → 2x OS → level, summed; optional constant-color drive (inverse ISO-226 11-pt LUT). MIDI mode (MidiConfig::Basic) replaces the detector with note-on f0. Done-bar met on FIRST attempt (no Fable escalation, no LR4 instability): (1) sliding-saw → band-1 centroid tracks f0 within ±1 semitone across the glide; (2) white noise → crossovers frozen (< 0.5 Hz drift over 1 s). Plus a param-fuzz stability test (48 dB drive, hard shaper, degenerate cutoffs → finite, ≤ 0 dBFS). clap-validator PASS, pluginval s8 SUCCESS across 44.1/48/96k blocks 64..1024 incl. Fuzz-parameters, CLAP installed. New suite-core APIs also: testsig::synth_vocal (replaces stub) + sliding_saw; all crates revalidated green via build.ps1 -All. 5 presets, 10 renders in renders/TRACER/. | FL: Find more plugins → add "Qeynos TRACER" on a monophonic pitched source (bass/808/vocal/lead); load "Sliding 808 Grit" and glide a note — the bands follow the pitch. For drums/bus set crossovers to Fixed or load "Fixed-Band Bus Saturator". Pitch Mode = MIDI to key the bands from notes.

2026-07-07 | OVERSEER | SHIPPED (full, [x]): mastering system — ONE crate/bundle exporting TWO plugins via multi-plugin `nih_export_clap!/vst3!` (PRD §3 tier 1). **Node** (per track): meter → 4-band EQ (RBJ LS/2×bell/HS biquads) → FF comp (10 ms RMS detector, soft knee, atk/rel, makeup) → level-preserving tanh sat → M/S width → trim → meter; text LABEL (persisted `RwLock<String>`, non-automatable); registers a slot on the same-DLL BUS. **Master**: 4-band EQ → 3-band multiband comp (LR4 = cascaded Butterworth TPT-SVF pairs) → lookahead limiter (2 ms delay + sliding-window max → anticipatory gain envelope, no attack overshoot; ceiling clamp guarantee; latency reported via set_latency_samples, dry path latency-matched) → BS.1770 LUFS meter (new reusable `suite_core::loudness`: sample-rate-correct K-weighting biquads, momentary 400 ms / short 3 s / gated integrated + reset) + 4x-OS true-peak-approx metering. BUS: `OnceLock<Bus>` registry, per-slot atomics only in process() (label mutex touched only from GUI/init), Master-written overrides (THRESH/RATIO/DRIVE/WIDTH/TRIM) win by timestamp, Node local touch steals back, GC via Arc strong-count (strictly stronger than heartbeat staleness; heartbeat still published for liveness). Master GUI = own chain + live NODES grid (label, PK/RMS/LUFS-M, 5 remote sliders + per-param release). Done-bar met on FIRST test run: (1) +6 dBFS sine, ceiling −1 → peak ≤ −0.9 dBFS (+ no-discontinuity click check); (2) LUFS: meter == own-K-filter analytic value ±0.5 LU (momentary + integrated), unweighted hook reads −20.0 ±0.1; (3) bus round-trip: override reaches Node next block, steal-back + GC asserted. One gate fix attempt: f32::clamp inverted-bounds PANIC in set_crossovers under pluginval param-fuzz at 44.1k (xo_low→20 kHz > 0.45·fs) — sanitized + regression test. clap-validator 26/0 (both plugins), pluginval s8 SUCCESS ×2 (44.1/48/96k, blocks 64..1024, incl. Fuzz), single overseer.clap installed. 6 presets (Node: Kick Strip/Vocal Strip/Bus Glue; Master: Techno/Gentle/Loud & Proud), 9 renders in renders/OVERSEER/. build.ps1 needed NO changes (already one-bundle-per-crate; both validators enumerate the two plugins inside it). All crates revalidated green (build.ps1 -All) after the suite-core loudness addition. No Fable escalation — the §8-eligible limiter passed first try. | FL: Find more plugins → add "Qeynos OVERSEER Node" on 2–3 tracks (set LABEL in each GUI), "Qeynos OVERSEER Master" on the master. Master GUI NODES grid lists them live; drag THRESH/DRIVE there → Node badges OVR + sound changes; touch the param on the Node → steals back. Keep "Make bridged" UNTICKED on these. Limiter: slam a hot mix, ceiling −1 dB → no peak past it; watch LUFS M/S/I + RESET LUFS.

## NOTES
- New suite-core API (OVERSEER, 2026-07-07): `loudness` module — ITU-R BS.1770.
  `loudness::LoudnessMeter::new(fs, channels)`; `push(&[f32])` per sample frame;
  `momentary_lufs()` (400 ms), `short_lufs()` (3 s), `integrated_lufs()` (two-stage
  gating: abs −70 LUFS then rel −10 LU, 400 ms blocks @ 75% overlap, ~60 min
  pre-reserved so push never allocates in process), `reset()`,
  `set_kweighting(bool)` (test hook: disables K-filters AND the −0.691 offset →
  reading becomes plain 10·log10(mean-square) ≈ dBFS-RMS). Also public:
  `KWeight` (per-channel shelf→RLB cascade), `shelf_biquad(fs)`/`highpass_biquad(fs)`
  (bilinear-derived, correct at any fs — not just the 48k table), `k_response(freq, fs)`
  / `k_response_db` (analytic magnitude — what the OVERSEER meter assertion checks
  against), `Biquad` (f64 TDF-II with `magnitude(w)`), `LUFS_OFFSET`. W7 REFERENCE-GAP
  reuses this. Gotcha fixed this iteration: `f32::clamp` panics on inverted bounds —
  pluginval fuzzes freq params to 20 kHz where `0.45·fs` at 44.1k is lower; order your
  clamp bounds for ALL inputs (see overseer dynamics::set_crossovers + regression test).
- OVERSEER architecture note: same-DLL bus lives in `plugins/overseer/src/bus.rs`
  (`OnceLock<Bus>`, slots = atomics + one label mutex never touched in process();
  GC = Arc strong-count on registry access). If FL "Make bridged" is ticked the two
  plugins land in different processes and the link silently degrades (audio fine,
  grid empty) — tier-2 memmap2 fallback deferred to NERVE/X-RAY (DEFERRED.md).
- New suite-core API (TRACER, 2026-07-07): `pitch` module. `pitch::Mpm::new(window, sr,
  f0_min, f0_max)` + `analyze(&[f32]) -> PitchResult{ f0_hz, confidence }` — allocation-free
  McLeod Pitch Method (NSDF type-II ACF, key-maximum peak pick @ k=0.85, parabolic interp;
  confidence = interpolated NSDF peak height 0..1). `pitch::PitchTracker::new(sample_rate,
  default_f0)` — streaming sample-in tracker: anti-aliased decimation to ~12 kHz, window
  1024/hop 256, median-of-5 + ±35-cent hysteresis + Hz/ms slew; `push(x)`, `f0()`,
  `confidence()`, `set_slew`, `set_confidence_gate`, `set_midi_note(Option<f32>)` (MIDI
  bypass), `reset()`. `default_f0` is the frozen pitch used before the first confident
  detect and whenever confidence < gate (0.6) — that freeze is what keeps crossovers still
  on noise. `pitch::cents(a,b)` helper. PLUCK and CHORALE reuse this (chromagram/MIDI/held
  tuning). Also new: `testsig::synth_vocal(freq, len, sr)` — saw + 5 Hz vibrato through 3
  formant band-passes (F1 700 / F2 1220 / F3 2600, /a/-like), peak-normalized to 0.7;
  REPLACES the old `synth_vocal_stub` (kept, now delegates). SEANCE reuses it.
  `testsig::sliding_saw(f_start, f_end, amp, len, sr)` (exponential glissando 808 stand-in)
  + `testsig::sliding_saw_f0(f_start, f_end, n, len)` for exact instantaneous-f0 in tests.
  TRACER's time-varying LR4 was built on the TPT SVF (unconditionally stable under cutoff
  modulation) rather than direct-form biquads — both done-bar tests + pluginval Fuzz passed
  on the FIRST attempt, so the §8 Fable escalation valve was never triggered.
- New suite-core API (IMPACT, 2026-07-07): `testsig::synth_kick(&KickSpec, len, sr) -> Vec<f32>`
  and `testsig::KickSpec { f_start, f_end, pitch_decay_s, amp_decay_s, click, sub_level,
  sub_ratio, drive }` (Default = 180→55 Hz, ~0.5 s tail, light click). This is IMPACT's own
  kick math (exp pitch env → phase-continuous sine → band-passed noise click → tanh drive →
  exp amp env w/ 1.5 ms attack → soft clip; deterministic, peak-bounded < 0 dBFS). It REPLACES
  the old decaying-sine stub; `testsig::synth_kick_stub(len, sr)` is kept and now delegates to
  `synth_kick(&KickSpec::default(), …)`. UNDERTOW (kick-duck test), SEANCE, and any later
  plugin needing a synthetic kick should use this. IMPACT's per-plugin f0 measurement pattern:
  streaming `stft::Stft` (fft 4096/hop 1024) + quadratic peak interpolation over the low band.
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
