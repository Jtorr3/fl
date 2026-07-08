# FLYBY — doppler spatializer (Transfer clone)

Flies a mono source around an **editable closed path** on an XY plane, with the listener
fixed at the origin. As the source moves, its distance `r` and azimuth `θ` to the listener
change every control block, and four physically-motivated cues are synthesised per sample:
a moving **doppler** pitch shift, an inverse-distance **level** falloff, distance-dependent
**air absorption**, and an **equal-power pan** (with optional micro-ITD), followed by a
**width** control.

```
 in(mono) ─► fractional delay line ─► distance gain 1/max(r,r0) ─► air LP(cutoff ∝ 1/r)
              read @ delay = r/c            (level)                     (absorption)
              (Catmull-Rom, rate-clamped)                                    │
                                                                             ▼
                                 equal-power pan(θ) ─► micro-ITD(θ) ─► width ─► mix ─► out
```

## The path

- A **closed Catmull-Rom loop** through **4–8 control points** (nodes) in normalized
  `[-2, 2]` space. The XY pad draws the curve and the listener (centre cross-hair); drag any
  node to reshape the loop live. A white **source dot** rides the path at the current
  traversal phase so the picture always matches the sound.
- Three **starting layouts** — **Circle / Ellipse / Figure-8** — are one-click buttons. They
  are placed **off-centre** from the listener, so even the Circle sweeps the source nearer
  and farther (a genuine fly-by, not a constant-radius orbit → real doppler). The Figure-8 is
  a Gerono lemniscate centred on the listener, crossing close by twice per loop.
- **Traversal** is phase-driven: a **free rate in Hz** (0.01–20 Hz = loops/second) or, with
  **Sync** on, a **BPM-locked loop length** of ½, 1, 2, or 4 bars (from host tempo).
- **Size** scales the node coordinates into distance units — bigger = farther passes, more
  doppler and more air/level travel.

## The four cues

- **Doppler.** The source is written into a fractional delay line and read back at
  `delay = r / c` (a scaled speed of sound `c` so musical sizes and rates give an audible
  bend). As `r` changes the read pointer slews, so the pitch bends — up on approach, down on
  recede — exactly like a real fly-by. The read is **4-point Catmull-Rom** interpolated. The
  **Doppler** knob scales how much distance maps to delay (0 % = a fixed base delay, no
  pitch move). The read-position change per sample is **rate-clamped** (`±0.5` samples), so a
  sharp path corner (or a very fast Figure-8) can never produce a pitch spike — the read
  speed stays within `0.5–1.5×`.
- **Distance level.** `gain = r0 / max(r, r0)` — an inverse-distance law, clamped to a
  reference distance `r0` near the origin so the source never blows up as it passes through
  the listener.
- **Air absorption.** A one-pole low-pass whose cutoff falls with distance (`∝ 1/r`, mapped
  musically from ~18 kHz near to a few hundred Hz far). The **Air** knob blends from no
  filtering (0 %) to the full distance-dependent darkening.
- **Pan + micro-ITD.** The horizontal direction cosine `x / r` drives an **equal-power** pan.
  With **ITD** on, the ear *away* from the source is also delayed by a sub-millisecond amount
  (≤ 0.6 ms, opposite channel), which strengthens the externalisation. **Width** is a final
  post-pan mid/side control (0–200 %).

## Latency

The fractional delay line **is** the effect (distance = delay), not fixed processing
latency, so FLYBY reports **zero latency** (`set_latency_samples(0)`), exactly like
OUROBOROS. Consequently the suite's lag-0 partial-mix single-coherent-peak regression does
**not** apply (there is no lag-0 wet to align); the DSP tests assert **`mix = 0` nulls
against the dry input** instead.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Path nodes (×8) | X/Y −2..2 | Drag on the XY pad; only the first **Nodes** are used |
| Nodes | 4–8 | Active control-point count |
| Circle / Ellipse / Figure-8 | button | Load a starting layout into the nodes |
| Speed | 0.01–20 Hz | Free traversal rate (loops/second) |
| Sync | on/off | Lock the loop length to host tempo |
| Division | ½ / 1 / 2 / 4 bar | Loop length when synced |
| Size | 1–30 | View / distance scale |
| Doppler | 0–100 % | Depth of the distance→delay pitch bend |
| Air | 0–100 % | Depth of the distance low-pass |
| ITD | on/off | Sub-ms opposite-ear micro-delay |
| Width | 0–200 % | Post-pan stereo width |
| Mix | 0–100 % | Dry/wet (0 % nulls the dry input exactly) |
| Out | −24..+24 dB | Output trim |

