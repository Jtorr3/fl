# VOXFIT — vocal character conformer

## What It Is

The final plugin of the VOX suite. Where VOXKEY fixes a vocal's *pitch* and W9-VOXRIP conforms its
*key/tempo*, VOXFIT conforms its **character** — it makes a ripped or foreign acapella *sit* in a
completely different production. A pitch-independent formant shift reshapes the timbre, then a
de-esser, a dynamic harshness tamer, a tilt EQ, and proximity + air shelves finish the tone. The
**SIT** macro sweeps a curated combination tuned for dropping a bright pop vocal into a dark mix.

VOXFIT reuses the suite's formant-preserving phase-vocoder shifter
(`suite_core::shift::ShiftEngine`, built by SEANCE) in **formant-only** mode — pitch stays exactly
put while the formants move.

## Signal Flow

```
in ─┬─ formant shift (±5 st, pitch-INDEPENDENT: pitch_ratio = 1,
    │   formant_ratio = 2^(st/12), envelope-preserve ON)
    │      ↓
    │   de-esser  (complementary split @5 kHz, EnvFollower-keyed downward gain on
    │              the sibilant band; Threshold / Amount / Listen)
    │      ↓
    │   harshness tamer (dynamic bell cut 2–5 kHz: subtract k·bandpass, k follows
    │              band energy over Threshold × Amount — no coefficient clicks)
    │      ↓
    │   tilt EQ   (complementary low + high shelves pivoting at 1 kHz, ±6 dB)
    │      ↓
    │   proximity (low-mid shelf ~300 Hz, ±)  →  air (high shelf 12 kHz, ±) ─── wet ──┐
    └─ delay(2048) ──────────────────────────────────────────────────── dry ─────────┤ Mix
                                                                                      └─ × Out ── clip ── out
```

Reported latency = the ShiftEngine FFT size (**2048 samples**); the dry path is delayed to match so
`Mix = 0` nulls exactly against the latency-matched dry. Every stage after the shift is
minimum-phase (biquad / SVF), so nothing else adds reported latency.

## Signal chain

1. **Formant shift.** Two `ShiftEngine`s (one per channel) run with `pitch_ratio = 1.0`,
   `formant_ratio = 2^(st/12)` and **envelope preservation ON**, so the pitch is untouched while the
   spectral envelope (the formants) slides. **Formant** `±5 st`: negative = bigger/deeper head,
   positive = smaller/brighter.
2. **De-esser.** A **3-way complementary split** (`low` = two cascaded SVF low-passes @5 kHz;
   `air` = two cascaded SVF high-passes @10 kHz; `sib = x − low − air`, the 5–10 kHz sibilant band —
   `low + sib + air = x` exactly). A fast peak `EnvFollower` keys the **sib** band; when its
   envelope exceeds **De-Ess Thresh**, an infinite-ratio downward gain `gr = (thresh / env)^amount`
   (smoothed ~2 ms so the onset doesn't click) pulls only the sibilant band down
   (`out = low + gr·sib + air`). Because the split sums back to `x` the reduction can be total, yet
   the vowel band below ~2 kHz **and the air above ~10 kHz are left untouched** — ducking an ess no
   longer dulls the vocal's sparkle. **De-Ess Listen** monitors the *removed* content (silent at
   rest, lights up on esses) for tuning.
3. **Harshness tamer.** A dynamic bell cut centred at ~3.2 kHz (spanning 2–5 kHz). A unity-gain
   band-pass feeds both a peak detector and the cut: `tamed = x − k·bandpass(x)`, where `k` follows
   how far the band energy sits over **Harsh Thresh**, scaled by **Harsh** amount (up to 18 dB of
   dip). Modulating a subtraction gain rather than re-solving biquad coefficients keeps the dynamic
   cut click-free.
4. **Tilt EQ.** Two complementary RBJ shelves pivoting at 1 kHz: **Tilt** `< 0` (dark) boosts lows
   and cuts highs symmetrically; `> 0` (bright) does the reverse. `±6 dB` at the extremes.
5. **Proximity.** A low-mid shelf at ~300 Hz, `±6 dB` — body/warmth (or thinning when cut).
6. **Air.** A high shelf at 12 kHz, `±6 dB` — open, silky top (or a duller bed when cut).
7. **SIT macro.** A single 0–100 % control that blends a curated conforming move on top of your
   base settings: a slight formant drop, mild de-ess, a 2–5 kHz presence dip, a tilt toward dark,
   and a touch of proximity. `Sit = 0` leaves every value exactly as set.
8. **Mix / Out.** Linear dry/wet (dry is latency-matched) then output trim, with a knee'd safety
   clip so the wet path can never exceed 0 dBFS while `Mix = 0` still nulls exactly.

## Parameters

| Param | Range | Notes |
|---|---|---|
| Formant | −5…+5 st | Pitch-independent formant move (preserve always on). |
| De-Ess Thresh | −60…0 dB | Sibilant-band envelope level above which de-essing engages. |
| De-Ess | 0–100 % | De-ess amount (0 = off, 100 % = pull the band to threshold on esses). |
| De-Ess Listen | on/off | Monitor the removed sibilant content for tuning. |
| Harsh Thresh | −60…0 dB | 2–5 kHz band level above which the dynamic bell cuts. |
| Harsh | 0–100 % | Depth of the dynamic harshness cut. |
| Tilt | −6…+6 dB | Complementary shelves @1 kHz. **< 0 = dark**, > 0 = bright. |
| Proximity | −6…+6 dB | Low-mid shelf ~300 Hz (body / thinning). |
| Air | −6…+6 dB | High shelf @12 kHz (top-end openness). |
| Sit | 0–100 % | Curated conform macro (formant + de-ess + presence dip + dark tilt + proximity). |
| Mix | 0–100 % | Dry/wet (dry is latency-matched; 0 nulls exactly). |
| Out | −24…+12 dB | Output trim. |

