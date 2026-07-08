# W1 — RumbleBassline.pyscript

FL Studio **piano-roll script** that generates offbeat / rolling **rumble-bass**
note patterns which lock *around* a 4-on-the-floor kick (dark / hard techno).
The kick is assumed to sit on the beats (PPQ positions 0, 1, 2, 3 of every 4/4
bar); every pattern is designed so the bass **avoids those kick positions** —
notes that do land on a beat are always **ghosted** so they duck under the kick.

- Script: `pyscripts/RumbleBassline.pyscript`
- Offline test gate: `pyscripts/tests/test_rumble.py`
- Installed to: `[MyDocuments]\Image-Line\FL Studio\Settings\Piano roll scripts\RumbleBassline.pyscript`

## Use in FL Studio

1. Open the Piano roll on a bass channel (Kick assumed on every beat).
2. **Piano roll menu → Tools → Scripting → Rumble Bassline** (or the Scripting
   chooser). Set the inputs (below) and click **OK** — notes are appended to the
   current piano roll, starting at the timeline **selection** if one is set,
   otherwise at bar 1. Clear the roll first (Ctrl+A, Delete) for a fresh pattern.

## Dialog inputs

| Input | Range / options | Default | Effect |
|---|---|---|---|
| Root note | C … B | F | Bass root pitch class |
| Octave | 0 … 4 | 1 | Root octave (F1 = MIDI 29, ~43.6 Hz) |
| Scale | Minor / Phrygian / Harmonic Minor | Minor | Pool for passing tones |
| Pattern | Offbeat 8ths / Rolling 16ths / Gallop / Broken | Offbeat 8ths | Rhythm engine |
| Bars | 1 … 8 | 2 | Length of the generated pattern |
| Note length % | 10 … 150 | 75 | Note length as a % of its grid step |
| Ghost velocity % | 0 … 60 | 35 | Velocity of ghost / on-kick notes |
| Accent velocity % | 40 … 100 | 95 | Velocity of the offbeat accents |
| Humanize velocity ± | 0 … 40 | 8 | Seeded ± velocity jitter (%) |
| Humanize timing ± ticks | 0 … 5 | 3 | Seeded ± timing jitter (bounded to ±5 ticks) |
| Fill density % | 0 … 100 | 20 | Probability of an extra ghost 16th in a non-beat gap |
| Seed | 0 … 9999 | 1 | RNG seed → fully deterministic output |

## Patterns (how each locks around the kick)

- **Offbeat 8ths** — one **accented** note on the "and" of each beat (16th
  indices 2, 6, 10, 14). Nothing on the beats at all. 4 notes/bar.
- **Rolling 16ths** — a continuous 16th stream. The **on-beat** 16ths (which
  land on the kick) are kept but **ghosted**; the "and" is accented, the "e"/"a"
  are ghosts. 16 notes/bar.
- **Gallop** — per beat: a ghosted **on-beat** 16th + an accented **"and"** + a
  ghosted **"a"** (the horse-gallop that ducks the kick). 12 notes/bar.
- **Broken** — a fixed syncopated 16th figure (indices 2, 3, 6, 10, 11, 14) that
  never lands on a beat. 6 notes/bar.

## Pitch & velocity rules

- **Root-dominant.** Every base note is the root, kept in the low register
  (root octave ±1). Occasional **passing tones** from the selected scale — the
  **b2** (phrygian only), the **5th**, the **b7** — appear at low probability and
  only on the extra *fill* ghost 16ths, never on the structural notes.
- **Two-tier velocity.** Offbeat structural hits get the **accent** velocity;
  everything else (including the ducked on-beat notes and all fills) gets the
  **ghost** velocity.
- **Humanize** is a seeded, deterministic jitter on velocity (±%) and timing
  (± ticks, hard-bounded to ±5). Same seed + same inputs → identical output.

## Design choices

- **Self-contained single file.** FL's piano-roll script sandbox blocks file
  access (per the machine's existing `ComposeWithLLM.pyscript`), so an adjacent
  shared `.py` helper cannot be imported reliably — all logic lives in the one
  `.pyscript`.
- **Append, not clear-and-write.** Mirrors `ComposeWithLLM`'s default action
  (which adds notes and only clears on an explicit request). Notes are appended
  starting at the timeline selection start, or tick 0.
- **Pure, importable generator.** The rhythm/pitch logic is the pure function
  `generate_bassline(...)` — no `flpianoroll` dependency, returns plain dicts.
  FL only calls `createDialog()` / `apply(form)`. The `import flpianoroll` is
  guarded so the module imports outside FL, which is what makes the offline test
  gate possible (FL cannot run headless).

## Offline testing (the gate)

`pyscripts/tests/test_rumble.py` injects a **mock `flpianoroll`** (Note class +
score list + ScriptDialog stub) and asserts:

1. No generated note starts on a kick position (a beat) unless the pattern
   ghosts it at ghost velocity; accents never land on a beat.
2. All pitches ∈ the selected scale; register bounds (root octave ±1) hold.
3. Timing humanize is bounded to ±5 ticks (and to the requested amount).
4. Output is deterministic per seed (same seed identical; different seed differs).
5. Base note count (fill_density = 0) matches the pattern math for every pattern.

Plus PPQ-independence (96/192/384/960), fill behaviour, and the FL `apply()`
glue driven through the mock dialog/score.

```
cd pyscripts/tests
uv run --python 3.12 test_rumble.py            # plain-assert gate  → 12/12
uv run --python 3.12 --with pytest pytest -q test_rumble.py
```

The mock `flpianoroll` in this test is intended to be **reused by W2-BREAK-CHOP**.
Live in-FL verification (the script actually appearing in the Scripting menu and
writing notes) is a **CHECKPOINTS.md** item — FL cannot be driven headless.
