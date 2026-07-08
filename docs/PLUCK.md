# PLUCK — Karplus-Strong strummer (Strum clone)

An **effect with MIDI input**. The **audio input is the exciter**: an onset detector fires
a **strum** — a staggered excitation across **six Karplus-Strong strings** — and the input's
timbre colors each pluck. The strings are tuned from a dark **chord table**, from **held MIDI
notes**, or from a coarse **chromagram key-detect** over the input. A small embedded **modal
body IR** colors the wet path. Zero reported latency; `mix = 0` nulls against dry.

```
 in(audio) ─► onset detect ─► capture 500-sample burst (Hann) ─┐
                                                                 ▼  (staggered per string)
   held MIDI / chord table / key-detect ─► tune 6 strings ─► KS loops ─► pan ─► Σ
                                                                                 │
                                          body IR (modal, 2048-tap) convolve ◄───┤
                                                                                 ▼
                                                          (1-body)·dry_wet + body·conv
                                                                                 ▼
                                                            dry ─► mix ─► out (Wet Solo)
```

## The strings (extended Karplus-Strong)

Each of the six strings is a resonant feedback loop:

- a **Catmull-Rom fractional delay line** (copied from FLYBY's `dsp::FracDelay`) — the loop
  length sets the pitch; the fractional read tunes it to any frequency exactly;
- a **one-pole (2-tap) damping low-pass** in the loop — sets brightness and the
  frequency-dependent decay (highs die faster than lows);
- a first-order **all-pass fine-tune** in the loop — its low-frequency phase delay is
  subtracted from the delay read so the fundamental stays in tune while the upper partials
  are gently stretched (a touch of string stiffness / inharmonicity).

The loop delay is set so that `frac_read + damp_delay + allpass_delay == sample_rate / f0`,
which keeps the tuned fundamental within a cent of target (verified to **±10 cents** by the
done-bar test). The loop **feedback** is derived from **Decay** for a target ~60 dB sustain
time (0.3 s … 12 s), per string (so higher strings ring shorter for the same setting, as real
strings do).

## Tuning sources

- **Chord** — a dark-taste chord table voiced across six strings (low→high) on a selectable
  **Root** (C…B): **Minor** `[0 7 12 15 19 24]`, **Minor 7** `[0 7 10 15 19 22]`,
  **Sus2** `[0 7 12 14 19 24]`, **Minor 9** `[0 7 10 14 15 19]`, **5th Stack**
  `[0 7 12 19 24 31]`, **Sus4** `[0 7 12 17 19 24]` (semitone offsets from the root; base
  note C2). **Spread** detunes the strings symmetrically by up to ±50 cents.
- **MIDI Held** — up to six held notes, voice-assigned low→high; extra strings octave-double
  the held notes. Play a chord on a MIDI track feeding PLUCK and the strings retune live.
- **Key Detect** — a coarse **chromagram** over the input (`suite_core::stft`, 4096/1024):
  the running 12-class chroma picks a **root** and **minor/major** quality; when the
  confidence clears the gate the strings tune to that key (minor → Minor voicing, major →
  Sus2), otherwise they fall back to the **Chord/Root** setting. Good for sympathetic
  resonance that tracks a mix.

## Excitation & strum

- On an **onset** (fast/slow envelope with a 40 ms refractory), PLUCK captures the **next
  ~500 samples** of the input, Hann-windowed, as the **exciter burst** — the actual pick
  attack and timbre.
- The burst is then injected into the strings **staggered**: a **Strum Time** of 5–80 ms is
  divided into five equal strides (six strings, five gaps ⇒ stride = **strum-time / 5**),
  in the configured **Direction** — **Up** (low→high), **Down** (high→low), or **Alternate**
  (flips each strum). Higher **Velocity** (onset level) raises the excitation gain and, via
  **Vel→Bright**, opens the damping (brighter on hard hits).
- **Continuous** drive feeds the input into every string at low gain constantly (a droning,
  sympathetic-resonance mode) independent of the strum.

## Body

A small **modal body IR** (2048 taps, per SPECS) is generated at init as a sum of a few decaying modal
resonances (≈98/196/392/740/1300/2600 Hz) plus a direct impulse, L2-normalized. It is
convolved into the wet path by direct FIR (cheap at this length) and blended by **Body**
(`(1-body)·wet + body·conv`), adding a woody/plausible instrument-body resonance. It is
causal (starts at tap 0), so it adds **no latency** and does not affect the `mix = 0` null.

## Stereo & output

- **Stereo Alt** pans the strings alternately left/right (equal-power), widening the strum.
- **Wet Solo** outputs the pure string/body signal (ignores Mix).
- **Mix** blends dry/wet; the dry path is a **zero-latency direct copy**, so `mix = 0` is an
  exact passthrough (nulls against dry < −80 dB). **Out** is the final trim.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Source | Chord / MIDI Held / Key Detect | tuning source |
| Root | C … B | root pitch class (Chord / Key-Detect fallback) |
| Chord | m / m7 / sus2 / m9 / 5th / sus4 | six-string voicing |
| Decay | 0–100 % | sustain time (0.3–12 s target) |
| Damp | 0–100 % | loop low-pass — brightness / HF decay |
| Strum Time | 5–80 ms | total strum span (stride = time/5) |
| Direction | Up / Down / Alternate | strum order |
| Exciter Gain | 0–2× | burst injection level |
| Continuous | off / on | constant low-gain input drive |
| Vel→Bright | 0–100 % | onset level opens the damping |
| Body | 0–100 % | modal body-IR convolution amount |
| Spread | 0–50 ct | per-string detune |
| Stereo Alt | 0–100 % | alternate strings L/R |
| Wet Solo | off / on | output pure resonance |
| Mix | 0–100 % | dry/wet (0 = exact passthrough) |
| Out | ±24 dB | output trim |

## Presets

**Dark Nylon** (warm close minor chord, the reference), **Metallic Cloud** (bright
continuously-driven 5th-stack), **Sub Harp** (deep slow m9, long decay/dark), **Sympathetic
Wash** (key-tracked continuous resonance under the dry), **Staccato Machine** (tight fast
short-decay sus4 downstrums), **Detuned Dream** (wide, heavily-detuned sus2 alternate strums).

## Done-bar (offline harness)

1. **Tuning** — C-minor trigger → each string's spectral peak lands within **±10 cents** of
   its chord fundamental (windowed single-frequency DFT peak search per string).
2. **Decay** — tail RMS drops **> 20 dB** over the decay setting's window (tested short and
   long).
