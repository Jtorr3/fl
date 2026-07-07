# PRD — Qeynos Audio Suite
### Technical design + execution playbook (this document is the source of truth during the build)

**Audience: me (Claude), executing autonomously.** WORK_ORDER.md is the *what*; this is
the *how*. Every plugin section below contains enough backend design to implement without
re-deriving decisions. When reality contradicts this doc, fix the doc in the same commit.

---

## 1. Execution protocol (the loop I follow, one plugin at a time)

```
┌─> 1. Re-read this PRD section for the plugin
│   2. `cargo new` crate from plugins/_template, register in workspace
│   3. DSP core first, GUI last:
│      a. implement engine in pure-DSP module (no plugin glue)
│      b. offline test harness (cargo test): render known signals through it,
│         assert on RMS/spectra/envelope shape — catches math bugs pre-GUI
│   4. Param layer: nih-plug params w/ smoothers, sensible ranges, units
│   5. GUI: suite-core theme + shared widgets; resizable; no custom art needed
│   6. Presets: ≥5 factory presets as embedded JSON
│   7. Build gate:  build.ps1 <crate>  →  release build → bundle .vst3
│                   → pluginval --strictness-level 8 → copy to Documents\VST3
│   8. Audible sanity: standalone target renders test WAVs to renders/<plugin>/
│   9. Docs: README table row + params reference in docs/<plugin>.md
│  10. git commit (one plugin = one commit minimum) + push
│  11. Tick checklist at bottom of this file, update memory if decisions changed
└── 12. Next plugin. NEVER two plugins in flight at once.
```

**Failure rules:** if a plugin is blocked >90 min on one issue, descope the feature
(note it in `docs/DEFERRED.md`), ship the rest, move on. A working suite with gaps
beats a stalled perfect plugin. If the *toolchain* breaks, fix that before anything.

**Checkpointing:** after each phase, write a short status report the user can read
cold: what shipped, what was descoped, what to test in FL.

---

## 2. Repo & infrastructure

```
qeynos-vst-suite/            (git repo, GitHub: private)
├── PRD.md  WORK_ORDER.md  README.md  CHANGELOG.md
├── Cargo.toml               (workspace)
├── build.ps1                (build → bundle → pluginval → install one crate or --all)
├── suite-core/              (shared lib crate)
│   ├── dsp/        filters (TPT SVF, biquad, LR4 crossover), delay lines, env followers,
│   │               waveshaper bank, oversampling (2x/4x halfband), STFT engine (realfft,
│   │               OLA, Hann), pitch detector (MPM), grain engine, FDN reverb core,
│   │               LFO/noise, LUFS meter (K-weighting), limiter core
│   ├── ui/         egui theme (minimal-dark), knob/slider/xy-pad/factor-band widgets,
│   │               spectrum + meter displays
│   ├── bus/        shared-state layer (see §3)
│   └── presets/    preset save/load (serde JSON) + embed macro
├── plugins/
│   ├── _template/  hello-gain reference crate (copied for each new plugin)
│   └── <one crate per plugin>
├── tools/                    (Phase 4, Python — uv-managed)
├── pyscripts/                 (Phase 4, FL piano roll scripts)
├── renders/                   (test WAVs, gitignored)
└── docs/                      (per-plugin param references, DEFERRED.md)
```

- **Toolchain:** rustup, `stable-x86_64-pc-windows-gnu` (no admin). If any crate
  hard-requires MSVC, fall back: `winget install Microsoft.VisualStudio.2022.BuildTools`
  (needs elevation — pause and tell user).
- **nih-plug** pinned to a git rev in workspace Cargo.toml. VST3 export only
  (FL Studio has no CLAP). GPLv3 applies to VST3 export — fine, personal project.
- **pluginval:** download Windows binary into `tools/bin/`, run headless in build.ps1.
- **Install dir:** `%USERPROFILE%\Documents\VST3` — user adds it once in FL:
  Options → File Settings → VST plugin extra search folder.
- **GitHub:** `gh repo create qeynos-vst-suite --private`, push after every plugin.
  (gh is authed as Jtorr3.)
