# HALT — performance buffer FX

*Phase 3. A 4-bar circular buffer that is always recording, replayed live by four momentary
performance modes: tape-stop, stutter, reverse, and half-speed.*

HALT sits on any track and continuously records the incoming stereo audio into a **4-bar
circular buffer**. While no mode is engaged it is a **bit-exact passthrough** — the input is
returned untouched. Trigger a mode (a button, host automation, or a MIDI note) and HALT stops
passing the live signal and instead plays the recent past back through a moving read head:
brake the tape to a halt, loop the last beat, run it backwards, or drop it to half speed. Every
transition is a **5 ms equal-power crossfade**, so it never clicks. It is a DJ/performance and
glitch tool for build-ups, drops, fills, and stutter edits.

## Signal flow

```
in ─┬───────────────────────────────────────────────────────── dry ──────────────────┐
    │                                                                                   ├─(1-mix)/mix─► out
    └─► 4-bar circular capture (ALWAYS recording) ──► read head(s) ──────────► wet ─────┘
            modes (momentary · last-pressed wins · 5 ms equal-power crossfades):
              • TAPE STOP  read rate ramps 1 → 0 over Stop Time (synced/free) with a curve
              • STUTTER    loop the last 1/4..1/64 (Decay + Pitch Step per repeat, quantized)
              • REVERSE    read backward from the trigger point
              • HALF-SPEED read forward at rate 0.5
```

## The modes

- **TAPE STOP** — the read rate ramps from 1 down to 0 over the **Stop Time** (a transport-
  synced length — 1 beat / ½ bar / 1 bar / 2 bar — or a **Free** time in seconds). As the rate
  falls the pitch drops and the playback reads slower into the past, exactly like braking a
  turntable/tape. The **Stop Curve** morphs the deceleration shape (exp ↔ linear ↔ log).
  **Release** = *Instant* (crossfade straight back to the live signal) or *Ramp* (spin the tape
  back up to speed first, then rejoin — a reverse tape-stop).
- **STUTTER** — loops the last **Stutter Div** (1/4, 1/8, 1/16, 1/32, 1/64) of the buffer.
  **Decay** lowers each successive repeat's level; **Pitch Step** transposes each repeat by a
  fixed interval (the loop *period* stays exact — the read speeds up within the slice rather
  than shortening the loop). **Quantize** (off / 1/16 / 1/8 / 1/4) snaps the loop's anchor to
  the beat grid so the stutter locks musically.
- **REVERSE** — reads the buffer backward from the trigger point.
- **HALF-SPEED** — reads forward at rate 0.5 (down an octave, half tempo), with a smooth
  engage/disengage.

## Triggering

Each mode is a **momentary button** in the GUI, a **host-automatable bool parameter** (so you
can automate the performance), and a **MIDI note**: notes **C1..D#1** (MIDI 36–39) map to
tape-stop / stutter / reverse / half-speed respectively (MidiConfig::Basic). Hold to engage,
release to disengage.

**Priority — last-pressed wins.** If several modes are held at once, the most-recently pressed
one sounds; releasing it falls back to the next-most-recent held mode, and releasing all
returns to the dry passthrough. Every switch is a 5 ms equal-power crossfade.

## Timing & transport

- **Tempo** drives the stutter division length and the synced tape-stop durations.
- **Quantize** uses the host **playhead** to snap the stutter loop to the beat grid.
- **Free-run:** when the host transport is **stopped**, the modes still run at the host tempo
  (quantize falls back to anchoring at the present, since there is no grid to snap to).

## Null / latency

- **Zero latency.** The dry path is never delayed — the wet is a re-timed creative signal.
- **Inactive → bit-exact passthrough.** With no mode engaged (and no crossfade in flight) HALT
  returns the input verbatim, sample for sample.
- **`mix = 0` → passthrough.** While a mode *is* active, `out = (1-mix)·dry + mix·wet`, so
  `mix = 0` is an exact passthrough of the dry (the suite null contract). **Mix** is usually
  left at 100%. **Out** trims the wet level.

## Buffer

The capture is a **4-bar circular buffer**, preallocated at `set_sample_rate` for the worst
case: **32 s** = 4 bars of 4/4 down to **30 BPM** at the host sample rate. Slower tempos clamp
the effective read window to that length (a benign limit for a momentary performance effect).
The buffer records continuously and `process()` is allocation-free.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Tape Stop / Stutter / Reverse / Half Speed | button (bool) | momentary mode triggers; also MIDI C1..D#1 |
| Stutter Div | 1/4 · 1/8 · 1/16 · 1/32 · 1/64 | stutter loop length |
| Decay | 0..100 % | per-repeat level loss (0 = no decay) |
| Pitch Step | −12..+12 st | per-repeat transpose (loop period stays exact) |
| Stop Time | Free · 1 Beat · ½ Bar · 1 Bar · 2 Bar | tape-stop duration (synced) |
| Stop Free | 0.05..4 s | tape-stop duration when Stop Time = Free |
| Stop Curve | 0..1 | deceleration shape (0 exp · 0.5 linear · 1 log) |
| Release | Ramp · Instant | tape-stop release behaviour |
| Quantize | Off · 1/16 · 1/8 · 1/4 | snap the stutter loop anchor to the beat grid |
| Mix | 0..100 % | dry/wet (usually 100 %) |
| Out | −24..+24 dB | wet output trim |

## Presets

**Classic Stop 1 Bar**, **Fast Brake**, **Stutter 16th Decay**, **Reverse Sweep**, **Half Time
Groove**, **DJ Kill**. Presets set the *character* knobs only — the four momentary mode buttons
are live performance state and are never stored.

## Modulation (NERVE)

