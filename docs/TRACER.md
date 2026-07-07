# TRACER — pitch-tracking multiband saturation

TRACER detects the fundamental (f0) of the incoming signal and locks a Linkwitz-Riley
crossover tree to it: each crossover cutoff rides a harmonic multiple of f0, so as a note
glides the bands follow, always saturating the same harmonic region (fundamental, body,
presence…). Each band is driven through the suite waveshaper bank at 2x oversampling and
summed. A MIDI note can replace the detector; when the detector loses confidence the
crossovers freeze.

```
in ─┬─ mono sum → MPM pitch detect (decimated ~12 kHz, window 1024)
    │            → median-5 → ±35-cent hysteresis → Hz/ms slew → f0, confidence
    │
    └─ LR4 crossover tree (cutoffs = harmonic × f0 × 2^SmartFreq, recomputed per
         32-sample control block; confidence < 0.6 freezes them)
           band0..3: [drive → shaper(bank) → 2x OS → level] → sum → mix → out
```

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

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Pitch Mode | Detect / MIDI | Detect | MIDI note replaces the detector |
| Bands | 2 / 3 / 4 | 3 | Number of active bands (band 0 = lowest) |
| Smart Freq | −2..+3 oct | 0 | Octave offset on every tracked crossover |
| Constant Color | on/off | on | Inverse equal-loudness drive weighting |
| Slew | 5..2000 Hz/ms | 200 | Pitch slew limit |
| Trim | −24..+24 dB | 0 | Input trim (wet path) |
| XO1/2/3 Mode | Track / Fixed | Track | Cutoff source per crossover |
| XO1/2/3 Fixed | 20..20000 Hz | 200/1000/4000 | Fixed cutoff when Mode = Fixed |
| Band 1–4 Drive | 0..48 dB | 10/8/6/4 | Drive into the band's shaper |
| Band 1–4 Shape | Tube / Tape / Fold / Hard | Tube/Tube/Tape/Tape | Waveshaper from the suite bank |
| Band 1–4 Level | −36..+12 dB | 0 | Band output level (−36 ≈ mute) |
| Mix | 0..100 % | 100 | Dry/wet. At 0 %, output nulls against dry. |
| Out | −24..+24 dB | 0 | Output trim (hard-ceilinged at ±0.999) |

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
