# TRACER — pitch-tracking multiband saturation

## What It Is

TRACER detects the fundamental (f0) of the incoming signal and locks a Linkwitz-Riley crossover
tree to it: each crossover cutoff rides a harmonic multiple of f0, so as a note glides the bands
follow, always saturating the same harmonic region (fundamental, body, presence…). Each band is
driven through the suite waveshaper bank at 2x oversampling and summed, giving pitch-consistent
harmonic color on basses, Reeses, vocals, and leads — or, with crossovers set Fixed, a classic
static multiband saturator for buses.

## Signal Flow

```
in ─┬─ mono sum → MPM pitch detect (decimated ~12 kHz, window 1024)
    │            → median-5 → ±35-cent hysteresis → Hz/ms slew → f0, confidence
    │
    └─ LR4 crossover tree (cutoffs = harmonic × f0 × 2^SmartFreq, recomputed per
         32-sample control block; confidence < 0.6 freezes them)
           band0..3: [drive → shaper(bank) → 2x OS → level] → sum → DC-block → mix → out
```

The summed wet path passes through a ~10 Hz DC blocker before the dry/wet mix: heavy
odd-symmetric saturation of an asymmetric bass envelope (and sub-audio detune wander)
leaks a small offset that would otherwise eat one-sided headroom on a bass. The blocker
is wet-only (the `Mix` = 0 null against dry is untouched) and costs ≈ −0.25 dB at 41 Hz,
so the lowest 808/Reese fundamentals stay powerful and clean.

## Pitch tracking (`suite_core::pitch`)

The **McLeod Pitch Method** (NSDF + key-maximum peak picking + parabolic interpolation)
runs on a mono-summed, anti-aliased, ~12 kHz-decimated stream over a 1024-sample window,
producing `(f0_hz, confidence)`. Post-processing: median-of-5, ±35-cent hysteresis (small
wobble is ignored), and a Hz/ms slew limit. When confidence drops below 0.6 the last
confident f0 is held, so the crossovers freeze rather than chase noise. In **MIDI** mode
the last note-on frequency drives the pitch and the detector is bypassed.

## Crossovers — time-varying LR4

Each crossover is a Linkwitz-Riley 4th-order split: two cascaded 2nd-order Butterworth
(Q = 1/√2) sections for the low and high halves. They are built from the suite's **TPT
state-variable filter**, which is topology-preserving and unconditionally stable under
per-block cutoff modulation — this is what makes the pitch-locked, gliding crossover safe
(SPECS calls the time-varying LR4 the hard part). Cutoffs are recomputed every 32 samples
with filter state preserved, clamped to `[20, 0.45·Fs]` and kept monotonic. A NaN/blow-up
guard resets the tree and crossfades the wet path back in (256-sample fade) if parameter
fuzzing ever pushes a section unstable.

**Smart Frequency** shifts every tracked crossover by `2^knob` octaves (detents at
fundamental / body / presence). Base harmonic multiples are `×1.5, ×4, ×8` of f0, so band 1
sits below `1.5·f0` and is dominated by the fundamental. Each crossover can be **Track**
(harmonic × f0) or **Fixed** (a set Hz), so TRACER doubles as a fixed-band multiband
saturator.

## Constant color

With **Constant Color** on, each band's drive is scaled by a coarse inverse
equal-loudness weight (an ISO-226-shaped 11-point lookup, log-interpolated) at the band
center: bands where the ear is less sensitive get proportionally more drive so the added
harmonic color reads evenly across the spectrum. It is a color compensation, not a
measurement; the multiplier is clamped to a sane range.

## Controls

- **Pitch Mode** — pitch source: **Detect** (MPM detector tracks the input f0) or **MIDI** (the
  last note-on frequency drives the crossovers, detector bypassed).
- **Bands** — number of active bands, **2 / 3 / 4** (band 0 = lowest); fewer for broad color,
  more for surgical per-region drive.
- **Smart Freq** — octave offset applied to every tracked crossover, sliding the whole band
  layout up or down relative to f0. −2…+3 oct.
- **Constant Color** — inverse equal-loudness drive weighting so harmonic color reads evenly
  across the spectrum. on/off.
- **Slew** — pitch slew limit; low values glide the crossovers smoothly through pitch changes,
  high values snap. 5…2000 Hz/ms.
- **Trim** — input trim into the wet path. −24…+24 dB.
- **XO1 Mode** — crossover 1 source: **Track** (harmonic × f0) or **Fixed** (a set Hz).
- **XO1 Fixed** — the fixed cutoff frequency for crossover 1 when **XO1 Mode** = Fixed.
  20…20000 Hz.
- **XO2 Mode** — crossover 2 source: **Track** or **Fixed**.
- **XO2 Fixed** — the fixed cutoff for crossover 2 when **XO2 Mode** = Fixed. 20…20000 Hz.
- **XO3 Mode** — crossover 3 source: **Track** or **Fixed**.
- **XO3 Fixed** — the fixed cutoff for crossover 3 when **XO3 Mode** = Fixed. 20…20000 Hz.
- **Band 1 Drive** — saturation drive into band 1's shaper (the lowest band). 0…48 dB.
- **Band 1 Shape** — waveshaper for band 1: **Tube / Tape / Fold / Hard**.
- **Band 1 Level** — output level of band 1 (−36 ≈ mute). −36…+12 dB.
- **Band 2 Drive** — saturation drive into band 2's shaper. 0…48 dB.
- **Band 2 Shape** — waveshaper for band 2: **Tube / Tape / Fold / Hard**.
- **Band 2 Level** — output level of band 2 (−36 ≈ mute). −36…+12 dB.
- **Band 3 Drive** — saturation drive into band 3's shaper. 0…48 dB.
- **Band 3 Shape** — waveshaper for band 3: **Tube / Tape / Fold / Hard**.
- **Band 3 Level** — output level of band 3 (−36 ≈ mute). −36…+12 dB.
- **Band 4 Drive** — saturation drive into band 4's shaper (the highest band; active with
  **Bands** = 4). 0…48 dB.