## Controls

- **Formant** — pitch-independent formant move, −5 … +5 st. Negative = bigger/deeper head,
  positive = smaller/brighter.
- **De-Ess Thresh** — sibilant-band envelope level above which de-essing engages, −60 … 0 dB.
- **De-Ess** — de-ess amount, 0–100 % (0 = off, 100 % pulls the sibilant band to threshold on
  esses).
- **De-Ess Listen** — on/off. Monitor only the removed sibilant content for tuning.
- **Harsh Thresh** — 2–5 kHz band level above which the dynamic bell cuts, −60 … 0 dB.
- **Harsh** — depth of the dynamic harshness cut, 0–100 % (up to ~18 dB dip).
- **Tilt** — complementary shelves pivoting at 1 kHz, −6 … +6 dB. **< 0 = dark**, > 0 = bright.
- **Proximity** — low-mid shelf ~300 Hz, −6 … +6 dB (body/warmth or thinning).
- **Air** — high shelf at 12 kHz, −6 … +6 dB (open silky top or a duller bed).
- **Sit** — curated conform macro (formant drop + de-ess + presence dip + dark tilt + proximity),
  0–100 % (0 = leaves every value as set).
- **Mix** — dry/wet blend, 0–100 % (dry is latency-matched; 0 nulls exactly).
- **Out** — output trim, −24 … +12 dB.

## Recipes

1. **Sit A Rip In A Dark Mix** — load *Sit In Dark Mix*, then turn **Sit** up to ~60–70 % until
   the bright pop vocal tucks under a dark-techno bed: Formant 0, De-Ess ~10 %, Tilt toward dark.
   The one-knob move that makes a foreign acapella belong.
2. **De-Harsh A Sibilant Acapella** — from *De-Harsh Rip*: De-Ess Thresh −32 dB, De-Ess 80 %,
   Harsh Thresh −30 dB, Harsh 60 %. Flip **De-Ess Listen** on to tune the threshold onto only the
   esses, then off. Tames a crunchy ripped vocal for atmospheric DnB.
3. **Deeper Voice Character** — from *Deeper Voice*: Formant −4 st, Proximity +2 dB, Air −1 dB,
   Tilt −1 dB. Drops a thin acapella into a weighty chest voice for a menacing lead.
4. **Airy Feature Up-Front** — from *Airy Feature*: Formant +1.5 st, Air +3 dB, Proximity +1 dB,
   De-Ess 40 %, Sit 0 %. Opens the top end so a feature vocal sits above the mix.

## Presets

Sit In Dark Mix · De-Harsh Rip · Radio Ghost · Deeper Voice · Airy Feature · Neutral Cleanup.

## Done-bar (offline tests)

1. **Formant shift** — a fixed 145 Hz vocal, **Formant +3 st** → the averaged log-spectral-envelope
   peaks move by **~2^(3/12) ≈ 1.189×** (within ±10 %) while the measured f0 stays within **±10
   cents** (pitch-independent).
2. **De-esser** — HP-filtered noise sibilant bursts riding a 150 Hz vowel tone → the **5–9 kHz** band
   energy during bursts is reduced (and reduced *more* with higher Amount) while the **< 2 kHz**
   vowel band stays within **±1 dB** *and* the **12–18 kHz air** is spared (reduced far less than the
   sibilant band — the 3-way split passes air at unity).
3. **Tilt** — at max dark the low-minus-high spectral balance of a log chirp shifts by clearly more
   than the shelf pair (measurably tilts the spectrum).
4. Universal: no NaN/inf, peak ≤ 0 dBFS, non-silent, and `Mix = 0` nulls against the
   latency-matched dry below −80 dB.

## Design notes

- **Formant-only shift.** `set_pitch_ratio(1.0)` + `set_formant_ratio(2^(st/12))` with preserve on
  gives a pitch-locked timbre move — the same engine VOXKEY uses for retune, run the other way.
- **3-way complementary de-ess split** rather than a band-pass subtraction: a sub-octave band-pass
  has a passband gain below 1, so `x − k·band` can never fully remove the sibilance and the reduced
  detector envelope inflates the computed gain. Splitting `low` (<5 kHz) / `air` (>10 kHz) /
  `sib = x − low − air` (5–10 kHz) sums back to `x`, so the reduction is complete, the < 2 kHz vowel
  band is provably untouched, **and the > 10 kHz air is provably untouched** — earlier the sibilant
  band was `x − low` (everything above 5 kHz), so a strong de-ess pulled the air down as hard as the
  sibilance (measured −20 to −33 dB of 11–18 kHz air) and dulled the whole top.
- **Local RBJ biquad.** The shelf/bell/band-pass sections are the suite's proven `overseer::eq`
  design, kept local to the crate (with a unity-peak band-pass added) rather than crossing a crate
  boundary — no `suite-core` change was needed.

## Using it in FL Studio

Find more plugins → add **Qeynos VOXFIT** on a ripped / imported vocal. Reach for **Sit In Dark
Mix** first (turn **Sit** up until the vocal tucks into the track), **De-Harsh Rip** on a bright or
sibilant acapella, **Deeper Voice** / **Radio Ghost** for character, or **Airy Feature** for an
up-front top end. Pair it after VOXKEY (pitch) / W9-VOXRIP (key + tempo) to fully conform a foreign
vocal. The host auto-compensates the +2048-sample latency.