**Mix**, **Out**, and **Decay** are exposed to the suite modulation bus — expand the **MOD**
section under the preset bar and route a **Qeynos NERVE** stream to them (see [NERVE](NERVE.md)).

## Done-bar (offline, `cargo test -p halt`)

Universal (finite / ≤ 0 dBFS / non-silent, `mix = 0` nulls) on all six preset renders
(→ `renders/HALT/`), plus:

1. **Tape-stop** — a 300 Hz sine glides **monotonically to < 50 Hz** within the configured
   duration (measured by zero-crossing frequency across the stop).
2. **Stutter** at 1/8 @ 120 BPM — the looped onset **period == 250 ms ±1 ms** across ≥4 repeats.
3. **Reverse** — the output segment **cross-correlates > 0.9** with the time-reversed buffer
   content (and correlates more with the reversed than the forward source).
4. **Transitions** — engage and disengage produce a **max sample-delta ≤ 3× steady-state**
   (no clicks).
5. **Inactive → bit-exact passthrough** (sample-for-sample equal), and **`mix = 0` while a mode
   is active** nulls against the dry (< −120 dB).

## Using it in FL Studio

Put **Qeynos HALT** on a track or bus (a drum bus, a full mix, a loop). Leave it idle and it is
transparent. Then:

- Automate or click **TAPE STOP** at the end of a phrase for a turntable/tape brake — set
  **Stop Time** to 1 Bar for a slow build-drop, **Fast Brake** for a quick halt.
- Hold **STUTTER** on a beat for a beat-repeat/roll; pick **Stutter Div** for the rate, add
  **Decay** for a fading roll and **Pitch Step** for a rising/falling glitch. Turn **Quantize**
  on (with the transport rolling) so it locks to the grid.
- **REVERSE** for reverse-fills; **HALF-SPEED** for a half-time drop.
- Drive the modes from a MIDI clip (notes **C1..D#1**) to sequence a performance, or automate
  the mode buttons. Multiple at once → the last one wins.

<!-- BUILT-IN-MANUALS: canonical sections rendered in-GUI by the '?' button (parsed by suite_core::manual). -->

## What It Is

A performance buffer FX: a 4-bar circular buffer is always recording, and four momentary modes
replay the recent past live — **tape-stop**, **stutter**, **reverse** and **half-speed**. Idle it
is a bit-exact passthrough; hold a mode (button, automation or MIDI note C1–D#1) for build-ups,
drops, fills and glitch stutter edits, with a 5 ms crossfade on every transition so it never clicks.

## Signal Flow

```
 in ─┬────────────────────────────────────────── dry ─────────────────────┐
     └─ 4-bar circular capture (always recording) ─ read head(s) ─ wet ─────┤─ (1−Mix)/Mix ─ Out ─► out
        momentary modes (last-pressed wins · 5 ms crossfades):             │
          Tape Stop  rate 1→0 over Stop Time, Stop Curve, Release          │
          Stutter    loop last Stutter Div · Stutter Decay · Pitch Step · Quantize
          Reverse    read backward from trigger      Half Speed  read at rate 0.5
```

## Controls

- **Tape Stop** — momentary button (MIDI C1): brakes playback rate 1→0 to a dead stop; hold to engage.
- **Stutter** — momentary button (MIDI C#1): loops the recent past as a beat-repeat; hold to engage.
- **Reverse** — momentary button (MIDI D1): plays the buffer backward from the trigger point.
- **Half Speed** — momentary button (MIDI D#1): reads forward at rate 0.5 (down an octave, half tempo).
- **Stutter Div** — stutter loop length: 1/4 / 1/8 / 1/16 / 1/32 / 1/64.
- **Stutter Decay** — per-repeat level loss, 0–100 % (0 = a flat, non-fading roll).
- **Pitch Step** — per-repeat transpose, −12…+12 st (the loop period stays exact — the read speeds up).
- **Stop Time** — synced tape-stop duration: Free / 1 Beat / ½ Bar / 1 Bar / 2 Bar.
- **Stop Free** — tape-stop duration when **Stop Time = Free**, 0.05–4 s.
- **Stop Curve** — tape-stop deceleration shape, 0–1 (0 exp · 0.5 linear · 1 log).
- **Release** — tape-stop release: **Ramp** (spin back up to speed, then rejoin) or **Instant** (jump straight back to live).
- **Quantize** — snap the stutter loop anchor to the beat grid: Off / 1/16 / 1/8 / 1/4.
- **Mix** — dry/wet blend, 0–100 % (usually 100 %); **Mix = 0** is an exact passthrough even while a mode is active.
- **Out** — wet output trim, ±24 dB.

Note: the four mode buttons are live performance state — presets store only the character knobs.

## Recipes

1. **Dark-techno power-down — "Warehouse Power-Down"** — at the end of a phrase, hold **Tape Stop**
   with **Stop Time 2 Bar**, **Stop Curve 0.85** (slow-then-fast collapse), **Release Ramp**,
   **Mix 100 %**, **Out −1 dB**. The whole groove brakes to a dead stop into the drop.
2. **Atmospheric-DnB half-time drop — "Half-Time Haze"** — hold **Half Speed** across a bar for an
   instant octave-down, half-tempo section; the preset sets **Stutter Decay 15 %**, **Stop Curve 0.65**,
   **Mix 100 %**, **Out −0.5 dB**. Perfect for dropping a break into a hazy half-time.
3. **Vocal-rip stutter chop — "Amen Skip 32nd"** — on a ripped vocal or break, hold **Stutter**:
   **Stutter Div 1/32**, **Stutter Decay 15 %**, **Quantize 1/8** (locks the roll to the grid),
   **Pitch Step 0**, **Mix 100 %**. Tap it rhythmically for glitchy beat-repeat stutters on the phrase.
