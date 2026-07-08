# CLEAVE — multi-slicer with a transport-locked step sequencer

*Slice clone (Phase 2b). A 2-bar rolling capture buffer, sliced (grid or transient) and
replayed by a step sequencer locked to the host playhead.*

CLEAVE records the incoming stereo audio into a **2-bar rolling buffer**, cuts it into
**slices**, and re-plays those slices from a **step sequencer** that follows the host
transport. Every step has its own lanes — which slice it plays, how long, forwards or
reversed, transposed, retriggered (rolled), how likely it is to fire, and how loud. It is a
performance/glitch instrument for chopping breaks, stutter-gating pads, and building
jungle/DnB/IDM rhythms out of whatever you feed it.

## Signal flow

```
in ─┬──────────────────────────────────────────────────────────── dry ───────────┐
    │                                                                               ├─(1-mix)/mix─► out
    └─► 2-bar rolling capture ring ──(latch a snapshot at every pattern wrap)──►    │
            playback buffer ──► slice:                                              │
              • Grid      : 1/8, 1/16, 1/32 (fixed musical divisions)               │
              • Transient : spectral flux onset detect + backtrack to zero-cross    │
            ──► step sequencer (transport-locked, 16–64 steps over 2 bars) ──►      │
            grain voices (windowed reads, 5 ms raised-cosine fades) with per step:  │
              slice idx / as-played · gate · reverse · pitch ±12 · roll ×2/3/4 ·    │
              probability · level ──────────────────────────── wet ────────────────┘
```

## How the timing works

- The **pattern is 2 bars long** (matching the capture) and is divided into **Steps**
  (16–64). 32 steps = 16th-notes over 2 bars, 64 = 32nds, 16 = 8ths.
- CLEAVE reads the host **playhead** (bar/beat position, tempo, time-signature) each block and
  advances an internal pattern position sample-accurately, so step onsets land on the grid.
- The source the slicer plays is a **snapshot latched at each pattern boundary** — you hear the
  *previous* 2 bars re-chopped. This is what keeps slice boundaries aligned to the musical grid
  (and is normal buffer-slicer behaviour).
- **Free-run:** when the host transport is **stopped**, CLEAVE free-runs an **internal clock**
  at the host tempo so it keeps slicing for standalone jamming. (`mix = 0` still nulls — see
  below.)

## The null contract (latency)

CLEAVE reports **zero latency**. The **dry path is a direct, zero-latency copy of the input**,
and the output is `out = (1-mix)·dry + mix·wet`. Therefore **`mix = 0` passes the input through
sample-for-sample** and nulls exactly against the dry signal (done-bar 5). The wet (sliced)
path is a *re-timed creative signal* and is intentionally **not** expected to null while the
pattern is active — there is no PDC to apply.

## Slicing modes

- **Grid** — the 2-bar snapshot is cut into equal slices at a fixed division: **1/8** (16
  slices), **1/16** (32), or **1/32** (64). Deterministic; the cleanest lock to the grid.
- **Transient** — onsets are found by **spectral flux** (a streaming STFT over the snapshot,
  summing positive magnitude changes across bins) with an adaptive threshold scaled by
  **Sensitivity** (higher = more slices); each onset is **backtracked to the nearest earlier
  zero crossing** so slices start cleanly. Falls back to a coarse 1/8 grid if too few onsets are
  found. (Re-detection runs once per pattern cycle, at the wrap.)

## The step grid

Per step (persisted with the project, edited on the widget — see below):

| Lane | Range | Meaning |
|---|---|---|
| **On** (active) | on/off | Whether the step fires at all |
| **Slice** | index / **as-played** | A fixed slice number, or "as played" = the slice at this step's time position (chronological) |
| **Gate** | 5–100 % | Grain length as a fraction of the step |
| **Reverse** | on/off | Play the slice time-reversed |
| **Pitch** | ±12 st | Transpose via a resample read (changes read speed) |
| **Roll** | ×1/2/3/4 | Retrigger the step that many times, tiling it |
| **Probability** | 0–100 % | Chance the step fires each cycle (0 = always silent) |
| **Level** | 0–100 % | Step output level |

Grain reads are windowed with a **5 ms raised-cosine fade** in and out, so no slice edge clicks.

## Parameters (automatable)

| Param | Range | Notes |
|---|---|---|
| **Slice Mode** | Transient / Grid | How the buffer is cut |
| **Grid** | 1/8, 1/16, 1/32 | Grid-mode slice division |
| **Sensitivity** | 0–100 % | Transient-mode onset threshold (higher = more slices) |
| **Steps** | 16–64 | Steps dividing the 2-bar pattern |
| **Swing** | 0–75 % | Delays the off-steps (odd steps) by up to half a step |
| **Density** | 0–100 % | The **Randomize** button's busyness |
| **Mix** | 0–100 % | Dry/wet (0 = exact passthrough) |
| **Out** | ±24 dB | Output trim |

