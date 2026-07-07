# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "mido>=1.3",
#   "python-rtmidi>=1.5",
# ]
# ///
"""session_bootstrap.py — one-command FL Studio session template tool (Qeynos W4).

Applies a JSON *session template* (mixer track names + colors, channel->mixer
routing, loop mode) to a running FL Studio instance in a single command. The two
shipped templates lay out a dark-melodic-techno session (TECHNO) and an
atmospheric-drum-&-bass session (DNB).

It talks to FL Studio through the user's **FL Studio MCP** controller
(device_FLStudioMCP.py) over MIDI SysEx. The SysEx command/response framing is a
thin, self-contained reimplementation of that server's protocol (see
`MidiTransport` and the DESIGN DECISION note below) so this tool is a single
uv-runnable file with no dependency on the MCP server's source tree.

------------------------------------------------------------------------------
DESIGN DECISION — transport layer: REIMPLEMENT (not import)
------------------------------------------------------------------------------
The FL MCP server (src/fl_studio_mcp/utils/midi_connection.py) exposes a
~60-line SysEx layer: JSON `{"action","params"}` chunked into
`F0 7D <type> <ascii-json> F7` frames (type 0x11 = final command chunk), and a
response reassembled from `0x02`/`0x12` frames into `{"success": bool, ...}`.

We REIMPLEMENT that layer here rather than importing MIDIConnection because:
  * this tool must live self-contained under C:\\dev\\qeynos-vst-suite\\tools\\ and
    must NOT depend on an absolute path into the user's OneDrive-redirected MCP
    repo (the PRD explicitly retires that location as a build root, and we are
    told not to modify it);
  * the protocol is small, fully documented in that module, and stable;
  * importing would require sys.path surgery into OneDrive + matching that repo's
    own virtualenv (mido/rtmidi), which is fragile across machines.
The wire format below is byte-for-byte compatible with that server, verified
against its source on 2026-07-07 (SYX_MFG 0x7D, CHUNK_SIZE 200, type bytes
0x01/0x11 command, 0x02/0x12 response).

------------------------------------------------------------------------------
SUPPORTED vs UNSUPPORTED template fields (ground truth: the MCP server + FL-side
device_FLStudioMCP.py action dispatch, read 2026-07-07)
------------------------------------------------------------------------------
  mixer_tracks[].name   -> mixer.setTrackName      SUPPORTED
  mixer_tracks[].color  -> mixer.setTrackColor     SUPPORTED (server sends r/g/b)
  routing[]             -> channels.routeToMixer   SUPPORTED
  loop_mode             -> transport.setLoopMode   SUPPORTED ("pattern"|"song")
  tempo                 -> (no command exists)      UNSUPPORTED -> reported, skipped

The FL MCP server exposes NO tempo/BPM setter (transport handlers: start/stop/
record/getStatus/setPosition/getLength/setLoopMode/setPlaybackSpeed only), so a
template's `tempo` is reported as a skipped field rather than failing the run.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

# ---------------------------------------------------------------------------
# SysEx wire protocol (reimplemented from the FL MCP server; see module docstring)
# ---------------------------------------------------------------------------
SYX_MFG = 0x7D          # non-commercial manufacturer id
SYX_CMD = 0x01          # command chunk, more to follow
SYX_CMD_END = 0x11      # final command chunk
SYX_RESP = 0x02         # response chunk, more to follow
SYX_RESP_END = 0x12     # final response chunk
CHUNK_SIZE = 200        # payload bytes per frame

DEFAULT_TIMEOUT = 3.0


# ---------------------------------------------------------------------------
# Op model
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class Op:
    """A single FL MCP command generated from a template."""

    action: str
    params: dict[str, Any]
    desc: str

    def to_dict(self) -> dict[str, Any]:
        return {"action": self.action, "params": self.params, "desc": self.desc}


class Transport(Protocol):
    """Anything that can send an FL MCP command and return its JSON response."""

    def send_command(self, action: str, params: dict[str, Any]) -> dict[str, Any]:
        ...

    def close(self) -> None:
        ...


# ---------------------------------------------------------------------------
# Color helpers
# ---------------------------------------------------------------------------
def parse_color(color: Any) -> tuple[int, int, int]:
    """Parse a template color into (r, g, b) 0-255.

    Accepts "#RRGGBB", "RRGGBB", or a [r, g, b] list/tuple.
    """
    if isinstance(color, (list, tuple)):
        if len(color) != 3:
            raise ValueError(f"color list must have 3 elements, got {color!r}")
        r, g, b = (int(c) for c in color)
    elif isinstance(color, str):
        h = color.lstrip("#").strip()
        if len(h) != 6:
            raise ValueError(f"hex color must be 6 digits, got {color!r}")
        try:
            r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
        except ValueError as e:
            raise ValueError(f"invalid hex color {color!r}: {e}") from e
    else:
        raise ValueError(f"color must be a hex string or [r,g,b], got {color!r}")
    for name, v in (("red", r), ("green", g), ("blue", b)):
        if not 0 <= v <= 255:
            raise ValueError(f"{name} component out of range 0-255 in {color!r}")
    return r, g, b


def color_hex(r: int, g: int, b: int) -> str:
    return f"#{r:02X}{g:02X}{b:02X}"


# ---------------------------------------------------------------------------
# Template validation + op generation
# ---------------------------------------------------------------------------
VALID_LOOP_MODES = ("pattern", "song")
MAX_MIXER_TRACK = 124  # FL: 0 = Master, 1..124 inserts


def validate_template(tpl: dict[str, Any]) -> None:
    """Validate a template dict against the W4 format. Raises ValueError."""
    if not isinstance(tpl, dict):
        raise ValueError("template must be a JSON object")

    name = tpl.get("name")
    if not isinstance(name, str) or not name.strip():
        raise ValueError("template 'name' is required and must be a non-empty string")

    if "tempo" in tpl and tpl["tempo"] is not None:
        if not isinstance(tpl["tempo"], (int, float)) or isinstance(tpl["tempo"], bool):
            raise ValueError("'tempo' must be a number if present")

    if "loop_mode" in tpl and tpl["loop_mode"] is not None:
        if tpl["loop_mode"] not in VALID_LOOP_MODES:
            raise ValueError(
                f"'loop_mode' must be one of {VALID_LOOP_MODES}, got {tpl['loop_mode']!r}"
            )

    tracks = tpl.get("mixer_tracks")
    if not isinstance(tracks, list) or not tracks:
        raise ValueError("'mixer_tracks' is required and must be a non-empty list")

    seen: set[int] = set()
    for i, t in enumerate(tracks):
        if not isinstance(t, dict):
            raise ValueError(f"mixer_tracks[{i}] must be an object")
        idx = t.get("index")
        if not isinstance(idx, int) or isinstance(idx, bool):
            raise ValueError(f"mixer_tracks[{i}].index must be an integer")
        if not 0 <= idx <= MAX_MIXER_TRACK:
            raise ValueError(
                f"mixer_tracks[{i}].index {idx} out of range 0..{MAX_MIXER_TRACK}"
            )
        if idx in seen:
            raise ValueError(f"duplicate mixer track index {idx}")
        seen.add(idx)
        tname = t.get("name")
        if not isinstance(tname, str) or not tname.strip():
            raise ValueError(f"mixer_tracks[{i}].name must be a non-empty string")
        if "color" in t and t["color"] is not None:
            parse_color(t["color"])  # raises on bad color

    routing = tpl.get("routing")
    if routing is not None:
        if not isinstance(routing, list):
            raise ValueError("'routing' must be a list if present")
        for i, r in enumerate(routing):
            if not isinstance(r, dict):
                raise ValueError(f"routing[{i}] must be an object")
            ch = r.get("channel")
            mt = r.get("mixer_track")
            if not isinstance(ch, int) or isinstance(ch, bool) or ch < 0:
                raise ValueError(f"routing[{i}].channel must be a non-negative integer")
            if not isinstance(mt, int) or isinstance(mt, bool) or not 0 <= mt <= MAX_MIXER_TRACK:
                raise ValueError(
                    f"routing[{i}].mixer_track must be an int 0..{MAX_MIXER_TRACK}"
                )


def generate_ops(tpl: dict[str, Any]) -> tuple[list[Op], list[str]]:
    """Turn a validated template into an ordered op list + a list of skip reports.

    Order (deterministic, snapshot-stable):
      1. loop mode
      2. per mixer track: setTrackName then setTrackColor
      3. routing (channel -> mixer)
    Idempotent: every op sets an absolute value (name/color/route/mode), so
    re-applying the same template produces the same session state.
    """
    ops: list[Op] = []
    skipped: list[str] = []

    tempo = tpl.get("tempo")
    if tempo is not None:
        skipped.append(
            f"tempo ({tempo} BPM): the FL Studio MCP server exposes no tempo/BPM "
            f"command — set the project tempo manually in FL Studio."
        )

    loop_mode = tpl.get("loop_mode")
    if loop_mode is not None:
        ops.append(
            Op("transport.setLoopMode", {"mode": loop_mode}, f"loop mode -> {loop_mode}")
        )

    for t in tpl.get("mixer_tracks", []):
        idx = t["index"]
        name = t["name"]
        ops.append(
            Op("mixer.setTrackName", {"track": idx, "name": name}, f"mixer {idx} name -> {name}")
        )
        color = t.get("color")
        if color is not None:
            r, g, b = parse_color(color)
            ops.append(
                Op(
                    "mixer.setTrackColor",
                    {"track": idx, "r": r, "g": g, "b": b},
                    f"mixer {idx} color -> {color_hex(r, g, b)}",
                )
            )

    for r in tpl.get("routing") or []:
        ch = r["channel"]
        mt = r["mixer_track"]
        ops.append(
            Op(
                "channels.routeToMixer",
                {"channel_index": ch, "mixer_track": mt},
                f"channel {ch} -> mixer {mt}",
            )
        )

    return ops, skipped


# ---------------------------------------------------------------------------
# Template loading
# ---------------------------------------------------------------------------
def templates_dir_default() -> Path:
    return Path(__file__).resolve().parent / "templates"


def resolve_template_path(spec: str, templates_dir: Path) -> Path:
    """Resolve a template spec (bare name like 'TECHNO', or a path) to a file."""
    p = Path(spec)
    if p.exists():
        return p
    # bare name -> templates_dir/<NAME>.json (case-insensitive-ish, try as given)
    for candidate in (
        templates_dir / spec,
        templates_dir / f"{spec}.json",
        templates_dir / f"{spec.upper()}.json",
    ):
        if candidate.exists():
            return candidate
    raise FileNotFoundError(
        f"template {spec!r} not found (looked for a file path and in {templates_dir})"
    )


def load_template(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as f:
        tpl = json.load(f)
    validate_template(tpl)
    return tpl


def list_templates(templates_dir: Path) -> list[tuple[str, dict[str, Any]]]:
    out: list[tuple[str, dict[str, Any]]] = []
    if not templates_dir.exists():
        return out
    for path in sorted(templates_dir.glob("*.json")):
        try:
            with path.open("r", encoding="utf-8") as f:
                tpl = json.load(f)
        except (OSError, ValueError):
            continue
        out.append((path.stem, tpl))
    return out


# ---------------------------------------------------------------------------
# Real MIDI transport (self-contained SysEx; mido imported lazily)
# ---------------------------------------------------------------------------
def _pick_port(names: list[str]) -> str | None:
    for name in names:
        if "IAC" in name:  # macOS
            return name
    for name in names:
        if "loopMIDI" in name or "FL" in name.upper():
            return name
    return names[0] if names else None


class MidiTransport:
    """SysEx transport to FL Studio over a virtual MIDI cable (loopMIDI/IAC)."""

    def __init__(self) -> None:
        self._out = None
        self._in = None
        self._mido = None
        self.out_name: str | None = None
        self.in_name: str | None = None

    def connect(self) -> None:
        try:
            import mido
        except ImportError as e:
            raise RuntimeError(
                "mido not installed. Run this tool via uv (it pins mido + python-rtmidi)."
            ) from e
        self._mido = mido
        out_name = _pick_port(mido.get_output_names())
        in_name = _pick_port(mido.get_input_names())
        if out_name is None or in_name is None:
            raise RuntimeError(
                "No MIDI ports found. On Windows create a loopMIDI port; on macOS "
                "enable the IAC Driver. FL Studio must be running with the "
                "FLStudioMCP controller enabled on that port."
            )
        self._out = mido.open_output(out_name)
        self._in = mido.open_input(in_name)
        self.out_name = out_name
        self.in_name = in_name

    def send_command(
        self, action: str, params: dict[str, Any], timeout: float = DEFAULT_TIMEOUT
    ) -> dict[str, Any]:
        assert self._out is not None and self._in is not None and self._mido is not None
        mido = self._mido
        payload = json.dumps(
            {"action": action, "params": params or {}},
            separators=(",", ":"),
            ensure_ascii=True,
        ).encode("ascii")

        for _ in self._in.iter_pending():  # drain stale echoes
            pass

        data = list(payload)
        for i in range(0, len(data), CHUNK_SIZE):
            chunk = data[i : i + CHUNK_SIZE]
            mtype = SYX_CMD_END if i + CHUNK_SIZE >= len(data) else SYX_CMD
            self._out.send(mido.Message("sysex", data=[SYX_MFG, mtype] + chunk))

        return self._wait_for_response(timeout)

    def _wait_for_response(self, timeout: float) -> dict[str, Any]:
        buf: list[int] = []
        start = time.time()
        while time.time() - start < timeout:
            for msg in self._in.iter_pending():
                if msg.type != "sysex" or len(msg.data) < 2:
                    continue
                d = msg.data  # mido strips F0/F7
                if d[0] != SYX_MFG:
                    continue
                mtype = d[1]
                if mtype not in (SYX_RESP, SYX_RESP_END):
                    continue  # our own echoed command frames
                buf.extend(d[2:])
                if mtype == SYX_RESP_END:
                    try:
                        return json.loads(bytes(buf).decode("ascii", "replace"))
                    except (ValueError, UnicodeDecodeError) as e:
                        return {"success": False, "error": f"Invalid JSON in response: {e}"}
            time.sleep(0.01)
        return {
            "success": False,
            "error": (
                f"Timeout after {timeout}s. FL Studio must be running with the "
                "FLStudioMCP controller enabled, and the loopMIDI port enabled in "
                "BOTH the MIDI Input and Output lists with the same port number."
            ),
        }

    def close(self) -> None:
        for port in (self._out, self._in):
            if port is not None:
                try:
                    port.close()
                except Exception:
                    pass
        self._out = None
        self._in = None


# ---------------------------------------------------------------------------
# Apply
# ---------------------------------------------------------------------------
@dataclass
class OpResult:
    op: Op
    ok: bool
    detail: str


@dataclass
class ApplyReport:
    results: list[OpResult] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)

    @property
    def ok_count(self) -> int:
        return sum(1 for r in self.results if r.ok)

    @property
    def fail_count(self) -> int:
        return sum(1 for r in self.results if not r.ok)


def apply_ops(
    ops: list[Op], skipped: list[str], transport: Transport
) -> ApplyReport:
    """Send each op through the transport, tolerating per-op failures.

    A single failing op (e.g. routing a channel that doesn't exist yet) is
    recorded as a warning and does NOT abort the run — the rest still apply.
    """
    report = ApplyReport(skipped=list(skipped))
    for op in ops:
        try:
            resp = transport.send_command(op.action, op.params)
        except Exception as e:  # transport-level error
            report.results.append(OpResult(op, False, f"transport error: {e}"))
            continue
        if isinstance(resp, dict) and resp.get("success", False):
            report.results.append(OpResult(op, True, "ok"))
        else:
            err = ""
            if isinstance(resp, dict):
                err = str(resp.get("error", resp))
            report.results.append(OpResult(op, False, err or "unknown error"))
    return report


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def cmd_list(args: argparse.Namespace) -> int:
    tdir = Path(args.templates_dir) if args.templates_dir else templates_dir_default()
    templates = list_templates(tdir)
    if not templates:
        print(f"No templates found in {tdir}")
        return 1
    print(f"Templates in {tdir}:\n")
    for stem, tpl in templates:
        name = tpl.get("name", stem)
        tempo = tpl.get("tempo")
        ntracks = len(tpl.get("mixer_tracks", []) or [])
        nroute = len(tpl.get("routing", []) or [])
        tempo_s = f"{tempo} BPM" if tempo is not None else "-"
        print(f"  {stem:<12} {name}")
        print(f"               {ntracks} mixer tracks, {nroute} routes, tempo {tempo_s}")
    return 0


def _print_ops(ops: list[Op], skipped: list[str]) -> None:
    for i, op in enumerate(ops, 1):
        print(f"  {i:>2}. {op.desc}   [{op.action}]")
    if skipped:
        print("\nSkipped (unsupported by the FL MCP server):")
        for s in skipped:
            print(f"  - {s}")


def cmd_apply(args: argparse.Namespace) -> int:
    tdir = Path(args.templates_dir) if args.templates_dir else templates_dir_default()
    try:
        path = resolve_template_path(args.template, tdir)
        tpl = load_template(path)
    except (FileNotFoundError, ValueError) as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2

    ops, skipped = generate_ops(tpl)
    label = tpl.get("name", path.stem)

    if args.dry_run:
        print(f"[dry-run] {label} ({path.name}) -> {len(ops)} ops:\n")
        _print_ops(ops, skipped)
        print(f"\n[dry-run] no session was touched.")
        return 0

    print(f"Applying '{label}' ({path.name}) -> {len(ops)} ops to FL Studio...")
    transport = MidiTransport()
    try:
        transport.connect()
    except RuntimeError as e:
        print(f"Error: could not connect to FL Studio: {e}", file=sys.stderr)
        print(
            "Tip: re-run with --dry-run to preview the op list without FL Studio.",
            file=sys.stderr,
        )
        return 3

    try:
        report = apply_ops(ops, skipped, transport)
    finally:
        transport.close()

    for r in report.results:
        mark = "ok " if r.ok else "FAIL"
        line = f"  [{mark}] {r.op.desc}"
        if not r.ok:
            line += f"   ({r.detail})"
        print(line)
    if report.skipped:
        print("\nSkipped (unsupported by the FL MCP server):")
        for s in report.skipped:
            print(f"  - {s}")

    print(
        f"\nDone: {report.ok_count} applied, {report.fail_count} failed, "
        f"{len(report.skipped)} skipped."
    )
    return 0 if report.fail_count == 0 else 1


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="session_bootstrap.py",
        description="Apply a Qeynos FL Studio session template (names/colors/routing/loop mode).",
    )
    p.add_argument(
        "--templates-dir",
        default=None,
        help="Directory of template JSON files (default: ./templates next to this script).",
    )
    sub = p.add_subparsers(dest="command", required=True)

    p_list = sub.add_parser("list", help="List available templates.")
    p_list.set_defaults(func=cmd_list)

    p_apply = sub.add_parser("apply", help="Apply a template to the running FL session.")
    p_apply.add_argument("template", help="Template name (e.g. TECHNO) or path to a .json.")
    p_apply.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the op list without touching FL Studio.",
    )
    p_apply.set_defaults(func=cmd_apply)
    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
