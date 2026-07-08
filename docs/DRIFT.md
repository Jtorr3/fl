# DRIFT — infinity filter (Shepard-tone filter illusion)

A *Sweep*-style endless filter. `N` peak (bell) filters are placed on the
log-frequency axis, evenly spaced across a `[Range Lo, Range Hi]` window, and all
glide together up or down at **Rate** — free-running in Hz or locked to the host
tempo. Each filter's center **wraps** at the range edges, and its boost follows a
raised-cosine (Hann) window over its log-frequency position, so every filter fades
in silently at the bottom, swells through the middle, and fades out at the top.
Because a filter reaching the top has already faded to unity gain, the wrap is
inaudible and the ear hears an **endless rise (or fall)** — the Shepard-tone
illusion, applied to resonant filtering.

DRIFT is pure minimum-phase IIR (TPT/Cytomic state-variable *bell* filters, the same
time-varying-safe topology TRACER uses). Dry and wet stay sample-aligned, so it
reports **zero latency** and needs no delay compensation.

## What It Is

An endless Shepard-tone illusion in the filter domain: N resonant bell filters, evenly
spaced across a log-frequency window, all glide together up or down and wrap silently at
the edges, so the ear hears a rise or fall that never arrives. Drop it on any sustained or
broadband source for perpetual, hypnotic filter motion — free-running or locked to tempo.

## Signal Flow

```
              phase(t) ── advances at Rate (Hz) or tempo·division ── wraps [0,1)
                 │
   for each filter i = 0..N:
     u_i   = frac( phase + i/N )                    (evenly spaced log positions)
     fc_i  = 2^( log2(Range Lo) + u_i · span )      (span = octaves across the range)
     g_i   = Depth · (0.5 − 0.5·cos(2π·u_i)) dB     (raised-cosine gain window)
   wet = bell_{N-1} ∘ … ∘ bell_1 ∘ bell_0 (x)       (series cascade, per channel)
   R channel rides phase + Stereo Offset            (0..0.5 cycle ⇒ stereo width)
   out = (dry·(1−Mix) + wet·Mix) · Out
```

Coefficients recompute every 32-sample control block from **smoothed** phase and
smoothed range / resonance / depth / offset, with filter state preserved across
recomputes so the glide is click-free. At a window edge `g_i → 0 dB`, so the bell
is a pass-through and the frequency wrap there carries no energy.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| **Rate** | 0.01–10 Hz | 0.1 Hz | Full range traversals per second (free-run mode). |
| **Sync** | off / on | off | When on, Rate is derived from the host tempo + Division. |
| **Division** | 4 Bars … 1/16 | 1 Bar | Beats per full glide cycle (4/4): 4 Bars = 16 beats … 1/16 = 0.25 beat. |
| **Direction** | Up / Down | Up | Rising or falling Shepard glide. |
| **Peaks** | 2–8 | 6 | Number of simultaneous filters. Spacing = span / N octaves. |
| **Resonance** | 0.3–12 | 3.0 | Shared bell Q. Higher = narrower, more vocal peaks. |
| **Range Lo** | 20 Hz–2 kHz | 50 Hz | Lower edge of the glide window. |
| **Range Hi** | 200 Hz–20 kHz | 3.2 kHz | Upper edge. Default is 6 octaves above Lo ⇒ at N=6 the filters sit ~1 octave apart (true Shepard). |
| **Depth** | 0–36 dB | 12 dB | Peak boost at each filter's window center. |
| **Stereo Offset** | 0–0.5 | 0.25 | R-channel glide phase offset (cycles). Avoid exactly 1/N — that maps R onto L (no width). |
| **Mix** | 0–100 % | 100 % | Dry/wet. |
| **Out** | −24…+24 dB | 0 dB | Output trim (output is soft-limited to 0 dBFS). |

## Presets

- **Endless Riser** — six filters ~1 octave apart over six octaves, ever rising.
- **Slow Descent** — the mirror illusion: an unbroken slow fall.
- **Hypnotic Sweep 1/4** — tempo-locked, one full glide per beat, resonant.
- **Wide Drift** — eight gentle filters over seven octaves, hard L/R phase split.
- **Subtle Motion** — a parallel-mix (60 %) shimmer that barely moves.
- **Falling Half-Note** — tempo-locked descent, one glide every two beats.

## Done-bar (offline, `cargo test -p drift`)

1. **Dominant peak advances & wraps** — white-noise input, Direction = Up: the
   dominant STFT peak (tracked with `suite_core::stft`) strictly advances over time
   and wraps back at the range edge. A companion test tracks an individual filter's
   center sweeping the full range and wrapping `Range Hi → Range Lo`.
2. **Periodicity / self-similarity** — Welch-averaged output spectra at `t` and
   `t + period/N` correlate > 0.9 (and more strongly than at `t + period/2N`): each
   filter has advanced into its neighbour's former position (the illusion's engine).

Plus the universal assertions (finite, ≤ 0 dBFS, non-silent, mix=0 nulls < −80 dB),
a partial-mix single-coherent-peak regression (minimum-phase alignment), a
parameter-fuzz stability test, and a stereo-decorrelation check. Renders are written
to `renders/DRIFT/`.

## Using it in FL Studio

Add **Qeynos DRIFT** on any sustained/broadband source (pad, noise, drone, drum bus).
Load **Endless Riser** and let it run — the perceived motion never stops. Tick
**Sync** and pick a **Division** to lock the sweep to the project tempo. Raise
**Resonance** and **Depth** for a vocal, obvious sweep; drop **Mix** for a subtle
underlying motion. **Stereo Offset** widens the effect across the L/R field.

## Controls

- **Rate** — full range traversals per second (free-run mode), 0.01–10 Hz.
- **Sync** — when on, Rate is derived from the host tempo + Division.
- **Division** — beats per full glide cycle when synced, 4 Bars … 1/16.
- **Direction** — rising (Up) or falling (Down) Shepard glide.
- **Resonance** — shared bell Q, 0.3–12; higher = narrower, more vocal peaks.
- **Range Lo** — lower edge of the glide window, 20 Hz–2 kHz.
- **Range Hi** — upper edge of the glide window, 200 Hz–20 kHz.
- **Peaks** — number of simultaneous filters, 2–8 (spacing = span / N octaves).
- **Stereo Offset** — R-channel glide phase offset, 0–0.5 cycles (avoid exactly 1/N).
- **Depth** — peak boost at each filter's window center, 0–36 dB.
- **Mix** — dry/wet, 0–100 %.
- **Out** — output trim, −24…+24 dB (soft-limited to 0 dBFS).

## Recipes

1. **Dark-techno sinking sweep** — load **Sinking Feeling** (Rate 0.05 Hz, Direction Down,
   Resonance 5.0, Range Lo 30 Hz, Range Hi 1.92 k, Peaks 6, Depth 15 dB, Mix 100 %). A low,
   resonant descent that drags the whole mix endlessly downward under a techno groove.
2. **Atmospheric-DnB riser** — load **Vapor Climb** (Rate 0.12 Hz, Direction Up, Resonance 4.0,
   Range Lo 120 Hz, Range Hi 4.8 k, Peaks 5, Stereo Offset 0.30, Depth 11 dB, Mix 75 %). Parallel
   motion woven under a pad or break rather than over it.
3. **Vocal-rip tempo shiver** — load **Eighth-Note Shiver** (Sync on, Division 1/8, Direction Down,
   Resonance 6.0, Range Lo 300 Hz, Range Hi 4.8 k, Peaks 4, Depth 14 dB, Mix 100 %). A tight,
   nervous resonant strobe over a chopped vocal.
