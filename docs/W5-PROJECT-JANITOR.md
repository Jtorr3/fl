# W5 — PROJECT-JANITOR (`tools/project_janitor.py`)

Heuristic auto-**name** + auto-**color** for a running FL Studio session. Scans
every channel-rack channel and every user-named mixer track, classifies each by
keyword heuristics on its current name, and proposes a canonical rename +
category color. Dry-run by default; `--apply` executes.

Talks to FL through the **FL Studio MCP** controller over MIDI SysEx, using a
self-contained transport **copied from `session_bootstrap.py` (W4)** — same wire
format (`F0 7D <type> …ascii-json… F7`, cmd `0x01/0x11`, resp `0x02/0x12`,
CHUNK_SIZE 200), so this is a single uv-runnable file with no dependency on the
MCP server source tree.

## Usage

`uv` is at `%USERPROFILE%\.local\bin\uv.exe` (not on PATH); the script pins
Python 3.12 via a PEP 723 header.

```powershell
# Preview the change plan (default; requires FL running to read the session)
uv run --python 3.12 tools\project_janitor.py

# Execute rename + recolor
uv run --python 3.12 tools\project_janitor.py --apply

# Only the channel rack, or only the mixer
uv run --python 3.12 tools\project_janitor.py --only channels
uv run --python 3.12 tools\project_janitor.py --only mixer

# Machine-readable plan (ops list + summary)
uv run --python 3.12 tools\project_janitor.py --json
```

## Classification

Whole-word keyword matching on the normalized (lowercased, punctuation→space)
name. **First category wins** (priority order below), so compound names resolve
sensibly (`bassdrum` → KICK, not BASS). A name matching **no** keyword —
including FL defaults (`Channel 1`, `Insert 5`, `Sampler`, `Audio 3`) — is left
**completely untouched**.

| Category | Color (hex) | Example keywords |
|---|---|---|
| KICK  | `#D62828` red    | kick, bd, bassdrum, kik |
| SNARE | `#E87A2A` orange | snare, snr, rimshot, rim |
| CLAP  | `#E84A8C` pink   | clap |
| HAT   | `#E8C83C` yellow | hat, hh, hihat, ride, crash, cymbal |
| PERC  | `#AA963C` olive  | perc, tom, conga, shaker, tamb, cowbell, snap, click |
| BASS  | `#7846C8` purple | bass, sub, 808, reese, wobble |
| VOX   | `#46A0E8` sky    | vox, vocal, adlib, verse, chorus, choir |
| PAD   | `#3CC8BE` teal   | pad, atmos, ambient, drone, texture, string, swell |
| LEAD  | `#5AC85A` green  | lead, pluck, arp, stab, synth, keys, piano, bell |
| FX    | `#8C8C96` grey   | fx, riser, sweep, impact, whoosh, foley, noise, drop |

## Guarantees

- **Idempotent.** A rename op is emitted only when the current name ≠ the
  canonical label, a recolor only when the current color ≠ the category color;
  re-running over a cleaned session produces **zero ops**.
- **Master safe.** Mixer track 0 (Master) is never read for changes.
- **Resilient.** Per-op failures under `--apply` are recorded as warnings, not
  aborts — the rest of the plan still applies.
- **Read ops** (dry-run + apply): `channels.getCount`, `channels.getInfo`,
  `mixer.getAllTracks` (`include_empty=False`), `mixer.getTrackInfo`.
  **Write ops** (apply only): `channels.setName/setColor`,
  `mixer.setTrackName/setTrackColor`.

## Offline test gate

`uv run --python 3.12 tools\test_project_janitor.py` — 52 checks, no FL/MIDI:
classifier (20 classified + 11 ambiguous→untouched), FL color parse/round-trip,
planning (rename+recolor / rename-only / recolor-only / idempotent / ambiguous),
op-list snapshot (channel vs mixer, name-then-color order), reads against a mock
(Master guard), and an end-to-end `run()` dry-run (writes nothing) → `--apply`
(exact 5-op write set) → idempotent re-run (0 writes).

## Live verification

FL was **not live** when this tool shipped (`fl_get_channel_count` → -1,
`fl_get_all_channels` → timeout; the MCP `fl_connection_status` can report
connected when it is not). Live scan/apply is deferred to a human step — see
`CHECKPOINTS.md`.
