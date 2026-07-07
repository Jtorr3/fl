# W4 — SESSION-BOOTSTRAP

One-command FL Studio **session template** tool. A JSON template describes a
session skeleton (mixer track names + colors, channel→mixer routing, loop mode);
`session_bootstrap.py apply <template>` pushes it into a running FL Studio in a
single command. Two production-taste templates ship: **TECHNO** (dark melodic
techno) and **DNB** (atmospheric drum & bass).

- Tool: [`tools/session_bootstrap.py`](../tools/session_bootstrap.py)
- Templates: [`tools/templates/`](../tools/templates/) — `TECHNO.json`, `DNB.json`
- Tests: [`tools/test_session_bootstrap.py`](../tools/test_session_bootstrap.py)
  + op-list snapshots in `tools/tests/fixtures/`
- Skill: [`.claude/skills/flsession/SKILL.md`](../.claude/skills/flsession/SKILL.md)

## How it talks to FL Studio

Through the user's **FL Studio MCP** controller (`device_FLStudioMCP.py`), over
MIDI SysEx. Commands are JSON `{"action","params"}` chunked into
`F0 7D <type> <ascii-json> F7` frames; responses come back as
`{"success": bool, ...}`. This tool **reimplements** that thin SysEx layer
(`MidiTransport`) rather than importing the MCP server package, so it is a single
uv-runnable file with no dependency on that repo's path or virtualenv. The wire
format is byte-for-byte compatible with the MCP server (verified against its
source 2026-07-07). See the module docstring for the full decision rationale.

**Prerequisite for `apply` (not for `--dry-run`):** FL Studio running with the
FLStudioMCP controller enabled, and the loopMIDI (Windows) / IAC (macOS) port
enabled in **both** the MIDI Input and Output lists with the same port number.

## Template format

```jsonc
{
  "name": "Dark Melodic Techno",   // required
  "tempo": 132,                     // optional — NOT APPLIED (see below), reported & skipped
  "loop_mode": "pattern",           // optional — "pattern" | "song"
  "mixer_tracks": [                 // required, non-empty
    { "index": 1, "name": "KICK", "color": "#8B1E1E" }  // color optional; "#RRGGBB" or [r,g,b]
  ],
  "routing": [                      // optional
    { "channel": 0, "mixer_track": 1 }
  ]
}
```

### Field support (ground truth: the MCP server + FL device dispatch)

| Template field         | FL MCP command          | Status |
|------------------------|-------------------------|--------|
| `mixer_tracks[].name`  | `mixer.setTrackName`    | supported |
| `mixer_tracks[].color` | `mixer.setTrackColor`   | supported (r/g/b) |
| `routing[]`            | `channels.routeToMixer` | supported |
| `loop_mode`            | `transport.setLoopMode` | supported (`pattern`/`song`) |
| `tempo`                | *(none exists)*         | **unsupported → reported, skipped** |

The FL Studio MCP server exposes **no tempo/BPM setter** (its transport handlers
are start/stop/record/getStatus/setPosition/getLength/setLoopMode/
setPlaybackSpeed only). A template's `tempo` is therefore printed as a *skipped*
field, not applied and not an error — set the project tempo by hand in FL. This
is recorded in `DEFERRED.md`.

## Idempotent by construction

Every op sets an **absolute** value (a name, a color, a route target, a loop
mode) — none toggle. Re-running `apply` on the same template reproduces the exact
same session state; safe to run repeatedly.

## Resilient apply

`apply` sends ops one by one and tolerates per-op failures: e.g. a `routing` op
for a channel that doesn't exist yet in the rack is reported as a warning and the
run continues (names/colors still land). Exit code is non-zero only if at least
one op failed.

## Usage

```powershell
# uv is at %USERPROFILE%\.local\bin\uv.exe (not on PATH); Python pinned 3.12 via PEP 723 header.

# List shipped templates
uv run --python 3.12 tools\session_bootstrap.py list

# Preview the op list WITHOUT touching FL (no FL needed)
uv run --python 3.12 tools\session_bootstrap.py apply TECHNO --dry-run

# Apply to the running FL session (default action; FL must be running + connected)
uv run --python 3.12 tools\session_bootstrap.py apply TECHNO
uv run --python 3.12 tools\session_bootstrap.py apply DNB

# A template can also be given as a path
uv run --python 3.12 tools\session_bootstrap.py apply path\to\MyTemplate.json
```

`apply` is **never interactive** — when connected it applies immediately;
`--dry-run` is the preview path. There is no confirmation prompt.

## Templates shipped

**TECHNO** (`tempo` 132, dark scheme): KICK, RUMBLE, BASS, PERC, HATS, ATMOS,
LEAD, CHORD, FX + REVERB/DELAY send returns; channels 0–8 routed to tracks 1–9.

**DNB** (`tempo` 174, cool scheme): KICK, SNARE, BREAKS, SUB, REESE, PADS, VOXFX,
FX + REVERB/DELAY send returns; channels 0–7 routed to tracks 1–8.

To add your own: drop a `NAME.json` in `tools/templates/` following the format
above; `list` and `apply NAME` pick it up automatically.

## Tests

```powershell
uv run --python 3.12 tools\test_session_bootstrap.py
```

Offline gate (no FL, no MIDI backend): shipped templates validate; op-list
generation is compared against stored snapshots (`tools/tests/fixtures/*.ops.json`);
the transport is mocked to verify command streams, success/failure handling,
per-op-failure tolerance, and idempotency; plus color-parsing and validation-error
coverage. 47 checks, all green.

**Live smoke:** requires FL running with the controller actually responding. On
the build machine the loopMIDI port opened but FL did not respond (every command
timed out), so the live apply is logged in `CHECKPOINTS.md` with the exact command.
