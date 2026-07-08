# CHAMBER — image-source space simulator (Eigen clone)

A physical **shoebox room**: a rectangular space of width **W** × depth **D** × height **H**
(2–40 m) with a **source** and a **listener** you drag around a top-down floor-plan. The room
response is synthesised in two stages that are summed as the *wet* signal — a discrete
**early-reflection image cluster** and a diffuse **FDN late field** — then blended against the
dry input by **Mix**.

```
  in(mono) ─┬─▶ shared input delay line ─▶ [image cluster: order-≤3 mirror images]
            │       per image i:  tap @ rᵢ/c · gain(r_direct/rᵢ × reflectⁿ) · HF-damp · pan(azimuthᵢ)
            │                                                        └──────────────┐  early reflections
            │                                                                        ▼
            └─▶ pre-delay (ER window + user) ─▶ Fdn8 late field ──────────────────── + ─▶ wet
                    RT60 = 0.161·V/ΣSα, damping from material HF          (ER/Late balance)

  out = (1−mix)·in  +  mix · wet · outTrim              (mix = 0 ⇒ exact input passthrough)
```

## Early reflections — the image-source model

For a shoebox room the reflections off the six walls are exactly modelled by **mirror image
sources**: reflect the source across each wall (and across the images, recursively). For
reflection indices `(kx, ky, kz) ∈ ℤ³` with `|kx|+|ky|+|kz| ≤ order`, the image coordinate on
each axis is

```
image(k, L, s) = k·L + s        (k even, a translated copy)
               = k·L + (L − s)  (k odd,  a mirrored copy)
```

The image count is the 3-D **L1 ball** — **7 images at order 1, 25 at order 2, 63 at order 3**
(including `k = (0,0,0)`, which is the direct path — the true source). Per image:

- **Delay** `= r / c` — the straight-line distance `r` from the image to the listener over the
  speed of sound (`c = 343 m/s`), read from **one shared fractional delay line** (4-point
  Catmull-Rom). The **direct** path has the smallest `r`, so it is always the first arrival.
- **Gain** `= (r_direct / r)^distance × reflect_walls^(|kx|+|ky|) · reflect_floor^nf ·
  reflect_ceiling^nc`. The inverse-distance term is normalised so the direct path is unity;
  **Distance** exaggerates the rolloff. Each wall-group's amplitude reflectance is
  `√(1 − α)` from its material absorption `α`; the vertical bounces are split between floor and
  ceiling by which surface the ray hits first.
- **HF damp** — a one-pole low-pass per image whose darkness grows with the accumulated
  per-bounce high-frequency loss of the materials it reflected off (the direct path is
  un-filtered).
- **Pan** — **equal-power** from the image's horizontal azimuth relative to the listener, so
  reflections arrive from their true directions and the stereo image widens with the room.

The image positions (and therefore all delays/gains/pans) are recomputed at **control rate**
whenever the room, source, or listener moves; per sample the read delays are slewed with a
**rate clamp** so a **moving source doppler-glides** naturally and click-free (like FLYBY).

## Late field — Sabine-tuned FDN

The diffuse tail is `suite_core::fdn::Fdn8` (the 8×8 Householder feedback delay network reused
from MURMUR):

- **Line lengths** scale from the room's **mean free path** `4V/S` (nudged mutually-prime-ish
  to avoid flutter).
- **RT60** from the **Sabine equation** `RT60 = 0.161 · V / A`, with total absorption
  `A = Σ Sᵢ·αᵢ` over the floor, ceiling and walls; clamped to 0.1–12 s. Set **RT60** to a
  non-zero value to override the physical prediction; `RT60 = Auto` uses Sabine.
- **Damping** tilt from the mean material high-frequency character (bright materials → a
  brighter tail).
- The FDN input is **pre-delayed** by the early-reflection window (room diagonal `/ c`) plus
  the user **Pre-Delay**, so the diffuse tail crosses in *after* the discrete image cluster.
