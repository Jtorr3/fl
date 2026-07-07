---
name: flsession
description: Bootstrap an FL Studio session from a Qeynos template — set mixer track names/colors, channel→mixer routing, and loop mode in one command. Use when the user wants to lay out a fresh techno or DnB session skeleton in a running FL Studio, or to preview/author such a template.
---

# flsession — FL Studio session bootstrap

Applies a JSON **session template** (mixer track names + colors, channel→mixer
routing, loop mode) to a running FL Studio via the FL Studio MCP controller.
Tool: `tools/session_bootstrap.py`. Templates: `tools/templates/`.

`uv` is at `%USERPROFILE%\.local\bin\uv.exe` (not on PATH); the script pins
Python 3.12 via a PEP 723 header, so always run it with `uv run --python 3.12`.

## Commands

```powershell
# List shipped templates (TECHNO = dark melodic techno, DNB = atmospheric dnb)
uv run --python 3.12 tools\session_bootstrap.py list

# Preview op list without touching FL (no FL Studio needed)
uv run --python 3.12 tools\session_bootstrap.py apply TECHNO --dry-run

# Apply to the running FL session (default action, non-interactive, idempotent)
uv run --python 3.12 tools\session_bootstrap.py apply TECHNO
```

## When to use which

- User asks to "set up / lay out / scaffold a techno (or DnB) session",
  "name and color my mixer tracks", "route my channels" → `apply <template>`.
- User wants to see what would happen first, or FL isn't running → `--dry-run`.
- User wants a new layout → author `tools/templates/NAME.json` (format in
  `docs/W4-SESSION-BOOTSTRAP.md`), then `apply NAME`.

## Prerequisites for `apply` (not `--dry-run`)

FL Studio running with the FLStudioMCP controller enabled, and the loopMIDI
(Windows) / IAC (macOS) port enabled in BOTH the MIDI Input and Output lists with
the same port number. If commands time out, that link isn't set up.

## Notes

- `tempo` in a template is **not applied** — the FL MCP server has no BPM command;
  it's reported as skipped. Set tempo by hand in FL.
- Idempotent: re-applying the same template is safe (sets absolute
  names/colors/routes/mode again).
- Per-op failures (e.g. routing a channel that doesn't exist yet) are warnings,
  not aborts — the rest of the template still applies.

Full reference: `docs/W4-SESSION-BOOTSTRAP.md`.
