# Qeynos Audio Suite — Work Order

> Working title only — rename anytime. One VST at a time, each fully finished
> (DSP + GUI + presets + built VST3 + validated) before the next begins.
> Stop the run at any point and everything completed so far is usable in FL Studio.

## Tech stack (decided)

| Choice | What | Why |
|---|---|---|
| Framework | **nih-plug** (Rust) | One-command `cargo` builds, no Steinberg SDK setup, exports VST3 (FL Studio's format), excellent for autonomous iteration |
| Toolchain | **rustup + `x86_64-pc-windows-gnu`** | Zero admin rights needed, self-contained linker. Fallback: MSVC Build Tools via winget if a crate demands it |
| GUI | **nih_plug_egui** | Most reliable to build unattended; custom dark theme shared across the whole suite so it looks like one product family |
| Layout | Cargo **workspace**: `suite-core` (shared DSP + theme + param utils) + one crate per plugin | Every plugin after the first gets faster to build |
| Validation | **pluginval** (headless CLI) on every built VST3 | Catches crashes/param bugs without needing FL open |
| Install target | `Documents\VST3` (add once to FL's plugin search paths) | No UAC prompts during the run |

FL Studio note: FL is VST3-native, all instances of one plugin DLL share a process —
this is what makes the Phase-4 master/client mastering architecture possible via shared state.

---

## Phase 0 — Bootstrap (once)

- Install rustup + stable-gnu toolchain, clone nih-plug template, create workspace
- `suite-core` crate: suite theme (egui), dB/Hz param helpers, common DSP (SVF filter, envelope follower, FFT helpers via `realfft`, soft-clip/waveshaper bank, delay line)
- Build + pluginval a "hello gain" plugin end-to-end to prove the pipeline
- Download pluginval, set up `build.ps1` (build → bundle → copy to Documents\VST3 → validate)

## Phase 1 — Your explicit asks

### 1. GRIT — Sidechained Distortion
Aux (sidechain) input controls distortion of the main signal.
- Modes: **Envelope→Drive** (sidechain loudness pushes drive), **Waveshape-by-sidechain**
  (sidechain signal *is* the transfer curve modulator — ring-mod-into-waveshaper territory),
  **Spectral mode** (distort main only where sidechain has energy)
- Waveshaper bank from suite-core (tube, fold, hard, tape), pre/post filters, mix, auto-gain
- Env follower attack/release, sidechain listen button
- *Also goes first because it proves aux-input plumbing for later plugins.*

### 2. EMBER — Temporal Smoother / Spectral Fader (Fletcher-style)
- FFT engine; current spectrum becomes a "target" the output slowly morphs toward
- **Per-band attack & decay** via ramped "factor bands" across the spectrum (Fletcher's signature)
- Decay keeps generating sound after input stops → infinite bloom/fade-out tails
- Freeze button, "Fitting" control (auto-levels spectral outliers), dry/wet
- Hardest DSP of Phase 1 — scheduled second so the pipeline is proven first

### 3. IMPACT — Kick Drum Synth
- Sine/tri body with **pitch envelope** (start Hz → end Hz, curve control, click-free)
- Amp envelope with adjustable curve + one-knob total length
- **Click layer**: filtered noise burst + selectable transient shapes, own level/decay
- Saturation stage, sub-harmonic octave, hard-clip output, key-tracking toggle
- Preset bank: 808-style, techno rumble, psy, house punch, distorted hardstyle

### 4. TRACER — Pitch-Tracking Smart Saturation (Saturn-style, but the bands follow the note)
- Real-time pitch detection (autocorrelation/YIN with confidence gating + hysteresis,
  so noise/transients don't yank the bands around; glide smoothing between notes)
- **Smart Frequency knob**: instead of a fixed Hz, you pick a *harmonic target* —
  fundamental, 2nd/3rd harmonic, "body", "presence" — and the band's center follows
  the detected pitch in real time. Knob sweeps continuously through harmonic space.
- Saturn-style multiband: up to 4 bands, per-band drive / saturation style (from the
  suite shaper bank) / mix / level — but any crossover can be **pitch-locked** or fixed
- **Constant-color drive**: optional loudness-contour compensation so perceived
  saturation intensity stays even as the note moves up or down the range
- Fixed-mode fallback = normal multiband saturator when pitch confidence is low
  (polyphonic/noisy material), with a MIDI-input mode to drive tracking from notes instead
- Killer use cases: sliding 808s/basslines that keep identical grit up the neck,
  vocal saturation that stays on the fundamental, leads that keep bite without harshness

### 5. OVERSEER — Suite Mastering System (master + per-track clients)
- **OVERSEER Node** on each mixer track: channel strip = 4-band EQ, compressor,
  saturation, stereo width, output trim + loudness metering
- **OVERSEER Master** on the master bus: sees every Node instance (shared in-process
  state), displays per-track meters, remote-controls every Node's strip from one GUI,
  plus its own master chain (EQ → multiband comp → limiter with LUFS metering)
- Honest scoping on the Ozone idea: hosting Ozone *inside* our VST means writing a
  full VST3 host in-plugin — flagged as a **stretch goal**, not in the critical path.
  The Node/Master design gives you the workflow (per-track mastering from one window)
  with our own DSP instead.
- Scheduled last in Phase 1 because it's an architecture, not a plugin — but before
  the catalog clones since it's a named want. **[If you'd rather have it earlier, say so.]**

## Phase 2 — Lese catalog clones (simplest → hardest)

| # | Working name | Lese ref | Spec sketch |
|---|---|---|---|
| 5 | DRIFT | Sweep (free) | Infinity filter — endlessly rising/falling Shepard-tone filter motion, resonance, BPM sync |
| 6 | WIRE | Codec (free) | Real Opus encode/decode in-line (`audiopus` crate): bitrate, packet loss, bandwidth, regen feedback = digital generation loss |
| 7 | OUROBOROS | Recurse | Feedback network: FX chain (pitch/filter/delay/shift) fed back into itself, decay control, limiter in loop |
| 8 | SWARM | Glow | Mass granulator: 100s of grains, size/spray/pitch-scatter, freeze, texture pad maker |
| 9 | SMUDGE | Smear | Spectral chaos: bin scrambling, spectral delay per band, blur/stretch |
| 10 | MURMUR | Hikari | Stochastic reverb: randomized IR generation per note/trigger, never the same tail twice |
| 11 | FLYBY | Transfer | Doppler spatializer: draw a flight path, pitch/pan/distance/air-absorption follow it |
| 12 | CLEAVE | Slice | Multi-slicer: rhythmic gate/repeat/reverse/pitch per slice, pattern sequencer |
| 13 | PLUCK | Strum | Karplus-Strong strummer: turns input/MIDI into plucked-string textures |
| 14 | SHAPESHIFT | Teuri | Morphing distortion: XY-morph between 4 distortion algorithms (shares GRIT's shaper bank) |
| 15 | CHAMBER | Eigen | Space simulator: geometric room model, movable source/listener (hardest — last) |
| — | (skip) | Frahm | Gestural processor is mostly a MIDI-controller UX play; low value cloned. Revisit on request. |

## Phase 3 — My additional ideas (pick your favorites)

1. **CARVE** — spectral ducker: sidechain input carves matching frequencies out of the main
   track (Trackspacer-style). Pairs perfectly with GRIT; shares its spectral engine.
2. **NERVE** — suite-wide modulation bus: envelope followers / LFOs / random in one plugin,
   broadcast to *any* other suite plugin's parameters via the shared-state layer.
   (This is the thing a plugin suite can do that single plugins can't.)
3. **HALT** — performance tape-stop / stutter / reverse buffer FX with momentary triggers.
4. **BANDAID** — per-band transient designer (attack/sustain per frequency band).
5. **PATINA** — lo-fi character: wow/flutter, vinyl noise floor keyed to input level,
   dropout, azimuth blur — the analog counterpart to WIRE's digital degradation.
6. **X-RAY** — shared analyzer: every suite plugin reports its spectrum to one overlay
   window so you see the whole mix's frequency interaction in one place.
7. **CHORALE** — resonator bank / sympathetic strings tuned to MIDI or detected key.

### Tailored to KAS:ST (dark melodic techno) + Cynthoni (atmospheric dnb/breakcore):

8. **UNDERTOW** — kick-to-rumble generator: feed it your kick, it renders the rumble bus
   in one plugin (reverb → saturation → lowpass → self-ducked against the dry kick).
   The hard/melodic techno low-end staple without the 4-track routing ritual.
   Natural companion to IMPACT.
9. **SEANCE** — ethereal vocal machine: pitch/formant shift + auto-chop + shimmer/drowned
   reverb + lo-fi washing in one chain. The Sewerslvt/Cynthoni vocal ghost sound as a
   single insert instead of a 6-plugin stack.
10. **ASCEND** — tension generator: keyed noise/tonal risers, downlifters, impacts,
    synced to project key + bar countdown. Melodic techno transitions on demand.

## Phase 4 — FL Studio workflow automations (no VST needed)

Leverages two things you already have: the **FL Studio MCP server** (this machine) and
FL's **piano roll scripting** (`.pyscript`, like your ComposeWithLLM script).

| # | Automation | Surface | What it does |
|---|---|---|---|
| W1 | Rumble bassline generator | .pyscript | Generates offbeat/rolling rumble bass note patterns that lock around your kick placement, in key |
| W2 | Break-chop patterns | .pyscript | Writes jungle-style chop/re-order patterns into piano roll for a sliced break (ghost hits, rolls, reverses) |
| W3 | Melodic techno progression tool | .pyscript | In-key dark progression + hypnotic arp generator (minor/phrygian presets, Afterlife-style suspensions) |
| W4 | Session bootstrap | MCP | One command: name + color + route mixer tracks from a template (KICK/RUMBLE/BASS/PERC/ATMOS/VOX/FX), set tempo & loop mode |
| W5 | Project janitor | MCP | Auto-name/color unnamed channels & mixer tracks by content type, normalize levels |
| W6 | Sample librarian | plain Python | Watch folder → detect key/BPM → rename & sort sample packs into a browsable structure (runs outside FL entirely) |
| W7 | Reference gap report | plain Python | Drop a reference track + your mix export → spectrum/LUFS/stereo/kick-tuning comparison report |

| W8 | **Claude-powered Vital preset generator** | plain Python + Claude API | Describe a sound in text ("hollow reese, slow drift, dark") → writes a valid `.vital` preset straight into Vital's user preset folder |

*W4/W5 are bounded by MCP limitations (can't load plugins or create patterns), but
naming/coloring/routing/tempo are exactly what it CAN do. W1–W3 persist notes properly
because piano roll scripts run inside FL.*

### W8 detail — Vital preset generator
- `.vital` files are plain JSON, and Vital is open source (github.com/mtytel/vital), so
  the full parameter schema is verifiable — ideal for generation
- Pipeline: sound description → Claude fills a validated preset schema (wavetable
  choice, osc tuning, filter routing, env/LFO shapes, FX chain) → schema-check →
  write to user presets → optionally iterate ("darker", "more movement")
- Ships as both a CLI (`vitalgen "cavernous mid bass"`) and a Claude Code skill so it
  works right in a session; batch mode generates a whole themed bank at once
- Style presets seeded from your taste profile: KAS:ST-style dark leads/plucked arps,
  Cynthoni-style grief pads and reeses
- **Serum 2: stretch goal.** Its preset format is a binary container, not documented
  JSON like Vital — needs a reverse-engineering pass first. Path of least resistance
  if wanted later: generate in Vital, or drive Serum via its own preset morphing.

## Definition of done (every plugin)

1. `cargo build --release` clean, bundled to `.vst3`
2. pluginval strictness level 8 pass
3. Copied to `Documents\VST3`, README entry with param reference
4. 5+ factory presets
5. Suite-consistent egui GUI (shared theme, resizable)
6. git commit per plugin — the repo is the changelog

## Open items needing your input (answer whenever)

- Suite name ("Qeynos Audio Suite" is a placeholder from your domain)
- OVERSEER priority: before or after the catalog clones?
- Phase 3 picks: which of the 7 ideas make the cut?
- GUI taste: minimal-dark (Lese-ish) is the default — any reference aesthetic you prefer?