> **Automation tradeoff.** The per-step lanes are **persisted host state** (saved with the
> project, edited on the step-grid widget), **not** automatable parameters: 16–64 steps × 8
> lanes is far too many to expose to the host automation tree / validator fuzzer. The global
> knobs above *are* automatable. Factory/user presets store the global params; factory presets
> additionally carry a compact **pattern archetype** that is expanded into the grid on load.

## GUI

- Shared **PresetBar** (factory + user presets, slug `cleave`).
- **Pattern** row: **Randomize** (uses Density), **Clear**, **Fill** (a straight rechop).
- **Step-grid widget**: one column per step; a **LANE** selector (Level / Gate / Pitch / Roll /
  Rev / Prob / On) re-targets what a click or vertical drag edits. Bars show the selected lane's
  value, reversed steps tint blue, roll subdivisions are drawn as ticks, and a white **playhead**
  tracks the current step while the transport rolls.
- Knob rows for the global params (suite knob conventions: drag, Ctrl-drag fine, double-click
  reset, scroll to step, click the value to type).

## Presets

Straight Rechop, Rolls & Ghosts, Reverse Accents, Half-Time Flip, Jungle Scatter (transient +
scatter), Four Flat. Each loads a global setting set **and** a per-step pattern.

## Done-bar tests (offline, `cargo test -p cleave`)

Driven by the shared `suite_core::testsig::FakeTransport` (a sample-accurate synthetic 4/4
playhead) against the pure stereo core; renders write to `renders/CLEAVE/`.

1. **On-grid onsets** — 120 BPM, grid slicing, straight pattern → detected output onsets land on
   the step grid within **±5 ms**.
2. **Reverse** — a reversed step's audio cross-correlates with the **time-reversed** source slice
   at **> 0.9** (and more than with the forward slice).
3. **Roll ×3** — three sub-onsets tile the step within **±2 ms**.
4. **Probability 0** — a prob-0 step is **silent** in its slot (neighbour audibly louder).
5. **`mix = 0` null** — output nulls against the dry input **< −120 dB** on both channels (also
   with the transport stopped).

Plus the universal assertions (finite / ≤ 0 dBFS / non-silent) on every preset render.

## Use in FL Studio

Put **Qeynos CLEAVE** on a track with rhythmic material (a drum loop, a break, a busy pad).
Start the transport — the step grid's playhead runs and you hear the buffer re-chopped. Load
**Straight Rechop** for a clean 1:1 re-slice, **Jungle Scatter** for chopped breaks, **Reverse
Accents** / **Half-Time Flip** for fills. Pick **Slice Mode = Transient** on a break so slices
snap to the hits (raise **Sensitivity** for more). Use the **LANE** buttons to paint gate,
reverse, roll, probability and level per step; **Randomize** (with **Density**) throws a new
pattern; **Swing** adds groove. Pull **Mix** down to blend the chop under the dry signal.

## What It Is

A beat-slicer that records whatever you feed it into a rolling 2-bar buffer, cuts it into
slices (on a musical grid or by transient), and replays them from a transport-locked step
sequencer. Every step chooses its slice, gate, reverse, pitch, roll, probability, and level, so
you can rebuild a break, stutter-gate a pad, or mangle a loop into a new rhythm.

## Signal Flow

```
in ─┬────────────────────────────────────── dry ─────────────────────────┐
    │                                                                      ├─ Mix ─► Out
    └─► 2-bar capture ─► slice (Slice Mode: Grid=1/8·1/16·1/32 | Transient·Sensitivity)
                          ─► step sequencer (Steps, Swing, transport-locked)
                          ─► grain voices · per-step gate/reverse/pitch/roll/prob/level ─ wet ┘
```

## Controls

- **Slice Mode** — how the buffer is cut: **Transient** (onset detection) or **Grid** (fixed divisions).
- **Grid** — grid-mode slice division: 1/8, 1/16, or 1/32.
- **Sensitivity** — transient-mode onset threshold, 0–100 % (higher = more slices).
- **Steps** — number of steps dividing the 2-bar pattern, 16–64.
- **Swing** — delays the off-steps by up to half a step, 0–75 %.
- **Density** — busyness of the **Randomize** button's generated pattern, 0–100 %.
- **Mix** — dry/wet, 0–100 % (0 = exact passthrough).
- **Out** — output trim, −24…+24 dB.

## Recipes

1. **Dark-techno rechop** — load **Warehouse Rechop** (Slice Mode Grid, Grid 1/8, Steps 16, Swing 10 %, Mix 100 %): a clean, driving 1:1 re-slice that locks a loop tight to the grid with a hint of groove.
2. **Atmospheric-dnb rollers** — load **Liquid Rollers** (Slice Mode Transient, Grid 1/16, Steps 32, Swing 8 %, Mix 80 %) on a break: soft transient-snapped rolls that sit under the dry drums. Raise **Sensitivity** for busier chops.
3. **Vocal-rip stutter** — load **Cynthoni Wash** (Slice Mode Transient, Steps 24, Swing 0 %, Mix 45 %) on a vocal, then use the **LANE** editor to paint reverse and roll on a few steps for glitched, half-present phrases.