## Factory presets

**Slow Orbit** (lazy wide circle), **Fast Circle 1/2** (tempo-locked half-note sweep),
**Figure-8 Wide** (big lemniscate, sharp near-crossings), **Distant Flyover** (far, dark,
slow — heavy air + distance), **Subtle Motion** (small, close, barely-there drift for width),
and **Vertigo** (fast Figure-8 with full doppler — disorienting, seasick motion).

## Done-bar (PRD §4)

Universal (no NaN/inf, peak ≤ 0 dBFS, RMS > −60, `mix = 0` nulls dry < −80 dB) plus
FLYBY-specific, all in `tests.rs`:

1. **Doppler present** — a sine on a circular path shows a **periodic f0 deviation at the
   traversal rate**: f0 tracked (zero-crossings on L+R) over 3 cycles bends > 2 % and
   autocorrelates strongly at one-cycle lag (and weaker at the half-cycle lag).
2. **Panning sweeps** — the windowed **L/R RMS ratio crosses 1.0 at least once per
   traversal** (twice on a circle).
3. **Air/distance** — at the **far** point of the path both the **spectral centroid** and the
   **level** are lower than at the **near** point (windowed comparison on broadband noise).
4. **Rate clamp** — at the **sharpest Figure-8 corner and maximum speed**, the largest
   sample-to-sample output step stays bounded (≤ 2.5× a stationary reference render) — no
   pitch spike.

Renders (each factory preset over pink noise + a full-band chirp) are written to
`renders/FLYBY/` by the offline harness.

## What It Is

A doppler spatializer that flies a mono source around an editable path on an XY plane, with you
(the listener) fixed at the centre. As the source sweeps near and far it bends pitch, changes
level, darkens with air absorption, and pans — a genuine fly-by, not a static auto-panner. Use
it to give synths, vocals, and drones real motion and depth.

## Signal Flow

```
 in(mono) ─► fractional delay (distance = delay, Doppler) ─► 1/r level ─► Air LP
                    │                                                        │
   path (Nodes on the XY pad, Speed / Sync·Division) ──► r, θ                ▼
                                            equal-power pan(θ) ─ ITD ─ Width ─ Mix ─ Out
```

## Controls

- **Node 0 X**, **Node 0 Y**, **Node 1 X**, **Node 1 Y**, **Node 2 X**, **Node 2 Y**, **Node 3 X**, **Node 3 Y**, **Node 4 X**, **Node 4 Y**, **Node 5 X**, **Node 5 Y**, **Node 6 X**, **Node 6 Y**, **Node 7 X**, **Node 7 Y** — the X/Y coordinates (−2…2 each) of the eight path control points; drag them on the XY pad. Only the first **Nodes** points are traversed.
- **Nodes** — number of active control points on the path, 4–8.
- **Speed** — free traversal rate, 0.01–20 Hz (loops per second) when not synced.
- **Sync** — lock the loop length to host tempo instead of the free Speed.
- **Division** — synced loop length: ½, 1, 2, or 4 bars.
- **Size** — distance scale, 1–30; bigger = farther passes with more doppler, air, and level travel.
- **Doppler** — depth of the distance→delay pitch bend, 0–100 % (0 = no pitch move).
- **Air** — depth of the distance-dependent low-pass, 0–100 %.
- **ITD** — sub-millisecond opposite-ear micro-delay for stronger externalisation, on/off.
- **Width** — post-pan stereo width, 0–200 %.
- **Mix** — dry/wet, 0–100 % (0 nulls the dry input exactly).
- **Out** — output trim, −24…+24 dB.

## Recipes

1. **Dark-techno rhythmic sweep** — load **Fast Circle 1/2** (Sync on, Division ½, Speed 2 Hz, Doppler 80 %, Size 7, Width 100 %): a tempo-locked half-note orbit that snaps a stab or hat to the groove and swings it hard left↔right.
2. **Atmospheric-dnb ghost trails** — load **Ghost Trails** (Figure-8, Doppler 50 %, Air 70 %, Width 160 %, Mix 70 %) on a pad: smeared, over-wide doppler trails that drift ghostly behind the beat.
3. **Vocal-rip distant flyover** — load **Distant Flyover** (Size 20, Doppler 90 %, Air 85 %, Speed 0.2 Hz, Mix 85 %) on a vocal chop so it passes far overhead, dark and slow, tumbling in and out of focus.
