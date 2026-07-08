# ASCEND — tension generator

## What It Is

A MIDI / **transport instrument** that builds risers, downlifters and swells locked to the song
grid. ASCEND reads the host transport, counts down to the next N-bar boundary, and runs **one
master tension envelope** across that countdown. The envelope simultaneously drives four things —
a filter sweep, a pitch rise, a stereo width bloom and a volume swell — over two blended sources
(filtered noise + a tonal root/fifth oscillator stack). At the target bar it fires an embedded
impact and auto-cuts to silence, then re-arms for the next boundary. It works standalone too: with
the transport stopped a manual **TRIGGER** (or a MIDI note) runs the same envelope over a
time-based length.

## Signal Flow

```
host transport (tempo, bar position)
   │
   ▼
COUNTDOWN to next boundary  ── Sync = 8 / 16 / 32 / Custom bars ──▶  phase p : 0 → 1 over the window
   │
   ▼
TENSION ENVELOPE  env = shape(p, Curve)          Curve : 0 exp · 0.5 linear · 1 log
   │   drives, all at once:
   ├─ SVF filter sweep     cutoff  Filter Start → Filter End   (exp interp, opening up)
   ├─ pitch rise           tonal stack × 2^(env · Rise/12)     (0–24 st)
   ├─ width bloom          mid/side  narrow → wide             (Width · env)
   └─ volume swell         quiet → full                        (floor 5 % → 100 %)
   │
SOURCES
   ├─ filtered noise   white ↔ pink (Color), decorrelated L/R for the width bloom
   └─ tonal stack      root + fifth,  saw ↔ sine (Saw/Sine),  Key + Octave
   │            (Noise/Tone balances the two)
   ▼
   mid/side → SVF sweep → volume swell × auto-cut gate ─┐
                                                        ├─▶ soft-clip → stereo out
   boundary ─▶ IMPACT (synth_kick, low) + AUTO-CUT ─────┘
```

## Countdown & the tension envelope

**Sync Target** picks the countdown window length: **8**, **16**, **32** bars, or **Custom** (the
**Bars** knob, 1–64). ASCEND derives the current bar position from the host transport (tempo + time
signature) and the phase `p` runs 0 → 1 across each window, reaching the boundary at the end. The
**Curve** knob morphs the envelope shape with a single exponent: **0** = exponential (slow start,
explosive finish), **0.5** = linear, **1** = logarithmic (fast start, long plateau).

Boundary detection is sample-accurate: the plugin advances its own bar position per sample from the
host's block-rate position, so the impact lands on the target bar within a few samples regardless of
buffer size.

## Sources

- **Tonal stack** — a **root + fifth** oscillator pair. Each oscillator crossfades **saw ↔ sine**
  (`Saw/Sine`). The root pitch comes from **Key** (C…B) and **Octave** (0–6); with **Key Track** on
  it instead follows the last played MIDI note. The whole stack is transposed up by `env · Rise`
  semitones (0–24) as the envelope climbs.
- **Filtered noise** — **white ↔ pink** blend (`Noise Color`). Two decorrelated streams feed the
  mid and side so the width bloom has something to open into; the noise is mono-compatible (the
  side cancels to mono at `Width = 0`).
- **Noise/Tone** balances the two sources (0 = all noise, 1 = all tonal).

## What the envelope drives

| Target | At env = 0 | At env = 1 |
|---|---|---|
| SVF cutoff | **Filter Start** (dark) | **Filter End** (open) — exponential interpolation |
| Tonal pitch | root/fifth | + **Pitch Rise** semitones (0–24) |
| Stereo width | mono | **Width** (wide) |
| Volume | quiet (5 % floor) | full |

Because the filter opens and the pitch rises together, the render's spectral centroid climbs
monotonically over the countdown — the audible "tension" of a riser.

## At the boundary

- **Impact** (on/off) — an embedded low boom, synthesized once at load from IMPACT's own
  `synth_kick` math (a sub-heavy ~42 Hz kick). **Impact Level** trims it. It is added *after* the
  auto-cut gate, so the drop hits clean while the riser sources are silenced beneath it.
- **Auto-Cut** (on/off) — at the boundary the sources gate to silence (a fast fade + a short hold),
  producing the classic "everything drops out for the impact" moment, then the next window's riser
  swells up from silence.

