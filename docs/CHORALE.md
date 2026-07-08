# CHORALE — resonator bank

An **effect with MIDI input**. A bank of **12–24 waveguide resonators** (extended
Karplus-Strong loops) is **continuously excited by the audio input**, so the tuned loops
**ring sympathetically** with whatever plays through them. The bank tunes to a **held MIDI
chord**, a **selected scale/chord on a Root** (spread across octaves), or a **chromagram
key-detect** over the input. Each resonator's input drive is optionally weighted by the
input's **band energy at its pitch**, so the bank "sings" the notes present in the source.
Zero reported latency; `mix = 0` nulls against the dry input.

## What It Is

CHORALE is a bank of tuned string-like resonators that the incoming audio plays. Feed it drums,
a pad, or a vocal and 12–24 waveguides ring sympathetically in a chord, scale, or the detected
key — turning any source into a shimmering, singing wash. It is zero-latency and nulls at
**Mix = 0**, so it sits harmlessly inline until you blend the resonance in.

## Signal Flow

```
 in(audio) ─┬─────────────────────────────────────────────────────────► dry ─┐
            ├─ SpectrumTap (input band energies) ─► per-resonator weights     │
            │                                                                  │
   held MIDI / scale+root / key-detect ─► tune 12–24 resonators               │
            │                                                                  │
            └─ continuous drive ×weightᵢ ─► KS loops (self-limiting) ─► pan ─► Σ ─► DC-block ─► wet
                                                                                          │
                                                                 (1-mix)·dry + mix·wet ◄──┘
                                                                                          ▼
                                                                          out (Wet Solo overrides mix)
```

## The resonators (extended Karplus-Strong)

Each resonator is a resonant feedback loop, the PLUCK recipe:

- a **Catmull-Rom fractional delay line** — the loop length sets the pitch; the fractional
  read tunes it to any frequency exactly;
- a **one-pole (2-tap) damping low-pass** in the loop — sets brightness and the
  frequency-dependent decay (highs die faster than lows);
- a first-order **all-pass fine-tune** in the loop — its low-frequency phase delay is
  subtracted from the delay read so the fundamental stays in tune.

The loop delay is solved so `frac_read + damp_delay + allpass_delay == sample_rate / f0`,
keeping the tuned fundamental within a cent of target (verified to **±10 cents** by the
done-bar). The loop **feedback** is derived from **Decay** for a target sustain time
(≈0.3 s … 18 s), per resonator (higher resonators ring shorter for the same setting).

Because the bank is driven **continuously** at high feedback, each loop's feedback passes
through a **tanh soft-clip**: the loop **self-limits** at a bounded amplitude instead of
running away, and — being a memoryless nonlinearity — this does **not** shift the resonant
pitch. The summed wet is **DC-blocked** (~5 Hz one-pole) before the mix.

## Tuning sources

- **Scale/Chord** — a **Root** (C…B) plus a scale/chord type, spread across the bank by
  walking its semitone offsets and stacking octaves (`offset[i mod L] + 12·(⌊i/L⌋ mod 5)`),
  so the resonators fan out over up to **five octaves**; past that the pitches wrap back
  down and stack (duplicates then detune under **Spread**, thickening the sound). Types:
  **Minor Triad** `[0 3 7]`, **Major Triad** `[0 4 7]`, **Minor 7** `[0 3 7 10]`,
  **Major 7** `[0 4 7 11]`, **Sus2** `[0 2 7]`, **Sus4** `[0 5 7]`, **5th Stack** `[0 7]`,
  **Minor Pentatonic** `[0 3 5 7 10]`, **Major Pentatonic** `[0 2 4 7 9]`, **Phrygian**
  `[0 1 3 5 7 8 10]`, **Dorian** `[0 2 3 5 7 9 10]`, **Octaves** `[0]` (base note C2).
- **MIDI Held** — the held notes are voice-assigned low→high, extra resonators octave-stack
  them. Play a chord on a MIDI track feeding CHORALE and the bank retunes live.
- **Key Detect** — a coarse **chromagram** over the input (`suite_core::stft`, 4096/1024)
  picks a **root** and **minor/major** quality; when the confidence clears the gate the bank
  tunes to a minor or major triad on that root, otherwise it falls back to the **Scale/Root**
  setting. Good for a resonant wash that tracks a mix.

**Spread** detunes the resonators by up to ±50 cents, **alternating** sign per resonator
(even +, odd −), which also detunes the octave-wrapped duplicates into a chorus.

## Sympathetic weighting

The input is analysed by a **32-band constant-Q filter bank** (`suite_core::spectrum::
SpectrumTap`, ~⅓-octave, alloc-free). Each resonator's continuous input gain is
`base × ((1 − amount) + amount · normalized band energy at its pitch)`. At **Sympathetic = 0**
every resonator is driven equally; at **1** the drive follows the source's spectrum, so a
resonator only "sings" when the input has energy near its pitch. The weights refresh a few
times a second (block-size-independent), cheaply.

## Stereo & output

- **Stereo** pans the resonators **alternately** left/right (even L / odd R, equal-power)
  with the amount as width.
- **Wet Solo** outputs the pure resonance (ignores Mix).
- **Mix** blends dry/wet; the dry path is a **zero-latency direct copy**, so `mix = 0` is an
  exact passthrough (nulls against dry < −80 dB). **Out** is the final trim.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Source | Scale/Chord · MIDI Held · Key Detect | tuning source |