- **Python tooling (Phase 4):** install `uv`; each tool gets inline script deps
  (PEP 723) so nothing needs a venv ritual.

---

## 3. Cross-plugin architecture — "the Bus"

Three plugins need instances to talk to each other: OVERSEER (Master ⇄ Nodes),
NERVE (mod sources → any plugin), X-RAY (all plugins → one analyzer).

**Problem:** Rust `static`s are per-DLL. Two different .vst3 files never share memory
even in the same process, and FL can bridge plugins out of process entirely.

**Design (two tiers):**
1. **Same-DLL tier** — OVERSEER Node + Master are exported from *one* library
   (`nih_export_vst3!(Node, Master)` — one .vst3, two plugin classes). They share a
   `static BUS: Mutex<Registry>` directly. Fast path, zero IPC.
2. **Cross-DLL tier** — named shared memory (`memmap2` over a file in `%TEMP%\qeynos-bus`,
   fixed-layout slabs). Every suite plugin maps it on init and claims an instance slot:
   `{ instance_id, plugin_kind, user_label, 32-band spectrum, peak/RMS/LUFS-M,
   8 mod-signal floats, param-override area, heartbeat_counter }`.
   Writers bump a per-slot seqlock; readers retry on odd counters. Heartbeat timestamps
   (block counter, not wall clock) let readers GC dead slots. No locks held across
   the audio callback; all fixed-size, no allocation in process().

**Honest constraints, documented in READMEs:**
- Bus signals have block-size granularity (~1–10 ms) — fine for meters/ducking/mod,
  not sample-accurate sync.
- Master-written param overrides bypass host automation/undo (they write the Node's
  own param space through the bus, Node applies at block start). Overrides are
  visually flagged in the Node GUI.