## Downlifter mode

**Downlifter** reverses the envelope: it starts **full at the boundary** and **falls away** over the
window (bright → dark, high → low, wide → narrow, loud → quiet). Use it for the tension release
*after* a drop. The impact fires on the drop that begins the fall.

## Free-run (transport stopped)

When the transport is not playing, ASCEND still works: press **Trigger** (a momentary button, also
automatable) or play a **MIDI note** to run the tension envelope over **Free Length** seconds
(0.1–30 s). In riser mode it swells then fires the impact + auto-cut at the end; in downlifter mode
the impact hits at the start of the fall. With **Key Track** on, the played note sets the root.

## Controls

- **Key** — root pitch class of the tonal stack, C … B.
- **Octave** — root octave, 0–6 (C2 ≈ 65 Hz by default).
- **Sync Target** — countdown window length: 8 / 16 / 32 bars or Custom.
- **Custom Bars** — window length when Sync Target = Custom, 1–64 bar.
- **Curve** — tension-envelope shape morph, 0–1 (0 = exponential slow-start, 0.5 = linear,
  1 = logarithmic fast-start plateau).
- **Noise/Tone** — balance between the two sources, 0–100 % (0 = all noise, 100 % = all tonal).
- **Noise Color** — white ↔ pink noise blend, 0–100 %.
- **Saw/Sine** — tonal oscillator waveshape blend, 0–100 %.
- **Filter Start** — SVF cutoff at the start of the countdown, 20 Hz – 18 kHz (dark end).
- **Filter End** — SVF cutoff at the boundary, 20 Hz – 18 kHz (open end).
- **Pitch Rise** — semitone rise applied to the tonal stack at full tension, 0–24 st.
- **Width Bloom** — maximum stereo width reached at full tension, 0–100 %.
- **Impact** — on/off. Fire the embedded low boom at the boundary.
- **Impact Level** — loudness of that impact, 0–100 %.
- **Auto-Cut** — on/off. Gate the sources to silence at the boundary (the drop-out moment).
- **Downlifter** — on/off. Reverse the envelope so it falls away *after* the boundary.
- **Free Length** — envelope length when triggered with the transport stopped, 0.1–30 s.
- **Level** — output trim, −24 … +6 dB.
- **Key Track** — on/off. Root follows the last played MIDI note.
- **Trigger** — momentary button to manually run the envelope in free-run (also automatable).

Stereo instrument, **zero latency**. A live **countdown** readout on the GUI shows the bars
remaining until the next boundary.

## Recipes

1. **8-Bar Warehouse Riser** — load *Riser 8 Dark*: Key C, Octave 2, Sync Target 8 bars,
   Curve 0.35 (exp), Noise/Tone ~40 %, Filter Start 200 Hz, Filter End 12 kHz, Pitch Rise 12 st,
   Width Bloom 70 %, Impact on, Auto-Cut on. An explosive-finish build into the drop of a
   dark-techno track — the filter and pitch climb together, then everything cuts for the boom.
2. **16-Bar Cathedral Build** — from *Riser 16 Wide* / *16-Bar Cathedral*: Sync Target 16 bars,
   Curve 0.65–0.7 (log), Key G, Noise/Tone 55 %, Width Bloom 90 %, Pitch Rise 10 st. A long,
   patient atmospheric-DnB swell that opens wide before the switch-up.
3. **Downlifter Release** — from *Downlifter 8*: Downlifter on, Sync Target 8 bars, Impact on,
   Impact Level 70 %, Filter Start 12 kHz, Filter End 300 Hz. Fires the impact on the drop, then
   the tension *falls away* (bright→dark, wide→narrow) as the section breathes out after it.
4. **Free-Run Noise Swell** — from *Noise Swell Short*: transport stopped, Noise/Tone 0 % (all
   noise), Free Length ~4 s, Curve 0.5, then hit **Trigger** (or send a MIDI note) to fire a
   one-shot white-wind riser into a fill — no host transport required.

## Presets

Riser 8 Dark · Riser 16 Wide · Sub Boom Drop · Downlifter 8 · Noise Swell Short · Melodic Fifth Rise.
Renders for each land in `renders/ASCEND/`.
