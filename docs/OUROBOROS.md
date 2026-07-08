# OUROBOROS — recursive feedback processor (Recurse clone)

A feedback delay whose loop is re-processed on every pass. Audio enters, sums with the
feedback, runs through a **delay line** (1 ms–2 s, free or BPM-synced), then a
**reorderable chain of three effect slots**, an **in-loop soft limiter** and a **DC
blocker** before the output tap feeds back at up to **110 %**. Because each repeat is
mutated by the slot chain, the sound evolves as it recirculates — pitching up an octave
per echo, closing a filter, shifting into inharmonic clangor, reversing, crushing, or
saturating into stable self-oscillation.

```
 in ─×gate─ + ─ delay(1 ms–2 s, free/sync) ─ [slot A → slot B → slot C] ─ limiter ─ DC ─┬─ out tap
            ▲                                       (order selectable)                    │
            └───────────────────────── × feedback (0–110 %, ×decay) ───────────────────────┘
```

## Loop conventions (stability)

- **In-loop soft limiter** — a `tanh` at unity threshold, placed *after* the slot chain
  (which can boost via filter resonance or saturation drive) and *before* a one-pole
  **DC blocker** (~20 Hz corner). This is WIRE's regen convention: past 100 % feedback the
  loop **self-oscillates**, but the limiter clamps every pass to a stable limit cycle
  instead of exploding, and the DC blocker stops any offset from ratcheting up.
  Self-oscillation is the feature — it is a bounded, sustained tone, not silence.
- **Zero latency.** The delay line *is* the effect, not fixed processing latency, so
  OUROBOROS reports `set_latency_samples(0)`. The granular pitch/reverse slots are short
  grain readers and the Hilbert frequency shifter is a minimum-phase IIR allpass pair — no
  FIR lookahead anywhere. (Consequently the suite's lag-0 partial-mix single-coherent-peak
  regression does not apply to a time-delay effect; the DSP tests assert **`mix = 0` nulls
  against dry** instead.)
- **Click-free delay modulation.** The delay read is fractional and **smoothed** (a ~40 ms
  one-pole glide on the delay length + linear interpolation), so changing the delay time
  while running slews the read tap rather than jumping it — no zipper, no click.

## Parameters

| Group | Param | Range | Notes |
|---|---|---|---|
| Loop | Delay | 1–2000 ms | Free delay time (skewed). Smoothed, click-free when changed. |
| Loop | Sync | on/off | Lock the delay to host tempo. |
| Loop | Division | 1/16 … 1 Bar | Sync delay length: 1/16, 1/8, 1/8·, 1/4, 1/4·, 1/2, 1 Bar. |
| Loop | Feedback | 0–110 % | Loop gain. Past 100 % the loop self-oscillates (bounded by the limiter). |
| Loop | Decay | 0–100 % | Fine multiplier on feedback. |
| Loop | Freeze | on/off | Mutes the input (smoothed) and forces 100 % feedback ⇒ an infinite tail. |
| Chain | Order | 6 perms | Slot visiting order: A→B→C, A→C→B, B→A→C, B→C→A, C→A→B, C→B→A. |
| Slot A/B/C | Type | Off / 8 effects | See slot table below. |
| Slot A/B/C | Amount | 0–100 % | Primary macro (meaning depends on Type). |
| Slot A/B/C | Param | 0–100 % | Secondary macro (meaning depends on Type). |
| Output | Mix | 0–100 % | Dry/wet. |
| Output | Out | −24…+24 dB | Output trim. |

### Slot types

Each slot picks one effect. The filter's LP/HP/BP "type select" is folded into the slot
type (simpler than a separate per-slot mode param). `Amount`/`Param` are always 0–100 %:

| Type | Amount | Param |
|---|---|---|
| **Off** | — | — (pass-through) |
| **Pitch Shift** | pitch −12…+12 st (50 % = unity) | grain size 512–4096 samples |
| **Filter LP / HP / BP** | cutoff/center 40 Hz–18 kHz (log) | resonance Q 0.5–8 |
| **Freq Shift** | shift −500…+500 Hz (50 % = none) | upper↔lower sideband blend |
| **Saturate** | drive ×1–16 (suite tube bank) | tanh↔wavefold blend |
| **Reverse** | chunk length ~21–167 ms | dry mix-back |
| **Bit Crush** | bit depth 16→4 | sample-rate decimation ×1–32 |

**Slot-order model.** A single `Order` enum over the 6 permutations of the 3 slots is used
rather than three per-slot position IntParams — the enum can't express a duplicate/degenerate
position, so it is the simpler and safer param model (build-brief decision).

**Frequency-shifter group delay.** The shifter uses a two-path IIR Hilbert transformer
(Niemitalo/Costello polyphase allpass network, 4 sections per branch, one branch fed a
one-sample-delayed input). It is minimum-phase IIR with negligible, un-reported group delay
(the delay line dominates the loop timing). The quadrature match is ≈ −19 dB single-sideband
suppression across 300 Hz–9 kHz — imperfect by design, and the residual sideband is on-brand
grit for a lo-fi feedback effect.

## Freeze

Freeze is a live performance toggle: it smoothly mutes the input and pins feedback to 100 %,
so whatever is circulating holds indefinitely (click-free entry/exit). No factory preset ships
with Freeze on — a from-scratch render with it engaged would be silent; "Frozen Drone" reaches
a near-infinite sustain with 110 % feedback instead.

## Presets

| Preset | Character |
|---|---|
| Dub Tail | Filtered, lightly saturated dub echoes that darken as they recirculate. |
| Shifter Spiral | Each repeat pitches up ~1 st through a band-pass — an endless rising spiral. |
| Crushed Echoes | Bit-crushed, low-passed lo-fi digital decay. |
| Frozen Drone | 110 % feedback into filter + saturator ⇒ a self-sustaining near-infinite drone. |
| Reverse Cascade | Reversed granules cascade through a high-pass — smeared backwards tails. |
| Frequency Clang | Frequency-shifted repeats detune into inharmonic, bell-like clangor. |

## Done bar (mechanical, PRD §4)

- Universal: no NaN/inf, peak ≤ 0 dBFS, non-silent, `mix = 0` nulls vs dry < −80 dB.
- 110 % feedback with saturator + filter in the loop, 30 s render → peak ≤ 0 dBFS, zero NaN,
  last-5 s RMS **stable** (not growing > 1 dB, not collapsing to silence).
- A delay-time change while running produces **no hard click** (max sample-to-sample delta
  around the change stays within a small factor of the steady-state render).
- Freeze sustains an audible tail with the input muted; every slot type stays finite and
  bounded at extreme macros under 110 % feedback.

## Try it in FL

Find more plugins → add **Qeynos OUROBOROS** on any source or bus. Load **Dub Tail** and play
a rhythmic loop; raise **Feedback** past 100 % for the self-oscillating **Frozen Drone**; put a
**Pitch Shift** in Slot A and load **Shifter Spiral** for the endless riser; tick **Sync** and
pick a **Division** to lock echoes to tempo; hit **Freeze** to hold the current wash as an
infinite pad. Reorder the slots (**Order**) to change how each repeat is mutated. Zero reported
latency; the delay-time knob glides click-free.

## Freeze Mix

**Freeze Mix** (0–100%, default 100%) works alongside the **Freeze** toggle. Freeze stays a
toggle; Freeze Mix sets how much of the held/frozen texture you hear versus the live signal
while Freeze is engaged. At 100% it is the classic hard freeze (unchanged); lower it to blend
the live source back in so the freeze is a smooth crossfade rather than a sudden jump. The
blend is smoothed (~15 ms) and only active while Freeze is on.