**Suite-wide conventions (apply to every plugin, don't repeat below):**
- All GUI-visible params smoothed (nih-plug smoothers, ~10–50 ms) — no zipper noise.
- Any nonlinear stage runs 2–4x oversampled (halfband polyphase from suite-core).
- Any FFT/lookahead plugin reports latency via nih-plug's latency API.
- DC blocker + output soft-clip guard on every feedback plugin.
- Bypass = crossfaded, click-free. Sample rates 44.1–192k supported.

---

## 4. Plugin technical specs

### Phase 0 — `_template` (hello-gain)
Proves: workspace builds, egui window opens, param automates in FL, pluginval passes,
build.ps1 end-to-end. One gain knob + meter. **This is the pipeline test, keep it forever.**

---

### 1. GRIT — sidechained distortion
```
main in ─ trim ─ pre-filter(SVF HP/LP) ─┐
                                        ├─ DISTORTION CORE ─ post-filter ─ auto-gain ─ mix ─ out
sidechain in ─ SC filter ─ env follower ┘        ▲
              (focus band)  (att/rel)            └ mode selects how SC drives the core
```
- **Mode A: Env→Drive.** drive_dB(t) = base + depth × env(t)^curve. Oversampled 4x.
- **Mode B: Waveshape-by-SC.** dynamic bias/fold: y = shape(x + bias·sc(t)) with
  shape from suite bank (tube/tape/fold/hard). Ring-mod-into-waveshaper territory.
- **Mode C: Spectral.** STFT both signals; per-bin drive ∝ smoothed SC bin magnitude
  (so main distorts only where SC has energy). Reports FFT latency.
- Auto-gain: match post RMS to pre RMS over 300 ms window, ±12 dB clamp.
- Params: mode, drive, depth, curve, attack, release, SC focus (freq+width), SC listen,
  shape select, pre/post filter, mix, out. Presets: kick-driven bass grit, vocal spectral
  crush, pad ring-fold, drum bus pump-drive, techno rumble driver.
- **Risk:** none serious — this is deliberately the plumbing-prover.

### 2. EMBER — spectral fader / temporal smoother (Fletcher-style)
```
in ─ STFT(2048, hop 512, Hann) ─ per-bin state machine ─ fitting ─ iSTFT/OLA ─ mix ─ out
                                      ▲
              factor-band curves: attack(f), decay(f)  (log-freq spline, UI-editable)
```
- Per bin k: `state[k] += coef(in>state ? atk(f_k) : dec(f_k)) × (in_mag[k] − state[k])`
  Coefs from ms values via `1 − exp(−hopTime/τ)`. Decay τ up to 60 s ⇒ blooms/fades
  continue after input stops.
- **Phase strategy:** while input is active (bin mag above gate) use input phase;
  when generating tail, advance phase by bin frequency (phase-vocoder integration)
  so tails stay tonal, not metallic.
- **Fitting:** compute spectral envelope (moving average over ~1/3 oct of bins); blend
  each bin toward envelope ⇒ levels out spectral outliers (Fletcher's "fitting").
- Freeze = force attack+decay τ→∞. Reports 2048-sample latency.
- Params: factor bands (2 spline curves), fitting, freeze, gate, tail gain, mix.
- **Risk (highest in Phase 1):** phase handling quality. Fallback: magnitude-only with
  random phase + heavy OLA works acceptably for pads/ambience — descope path exists.

### 3. IMPACT — kick synth (MIDI instrument)
```
note-on ─ pitch env(f_start→f_end, curve) ─ sine/tri osc ─┐
        ─ click layer: noise burst → BP/HP + transient PCM ├─ mix ─ drive ─ amp env ─ clip ─ out
        ─ sub osc (f_end × ratio)                          ┘
```
- Mono voice, phase-continuous retrigger + 1.5 ms declick ramp. Pitch env exponential:
  `f(t) = f_end + (f_start−f_end)·e^(−t/τ_p)` with curve morphing τ shape.
- Length macro scales amp-env decay + pitch τ together (one-knob short↔rumble).
- Key-track toggle: MIDI note sets f_end (A1 = 55 Hz etc.), so kicks sit in key.
- Click layer: white noise → SVF (BP 1–8 kHz) with own 5–50 ms decay + 3 baked PCM
  transients (generated offline, embedded). Saturation pre-amp-env so drive shapes body.
- Presets: 808 long, techno rumble kick, psy snap, house punch, hardstyle distorted.

### 4. TRACER — pitch-tracking multiband saturation
```
in ─┬─ mono sum → decimate to ~12 kHz → MPM pitch det (1024) → confidence gate
    │            → median(5) → hysteresis (±35 cents) → slew (Hz/ms) → f0
    └─ LR4 crossover tree (cutoffs = harmonic multiples of f0, coef-interp @ control rate)
         band1..4: [drive → shaper(bank) → 2x OS → mix/level] → sum → out
```
- **Smart Frequency knob:** continuous harmonic-space position; crossover center =
  f0 × 2^(knob). Detents at fundamental/2nd/3rd/"body"(×4–6)/"presence"(×8–12).
  Each crossover independently pitch-locked or fixed-Hz.
- Confidence < 0.6 ⇒ crossovers freeze at last-confident values (graceful fixed-band
  fallback for polyphonic input). MIDI mode replaces detector with incoming notes.
- **Constant-color drive:** per-band drive scaled by inverse equal-loudness weight at
  band center (approx ISO 226 curve baked as lookup) — perceived grit stays even as
  notes move.
- Time-varying LR4: recompute coefs per 32-sample control block, linear-interp states;
  crossfade filter pairs if instability detected (guard).
- **Risk:** detector jitter on real material — mitigations above; test with sliding 808
  render + vocal stem before calling done.

### 5. OVERSEER — mastering system (one .vst3, two plugins)
```
NODE (per track):  in → meter → 4-band EQ → comp → sat → M/S width → trim → meter → out
                                └── slot in same-DLL BUS: meters, params, override area
MASTER (master bus): own chain: EQ → 3-band comp (LR4) → lookahead limiter → LUFS meter
                     GUI: grid of all live Nodes (name, meters, strip controls)
                     writes param overrides into Node slots via BUS
```
- Node chain DSP all from suite-core: biquad EQ (LS/2×bell/HS), feed-forward comp
  (RMS det, soft knee), tanh sat, M/S width, LUFS-M meter.
- Master limiter: 2 ms lookahead, smoothed gain envelope, true-peak-ish (4x OS meter),
  reports latency. Integrated LUFS with reset.
- Node GUI shows an "override" badge when Master holds a param; local touch steals back.
- Naming: instance label param (user types "KICK") — VST3 can't read FL's track name.
- **Stretch (explicitly deferred):** hosting Ozone inside = full VST3 host; revisit only
  if suite completes early. DEFERRED.md gets the design sketch.

---

### Phase 2 — Lese clones

### 6. DRIFT — infinity filter (Sweep)
Shepard-tone filter illusion: N=6 peak filters spaced one octave apart on log-freq axis,
all gliding up (or down) at Rate; each wraps at range edge; per-filter gain follows a
raised-cosine window over log-freq so filters fade in at bottom, out at top ⇒ endless
rise. Params: rate (Hz/BPM sync), direction, resonance, range lo/hi, peaks count,
stereo phase offset, mix. Cheap, pure biquads. **First Phase 2 plugin on purpose.**

### 7. WIRE — codec degradation (Codec)
```
in ─ resample 48k ─ [crunch: bit/SR reduce] ─ Opus encode → (loss sim: drop/PLC) → decode ─ regen loop ─ out
```
- `audiopus` (libopus bindings). Frame 20 ms ⇒ latency reported (~40 ms w/ buffer).
- Params: bitrate 6–128 kbps, packet loss %, bandwidth (NB/MB/WB/SWB/FB), voice/music
  mode (LPC vs MDCT character), FEC on/off, crunch, regen (delay + re-encode feedback
  = generation loss), width. Codec runs in audio thread (opus is realtime-safe at these
  sizes); if not, ring-buffer worker thread pattern is the fallback.
- **Risk:** audiopus build on windows-gnu — if libopus won't link, vendor `libopus-sys`
  with cmake-less build or use pure-Rust `opus-embedded`. Test in first hour.

### 8. OUROBOROS — recursive processor (Recurse)
```
in ─ + ─ delay(1 ms–2 s, sync) ─ [slot A ─ slot B ─ slot C] ─ limiter ─ DC block ─┬─ out
     ▲                                                                            │
     └───────────────────────────── × feedback (0–110%) ─────────────────────────┘
```
Slots choose from: pitch shift (granular ±12 st), SVF filter, freq shifter (Hilbert pair),
saturator, reverse chunk, bit crush. Drag-to-reorder. Freeze = feedback 100% + input mute.
In-loop limiter keeps 110% feedback usable. Params: per-slot amount, delay, feedback,
decay-scale, freeze, mix.

### 9. SWARM — mass granulator (Glow)
Circular capture buffer 10 s. Scheduler: density 1–500 grains/s (poisson or grid-sync);
per-grain randomized: position spray, pitch scatter (free or semitone-quantized),
size 10–500 ms, Tukey envelope, pan, reverse probability. Sum → optional +12 st shimmer
feedback send back into buffer. Freeze locks write head. Voice cap 128 grains,
steal oldest. Params: density, size, spray, scatter, quantize, reverse %, shimmer,
freeze, width, mix.

### 10. SMUDGE — spectral chaos (Smear)
STFT 2048. Per-frame ops, each with amount knob: **scramble** (permute bins within
neighborhoods of ±N bins), **spectral delay** (per-1/3-oct-band delay on bin frames w/
feedback), **blur** (temporal magnitude averaging, τ per band), **smear/stretch**
(bin index remap ×0.5–2). Chaos macro randomizes op parameters via slow S&H. Phase:
keep input phase for scramble/delay; blur uses vocoder phase advance (EMBER's engine
reused). Latency reported.

### 11. MURMUR — stochastic reverb (Hikari)
FDN 8×8 (Householder matrix) from suite-core, but **re-randomized per onset**: onset
detector (spectral flux) triggers new random draw of delay lengths (within size range),
diffusion allpass coefs, per-line damping color ⇒ every hit gets a different room.
Crossfade old/new FDN state over 50 ms to avoid clicks (two FDN instances, ping-pong).
Params: size, decay, color (damping tilt), randomness amount, onset sensitivity,
manual re-roll button, freeze, mix.

### 12. FLYBY — doppler spatializer (Transfer)
Path editor: bezier loop on XY pad (listener at origin), traversal synced to BPM or Hz.
Per block compute source pos → distance r, azimuth θ:
- Doppler: fractional-delay read `delay = r/c`, interpolated (Catmull-Rom), rate-clamped
- Distance: gain 1/max(r,r₀), air absorption = one-pole LP with cutoff ∝ 1/r
- Pan: equal-power from θ + optional micro-ITD (≤0.6 ms L/R offset)
Params: path (4–8 nodes), speed/sync, size scale, doppler amount, air, width, mix.

### 13. CLEAVE — multi slicer (Slice)
2-bar rolling capture buffer, sliced by transient detect (spectral flux + backtrack)
or grid (1/8–1/32). Step sequencer 16–64 steps, per step: slice index (or "as-played"),
gate len, reverse, pitch ±12, roll ×2/3/4, probability, level. Playback = grain-windowed
slice reads (no clicks). Host-transport locked. Params: slice mode/sensitivity, pattern
lane editing, swing, mix. Pattern randomizer button w/ density control.

### 14. PLUCK — strummer (Strum)
Karplus-Strong core: delay line + one-pole damp LP + allpass fine-tune in loop.
N=6 strings tuned to chord (chord select or MIDI-held notes or key-detect via
chromagram). Trigger: input onsets or MIDI. **Strum** = staggered excitation across
strings (5–80 ms stride, up/down/alternate). Exciter = burst of input audio (500 samp
window) ⇒ input timbre colors the pluck. Small body IR (embedded, 2048-tap conv).
Params: tuning/chord, damp, decay, strum time/direction, body, velocity→brightness, mix.

### 15. SHAPESHIFT — morphing distortion (Teuri)
```
in ─ pre-gain ─ 4x OS ─ [shaper A][B][C][D] ─ bilinear XY blend ─ post LP ─ mix ─ out
```
Corners pick any shaper from suite bank (tube, tape, diode, fold, sine-fold, digital
hard, asym, chebyshev). XY pad automatable + built-in orbit LFO (rate, shape, radius).
Per-corner pre-gain trim. Output y = Σ wᵢ(x_pos,y_pos)·shaperᵢ(g·x). Reuses GRIT bank —
mostly a UI + morph-weights project.

### 16. CHAMBER — space simulator (Eigen) *(hardest — last clone)*
Shoebox image-source model: room W×D×H, source + listener draggable on floor plan.
Early reflections to order 3 (≈ 60 images): per image → delay r/c, gain (1/r ×
material absorption^bounces), HF damp one-pole, stereo pan by arrival azimuth.
Late field: FDN (suite-core) with RT60 derived from Sabine eq (volume, absorption)
crossfaded in after ER window. Moving source = interpolated delay updates (doppler for
free, clamped). Params: room dims, materials (4 presets/wall-group), src/listener pos,
ER/late balance, distance, mix. **Descope path:** cap at order 2 (~25 images) if CPU-heavy.

---

### Phase 3 — idea pool

### 17. CARVE — spectral ducker
STFT main + sidechain (GRIT mode-C plumbing reused). Per 1/3-oct band: gain reduction
= soft-knee function of SC band energy vs threshold, with attack/release per band-group,
tilt control (duck lows vs highs harder), max depth. iSTFT. Latency reported.
Params: amount, threshold, tilt, att/rel, sensitivity curve, listen-Δ, mix.

### 18. NERVE — suite modulation bus
GUI-only-ish plugin: 4 LFOs (sync/free, 8 shapes), 2 env followers (its own input),
2 random S&H, 4 macro knobs → publishes 8 float streams to cross-DLL bus (§3).
Every other suite plugin gets a per-param "listen" menu (source × depth × curve),
applied at block rate pre-smoother — shipped as suite-core feature, retrofit = one
line per param. Depends on bus tier 2; build after OVERSEER proves tier 1.

### 19. HALT — performance buffer FX
4-bar circular buffer. Momentary modes (MIDI-note or param-button triggered, all
declick-crossfaded 5 ms): **tape stop** (rate ramps 1→0, curve + duration synced),
**stutter** (loop last 1/4–1/64 with optional decay/pitch step per repeat),
**reverse** (read backward from trigger point), **half-speed**. Retrigger quantize
to grid. Params per mode + global quantize, mix.

### 20. BANDAID — multiband transient designer
LR4 3-band split. Per band: transient detect = fast env (1 ms) − slow env (50 ms);
positive diff ⇒ attack region, apply attack gain (±12 dB); tail region ⇒ sustain gain.
Smooth gain application (5 ms). Params: 2 crossover freqs, per-band attack/sustain,
global output, listen-solo per band.

### 21. PATINA — analog lo-fi character
```
in ─ wow/flutter (frac delay ← LFO stack: 0.4 Hz wow + 8 Hz flutter + random walk)
   ─ saturation ─ head-bump EQ (LS boost 60–120 Hz) ─ azimuth (L/R HF phase skew)
   ─ dropouts (random gain dips, depth/rate) ─ + noise layer (hiss + hum + crackle,
     keyed to input env so it breathes) ─ age macro drives all ─ mix ─ out
```
Params: wow, flutter, sat, bump, azimuth, dropout, noise type/level/key-amount, age, mix.

### 22. X-RAY — shared analyzer
Pure bus consumer: renders every live suite instance's published 32-band spectrum as
colored overlay curves + peak/RMS list; hover to highlight, click to solo-dim others
(visual only). Needs every plugin publishing spectra — publishing is in suite-core's
plugin wrapper, so it's free once bus tier 2 lands. Trivial DSP; it's a GUI project.

### 23. CHORALE — resonator bank
12–24 waveguide resonators (KS loops @ high feedback, damped) tuned to: held MIDI notes,
selected scale/chord, or chromagram key-detect. Input audio excites all resonators
(gain per resonator = input band energy at its pitch, so it "sings" sympathetically).
Params: tuning source, decay, damp, spread (detune cents), stereo alternate, wet solo, mix.

### 24. UNDERTOW — kick-to-rumble generator
```
in(kick) ─┬───────────────────────────────────────────┬─ dry ─ + ─ out
          └ transient strip (env-gated: keep tail) ─ sat ─ FDN reverb (small/dark)
            ─ LP 90–250 Hz ─ resonant tune peak (key-lockable) ─ ducker (keyed by
            dry kick env, att 1 ms rel 80–300 ms) ─ rumble gain ┘
```
Ducker ensures rumble breathes *around* the kick (the classic rumble-bus trick in one
insert). Tune control locks the LP resonance/peak to project key note. Params: strip,
drive, reverb size/decay, LP freq, tune note, duck depth/release, rumble level, width.

### 25. SEANCE — ethereal vocal machine
```
in ─ pitch shift (±12 st, formant-preserving: PV + spectral-envelope lift) ─ formant knob
   ─ chopper (synced gate patterns / random, smooth edges) ─ shimmer verb (FDN with
     +12 st pitch in feedback path) ─ wash (LP + wow from PATINA core) ─ ducker (keyed
     to dry) ─ macro layer ─ mix ─ out
```
Macros: **Ghost** (pitch+formant+wash), **Drown** (verb size/wet/duck), **Chop**
(pattern density). Phase-vocoder pitch shift w/ envelope preservation from EMBER/SMUDGE
engines. Presets: grief pad vox, drowned lead vox, whisper choir, formant ghost.

### 26. ASCEND — tension generator (MIDI/transport instrument)
Reads host transport: bars-until-target (user sets drop bar or "next 8/16/32 boundary").
Sources: filtered noise + tonal osc stack (root+5th of set key). Over countdown window:
filter sweep up, pitch rise (0–24 st), width bloom, volume curve — all from one
tension envelope with curve control. At target bar: optional impact (embedded PCM) +
auto-cut. Downlifter mode = reversed envelope after the drop. Params: key, length,
curve, noise/tone balance, rise amount, impact, sync target.

---

## 5. Phase 4 — FL workflow automations (technical notes)

| # | Deliverable | Stack | Design notes |
|---|---|---|---|
| W1 | `pyscripts/RumbleBassline.pyscript` | FL `flpianoroll` API | Inputs: key, density, ghost-note velocity range. Emits offbeat/rolling 16th patterns avoiding beat-1 kick collisions; humanize (±vel, ±5 ticks) |
| W2 | `pyscripts/BreakChop.pyscript` | flpianoroll | Operates on selected slice-notes: permute, roll insert, reverse flags via slice-note properties, probability per step |
| W3 | `pyscripts/DarkProgression.pyscript` | flpianoroll | Minor/phrygian/harmonic-minor pools, voice-leading rules, hypnotic arp emitter (up/down/rand, octave span), tension presets |
| W4 | `tools/session_bootstrap.py` | FL MCP server | Template JSON → fl_set_track_name/color, fl_route_channel_to_mixer, tempo, loop mode. Templates: TECHNO, DNB |
| W5 | `tools/project_janitor.py` | FL MCP | Heuristic naming from channel plugin/sample names; color map by category; report of changes |
| W6 | `tools/sample_librarian.py` | Python: librosa/soundfile | Watch/scan dir → BPM (onset autocorr) + key (chromagram) → rename `{key}_{bpm}_{name}` + sort folders. Dry-run mode first, never destructive without `--apply` |
| W7 | `tools/reference_gap.py` | Python: pyloudnorm/numpy | Ref vs mix: LUFS-I, 1/3-oct spectrum diff plot, stereo width by band, kick fundamental detect + tuning suggestion → HTML report |
| W8 | `tools/vitalgen/` CLI + skill | Python + Claude API | See below |

**W8 — Vital preset generator:**
1. Extract param schema from Vital source (github.com/mtytel/vital, `.vital` = JSON:
   `settings` dict of param→float + wavetable/LFO/modulation lists). Build a pydantic
   schema with real ranges/enums.
2. Prompt pipeline: sound description + taste profile → Claude (claude-fable-5, tool-use
   with schema) → JSON → validate → clamp ranges → write to
   `Documents\Vital\User\<bank>\<name>.vital`.
3. Iterate mode (`--tweak "darker"`) rereads last preset, asks for a delta.
   Batch mode (`--bank "grief pads" -n 12`).
4. Ship as CLI + `.claude/skills/vitalgen/SKILL.md` so it works mid-session.
5. **Serum 2 = DEFERRED.md** (binary container format, needs RE pass).

---

## 6. Build order & progress checklist

Order within phases is dependency-aware: GRIT before TRACER/SHAPESHIFT/CARVE (shaper
bank, SC plumbing); EMBER before SMUDGE/SEANCE (STFT engine); OVERSEER before
NERVE/X-RAY (bus tiers); IMPACT before UNDERTOW (kick context); MURMUR's FDN before
CHAMBER/UNDERTOW/SEANCE reverbs. Phase 4 can interleave if a break from Rust is useful —
W8 and W4 are the highest-value quick wins.

- [ ] Phase 0: toolchain, workspace, _template, build.ps1, pluginval green
- [ ] 1 GRIT   - [ ] 2 EMBER   - [ ] 3 IMPACT   - [ ] 4 TRACER   - [ ] 5 OVERSEER
- [ ] 6 DRIFT  - [ ] 7 WIRE    - [ ] 8 OUROBOROS - [ ] 9 SWARM   - [ ] 10 SMUDGE
- [ ] 11 MURMUR - [ ] 12 FLYBY - [ ] 13 CLEAVE  - [ ] 14 PLUCK   - [ ] 15 SHAPESHIFT
- [ ] 16 CHAMBER
- [ ] 17 CARVE - [ ] 18 NERVE  - [ ] 19 HALT    - [ ] 20 BANDAID - [ ] 21 PATINA
- [ ] 22 X-RAY - [ ] 23 CHORALE - [ ] 24 UNDERTOW - [ ] 25 SEANCE - [ ] 26 ASCEND
- [ ] W1 - [ ] W2 - [ ] W3 - [ ] W4 - [ ] W5 - [ ] W6 - [ ] W7 - [ ] W8
