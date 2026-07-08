# SEANCE — ethereal vocal machine

A ghost-vocal processor (Cynthoni / Sewerslvt-style): drop it on a vocal (or any melodic
source) and it drowns, chops, shimmers and pitch-warps it into a haunted, drifting texture.
SEANCE is also the **keystone** for the VOX suite — it builds the formant-preserving
phase-vocoder shifter (`suite_core::shift::ShiftEngine`) that VOXKEY (retune) and VOXFIT
(character conform) reuse.

```
in ─┬─ delay(latency) ─────────────────────────────────────────────── dry ──┐
    └ ShiftEngine  (pitch ±12 st + formant knob, envelope-preserving PV)     │  Mix
      → chopper    (BPM-synced gate: 4 shapes + Random, 5 ms smoothed edges) │   ↓
      → shimmer verb (Fdn8 + a +12 st shifter in the feedback loop,          ├── + ── Out
        soft-limit + DC block)                                               │
      → wash       (LP darkening + wow: slow fractional-delay pitch drift)   │
      → ducker     (keyed by the DRY env — the wet SWELLS when the vocal     │
        pauses: the drowned-vocal trick) ──────────────────────── wet ───────┘
```

Reported latency = the main shifter's FFT size (**2048 samples**); the dry path is delayed to
match so the `Mix` knob aligns. Two shift engines run for stereo; the shimmer verb runs one
cheap mono +12 st engine inside its feedback loop.

## Signal chain

1. **Formant-preserving shift.** `suite_core::shift::ShiftEngine` (2048/512 phase vocoder).
   It separates the spectral **envelope** (formants, by cepstral liftering) from the
   **excitation** (harmonics), shifts the excitation by the **Pitch** ratio and re-applies the
   envelope moved by the **Formant** ratio — so pitch and formants move independently.
   **Preserve** off = raw-magnitude shift (formants follow pitch, the chipmunk mode).
2. **Chopper.** A tempo-synced gate over one **Rate** division (1/2 … 1/32). **Pattern**
   selects Square / Stutter / Ramp / Double / **Random** (per-division sample-and-hold); edges
   are one-pole slewed (~5 ms) so it never clicks. **Depth** blends the gate toward unity.
3. **Shimmer verb.** A lush stereo **`suite_core::fdn::Fdn8`** (delays scaled by **Size**,
   RT60 = **Decay**) with a **+12 st** phase-vocoder shifter in its feedback loop — the
   **Shimmer** amount feeds the octave-up signal back in, `tanh` soft-limited and DC-blocked so
   the loop blooms without runaway. **Wet** sets the send level.
4. **Wash.** A low-pass that darkens as **Wash** rises, plus a subtle **wow** — a slow
   (~0.45 Hz) fractionally-modulated delay that adds tape-like pitch drift. Wash = 0 bypasses
   the block entirely (no added delay/colour). The two channels use decorrelated wow phases.
5. **Ducker (drowned-vocal swell).** A level-normalised envelope of the **dry** input keys an
   **inverse** duck on the wet: while the vocal is active the wet is pulled down by up to
   **Duck Depth**, and in the silence after it **swells** back up over **Duck Release**. This is
   the "underwater / drowned" motion — the ghost blooms between phrases.
6. **Macros → Mix → Out.** Three macros each drive several params, then a linear dry/wet
   **Mix** and output **Out** trim. A knee'd safety clip (identity below 0.9) bounds the
   shimmer/verb build-up to ≤ 0 dBFS without touching the `Mix = 0` dry null.

## Macros

| Macro | Drives |
|---|---|
| **GHOST** | Formant up (to +7 st) + Wash + shift blend (Mix) |
| **DROWN** | Verb Size + Wet + Duck Depth |
| **CHOP** | Chop Depth (pattern density/depth) |

## Parameters

| Param | Range | Notes |
|---|---|---|
| Pitch | −12 … +12 st | Formant-preserving pitch shift |
| Formant | −12 … +12 st | Independent formant move (Preserve on) |
| Formant Preserve | on/off | Off = formants follow pitch |
| Chop Pattern | Square/Stutter/Ramp/Double/Random | Gate shape |
| Chop Rate | 1/2 … 1/32 | BPM-synced division |
| Chop Depth | 0 … 100 % | Gate → unity blend |
| Verb Size | 0 … 100 % | FDN delay scale |
| Verb Decay | 0.3 … 8 s | RT60 |
| Shimmer | 0 … 100 % | +12 st feedback amount |
| Verb Wet | 0 … 100 % | Reverb send |
| Wash | 0 … 100 % | LP darkening + wow depth (0 = bypass) |
| Duck Depth | 0 … 100 % | Drowned-vocal swell amount |
| Duck Release | 40 … 800 ms | Swell recovery |
| Ghost / Drown / Chop | 0 … 100 % | Macros (above) |
| Mix | 0 … 100 % | Dry/wet (0 = latency-matched dry) |
| Out | −24 … +12 dB | Output trim |

## Presets

Grief Pad Vox · Drowned Lead · Whisper Choir · Formant Ghost · Chopped Ether · Sunken Chorus.

## Done-bar (measured)

- **+12 st doubles f0** of a synthetic vocal within ±20 cents (MPM detector).
- **Chop gate periods** match the selected BPM division within ±1 ms at 120 BPM.
- **Ducker swell** ≥ 6 dB: on a burst-then-silence vocal the wet in the silence is ≥ 6 dB
  above the wet during the active burst.
- Universal: no NaN/inf, peak ≤ 0 dBFS, non-silent, `Mix = 0` nulls against the
  latency-delayed dry.

## Engine reuse note (VOXKEY / VOXFIT)

The shifter is `suite_core::shift::ShiftEngine::new(fft, hop, sr)` with
`set_pitch_ratio` / `set_formant_ratio` / `set_envelope_preserve` / `process(x) -> f32` /
`latency()` / `reset()`. PV identity is lossy (phase is *reconstructed*): a unity-ratio wet
nulls only to ≈ −15 dB on steady tones and ≈ −8 dB on vibrato — so retune plugins must gate
`Mix = 0` on the **dry** path, never the wet null. See `suite-core/src/shift.rs`.
