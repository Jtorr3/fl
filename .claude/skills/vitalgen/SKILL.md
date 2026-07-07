---
name: vitalgen
description: >-
  Generate, tweak, and validate Vital (1.5.x) synth presets from natural-language
  descriptions using Claude. Use when the user wants a new Vital patch (e.g.
  "cavernous mid bass", "grief pad, slow attack, drowned"), wants to modify an
  existing .vital preset, or wants to validate one offline. Part of the Qeynos suite (W8).
---

# vitalgen — Claude-powered Vital preset generator

A Python CLI (`tools/vitalgen/vitalgen/vitalgen.py`) that asks Claude to fill a
**constrained** subset of Vital synth parameters, merges them onto an embedded
known-good 1.5.5 base patch, validates/clamps via pydantic, and writes a `.vital`
file that always loads. Claude never emits a whole preset file.

## Running it

The tool is a PEP 723 script pinned to Python 3.12. Run it with `uv` (uv lives at
`%USERPROFILE%\.local\bin\uv.exe`, which is NOT on PATH — use the absolute path):

```
uv run --python 3.12 C:\dev\qeynos-vst-suite\tools\vitalgen\vitalgen\vitalgen.py <subcommand> ...
```

`generate` and `tweak` call the Claude API and need `ANTHROPIC_API_KEY` set in the
environment (model defaults to `claude-opus-4-8`; override with `--model`).
`validate` is fully offline.

## Subcommands

- Generate a preset into the Vital user folder (`Documents\Vital\User\<bank>\`, or
  `Documents\Vital\User\Presets\` with no `--bank`):
  ```
  uv run --python 3.12 ...\vitalgen.py generate "cavernous mid bass" --name "Cavern Bass" --bank Qeynos
  ```
- Generate several variations at once, or to an explicit folder:
  ```
  uv run --python 3.12 ...\vitalgen.py generate "grief pad, slow attack, drowned" -n 3 --out ./out
  ```
- Tweak an existing preset (writes a new file alongside, suffixed `_tweaked`):
  ```
  uv run --python 3.12 ...\vitalgen.py tweak "C:\path\Cavern Bass.vital" "darker, more reverb, longer release"
  ```
- Validate a preset offline (no API key, no network):
  ```
  uv run --python 3.12 ...\vitalgen.py validate "C:\path\Cavern Bass.vital"
  ```

## What Claude controls (the constrained subset)

Oscillator levels / transpose / tune / wavetable frame / unison; filter routing,
cutoff (8..136 MIDI-note scale), resonance, drive, model/style; envelope ADSR
(quartic 0..2.378 scale); LFO shapes as point lists; FX chain amounts
(reverb/delay/chorus/distortion on + dry-wet); macro names. Everything else comes
from the base patch. Continuous params are clamped to Vital's real ranges; enum
params (models, types, on/off, unison voice count) are rejected if out of set.

Every generation prompt carries a fixed style block biased toward dark melodic
techno (KAS:ST) and atmospheric dnb / breakcore (Cynthoni / Sewerslvt): grief pads,
hollow reeses, drowned leads.

## Tests (offline gate, no API key)

```
uv run --python 3.12 C:\dev\qeynos-vst-suite\tools\vitalgen\vitalgen\test_vitalgen.py
```