3. **Strum** — per-string onset offsets staggered by **strum-time/5 ±20 %** in the configured
   direction (Up increasing, Down decreasing).
4. **MIDI** — held **E2 + G2 + B2** retunes the strings; peaks land at those pitches and
   favor E2 over the C-chord's C2.
5. Universal (finite, ≤ 0 dBFS, non-silent) on all six preset renders (both channels);
   `mix = 0` nulls against dry < −80 dB (both channels).

Renders are written to `renders/PLUCK/*.wav`.

## Using it in FL Studio

Add **Qeynos PLUCK** on an audio track (drums, a vocal chop, a synth stab, noise). Hit play:
the input's transients strum the strings. Pick a **Chord** + **Root**, set **Strum Time** and
**Direction**, and dial **Decay/Damp** for the tone. Feed PLUCK from a **MIDI** track and set
**Source = MIDI Held** to play chords on the strings. **Source = Key Detect** makes the
strings track the input's key for a sympathetic wash — try it with **Continuous** on and a low
**Mix**. **Body** adds the woody resonance; **Spread** + **Stereo Alt** widen it; **Wet Solo**
auditions the pure resonance.

## What It Is

A Karplus-Strong strummer that turns your audio into the pick: an onset detector strums six
tuned strings using the input's transient as the exciter, so drums, vocals, or noise become
plucked chords. The strings are tuned from a dark chord table, held MIDI notes, or a key-detect
that tracks the input, and a modal body IR gives it a woody instrument resonance.

## Signal Flow

```
 in(audio) ─► onset ─► capture exciter burst ─┐  (staggered by Strum Time / Direction)
 held MIDI / Chord+Root / Key Detect ─► tune 6 strings ─► KS loops (Decay, Damp) ─► Σ
                                                          Body IR · Spread · Stereo Alt
                                                                       │
                                                    dry ─ Mix / Wet Solo ─ Out ◄──┘
```

## Controls

- **Source** — string tuning source: Chord, MIDI Held, or Key Detect.
- **Root** — root pitch class (C…B) for the Chord / Key-Detect fallback.
- **Chord** — six-string voicing: Minor, Minor 7, Sus2, Minor 9, 5th Stack, or Sus4.
- **Decay** — string sustain time, 0–100 % (≈0.3–12 s target).
- **Damp** — loop low-pass; brightness and how fast highs die, 0–100 %.
- **Strum Time** — total strum span across the six strings, 5–80 ms (stride = time/5).
- **Direction** — strum order: Up, Down, or Alternate.
- **Exciter Gain** — level of the captured burst injected into the strings, 0–2×.
- **Continuous** — feed the input into every string constantly for a droning sympathetic mode, on/off.
- **Vel→Bright** — how much onset level opens the damping (brighter on hard hits), 0–100 %.
- **Body** — modal body-IR convolution amount, 0–100 %.
- **Spread** — per-string detune, 0–50 cents.
- **Stereo Alt** — pan the strings alternately L/R to widen the strum, 0–100 %.
- **Wet Solo** — output the pure string/body signal, ignoring Mix, on/off.
- **Mix** — dry/wet, 0–100 % (0 = exact passthrough).
- **Out** — output trim, −24…+24 dB.

## Recipes

1. **Dark-techno pluck stab** — load **Warehouse Pluck** (Source Chord, Chord 5th Stack, Root C, Decay 35 %, Damp 45 %, Strum Time 10 ms, Direction Up, Mix 100 %): a tight, dry, percussive power-5th hit that reads like a warehouse-techno stab.
2. **Atmospheric-dnb bed** — load **Nocturne Drift** (Chord Minor 9, Root Eb, Decay 85 %, Strum Time 60 ms, Spread 18 ct, Stereo Alt 70 %, Mix 75 %): slow, hazy, wide upstrums for a nocturne pad under the beat.
3. **Vocal-rip sympathetic wash** — load **Sympathetic Wash** (Source Key Detect, Continuous on, Decay 95 %, Body 70 %, Mix 60 %) on a vocal so the strings ring in the vocal's own key, a resonant ghost trailing under each line.