| Root | C … B | root pitch class (Scale / Key-Detect fallback) |
| Scale/Chord | 12 types (see above) | bank voicing, spread across octaves |
| Resonators | 12 … 24 | number of active resonators |
| Decay | 0–100 % | ring/sustain time (≈0.3–18 s target) |
| Damp | 0–100 % | loop low-pass — brightness / HF decay |
| Spread | 0–50 ct | alternating ± detune |
| Sympathetic | 0–100 % | weight drive by input band energy at each pitch |
| Excite | 0–2× | continuous input drive level |
| Stereo | 0–100 % | alternate resonators L/R with width |
| Wet Solo | off / on | output pure resonance (ignores Mix) |
| Mix | 0–100 % | dry/wet (0 = exact passthrough) |
| Out | ±24 dB | output trim |

## Controls

- **Source** — tuning source for the bank: **Scale/Chord**, **MIDI Held**, or **Key Detect**.
- **Root** — root pitch class (C…B) for the Scale voicing and the Key-Detect fallback.
- **Scale/Chord** — the chord/scale type spread across the bank (Minor Triad … Octaves), 12 types.
- **Resonators** — number of active resonators (density of the bank), 12–24.
- **Decay** — ring / sustain time of each resonator (≈0.3–18 s target), 0–100 %.
- **Damp** — in-loop low-pass: brightness and how fast the highs die, 0–100 %.
- **Spread** — alternating ± detune per resonator for chorus thickness, 0–50 cents.
- **Sympathetic** — weights each resonator's drive by the input's band energy at its pitch, so the
  bank only sings where the source has energy, 0–100 %.
- **Excite** — continuous input drive level into the bank, 0–2×.
- **Stereo** — pans resonators alternately L/R as a width control, 0–100 %.
- **Wet Solo** — outputs the pure resonance, ignoring Mix (audition the bank alone).
- **Mix** — dry/wet blend; 0 % is an exact zero-latency passthrough, 0–100 %.
- **Out** — output trim, ±24 dB.

## Recipes

1. **Iron Drone Bed (dark techno)** — **Phrygian Drone Bed**, **Source = Scale/Chord**, **Root** E,
   **Decay** ~96 %, **Damp** high, **Mix** ~65 %. Feed it a rumble or noisy pad and it becomes a
   slow, dark harmonic bed under the kick. Raise **Spread** for a wider, more unstable drone.
2. **Ghost In The Break (atmospheric dnb)** — **Ghost In The Signal**, **Source = Key Detect**,
   **Sympathetic** 100 %, **Mix** ~50 % on a break bus. The bank tracks the tune's key and only
   rings where the drums leave space — a haunted wash that follows the music.
3. **Choir From A Vocal (vocal-rip)** — **Glass Choir** on a chopped vocal, **Source = Scale/Chord**
   (or **MIDI Held** to play chords in over it), low **Damp**, **Stereo** ~85 %, **Wet Solo** on to
   audition. The vocal excites a wide glassy choir you can print and resample.

## Presets

**Sympathetic Am** (A-minor bank singing under the source, the reference), **Phrygian Drone
Bed** (dark, slow, damped E-Phrygian, wide), **Glass Choir** (bright low-damped major-7,
full bank, big spread + stereo), **Sub Resonance** (deep octave-stacked C, narrow/dark),
**Wide Shimmer Strings** (lush heavily-detuned sus2, very wide), **Tight Body** (short
resonant power-5, wet-forward).

## Done-bar (offline harness)

1. **Tuning** — noise excitation, A-minor selected → the **strongest N** resonator peaks land
   within **±10 cents** of their tuned pitches (windowed single-frequency DFT peak search).
2. **Decay scales RT** — after a burst then silence, a short **Decay** collapses the tail far
   more than a long one over the same window (short drop > 20 dB and ≥ long + 12 dB).
3. **MIDI** — held **E2 + G2 + B2** retunes the bank; peaks land at those pitches and favor
   E2 over the scale root's C2.
4. **Null / Wet Solo** — `mix = 0` nulls against the dry input < −80 dB (both channels);
   **Wet Solo** ignores mix and outputs non-silent resonance far from the dry.
5. **Sympathetic weighting** — fed white noise (which a constant-Q analyser reads as rising),
   full weighting emphasizes the high-band resonators (top/bottom energy ratio climbs clearly
   above the flat case); plus universal (finite, ≤ 0 dBFS, non-silent) on all six preset
   renders and an extremes fuzz.

Renders are written to `renders/CHORALE/*.wav`.

## Using it in FL Studio

Add **Qeynos CHORALE** on an audio track or bus. Neutral it is an exact passthrough at
**Mix = 0**; raise **Mix** to blend the resonance in. Pick a **Scale/Chord** and **Root** (or
feed a **MIDI** chord and set **Source = MIDI Held**, or **Key Detect** to track the input's
key). Play material through it and the bank rings sympathetically. Raise **Sympathetic** so
the resonators only sing where the source has energy; set **Decay/Damp** for the tail and
tone, **Spread** + **Stereo** to widen, **Count** for density, **Excite** for drive. **Wet
Solo** auditions the pure resonance. Try **Sympathetic Am** under a vocal or drum loop,
**Phrygian Drone Bed** / **Sub Resonance** for dark beds, **Glass Choir** / **Wide Shimmer
Strings** for lush washes, **Tight Body** to add a resonant body to percussion.
