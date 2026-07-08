---
name: refgap
description: Compare a reference track against your mix render and produce a self-contained HTML report — LUFS-I loudness delta, 1/3-octave spectral balance difference, stereo width by band, and kick fundamental detection with a tuning suggestion. Use when the user wants to A/B their mix against a reference/master, find where their mix is too bright/dull/quiet/narrow, or check/tune their kick's pitch.
---

# refgap — reference vs mix gap report

Compare a reference (mastered) track against your mix render. Tool:
`tools/reference_gap.py`. Outputs a self-contained HTML report (inline SVG, no
CDN).

`uv` is at `%USERPROFILE%\.local\bin\uv.exe` (not on PATH); the script pins
Python 3.12 via a PEP 723 header (numpy/scipy/soundfile/pyloudnorm), so always
run it with `uv run --python 3.12`.

## Commands

```powershell
uv run --python 3.12 tools\reference_gap.py "ref.wav" "mymix.wav"
uv run --python 3.12 tools\reference_gap.py ref.wav mix.wav --key C --out report.html
```

## When to use

- "Compare my mix to this reference", "why is my mix quieter/duller/narrower",
  "A/B against a master", "is my kick in tune / what note is my kick" → run it,
  then open the HTML report (and relay the console summary: LUFS delta, biggest
  spectral gaps, kick tuning).
- Pass `--key <root>` (e.g. `C`, `F#`, `Am`) to get the kick-to-key-root tuning
  move.

## What it reports

- **LUFS-I** for ref + mix + delta.
- **1/3-octave spectral balance** diff (bright/dull per band).
- **Stereo width by band** (0 = mono, ~0.5 = wide).
- **Kick fundamental** + nearest-note/cents + tuning suggestion.

Full reference: `docs/W7-REFERENCE-GAP.md`.
