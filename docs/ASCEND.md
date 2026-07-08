# ASCEND — tension generator

A MIDI / **transport instrument** that builds risers, downlifters and swells locked to the song
grid. ASCEND reads the host transport, counts down to the next N-bar boundary, and runs **one
master tension envelope** across that countdown. The envelope simultaneously drives four things —
a filter sweep, a pitch rise, a stereo width bloom and a volume swell — over two blended sources
(filtered noise + a tonal root/fifth oscillator stack). At the target bar it fires an embedded
impact and auto-cuts to silence, then re-arms for the next boundary. It works standalone too: with
the transport stopped a manual **TRIGGER** (or a MIDI note) runs the same envelope over a
time-based length.

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

| Control | Range | What it does |
|---|---|---|
| **Key** | C … B | Root pitch class of the tonal stack |
| **Octave** | 0 – 6 | Root octave (C2 ≈ 65 Hz by default) |
| **Sync Target** | 8 / 16 / 32 / Custom | Countdown window length |
| **Custom Bars** | 1 – 64 bar | Window length when Sync = Custom |
| **Curve** | 0 – 1 (exp / lin / log) | Tension envelope shape morph |
| **Noise/Tone** | 0 – 100 % | Balance: 0 = all noise, 1 = all tonal |
| **Noise Color** | 0 – 100 % | White ↔ pink noise |
| **Saw/Sine** | 0 – 100 % | Tonal oscillator waveshape blend |
| **Filter Start** | 20 – 18 k Hz | SVF cutoff at the start of the countdown |
| **Filter End** | 20 – 18 k Hz | SVF cutoff at the boundary |
| **Pitch Rise** | 0 – 24 st | Semitone rise applied to the tonal stack at full tension |
| **Width Bloom** | 0 – 100 % | Maximum stereo width at full tension |
| **Impact** | on / off | Fire the embedded boom at the boundary |
| **Impact Level** | 0 – 100 % | Impact loudness |
| **Auto-Cut** | on / off | Gate the sources to silence at the boundary |
| **Downlifter** | on / off | Reverse the envelope (fall away after the boundary) |
| **Free Length** | 0.1 – 30 s | Envelope length when triggered with the transport stopped |
| **Level** | −24 … +6 dB | Output trim |
| **Key Track** | on / off | Root follows the last MIDI note |
| **Trigger** | momentary | Manually run the envelope (free-run) |

Stereo instrument, **zero latency**. A live **countdown** readout on the GUI shows the bars
remaining until the next boundary.

## Presets

Riser 8 Dark · Riser 16 Wide · Sub Boom Drop · Downlifter 8 · Noise Swell Short · Melodic Fifth Rise.
Renders for each land in `renders/ASCEND/`.
