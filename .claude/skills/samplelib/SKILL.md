---
name: samplelib
description: Analyze, rename, and sort a folder of audio samples — detect BPM (librosa) and musical key (chromagram), rename to {key}_{bpm}_{name}, and file each sample into category folders (kick/snare/hat/perc/bass/vocal/fx/synth/loop). Use when the user wants to organize a messy sample library, tag drum/loop/one-shot files with tempo and key, or sort downloaded packs. Dry-run by default; includes an undo manifest.
---

# samplelib — sample library organizer

Analyze + rename + sort a sample tree. Tool: `tools/sample_librarian.py`.

`uv` is at `%USERPROFILE%\.local\bin\uv.exe` (not on PATH); the script pins
Python 3.12 via a PEP 723 header (librosa/soundfile/numpy/scipy), so always run
it with `uv run --python 3.12`. First run downloads deps into a uv env.

## Commands

```powershell
# Preview (default — nothing is moved)
uv run --python 3.12 tools\sample_librarian.py sort "D:\Samples\unsorted"

# Execute (in place, or into --dest)
uv run --python 3.12 tools\sample_librarian.py sort "D:\Samples\unsorted" --apply
uv run --python 3.12 tools\sample_librarian.py sort "D:\in" --dest "D:\out" --apply

# Undo the last apply
uv run --python 3.12 tools\sample_librarian.py undo "<...>\sample_librarian_undo_<ts>.json"
```

## When to use

- "Organize / sort / tag my samples", "detect BPM and key and rename", "sort
  this pack into folders" → run the dry-run, show the plan, then `--apply`.
- Always preview first (default). Only `--apply` writes; every apply prints an
  undo-manifest path you can replay with `undo`.

## Behavior

- **BPM** (librosa onset autocorr) only for files > 1.5 s; **key** (Krumhansl
  chromagram) only for tonal categories (bass/vocal/synth/loop).
- **Categories** from filename keyword (first match wins, `bassdrum`→kick) with a
  duration fallback (unlabeled ≥ 2 s → loop, else other): kick, snare, clap, hat,
  perc, bass, vocal, fx, synth, loop, other.
- Renames to `{key}_{bpm}_{name}`; tokens only when they apply. **Idempotent** and
  **never overwrites** (collision → `_1`/`_2` suffix).

Full reference: `docs/W6-SAMPLE-LIBRARIAN.md`.
