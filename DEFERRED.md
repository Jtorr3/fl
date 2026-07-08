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

## WIRE — true per-bandwidth Opus internal rate + real FEC/PLC recovery
- **Deferred 2026-07-07 (crate limitation + PRD-sanctioned approximation, not an
  attempt-budget descope).** WIRE shipped complete ([x], full): Plan A (`opus-rs`)
  landed, all specced params present, both done-bar assertions met.
- **What is approximated:**
  1. **Bandwidth** (NB→FB) is realised as a *pre-codec low-pass*, not by switching the
     Opus encoder's internal sampling rate (8/12/16/24/48 k). The link-test showed
     `opus-rs` 0.1.23's SILK-resampler paths at 12 k and 24 k are **buggy** (decode
     decorrelates to ~0.05 correlation at several bitrates), while the 48 k path is
     reliable and monotonic in bitrate. Running the codec always at 48 k and low-passing
     ahead of it is exactly the "approximate with bandwidth limiting and note it"
     fallback PRD §5 allows, and dodges every broken path.
  2. **FEC** is wired to the encoder (`use_inband_fec` + `packet_loss_perc`), but
     `opus-rs`' `decode()` has no true FEC/PLC recovery entry point (it errors on empty
     input), so WIRE synthesises its own click-free concealment (zero-fill + crossfade)
     for dropped frames. FEC therefore has limited audible benefit under loss.
- **How to resume:** (a) once `opus-rs` fixes its 12 k/24 k resampler (or by switching to
  Plan B `audiopus`/libopus via the portable CMake in `tools/bin/cmake`), map `Bandwidth`
  to `OpusEncoder::new(rate, …)` at 8/12/16/24/48 k with matching decoder + SRC, dropping
  the pre-LP; (b) with a codec exposing `decode_fec`/PLC (libopus does), feed the decoder
  the loss flag so FEC/PLC actually reconstruct dropped frames. WIRE's `Settings` already
  carry `bandwidth`/`fec`/`loss_pct`, so only `dsp::ChannelCodec` changes.

## VOXKEY — detector: `Mpm` directly instead of `PitchTracker` (design decision, not a descope)
- **Decided 2026-07-07 (PRD §0 in-commit decision; VOXKEY shipped full, [x]).** The build
  brief specifies `suite_core::pitch::PitchTracker` for detection. VOXKEY instead reads
  `suite_core::pitch::Mpm` directly on the same anti-aliased ~12 kHz decimated front end
  (1024 window, light median-3), with **no ±35-cent re-lock hysteresis**.
- **Why:** `PitchTracker`'s hysteresis + median are tuned for TRACER's crossover *stability*
  (it deliberately refuses updates < 35 cents). On a retuner that is a defect: right after a
  note change the detector reads up to ~35 cents below the true pitch and then STICKS there,
  and since the corrected output is `input × target/detected`, that bias lands the retuned
  note up to 35 cents off the scale tone. Measured empirically: with `PitchTracker` only
  ~60 % of pitched frames fell within the ±15-cent done-bar; with the hysteresis-free `Mpm`
  read it is ≥ 80 % (the tone-accuracy the SPECS done-bar requires). Same module, same MPM
  core — only the smoothing that is wrong for pitch correction is dropped.
- **How to resume / revisit:** if `suite_core::pitch::PitchTracker` later gains a
  configurable/zero hysteresis mode (e.g. `set_hysteresis_cents(0.0)`) or a "retune" preset,
  VOXKEY can switch back to it and delete its private `RetunePitch` wrapper (in
  `plugins/voxkey/src/dsp.rs`); confidence-gating and the retune math are unchanged.

## UI-CORE-FIX — window scaling: no host-resize API from shared code (accepted limitation)
- **Decided 2026-07-07 (PRD §0 in-commit decision; UI-CORE-FIX shipped full, [x]).**
- **What:** `nih_plug_egui` exposes no public way for shared code (suite-core) to *request*
  a host window resize — `EguiState::set_requested_size` is `pub(crate)` and only
  `ResizableWindow`'s corner-drag calls it. So `suite_core::ui::ScaledWindow`'s size menu
  snaps the **zoom of the current window** (75/100/125/150 %); it cannot grow the OS window
  programmatically. To reach a larger physical window at a snapped zoom, the user drags the
  corner (the drag snaps onto the stops). Content is authored at a fixed base logical size
  and `min_size` = base, so nothing clips at 100 %.
- **Why acceptable:** the user complaint was "rescaling is clunky" (layout reflow on
  resize). Uniform egui zoom fixes that — the editor scales as one unit and the effective
  size/zoom persists via `EguiState`. The only lost nicety is a menu that also resizes the
  OS window; the corner-drag path covers it. The exact snap **lock** is session state; the
  effective window size (hence scale) is what persists.
- **How to resume / revisit:** if a future nih-plug rev adds a public
  `EguiState::request_resize` (or `ctx.send_viewport_cmd(InnerSize)` starts working under
  baseview), make the menu buttons call it so a snap grows the window to `base × snap`, and
  drop this note. All logic lives in `suite_core::ui::ScaledWindow` / `size_menu`.

