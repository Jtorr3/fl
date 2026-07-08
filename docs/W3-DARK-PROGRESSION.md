# W3 — DarkProgression.pyscript

FL Studio **piano-roll script** that generates a **dark, in-key chord
progression** — natural minor / phrygian / harmonic minor — and, optionally, a
**hypnotic arp** lane riding above the pad. This is the Afterlife /
melodic-techno vocabulary: brooding triad/7th/add9 pads that *hold hands* through
voice-leading, with a driving arpeggio derived from the chord underneath.

- Script: `pyscripts/DarkProgression.pyscript`
- Offline test gate: `pyscripts/tests/test_darkprog.py` (shared mock:
  `pyscripts/tests/mock_fl.py`)
- Installed to: `[MyDocuments]\Image-Line\FL Studio\Settings\Piano roll scripts\DarkProgression.pyscript`

## Use in FL Studio

1. Open the Piano roll on a **pad / keys** channel (a warm poly synth).
2. Optionally clear the roll (**Ctrl+A, Delete**) — the script **appends**.
3. **Piano roll menu → Tools → Scripting → Dark Progression**. Set the inputs
   and click **OK**. Same **Seed** → identical result, so audition freely.
4. Generation starts at the **timeline selection** start if one exists, else at
   **tick 0**. Chords are written low; the arp (if enabled) sits above the pad.

## Dialog inputs

| Input | Options / range | Default | Effect |
|---|---|---|---|
| Root | C … B | C | Key root pitch class |
| Octave | 1 … 5 | 3 | Pad register (C3 = MIDI 48) |
| Scale | Natural minor / Phrygian / Harmonic minor | Natural minor | The dark mode all pitches come from |
| Progression | Dark Pop / Hypnotic / Tension / Wander / Random | Dark Pop | Preset chord walk (see below) |
| Bars per chord | 1 / 2 | 1 | How long each chord is held |
| Total bars | 4 / 8 / 16 | 8 | Total length; the preset cycles to fill it |
| Chord voicing | Triad / 7th / add9 spread | Triad | Chord tone stack |
| Voice leading | on / off | on | Nearest-voice inversions minimising motion |
| Arp | Off / Up / Down / Up-Down / Random | Off | Arp lane direction (Off = pad only) |
| Arp rate | 8th / 16th | 16th | Arp grid subdivision |
| Arp octave span | 1 / 2 | 1 | Octaves the arp spans above the pad |
| Arp gate % | 5 … 100 | 70 | Arp note length as a % of the rate step |
| Suspension % | 0 … 100 | 20 | Chance of a sus2/sus4 colour on a non-tonic chord |
| Velocity base % | 20 … 100 | 90 | Base velocity for both lanes |
| Velocity humanize ± | 0 … 40 | 8 | Seeded ± velocity jitter (%) |
| Timing humanize ± ticks | 0 … 5 | 3 | Seeded ± timing jitter (arp lane only; ≤ 5 ticks) |
| Seed | 0 … 9999 | 1 | RNG seed → fully deterministic output |

## Progression presets

Roman numerals are **scale degrees** of the chosen minor mode (i = tonic):

- **Dark Pop** — i · VI · III · VII (the classic dark-pop loop)
- **Hypnotic** — i · i · VI · VII (two bars of tonic before the lift)
- **Tension** — i · **bII** · i · VII — the bII is the **b2 scale degree**, so this
  reads as a true Neapolitan flavour only with the **Phrygian** scale (in natural
  minor degree 1 is the natural 2). Pair it with Phrygian.
- **Wander** — i · v · VI · iv
- **Random-seeded** — a deterministic dark-degree walk that always starts on the
  tonic and never immediately repeats a degree.

## Rules (guaranteed by the generator)

- **Strictly in-key.** Every pitch — chord tones, suspensions, and arp notes — is
  a scale degree of the selected mode (built by stacking diatonic thirds), so
  nothing ever falls outside the key.
- **Voice leading.** The first chord is **root position**; each subsequent chord
  is re-voiced to the **inversion (± one octave)** that minimises the summed
  nearest-voice semitone motion from the previous chord, with a deterministic
  **lowest-bass** tiebreak. The offline gate proves this is always *strictly less*
  motion than the root-position rendering (and ≤ 6 semitones total per step for
  triads).
- **Suspensions.** On **non-tonic** chords only, a seeded low-probability sus2 or
  sus4 replaces the third (still in-scale, tone-count preserved so voice-leading
  still pairs up). Tonic chords are never suspended.
- **Arp derives from the chord.** Every arp note is a **chord voicing tone lifted
  by whole octaves** to sit **above the pad**, so it is always both in the chord
  and in a higher register. Direction (up / down / up-down / random), 8th/16th
  rate, 1–2 octave span, and a gate % (note length) all apply.
- **Grid-locked chords.** Chord changes land **exactly** on the bars-per-chord
  grid; the pad is never timing-humanised (only the arp is), so the harmony sits
  still while the arp breathes.
- **Deterministic per seed.** Same inputs + seed → identical output.

## Design choices

- **Self-contained single file.** FL's piano-roll script sandbox blocks file
  access, so an adjacent shared `.py` helper can't be imported — all logic lives
  in the one `.pyscript` (same rule as W1 / W2).
- **Append, not clear-and-write** (W1 semantics): `apply()` adds the generated
  chord + arp notes to whatever is already in the roll, at the selection start
  (or tick 0). Clear the roll first for a fresh progression.
- **Pure, importable generator.** `generate_progression(...)` (plus
  `chord_voicings()` / `progression_degrees()` / `total_motion()`) has **no**
  `flpianoroll` dependency — plain dicts/lists in and out. FL only calls
  `createDialog()` / `apply(form)`, and `import flpianoroll` is guarded so the
  module imports headless, which is what makes the offline gate possible.

## Offline testing (the gate)

`pyscripts/tests/test_darkprog.py` uses the **shared mock `flpianoroll`**
(`pyscripts/tests/mock_fl.py`) and asserts (19 tests / 44 asserts):

1. **In-scale** — every chord and arp pitch belongs to the selected scale, across
   all presets × scales × voicings, with suspensions forced.
2. **Grid boundaries** — chord starts land exactly on the bars-per-chord grid and
   the pad sustains one chord span, across bars-per-chord × total-bars × PPQ.
3. **Voice leading** — total nearest-voice motion is **strictly less** than the
   root-position rendering and within bound (≤ 6 semitones/step triads, ≤ 8 for
   7ths); the first chord is root position.
4. **Arp** — every arp note is a subset of the current chord tones (± octave),
   sits above the pad, locks to the exact 8th/16th grid, and honours the gate %;
   step counts match the rate; Off writes no arp lane.
5. **Deterministic** — same seed identical, different seed differs; the Random
   preset always starts on the tonic with no immediate repeats.
6. **Humanize bounds** — timing jitter ≤ 5 ticks (clamped even when a larger value
   is requested), the chord lane is never timing-humanised, and velocity stays
   within base ± humanize.

Plus the FL `apply()` glue (writes notes, respects the timeline selection offset,
writes both lanes when the arp is on).

```
cd pyscripts/tests
uv run --python 3.12 test_darkprog.py                   # plain-assert gate → 19/19
uv run --python 3.12 --with pytest pytest -q test_darkprog.py
```

Live in-FL verification (the script appearing in the Scripting menu and writing
the progression + arp) is a **CHECKPOINTS.md** item — FL cannot be driven
headless.