- **Band 4 Shape** — waveshaper for band 4: **Tube / Tape / Fold / Hard**.
- **Band 4 Level** — output level of band 4 (−36 ≈ mute). −36…+12 dB.
- **Mix** — dry/wet blend; at 0 % the output nulls against dry. 0…100 %.
- **Out** — output trim (hard-ceilinged at ±0.999). −24…+24 dB.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Pitch Mode | Detect / MIDI | Detect | MIDI note replaces the detector |
| Bands | 2 / 3 / 4 | 3 | Number of active bands (band 0 = lowest) |
| Smart Freq | −2..+3 oct | 0 | Octave offset on every tracked crossover |
| Constant Color | on/off | on | Inverse equal-loudness drive weighting |
| Slew | 5..2000 Hz/ms | 200 | Pitch slew limit |
| Trim | −24..+24 dB | 0 | Input trim (wet path) |
| XO1 Mode / XO2 Mode / XO3 Mode | Track / Fixed | Track | Cutoff source per crossover |
| XO1 Fixed / XO2 Fixed / XO3 Fixed | 20..20000 Hz | 200/1000/4000 | Fixed cutoff when Mode = Fixed |
| Band 1–4 Drive | 0..48 dB | 10/8/6/4 | Drive into the band's shaper |
| Band 1–4 Shape | Tube / Tape / Fold / Hard | Tube/Tube/Tape/Tape | Waveshaper from the suite bank |
| Band 1–4 Level | −36..+12 dB | 0 | Band output level (−36 ≈ mute) |
| Mix | 0..100 % | 100 | Dry/wet. At 0 %, output nulls against dry. |
| Out | −24..+24 dB | 0 | Output trim (hard-ceilinged at ±0.999) |

## Recipes

1. **Sliding 808 Grit** *(start: Sliding 808 Grit)* — **Pitch Mode** = Detect, **Bands** = 3,
   **Smart Freq** 0, keep the crossovers on **Track** (**XO1 Mode** / **XO2 Mode** / **XO3 Mode** =
   Track). Push **Band 1 Drive** ~14 dB with **Band 1 Shape** = Tube for fundamental weight and
   **Band 2 Drive** ~10 dB / **Band 3 Shape** = Tape for harmonic bite; as the 808 glides the grit
   stays locked to the note. **Constant Color** on, **Mix** 100 %.
2. **Vocal-Rip Fundamental Warmth** *(start: Vocal Fundamental Warmth)* — **Bands** = 3, **Band 1
   Drive** ~8 dB (**Band 1 Shape** = Tube) to thicken the vocal fundamental, keep **Band 3 Level**
   low so sibilance isn't amplified. Lower **Slew** (~80 Hz/ms) so the tracking glides smoothly over
   vibrato. Blend with **Mix** ~70 % for a warm, harmonically-fattened vocal.
3. **Dark-Techno Reese Bite** *(start: Bass Harmonic Push)* — **Bands** = 4, drive the upper bands
   hard (**Band 3 Drive** ~18 dB, **Band 4 Drive** ~16 dB with **Band 3 Shape** / **Band 4 Shape** =
   Fold) while keeping **Band 1 Drive** modest so the sub stays clean. Nudge **Smart Freq** −0.5 to
   pull the harmonic content down for a growling Reese.
4. **Fixed-Band Bus Saturator** *(start: Fixed-Band Bus Saturator)* — set every crossover to Fixed
   (**XO1 Mode** / **XO2 Mode** / **XO3 Mode** = Fixed) with **XO1 Fixed** ~200 Hz, **XO2 Fixed**
   ~1 kHz, **XO3 Fixed** ~4 kHz, and moderate per-band drive (**Band 1–4 Drive** ~6 dB). TRACER now
   runs as a classic static multiband saturator for drum buses and full mixes; **Out** −1 dB for
   headroom.

## Factory presets

Sliding 808 Grit · Vocal Fundamental Warmth · Lead Bite · Bass Harmonic Push ·
Fixed-Band Bus Saturator.

## Done-bar (mechanical)

1. **Sliding-saw**, pitch-locked band 1 → band-1 output energy centroid tracks f0 within
   ±1 semitone across the slide.
2. **White noise** (confidence collapses) → crossover frequencies frozen (unchanged over
   1 s).

Plus the universal assertions and a parameter-fuzz stability test (max drive, hard shaper,
degenerate fixed cutoffs → finite, ≤ 0 dBFS).

## Testing in FL

1. Options → Manage plugins → "Find more plugins", then add **Qeynos TRACER** to a
   channel/mixer insert.
2. Feed it a monophonic, pitched source (bass, 808, vocal, lead). Load **Sliding 808
   Grit**; as the note glides the bands follow the pitch.
3. For unpitched or drum-bus use, set the crossovers to **Fixed** (or load **Fixed-Band
   Bus Saturator**) to run as a classic multiband saturator.
4. To key the bands from MIDI instead of detection, set **Pitch Mode = MIDI** and send
   notes (route MIDI to the plugin).

Offline audition renders (each preset over a sliding saw and a synthetic vocal) are in
`renders/TRACER/`.