## NERVE listen-layer retrofit — stragglers deferred to a follow-up sweep
- **Decided 2026-07-08 (PRD §1.5 explicit "retrofit the tractable majority and DEFER the
  stragglers" clause; NERVE shipped with a listening MAJORITY).**
- **What:** the per-param "listen" layer (`suite_core::modlisten` + `ui::mod_section`) was wired
  into every plugin that funnels its params through a clean `snapshot() -> Settings` +
  `core.configure(&settings)` block-rate hook (the mechanical 5-line retrofit proven on GRIT).
  A handful of plugins do NOT have that single clean hook and are deferred:
  - **OVERSEER** — one bundle / TWO plugins (Node + Master) with 11 configure sites and its own
    tier-1 override bus already writing effective params; wiring a second modulation source in
    needs per-plugin care (which of Node vs Master, interaction with the override/steal-back
    timestamp logic). Deferred to avoid regressing the tier-1 remote-control contract.
  - **EMBER** — 3 configure calls (multi-stage STFT state machine); no single settings choke point.
  - **SEANCE / VOXFIT / VOXKEY** — no `fn snapshot`; params are read inline across a multi-stage
    voice chain (ShiftEngine etc.), so there is no one place to inject a modulated value cleanly.
  - (any additional per-plugin defers recorded by the retrofit sweep are listed in STATUS.md LOG.)
- **Why acceptable:** the listen layer, the bus, NERVE, and the round-trip done-bar all ship
  green, and a clear majority of plugins listen. The deferred ones are exactly the crates whose
  param→DSP path isn't the uniform `snapshot/configure` shape, so a mechanical retrofit would risk
  correctness there. NERVE's value (one mod source driving the suite) is delivered.
- **How to resume / revisit:** give each deferred plugin a single block-rate settings struct (or,
  for OVERSEER, decide the modulation-vs-override precedence and modulate the Node's effective
  values), then apply the same 5-part GRIT recipe: persisted `mod_routes` field, `modulated_float`
  over the block settings before `configure`, and a `ui::mod_section` call in the editor. Nothing
  in `suite_core` needs to change — the API is complete and tested.

## X-RAY spectrum-publishing — non-publishers deferred
- **Decided 2026-07-08 (PRD §1.5 tractable-majority clause; X-RAY shipped with a publishing
  MAJORITY — 25 plugins + X-RAY itself publish their 32-band output spectrum to the tier-2 bus).**
- **What:** `suite_core::spectrum::{SpectrumTap, SpectrumPublisher}` + a uniform 4-part retrofit
  (field, `init` in `initialize`, `feed`-loop + `publish` at the end of `process`, `release` in
  `Drop`) were wired into: ascend, bandaid, carve, chamber, cleave, drift, ember, flyby, grit,
  halt, impact, murmur, ouroboros, patina, pluck, seance, shapeshift, smudge, snap, swarm, tracer,
  undertow, voxfit, voxkey, wire. These do NOT publish:
  - **OVERSEER** — one bundle exporting TWO plugins (Node + Master) with its own tier-1 override
    bus; a per-plugin spectrum publisher (which struct owns the slot, interaction with the
    override/steal-back logic) needs care. Deferred to avoid regressing the tier-1 contract.
  - **NERVE** — already claims a bus slot as a modulation *source* (kind Nerve, publishing 8 mod
    streams). It is a transparent modulation utility, rarely the thing you analyze; publishing a
    spectrum into the same slot would entangle two publish paths. Deferred.
  - **_template** — the Phase 0 reference crate, kept intentionally minimal.
- **Why acceptable:** X-RAY's value (see the whole session's spectral balance at once) is delivered
  by the majority; the deferred three are exactly the structurally-unusual crates. The two-instance
  done-bar and bit-exact passthrough both ship green.
- **How to resume:** OVERSEER — pick the Node (post-processing) output to tap and give it its own
  `SpectrumPublisher`; NERVE — either publish its input spectrum into its existing slot right after
  `publish_mods`, or claim a second slot. `suite_core::spectrum` needs no change.

## TRIAGE-P2 backlog (SUITE-TRIAGE 2026-07-08 — audited minors, deliberately deferred)

From docs/TRIAGE-2026-07-08.md (P2 tier of the fix program; per-plugin detail in
docs/triage/cluster*.md). None of these block daily use; they are quality-polish items:

- **WIRE**: PLC re-entry click; FEC approximation (true per-bandwidth Opus internal rate);
  `reset()` allocs on the audio thread; latency-rescale on bandwidth change.
- **SWARM**: synced bursts ~+9 dB hot (burst normalisation); mono-sums stereo capture;
  grain-clock transport phase-lock.
- **CARVE**: SC-listen unaligned with the wet path; hop-rate envelope option.
- **CHAMBER**: room-size / material changes crackle (needs param crossfade); wet re-adds
  the direct arrival (ER0 vs dry double-hit).
- **HALT**: tape-stop decay curve; MIDI-note mapping undocumented in the GUI; reverse
  buffer wrap crackle at loop end.
- **FLYBY / DRIFT / SHAPESHIFT orbit**: "sync" is rate-only — no transport phase-lock
  (SEANCE's chopper got the P0 fix; these three follow the same pos_beats pattern).
- **SMUDGE**: scramble makeup gain acts as a pure volume boost at low RANGE.
- **PATINA**: reported latency understated with wow/age up (group delay grows past PDC).
- **ASCEND**: retrigger click at cycle boundary.
- **X-RAY**: stale-slot drop UX (legend entries vanish without notice).
- **BANDAID**: unlinked stereo detectors (optional stereo-link switch).
- **VOXFIT**: dedicated de-ess band tuning (current band fixed 5-9 kHz).
- **EMBER**: freeze-release fade shape (linear now; equal-power would be smoother).
