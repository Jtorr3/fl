# Qeynos Audio Suite

A suite of CLAP/VST3 audio plugins (built on [nih-plug](https://github.com/robbert-vdh/nih-plug))
plus FL Studio automation tools, built autonomously. See `PRD.md` for the design and
execution playbook, `STATUS.md` for current progress, and `SPECS.md` for per-plugin
DSP specs.

CLAP bundles install to `%LOCALAPPDATA%\Programs\Common\CLAP\Qeynos\` (per-user, no
admin; FL Studio ≥ 2024.1 scans it). VST3 installs alongside only if the optional
admin junction exists (see `CHECKPOINTS.md`).

## Plugins

| Plugin | Type | Summary | Docs |
|---|---|---|---|
| Qeynos Template | Utility | Hello-gain reference (Phase 0 gate); one smoothed gain + peak meter | — |
| GRIT | Distortion | Sidechained distortion — envelope- and waveshape-driven saturation, 4x oversampled, auto-gain, dry/wet | [docs/GRIT.md](docs/GRIT.md) |
| EMBER | Spectral | Spectral fader / temporal smoother — per-bin STFT state machine, 8-band attack/decay curves, phase-vocoder tails, freeze; reports 2048-sample latency | [docs/EMBER.md](docs/EMBER.md) |
| IMPACT | Instrument | Kick drum synth (MIDI) — exponential pitch/amp envelopes, sine/tri body, band-passed click + 3 embedded PCM transients, sub osc, waveshaper drive; phase-continuous declicked retrigger, key-track, Length macro | [docs/IMPACT.md](docs/IMPACT.md) |
| TRACER | Distortion | Pitch-tracking multiband saturation — MPM f0 detection locks a time-varying LR4 crossover tree to the note; per-band waveshaper drive (2x OS), Smart Frequency, constant-color, confidence freeze, MIDI mode | [docs/TRACER.md](docs/TRACER.md) |
| OVERSEER | Mastering | ONE bundle, TWO plugins: **Node** channel strip (EQ, RMS comp, tanh sat, M/S width) + **Master** bus (EQ, 3-band LR4 multiband comp, 2 ms lookahead limiter w/ reported latency, BS.1770 LUFS meter). Master remote-controls Nodes over a same-DLL bus (override badges, local steal-back). Caveat: FL "Make bridged" severs the link | [docs/OVERSEER.md](docs/OVERSEER.md) |
| DRIFT | Filter | Infinity filter — endless Shepard-tone filter sweep. N (2–8) octave-spaced TPT bell filters glide up/down over a log-freq range (free Hz or BPM-synced), wrapping at the edges with a raised-cosine gain window ⇒ seamless endless rise/fall. Shared resonance, depth, stereo phase offset, dry/wet. Zero latency (minimum-phase) | [docs/DRIFT.md](docs/DRIFT.md) |
| WIRE | Lo-Fi | Codec degradation — a real **Opus** round-trip (pure-Rust `opus-rs`, 48 k internal) abused as an effect: bandwidth low-pass + **crunch** (bit/SR reduce) → encode (Bitrate 6–128 kbps, Voice/Music, FEC) → **packet-loss** dropouts (click-free concealment) → decode → re-encoding **regen** generation-loss loop → width/mix. 20 ms frames, latency reported + dry PDC-aligned. Codec runs in the audio thread (~0.3 % RT) | [docs/WIRE.md](docs/WIRE.md) |
| OUROBOROS | Delay | Recursive feedback processor — a delay loop (1 ms–2 s, free/BPM-synced) through a **reorderable 3-slot effect chain** (granular pitch ±12 st, SVF LP/HP/BP, Hilbert freq-shift, saturate, reverse-granule, bit-crush) → in-loop `tanh` **limiter** → DC blocker → **110 % feedback**. Each repeat is re-processed; past unity it self-oscillates but stays bounded. **Freeze** mutes input + pins 100 % feedback for infinite tails. Zero latency; fractional-smoothed delay glides click-free | [docs/OUROBOROS.md](docs/OUROBOROS.md) |
| SWARM | Granular | Mass granulator — a **10 s stereo capture buffer** sprayed into up to **128 concurrent grains** (poisson or tempo-grid scheduler). Per grain, randomised at spawn: position **spray**, pitch **scatter** (±24 st, free/semitone-quantised), **size** 10–500 ms, Tukey window, equal-power **pan**, **reverse** probability; interpolated buffer reads, density-normalised. Sum → optional **+12 st shimmer** feedback (in-loop `tanh` + DC blocker) that re-enters the buffer to bloom. **Freeze** locks the write head for infinite evolving textures. Voice cap 128 (steal oldest); zero latency | [docs/SWARM.md](docs/SWARM.md) |
| SMUDGE | Spectral | Spectral chaos — an STFT (2048/512) runs four per-frame ops in fixed order, each **exactly bypassed at amount 0**: **1 scramble** (permute bins in ±N neighbourhoods, redraw rate), **2 spectral delay** (per-⅓-oct-band frame delays from a tilt curve, bounded feedback), **3 blur** (per-bin temporal magnitude averaging, τ per band + phase-vocoder advance), **4 smear/stretch** (bin-index remap ×0.5–2, energy-normalised). A **chaos** macro slow-S&H-modulates the op params. Reports 2048-sample latency | [docs/SMUDGE.md](docs/SMUDGE.md) |

## Tools (Phase 4)

| Tool | Type | Summary | Docs |
|---|---|---|---|
| W8-VITALGEN | Python + Claude API | Generate/tweak/validate Vital 1.5.x synth presets from natural-language descriptions. Claude fills a constrained parameter subset (osc/filter/env/LFO/FX/macros) merged onto an embedded known-good 1.5.5 base patch; pydantic clamps ranges and rejects bad enums so output always loads. Offline tests run without an API key. Skill: `.claude/skills/vitalgen`. | [docs/W8-VITALGEN.md](docs/W8-VITALGEN.md) |
| W4-SESSION-BOOTSTRAP | Python + FL MCP | One-command FL Studio session bootstrap: a JSON template sets mixer track names + colors, channel→mixer routing, and loop mode via the FL Studio MCP controller (SysEx). Ships TECHNO (dark techno) + DNB (atmospheric dnb) templates; `apply`/`list`, `--dry-run` preview, idempotent, resilient to per-op failures. `tempo` is reported/skipped (no MCP command exists). Skill: `.claude/skills/flsession`. | [docs/W4-SESSION-BOOTSTRAP.md](docs/W4-SESSION-BOOTSTRAP.md) |

## Building

```powershell
powershell -ExecutionPolicy Bypass -File build.ps1 <crate>   # e.g. grit
powershell -ExecutionPolicy Bypass -File build.ps1 -All
```

Each crate builds (release) → tests → bundles `.clap`+`.vst3` → validates with
clap-validator + pluginval (strictness 8) → installs. Requires `tools/bin/mingw64`
(portable MinGW-w64 binutils; gitignored) — `build.ps1` puts it on PATH automatically.

Offline audition renders are written to `renders/<plugin>/*.wav` by the crate tests.
