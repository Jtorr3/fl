# VOXKEY — vocal retuner

Autotune-style pitch correction for the VOX suite. Drop it on a monophonic vocal (or lead) and
it snaps the sung pitch to the nearest tone of a **Root + Scale**, or to a **held MIDI note**.
Retune Speed goes from hard-snap (the classic autotune artifact) to a natural glide; Amount sets
how much correction; Humanize adds life; a Formant Offset moves the formants independently; and a
Confidence Gate leaves breaths and silence untouched.

VOXKEY reuses the suite's formant-preserving phase-vocoder shifter
(`suite_core::shift::ShiftEngine`, built by SEANCE) so the vocal keeps its natural timbre while
the pitch moves.

```
in ─┬─ mono sum → pitch detect (suite_core::pitch::Mpm) ── detected f0 + confidence
    │                                     │
    │        target = nearest scale tone (Root+Scale)  OR  held MIDI note
    │                                     │
    │        correction cents = 1200·log2(target / detected), clamped ±1 octave
    │        + Humanize drift, one-pole GLIDE (Retune 0–400 ms), × Amount
    │                                     ↓  pitch ratio
    ├─ delay(2048) ──────────────────────────────────────────────── dry ──┐
    └ TWO ShiftEngines (stereo, envelope-preserve ON,                      │ Mix
      formant offset via set_formant_ratio) ──────────────────── wet ──────┴── + ── Out
```

Reported latency = the ShiftEngine FFT size (**2048 samples**); the dry path is delayed to match
so `Mix = 0` nulls exactly against the latency-matched dry.

## Signal chain

1. **Pitch detection.** The mono sum is analysed by `suite_core::pitch::Mpm` (McLeod Pitch
   Method) on an anti-aliased ~12 kHz decimated stream (1024-sample window, median-3), giving a
   detected `f0` + a clarity **confidence**.
2. **Target note.** The detected pitch is snapped to the nearest tone of the chosen **Root**
   (12 notes) and **Scale** (Chromatic, Major, Natural Minor, Harmonic Minor, Phrygian, Dorian,
   Minor Pentatonic — a semitone-class mask). In **MIDI Mode**, a held note becomes the target
   directly and the scale is ignored while it is held (last-note priority).
3. **Correction.** `ratio = target / detected` expressed in cents, clamped to **±1 octave**.
   **Retune Speed** one-pole-glides that correction in the log (cents) domain: `0 ms` = a hard
   snap (the autotune stair-step), up to `400 ms` = a slow, natural slide between notes.
   **Amount** (0–100 %) scales the applied deviation — 100 % pins to the tone, lower values keep
   some of the original expression. **Humanize** adds a slow random ±cents drift on the target so
   sustained notes are not robotically static.
4. **Confidence Gate.** When the detector's confidence falls below the gate (breaths, consonants,
   silence, unpitched noise) the correction glides back to 1.0 — nothing is retuned, so there are
   no artifacts on non-pitched material.
5. **Formant-preserving shift.** Two `ShiftEngine`s (one per channel) apply the pitch ratio with
   **envelope preservation ON**, so the formants stay put as the pitch moves (no chipmunking).
   **Formant Offset** (`±12 st`) moves the formants independently via `set_formant_ratio` for a
   darker/bigger or brighter/smaller character.
6. **Mix / Out.** Linear dry/wet (dry is latency-matched) then output trim, with a knee'd safety
   clip so the wet path can never exceed 0 dBFS while `Mix = 0` still nulls exactly.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Root | C … B | Tonic of the scale (ignored while a MIDI note is held in MIDI Mode). |
| Scale | Chromatic / Major / Natural Minor / Harmonic Minor / Phrygian / Dorian / Minor Pentatonic | Allowed tones the pitch snaps to. |
| Retune Speed | 0–400 ms | Glide time of the correction. **0 = hard snap** (autotune artifact). |
| Amount | 0–100 % | How much of the cents deviation is applied (100 % = full correction). |
| Humanize | 0–50 ct | Slow random ±cents drift on the target note. |
| Formant Offset | ±12 st | Moves formants independently of pitch (preserve always on). |
| Confidence Gate | 0–1 | Below this detector clarity, the correction holds at 1.0 (no retune). |
| MIDI Mode | on/off | Held MIDI note becomes the target (scale ignored while held). |
| Mix | 0–100 % | Dry/wet (dry is latency-matched; 0 nulls exactly). |
| Out | −24…+12 dB | Output trim. |

The GUI shows a live **IN → TGT** read-out (detected note/Hz → target note/Hz) plus the current
detection confidence.

## Presets

Hard Snap Am · Gentle Glide · Phrygian Dark · T-Pain Extreme · Subtle Live · MIDI Puppet ·
Doll Formant.

## Done-bar (offline tests)

1. **Retune accuracy** — a vibrato-free vocal-like source (saw through the `/a/` formant bank)
   stepped across a fifth, Root A / Natural Minor, Retune 0, Amount 100 % → measured output f0
   sits within **±15 cents** of an A-minor scale tone for ≥ 80 % of pitched frames.
2. **Formant preservation** — a forced **+5 st** correction (MIDI mode) on a fixed note → the
   spectral-envelope shift ratio stays within **±8 %** of 1.0 (formants held) while f0 moves +5 st.
3. **Confidence gate** — white-noise input → the pitch-shift ratio stays within ±6 % of 1.0 (no
   retune, no pitch pumping).
4. Universal: no NaN/inf, peak ≤ 0 dBFS, non-silent, and `Mix = 0` nulls against the
   latency-matched dry below −80 dB.

## Design note — why `Mpm` directly instead of `PitchTracker`

The build brief calls for `suite_core::pitch::PitchTracker`. That tracker carries a **±35-cent
re-lock hysteresis** (and a median) tuned for TRACER's crossover *stability* — it deliberately
refuses small updates. On a retuner that is wrong: right after a note change the detector reads
up to ~35 cents below the true pitch and then **sticks** there, and because the output pitch is
`input × target/detected`, that bias lands the corrected note up to 35 cents off the scale tone
(measured: 60 % on-scale vs the 80 % done-bar). VOXKEY therefore detects with
`suite_core::pitch::Mpm` on the same decimated front end but **without the hysteresis** (light
median-3 only), which is accurate to a few cents — exactly what pitch correction needs. Same
module, same detector core; just the retune-appropriate smoothing. Recorded in DEFERRED.md.

## Using it in FL Studio

Find more plugins → add **Qeynos VOXKEY** on a monophonic vocal / lead. Load **Hard Snap Am** for
classic autotune, **Gentle Glide** for transparent correction, **Phrygian Dark** / **Doll
Formant** for character, or **MIDI Puppet** and route a MIDI clip to drive the pitch note-by-note.
Set Root + Scale to your key. The host auto-compensates the +2048-sample latency.
