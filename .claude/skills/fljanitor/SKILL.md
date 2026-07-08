---
name: fljanitor
description: Auto-name and auto-color an FL Studio session — heuristically classify every channel and mixer track by name keyword (kick/snare/hat/perc/bass/vox/pad/lead/fx) and rename + recolor them to a clean category scheme. Use when the user wants to tidy/clean up an unnamed or messy FL project, color-code their channels/mixer, or standardize track naming in a running FL Studio.
---

# fljanitor — FL Studio project janitor

Heuristic auto-name/color for a running FL Studio session via the FL Studio MCP
controller (SysEx). Tool: `tools/project_janitor.py`. Reuses W4's transport.

`uv` is at `%USERPROFILE%\.local\bin\uv.exe` (not on PATH); the script pins
Python 3.12 via a PEP 723 header, so always run it with `uv run --python 3.12`.

## Commands

```powershell
# Preview the rename/recolor plan (default, touches nothing)
uv run --python 3.12 tools\project_janitor.py

# Execute it
uv run --python 3.12 tools\project_janitor.py --apply

# Scope + machine-readable output
uv run --python 3.12 tools\project_janitor.py --only channels   # or: mixer | both
uv run --python 3.12 tools\project_janitor.py --json
```

## When to use

- "Clean up / tidy / organize my project", "name my channels", "color-code my
  mixer", "my tracks are all called Sampler/Insert" → run the dry-run, show the
  plan, then `--apply`.
- Always preview first (default). Only `--apply` writes to FL.

## Behavior

- Classifies by whole-word keyword on the current name; **first category wins**
  (kick/bd → KICK red, snare/rim → orange, clap → pink, hat/ride/crash → yellow,
  perc/tom/shaker → olive, bass/sub/808 → purple, vox/adlib → sky, pad/atmos →
  teal, lead/pluck/synth → green, fx/riser/impact → grey).
- Names matching no keyword (Channel N, Insert N, Sampler, Audio N, or any
  unrecognised name) are **left untouched**. Master (mixer track 0) is never
  modified.
- **Idempotent**: re-running a cleaned session produces zero ops.

## Prerequisites for `--apply`

FL Studio running with the FLStudioMCP controller enabled, and the loopMIDI
(Windows) / IAC (macOS) port enabled in BOTH the MIDI Input and Output lists with
the same port number. If it times out, that link isn't set up.

Full reference: `docs/W5-PROJECT-JANITOR.md`.
