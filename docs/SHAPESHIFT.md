# SHAPESHIFT — XY-morphing distortion (Teuri clone)

A distortion whose **character is a point on a 2-D pad**. Four **corners** (A, B, C, D) each
select a waveshaper from an 8-curve bank and carry a per-corner input **gain trim**. An **XY
position** sets **bilinear** blend weights so the output morphs continuously between the four
shaper characters. A built-in **orbit LFO** can rotate the XY point around the user position for
hands-free, evolving distortion. The nonlinear path is **4x oversampled**; the dry path is
delay-compensated so partial mix stays phase-coherent.

```
in ─ pre-gain ─ 4x OS ─ [shaper A][B][C][D] ─ bilinear XY blend ─ DC block ─ post LP ─ mix ─ out
                                  ▲
                        XY point (+ orbit LFO) → weights wA..wD
```

The morph, per (oversampled) sample:

```
y = wA·A(gA·pre·x) + wB·B(gB·pre·x) + wC·C(gC·pre·x) + wD·D(gD·pre·x)

wA = (1-X)(1-Y)   wB = X(1-Y)   wC = (1-X)Y   wD = X·Y      (wA+wB+wC+wD = 1)
```

Because the weights are a **partition of unity**, the blend is a convex combination of the four
(bounded) shaper outputs, so the morph is always bounded. Corner **A = (0,0)** (bottom-left),
**B = (1,0)** (bottom-right), **C = (0,1)** (top-left), **D = (1,1)** (top-right). Pushing the XY
point hard into a corner reduces the output to that single shaper — SHAPESHIFT at corner A nulls
against "shaper A alone" (same pre-gain and corner gain) below −60 dB.

## The shaper bank (per corner)

The bank is local to the crate (nothing added to `suite-core`); each curve maps an already
gain-scaled sample to a bounded output:

| # | Corner curve | Character |
|---|---|---|
| 0 | **Tube tanh** | Smooth odd-harmonic saturation (`tanh`). |
| 1 | **Tape soft** | Gentle cubic soft-knee (suite `tape_soft`). |
| 2 | **Diode asym** | Asymmetric clip — positive half driven harder → even + odd harmonics. |
| 3 | **Hard clip** | Flat-top clip at ±1. |
| 4 | **Sine fold** | Rounded wavefolder — folds back on overdrive. |
| 5 | **Tri fold** | Triangle wavefolder (`asin(sin)`) — sharper fold than the sine fold. |
| 6 | **Cheby-3** | 3rd Chebyshev polynomial `4x³−3x` — pure 3rd-harmonic generator at unity. |
| 7 | **Bit soft** | Soft digital bit-crush — quantise to a few levels, blended back toward linear. |

Each corner's **gain trim** scales its input *before* the curve, so you can, e.g., keep one
corner clean and push the opposite corner into hard clipping — the morph then sweeps from clean
to crushed as the XY point (or the orbit) crosses the pad.

## Orbit LFO

An internal XY LFO adds an offset to the user point so the sound morphs by itself:

- **Orbit** on/off.
- **Orbit Rate** — free rate in Hz (0.01–20), or…
- **Orbit Sync** + **Orbit Division** — one full orbit per **1/2 / 1 bar / 2 bar / 4 bar**
  (from the host tempo).
- **Orbit Radius** — how far the orbit swings around the user point (0–0.5 of the pad).
- **Orbit Shape** — **Circle** or **Figure-8** (lemniscate of Gerono).
- **Orbit Phase** — start-phase offset.

On the XY pad the **user point** is the amber dot you drag; when the orbit is on, its **path** is
drawn and a **white dot** rides it at the live orbit phase, so you can see exactly which corners
are being blended over time. With a 1 Hz circular orbit the spectral character (e.g. THD) tracks
periodically at the orbit rate.

## Oversampling & latency (PDC)

The whole shaper blend runs inside a **4x oversampler** (polyphase halfband FIR, `suite_core`),
which removes most of the aliasing the folders/clippers/crusher would otherwise generate. The
oversampler's linear-phase FIRs impose a fixed group delay; the **dry/parallel path is delayed by
the same integer amount** (`Oversampler4x::measure_group_delay()`), and that delay is reported to
the host as latency. As a result:

- **`mix = 0` nulls** against the (latency-matched) dry signal below −80 dB.
- At partial mix the dry and wet paths stay **sample-aligned** — no comb filtering (a single
  coherent peak on an impulse at `mix = 0.5`), the GRIT / HARD CHECKPOINT 1 discipline.

## Post & output

- **DC block** (~5 Hz) removes the offset the asymmetric/diode curve introduces.
- **Post LP** — a state-variable low-pass that tames the harshness of the folders and the
  bit-crush.
- **Auto-Gain** (optional) — matches the output RMS to the input RMS over ~300 ms (±12 dB clamp),
  so driving harder doesn't just get louder.
- **Mix** (dry/wet) and **Out** trim, with a hard safety ceiling at ±0.999 (≤ 0 dBFS).

## Parameters

