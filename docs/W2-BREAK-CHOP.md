# W2 — BreakChop.pyscript

FL Studio **piano-roll script** that rewrites the **SELECTED** notes into jungle /
drum-and-bass **chop** patterns. Point it at a sliced break (Fruity Slicer
slice-notes) or **any** selected note sequence — each note is treated as one
**slice** — select the notes to mangle, and run: it **permutes**, **rolls**,
**stutter-gates** and **reverses** those slices in place, re-filling the **same
total span** so the loop point stays intact.

- Script: `pyscripts/BreakChop.pyscript`
- Offline test gate: `pyscripts/tests/test_breakchop.py` (shared mock:
  `pyscripts/tests/mock_fl.py`)
- Installed to: `[MyDocuments]\Image-Line\FL Studio\Settings\Piano roll scripts\BreakChop.pyscript`

## Use in FL Studio

1. Open the Piano roll on the channel holding your sliced break (or any notes).
2. **Select** the notes you want to chop (Ctrl+A selects all). If nothing is
   selected the **whole score** is chopped (FL script convention).
3. **Piano roll menu → Tools → Scripting → Break Chop** (or the Scripting
   chooser). Set the inputs and click **OK** — the selected notes are **deleted
   and rewritten** with the chop pattern; every unselected note is left
   untouched. Same **Seed** → identical result, so audition and re-run freely.

## Dialog inputs

| Input | Range | Default | Effect |
|---|---|---|---|
| Intensity % | 0 … 100 | 80 | How much of the sequence gets touched (0 = no change) |
| Permute amount % | 0 … 100 | 50 | Fraction of touched slots whose slice content is shuffled |
| Roll chance % | 0 … 100 | 30 | Per touched slot: chance of a roll |
| Roll count | 2 / 3 / 4 | 3 | Retriggers per roll |
| Roll decay % | 0 … 95 | 30 | Velocity drop per roll hit |
| Stutter chance % | 0 … 100 | 20 | Per touched slot: chance of a stutter-gate |
| Stutter gate % | 5 … 100 | 50 | Note length kept per stutter sub-slot (gap in the rest) |
| Reverse chance % | 0 … 100 | 15 | Per touched slot: chance of a reverse (emulated — see below) |
| Keep first beat | on / off | on | Protect the downbeat slice (emit it unchanged) |
| Humanize velocity ± | 0 … 40 | 6 | Seeded ± velocity jitter (%) on touched notes only |
| Seed | 0 … 9999 | 1 | RNG seed → fully deterministic output |

## Operations (how each chop works)

- **Permute** — reorders which slice's **pitch/velocity** plays in each time-slot
  (the classic amen shuffle). The selection's **time-grid is preserved**; only
  the content assigned to each slot moves, and only **among the touched slots**,
  so the pitch multiset is a rearrangement (nothing added/removed).
- **Roll** — subdivides a slot into `Roll count` (2/3/4) **rapid retriggers** of
  its slice, velocity **decaying** by `Roll decay` each hit. The sub-notes tile
  the original slice span exactly (no gaps/overhang).
- **Stutter-gate** — **shorten + repeat**: splits the slot in two and gates each
  repeat to `Stutter gate %` of its sub-slot (silence in the gaps).
- **Reverse** — **emulated.** FL's piano-roll `flpianoroll.Note` exposes **no**
  reverse/porta property that flips slice playback (reverse is a Fruity Slicer /
  audio-clip property, not a MIDI-note property), so — per the work order — this
  is rendered as a **double-time repeat** (two full-length retriggers = a fast
  roll into the next slice). If a future FL exposes a real note-reverse flag, set
  `REVERSE_AVAILABLE = True` at the top of the script (the code already writes the
  flag when present).

Each operation is gated by its own probability, **inside** the master **Intensity**
gate that decides whether a slot is touched at all. Draw order is fixed
(touched → permute → roll → stutter → reverse) so output is deterministic per seed.

## Rules (guaranteed by the generator)

- **Selection only.** Only the selected notes are altered; unselected notes are
  never read or written. Fallback: nothing selected → the whole score is chopped.
- **Span preserved exactly.** Every slot's output tiles `[slot_start, slot_end]`,
  and the slice with the **maximum end** always sustains to its end (even under a
  stutter-gate), so the break's total start and end are preserved **exactly** —
  well within the "± one roll subdivision" tolerance the work order allows.
- **Keep-first-beat.** The downbeat slice — the earliest selected note that lands
  on a bar line, else the earliest selected note — is emitted **byte-for-byte
  unchanged** so the "one" never wanders.
- **Deterministic per seed.** Same inputs + seed → identical output.
- **Velocities scaled sensibly.** Rolls decay, stutters drop gently after the
  first hit, everything is clamped to a musical floor (≥ 0.02), and the optional
  humanize jitter is applied to **touched notes only** (protected/untouched notes
  stay exact).

## Design choices

- **Self-contained single file.** FL's piano-roll script sandbox blocks file
  access, so an adjacent shared `.py` helper can't be imported — all logic lives
  in the one `.pyscript` (same as W1).
- **Rewrite-in-place**, unlike W1's append: `apply()` collects the selected note
  indices, runs the pure chopper, **deletes only those notes** (descending index
  so earlier indices stay valid), then adds the chopped output. Unselected notes
  are untouched.
- **Pure, importable chopper.** The chop logic is `chop_notes(...)` — no
  `flpianoroll` dependency, plain dict in/out. FL only calls `createDialog()` /
  `apply(form)`, and `import flpianoroll` is guarded so the module imports
  headless, which is what makes the offline gate possible (FL can't run headless).

## Offline testing (the gate)

`pyscripts/tests/test_breakchop.py` uses the **shared mock `flpianoroll`**
(`pyscripts/tests/mock_fl.py`, factored out of W1's test now that two scripts
share it) and asserts:

1. **Only selected notes altered** — unselected notes stay byte-for-byte through
   `apply()` (mixed selection + whole-score fallback + rewrite paths).
2. **Span preserved** — output end == original end exactly across a sweep of
   operation mixes/seeds, including a long **leading** note (max-end ≠ last-in-time)
   and stutter-heavy tails.
3. **Rolls subdivide correctly** — `roll_count` notes tiling the slice span
   contiguously, with non-increasing / decaying velocity.
4. **Deterministic per seed** — same seed identical, different seed differs.
5. **Keep-first-beat** — exactly one unchanged note on the downbeat.
6. **Intensity 0 = no changes** — output == input.

Plus PPQ-independence (96/192/384/960), reverse-emulation (double-time) vs the
real-flag path, and permutation-stays-within-selection.

```
cd pyscripts/tests
uv run --python 3.12 test_breakchop.py                 # plain-assert gate → 16/16
uv run --python 3.12 --with pytest pytest -q test_breakchop.py
```

Live in-FL verification (the script appearing in the Scripting menu and
rewriting the selected notes) is a **CHECKPOINTS.md** item — FL cannot be driven
headless.