- **ER/Late** is an equal-power balance between the image cluster and the late field.

## The direct path is the dry — null contract

The direct path is image order 0: it *is* the dry sound (CHAMBER replaces the room). It sits at
its geometric delay `r_direct / c` (sound takes time to arrive), so the wet is **not** aligned
with the dry at lag 0 — exactly like FLYBY/MURMUR, CHAMBER therefore reports **zero processing
latency** and **`Mix = 0` passes the input through exactly** (bit-for-bit null, no PDC needed).

## Materials

Four presets per wall-group (Walls / Floor / Ceiling), each an absorption + HF-character pair:

| Material | Absorption α | Per-bounce HF keep | Character |
|---|---|---|---|
| Concrete | 0.03 | 0.90 | very live, bright |
| Wood     | 0.12 | 0.72 | warm, natural |
| Curtain  | 0.55 | 0.35 | dead, dark |
| Glass    | 0.07 | 0.86 | live, edgy/bright |

## CPU rule (PRD §4)

The image cluster cost scales with the order (7 / 25 / 63 taps). The mean `process()` cost per
512-sample block at 48 kHz was benched in release:

| ER Order | Images | ns / block | % of real-time budget |
|---|---|---|---|
| 1 | 7  | ~64 k  | 0.6 % |
| 2 | 25 | ~187 k | 1.8 % |
| 3 | 63 | ~425 k | **4.0 %** |

Order 3 costs **4.0 %** of the real-time budget — far under the 30 % threshold — so
**`ER Order = Auto` uses order 3** (the descope ladder 3 → 2 → 1 + a bigger late field is
wired but not needed on this machine). You can force **3 / 2 / 1** manually.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Width / Depth / Height | 2–40 / 2–40 / 2–20 m | Room dimensions (skewed). |
| Source X/Y, Listener X/Y | floor-plan | Drag the amber (source) & ringed (listener) handles. |
| Source Height, Listener Height | 0–100 % of H | Vertical positions. |
| Walls / Floor / Ceiling | 4 materials | Concrete / Wood / Curtain / Glass. |
| ER Order | Auto / 3 / 2 / 1 | Reflection order (Auto = 3 per the CPU bench). |
| ER/Late | 0–100 % | Balance between the image cluster and the FDN tail. |
| Distance | 0.5–3.0 | Inverse-distance rolloff exaggeration. |
| Pre-Delay | 0–200 ms | Extra delay before the late field. |
| RT60 | Auto / 0.1–12 s | 0 (Auto) = Sabine prediction; else overrides it. |
| Width | 0–200 % | Stereo width of the wet (mid/side). |
| Mix | 0–100 % | Dry/wet (0 = exact passthrough). |
| Out | ±24 dB | Output trim. |

## Presets

Small Dead Booth · Wood Room · Warehouse · Cathedral-ish · Tight Drum Room · Distant Hall.

## Done-bars (PRD §4, all met)

1. **First arrival = direct `r/c` ±1 sample** — an impulse's first (loudest) arrival lands at
   the geometric direct-path delay computed from the source/listener positions.
2. **Late RT60 within ±25 % of Sabine** — measured (`suite_core::fdn::measure_rt60`) for a
   small dead room and a large live room.
3. **Moving source ⇒ no click** — a mid-render source sweep keeps the max sample-to-sample
   delta within a static reference (rate-clamped delays + smoothed gains).
4. **`Mix = 0` exact null** — passthrough residual < −80 dB on both channels.

Plus the universal assertions (finite, ≤ 0 dBFS, non-silent) on every preset render, an image
count check (7/25/63), an extremes fuzz, and the CPU bench above. Renders are written to
`renders/CHAMBER/`.

<!-- BUILT-IN-MANUALS: canonical sections rendered in-GUI by the '?' button (parsed by suite_core::manual). -->

## What It Is

