# W6 — SAMPLE-LIBRARIAN (`tools/sample_librarian.py`)

Analyze, rename, and sort a directory tree of audio samples. For each file it
detects **BPM** (librosa onset-strength autocorrelation beat tracker, only for
files > 1.5 s), detects **key** (chromagram vs Krumhansl-Schmuckler profiles,
import-copied from W9 voxrip; only for tonal categories), classifies it into a
**category folder**, and renames it to `{key}_{bpm}_{origname}` — moving it into
`<dest>/<category>/`.

**Dry-run is the default.** `--apply` is required to touch files. Moves never
overwrite (collisions get a `_1`, `_2`… suffix). Every applied run writes an
**undo manifest**; `sample_librarian.py undo <manifest>` restores the original
layout.

## Usage

```powershell
# Preview the plan (default; nothing moves)
uv run --python 3.12 tools\sample_librarian.py sort "D:\Samples\unsorted"

# Execute (in place: creates category folders under the scanned root)
uv run --python 3.12 tools\sample_librarian.py sort "D:\Samples\unsorted" --apply

# Sort into a separate destination tree
uv run --python 3.12 tools\sample_librarian.py sort "D:\in" --dest "D:\out" --apply

# Top level only, or JSON plan
uv run --python 3.12 tools\sample_librarian.py sort "D:\in" --no-recursive
uv run --python 3.12 tools\sample_librarian.py sort "D:\in" --json

# Undo a previous apply
uv run --python 3.12 tools\sample_librarian.py undo "D:\out\sample_librarian_undo_20260708_141530.json"
```

## Categories

Filename keyword (first match wins, `bassdrum`→kick) then a duration fallback
(unlabeled ≥ 2 s → `loop`, else `other`):

`kick · snare · clap · hat · perc · bass · vocal · fx · synth · loop · other`

**Tonal** categories (key detected): `bass, vocal, synth, loop`. Percussive
categories get BPM (if > 1.5 s) but no key.

## Rename scheme

`{key}_{bpm}_{stem}{ext}` — tokens are included only when they apply:
- a 0.3 s `Kick_01.wav` → `kick/Kick_01.wav` (no tokens)
- a 128-BPM `perc_loop.wav` → `loop/A_128_perc_loop.wav`
- a C-major `synth_chord.wav` → `synth/C_synth_chord.wav`

**Idempotent**: leading key/bpm tokens are stripped before re-prefixing, and a
file already at its target path is skipped, so re-running a sorted library plans
zero moves.

## Safety

- **Dry-run default**; `--apply` gates all filesystem writes.
- **Never overwrites** — collision-safe suffixing considers both in-plan targets
  and files already on disk (case-insensitively, for Windows).
- **Undo manifest** JSON (`sample_librarian_undo_<ts>.json`) records every
  `{from,to}`; `undo` replays it in reverse.
- Unreadable/corrupt files are skipped with a warning, never abort the run.

## Offline test gate

`uv run --python 3.12 tools\test_sample_librarian.py` — 37 checks, no network/FL.
Synthesizes fixtures (120-BPM click loop, C-major tonal sample, short one-shots)
and covers classification (11 cases + duration fallback), token strip / name
build / idempotent re-tokenize, analysis (BPM within tolerance incl. half/double,
key ∈ {C, Am}, one-shot → no BPM/key), and a tmp-dir plan→apply→undo round-trip
with collision suffixing and an idempotent re-plan (0 moves).