| Param | Range | Notes |
|---|---|---|
| X, Y | 0–1 | XY morph position (drag on the pad, or automate). |
| Corner A/B/C/D | 8 curves | Shaper for each pad corner. |
| Gain A/B/C/D | ±24 dB | Per-corner input drive trim. |
| Pre-Gain | −12…+36 dB | Drive into the whole shaper bank. |
| Orbit | on/off | Enable the XY orbit LFO. |
| Orbit Rate | 0.01–20 Hz | Free orbit rate (when not synced). |
| Orbit Sync | on/off | Lock the orbit to host tempo. |
| Orbit Division | ½/1/2/4 bar | One orbit per division when synced. |
| Orbit Radius | 0–0.5 | Orbit size around the user point. |
| Orbit Shape | Circle / Figure-8 | Orbit trajectory. |
| Orbit Phase | 0–1 | Start-phase offset. |
| Post LP | 200 Hz–20 kHz | Output low-pass. |
| Auto-Gain | on/off | 300 ms RMS loudness match. |
| Mix | 0–100% | Dry/wet (0 nulls dry). |
| Out | ±24 dB | Output trim. |

## Factory presets

Warm-Fold Morph · Diode Drive Orbit · Cheby Shimmer · Bit Edge · Tape Corner · Full Chaos Orbit.

## How to test in FL

Find more plugins (rescan) if **Qeynos SHAPESHIFT** isn't listed yet, then drop it on any
track/bus. Load a preset from the top bar, then **drag the amber dot around the XY pad** — the
four corner labels show which shaper each corner uses, and the sound morphs between them. Turn
**Orbit** on and watch the white dot fly the path (pick **Circle / Figure-8**, set **Radius** and
**Rate**, or tick **Orbit Sync** + a **Division** to lock it to the groove). Push **Pre-Gain** or
a corner's **Gain** to drive harder; pull **Post LP** down to tame fold/crush fizz; **Auto-Gain**
keeps the level steady; **Mix** blends back the dry. A sustained synth, bass, or drum bus shows
the morph best.

## What It Is

A distortion whose character is a point on a 2-D pad. Four corners each pick one of eight
waveshapers, and an XY position bilinearly blends between them, so the tone morphs continuously
from clean tube to crushed fold as you move. A built-in orbit LFO can fly the point around on
its own for hands-free, evolving grit — everything 4x oversampled to keep it clean.

## Signal Flow

```
in ─ Pre-Gain ─ 4x oversample ─ [Corner A][B][C][D] ─ bilinear XY blend ─ DC block ─ Post LP ─ Mix ─ Out
                                        ▲
                        XY point (X, Y) + Orbit LFO (Rate / Sync·Division, Radius, Shape, Phase)
```

## Controls

- **X** — horizontal morph position on the pad, 0–1 (blends left↔right corners).
- **Y** — vertical morph position on the pad, 0–1 (blends bottom↔top corners).
- **Corner A** — waveshaper at the bottom-left corner (8-curve bank).
- **Corner B** — waveshaper at the bottom-right corner.
- **Corner C** — waveshaper at the top-left corner.
- **Corner D** — waveshaper at the top-right corner.
- **Gain A** — input drive trim into Corner A's shaper, −24…+24 dB.
- **Gain B** — input drive trim into Corner B's shaper, −24…+24 dB.
- **Gain C** — input drive trim into Corner C's shaper, −24…+24 dB.
- **Gain D** — input drive trim into Corner D's shaper, −24…+24 dB.
- **Pre-Gain** — drive into the whole shaper bank, −12…+36 dB.
- **Orbit** — enable the XY orbit LFO, on/off.
- **Orbit Rate** — free orbit rate, 0.01–20 Hz (when not synced).
- **Orbit Sync** — lock the orbit to host tempo, on/off.
- **Orbit Division** — synced orbit length: ½, 1, 2, or 4 bars.
- **Orbit Radius** — how far the orbit swings around the user point, 0–0.5 of the pad.
- **Orbit Shape** — orbit trajectory: Circle or Figure-8.
- **Orbit Phase** — orbit start-phase offset, 0–1.
- **Post LP** — output low-pass to tame fold/crush harshness, 200 Hz–20 kHz.
- **Auto-Gain** — match output loudness to input over ~300 ms, on/off.
- **Mix** — dry/wet, 0–100 % (0 nulls the dry).
- **Out** — output trim, −24…+24 dB.

## Recipes

1. **Dark-techno erosion** — load **Rusted Godhead** (Corner shapers Tri/Hard fold, Pre-Gain 20 dB, Post LP 8 kHz, Mix 100 %, Out −3 dB): a fixed, brutal fold-clip that corrodes a bassline or stab into gravel.
2. **Atmospheric-dnb drift** — load **Tidal Drift** (Orbit on, Orbit Rate 0.15 Hz, Orbit Radius 0.30, Circle, Pre-Gain 9 dB, Mix 90 %): a slow circular orbit that keeps a pad's harmonic character gently shifting under the beat.
3. **Vocal-rip digital bloom** — load **Chlorine Bloom** (Corner bit-crush/hard shapers, Pre-Gain 9 dB, Post LP 8 kHz, Mix 100 %, Out −1 dB) on a vocal chop for a chlorinated, datamoshed digital edge.