A physical shoebox-room reverb: set the room's size, drag a **source** and a **listener** around
a top-down floor-plan, and CHAMBER synthesises the true early reflections plus a Sabine-tuned
diffuse tail for that exact geometry. Because the direct path *is* the dry sound, **Mix = 0**
passes audio through untouched — turn it up and your source is placed in a believable space, from
a curtained vocal booth to a collapsing concrete cavern.

## Signal Flow

```
 in ─┬─ image-source early reflections (order ≤ ER Order) ─┐
     │     per image: delay r/c · gain 1/r·reflectⁿ · HF-damp · pan │
     │                                                    ├─ ER/Late ─ Width ─ wet ─┐
     └─ Pre-Delay ─ Sabine FDN late field (RT60) ─────────┘                         │
                                                                                     ▼
   out = (1 − Mix)·dry  +  Mix · wet · Out            (Mix = 0 ⇒ exact passthrough)
```

## Controls

- **Width** — room left↔right size, 2–40 m (skewed). Bigger rooms give longer reflections and a wider image.
- **Depth** — room front↔back size, 2–40 m. Sets the source→listener distance range.
- **Height** — room floor↔ceiling size, 2–20 m. Taller rooms ring lower and longer.
- **Source X** — the sound's left/right spot on the floor-plan, 0–100 % of Width; drag the amber dot.
- **Source Y** — the sound's front/back spot on the floor-plan, 0–100 % of Depth.
- **Source Height** — the source's vertical position, 0–100 % of room Height.
- **Listener X** — the mic's left/right spot, 0–100 % of Width; drag the ringed handle.
- **Listener Y** — the mic's front/back spot, 0–100 % of Depth.
- **Listener Height** — the listener's vertical position, 0–100 % of room Height.
- **Walls** — wall material: Concrete / Wood / Curtain / Glass — sets absorption + per-bounce HF darkness.
- **Floor** — floor material (same four); a curtain floor deadens fast, concrete rings bright.
- **Ceiling** — ceiling material (same four); the vertical bounces split between Floor and Ceiling.
- **ER Order** — early-reflection image order: Auto / 3 / 2 / 1; higher = denser, more expensive reflections.
- **ER/Late** — balance between the discrete early-reflection cluster and the diffuse FDN tail, 0–100 %.
- **Distance** — inverse-distance rolloff exaggeration, 0.5–3.0; higher pushes the source further back.
- **Pre-Delay** — extra gap before the late field, 0–200 ms; widens the sense of size/separation.
- **RT60** — late-tail decay time: Auto (physical Sabine prediction) or a 0.1–12 s override.
- **Width** — stereo width of the wet field, 0–200 % (mid/side); the room already widens with size, this trims it.
- **Mix** — dry/wet blend, 0–100 %. **Mix = 0** is a bit-exact passthrough (the direct path is the dry).
- **Out** — output trim, ±24 dB.

## Recipes

1. **Dark-techno concrete cavern — "Abandoned Reservoir"** — a 30 × 38 × 14 m all-concrete box,
   **ER/Late 80 %**, **RT60 8.0 s**, **Distance 1.5**, **Pre-Delay 50 ms**, **Width 1.4**,
   **Mix 48 %**, **Out −2 dB**. Drag the source far from the listener for a huge, sub-heavy tail
   behind stabs and toms without washing out the low end.
2. **Atmospheric-DnB wash — "Fog Bank"** — a 14 × 20 × 8 m curtain-lined room, **ER/Late 90 %**
   (almost all diffuse), **RT60 6.0 s**, **Distance 1.3**, **Width 1.6**, **Mix 50 %**,
   **Out −2 dB**. Sits pads, Rhodes and vocal chops in a slow, blooming grey haze.
3. **Vocal-rip booth — "Vocal Isolation"** — a tiny 2.6 × 3.2 × 2.4 m curtain booth, **ER Order 3**,
   **ER/Late 25 %** (mostly tight early reflections), **Width 0.7**, **Mix 28 %**. Gives a dry,
   ripped a-cappella just enough believable room to sit in a mix without smearing consonants.
