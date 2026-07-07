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

## OVERSEER — Ozone (3rd-party plugin) hosting inside Master
- **Deferred by spec (SPECS "OVERSEER": "Ozone hosting: DEFERRED.md only"), recorded
  2026-07-07 at ship time.** OVERSEER shipped complete without it.
- **Why:** hosting an external VST inside a plugin requires a full plugin-host layer
  (scanning, editor embedding, state proxying) — far out of scope for one iteration.
- **How to resume:** add a hosted-FX slot to `MasterCore` post-multiband/pre-limiter
  using a minimal VST3 hosting layer, proxy bypass/latency, embed its editor in a
  separate window. Revalidate via build.ps1 overseer.

## OVERSEER — tier-2 (cross-process) bus fallback
- **Deferred 2026-07-07 (design decision per PRD §3, not a failure).** The Node↔Master
  link is tier 1 (same-DLL `static` registry); FL "Make bridged" on either instance
  severs it (documented in README/docs/CHECKPOINTS; audio processing is unaffected).
- **How to resume:** NERVE/X-RAY build the `memmap2` shared-memory bus in suite-core
  (PRD §3 tier 2: fixed-layout slots, per-slot seqlock, heartbeat GC); port
  `plugins/overseer/src/bus.rs` onto it keeping the same `Slot` API.

## W4-SESSION-BOOTSTRAP — tempo (BPM) application
- **Deferred 2026-07-07 (server limitation, not a code descope).** The SPECS.md W4
  row lists "tempo" as a template field, but the FL Studio MCP server exposes NO
  tempo/BPM setter — its transport handlers are only start/stop/record/getStatus/
  setPosition/getLength/setLoopMode/setPlaybackSpeed (confirmed in
  `device_FLStudioMCP.py` dispatch + `tools/transport.py`). The tool keeps `tempo`
  in the template format and reports it as a **skipped** field (printed, not
  applied, not an error) so templates carry the intended BPM as documentation.
- **Why:** adding a tempo command would require modifying the user's FL MCP repo
  (out of scope: "Do NOT modify that repo") and a matching FL-side handler.
- **How to resume:** if the MCP server later gains a `transport.setTempo` (FL API:
  `mixer.setMasterTempo` / processMECEvent tempo, or a general.setTempo), map the
  template's `tempo` to it in `generate_ops()` (drop the skip report) and add a
  snapshot/mock test. No format change needed — the field already exists.

## W8-VITALGEN — Serum 2 preset generation
- **Deferred by spec** (SPECS.md "W8 vitalgen": "Serum 2 = DEFERRED"), recorded
  2026-07-07 at ship time. VITALGEN shipped for Vital 1.5.x only.
- **Why:** Serum 2's preset format and parameter model differ entirely from Vital's
  flat JSON `settings` map; it needs its own schema ground-truth (a Serum 2 install +
  saved presets to diff), its own base template, and its own PARAM_SPEC. Out of scope
  for one iteration, and no Serum 2 install was present on the build machine to derive
  the schema from.
- **How to resume:** add a `serumgen.py` (or a `--target serum2` backend to vitalgen)
  with a Serum-2 base template + PARAM_SPEC derived from real saved Serum 2 presets,
  reusing the same constrained-subset-merged-onto-base architecture and pydantic
  validation. Add fixtures + offline tests mirroring the Vital ones.
