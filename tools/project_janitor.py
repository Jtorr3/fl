# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "mido>=1.3",
#   "python-rtmidi>=1.5",
# ]
# ///
"""project_janitor.py — heuristic auto-name/color for an FL session (Qeynos W5).

Reads every channel-rack channel and every named mixer track from a running FL
Studio instance, classifies each by keyword heuristics on its current name
(kick/bd -> KICK red family, clap/snare, hat, perc, bass/sub/808, vox/vocal,
pad/atmos, lead/pluck, fx/riser...) and proposes a canonical rename + a
category color. `--dry-run` (the default) prints the change plan without
touching FL; `--apply` executes rename + recolor ops.

It talks to FL Studio through the user's **FL Studio MCP** controller
(device_FLStudioMCP.py) over MIDI SysEx. The SysEx command/response framing is a
thin, self-contained reimplementation of that server's protocol, COPIED verbatim
from the sibling tool `session_bootstrap.py` (Qeynos W4) so this tool is a single
uv-runnable file with no dependency on the MCP server's source tree.

------------------------------------------------------------------------------
DESIGN DECISION — transport layer: COPY session_bootstrap's REIMPLEMENTATION
------------------------------------------------------------------------------
Per the W4 tool's DESIGN DECISION note (reproduced here): the FL MCP server's
~60-line SysEx layer — JSON `{"action","params"}` chunked into
`F0 7D <type> <ascii-json> F7` frames (0x11 = final command chunk), response
reassembled from `0x02`/`0x12` frames into `{"success": bool, ...}` — is copied
rather than imported, because (a) this tool must be self-contained under
C:\\dev\\qeynos-vst-suite\\tools\\ and must NOT reach into the OneDrive-redirected
MCP repo (retired as a build root), and (b) the protocol is small and stable
(SYX_MFG 0x7D, CHUNK_SIZE 200, cmd 0x01/0x11, resp 0x02/0x12; verified against
the server source 2026-07-07/08). The FL device wraps every handler result as
`{"success": True, **result}`, and returns colors as a `"0xBBGGRR"` hex string
(FL's native BGR int, i.e. `(b<<16)|(g<<8)|r`).

------------------------------------------------------------------------------
READ ops used (all read-only): channels.getCount, channels.getInfo,
mixer.getAllTracks (include_empty=False -> only user-named tracks),
mixer.getTrackInfo. WRITE ops (only under --apply): channels.setName /
channels.setColor / mixer.setTrackName / mixer.setTrackColor.

Idempotent: a rename op is emitted only when the current name != the canonical
label, and a recolor only when the current color != the category color, so a
second run over a cleaned session produces zero ops. Ambiguous / unrecognised
names are left completely untouched. Mixer track 0 (Master) is never modified.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from dataclasses import dataclass, field
from typing import Any, Protocol

# ---------------------------------------------------------------------------
# SysEx wire protocol (copied from session_bootstrap.py; see module docstring)
# ---------------------------------------------------------------------------
SYX_MFG = 0x7D          # non-commercial manufacturer id
SYX_CMD = 0x01          # command chunk, more to follow
SYX_CMD_END = 0x11      # final command chunk
SYX_RESP = 0x02         # response chunk, more to follow
SYX_RESP_END = 0x12     # final response chunk
CHUNK_SIZE = 200        # payload bytes per frame

DEFAULT_TIMEOUT = 3.0

MAX_MIXER_TRACK = 124   # FL: 0 = Master, 1..124 inserts


# ---------------------------------------------------------------------------
# Op model
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class Op:
    """A single FL MCP command generated from a planned change."""

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
# Category system + keyword classifier
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class Category:
    """A track/channel category: canonical label, color, and keyword set."""

    label: str
    color: tuple[int, int, int]  # (r, g, b) 0-255
    keywords: tuple[str, ...]


# Priority order: FIRST match wins. More specific / compound categories come
# before generic ones so e.g. "bassdrum" -> KICK (not BASS) and a bare "riser"
# -> FX only after every instrument family has had a chance to claim it.
CATEGORIES: tuple[Category, ...] = (
    Category("KICK", (214, 40, 40), (
        "kick", "kicks", "kik", "bd", "bassdrum", "bass drum", "bass drums",
    )),
    Category("SNARE", (232, 122, 42), (
        "snare", "snares", "snr", "rimshot", "rim", "rims",
    )),
    Category("CLAP", (232, 74, 140), (
        "clap", "claps",
    )),
    Category("HAT", (232, 200, 60), (
        "hat", "hats", "hihat", "hihats", "hi hat", "hi hats", "hh", "ohh",
        "chh", "open hat", "closed hat", "cymbal", "cymbals", "ride", "crash",
        "crashes",
    )),
    Category("PERC", (170, 150, 60), (
        "perc", "percs", "percussion", "tom", "toms", "conga", "congas",
        "bongo", "bongos", "shaker", "shakers", "tambourine", "tamb",
        "cowbell", "clave", "claves", "woodblock", "block", "snap", "snaps",
        "click", "clicks",
    )),
    Category("BASS", (120, 70, 200), (
        "bass", "basses", "sub", "subs", "808", "808s", "reese", "wobble",
        "bassline", "sub bass",
    )),
    Category("VOX", (70, 160, 232), (
        "vox", "vocal", "vocals", "voc", "vocs", "acapella", "acappella",
        "acap", "adlib", "adlibs", "ad lib", "verse", "chorus", "harmony",
        "harmonies", "choir",
    )),
    Category("PAD", (60, 200, 190), (
        "pad", "pads", "atmos", "atmosphere", "ambient", "ambience", "drone",
        "drones", "texture", "textures", "string", "strings", "swell",
        "swells",
    )),
    Category("LEAD", (90, 200, 90), (
        "lead", "leads", "pluck", "plucks", "arp", "arps", "stab", "stabs",
        "synth", "synths", "saw", "seq", "melody", "keys", "piano", "bell",
        "bells",
    )),
    Category("FX", (140, 140, 150), (
        "fx", "sfx", "riser", "risers", "uplifter", "downlifter", "sweep",
        "sweeps", "impact", "impacts", "whoosh", "foley", "noise",
        "transition", "boom", "drop", "glitch",
    )),
)

CATEGORY_BY_LABEL: dict[str, Category] = {c.label: c for c in CATEGORIES}


def _normalize(name: str) -> str:
    """Lowercase and collapse every run of non-alphanumerics to a single space."""
    return re.sub(r"[^a-z0-9]+", " ", name.lower()).strip()


# Precompile one whole-word regex per keyword. `\b` around an escaped keyword
# gives whole-word matching on the normalized (space-separated) name, so "bass"
# does NOT fire on "brass" and "sub" does NOT fire on "subtle", while multiword
# keywords like "bass drum" still match.
_KEYWORD_RES: tuple[tuple[Category, tuple[re.Pattern[str], ...]], ...] = tuple(
    (cat, tuple(re.compile(rf"\b{re.escape(kw)}\b") for kw in cat.keywords))
    for cat in CATEGORIES
)


def classify(name: str) -> Category | None:
    """Classify a channel/track name into a Category, or None if ambiguous.

    Returns None for empty names, FL default names (Channel N / Insert N /
    Sampler / Audio N ...), and anything that matches no category keyword — the
    caller leaves those untouched.
    """
    norm = _normalize(name)
    if not norm:
        return None
    for cat, patterns in _KEYWORD_RES:
        for pat in patterns:
            if pat.search(norm):
                return cat
    return None


# ---------------------------------------------------------------------------
# Color helpers
# ---------------------------------------------------------------------------
def fl_hex_to_rgb(color: Any) -> tuple[int, int, int] | None:
    """Parse the FL device's color field ("0xBBGGRR" hex or int) to (r,g,b).

    FL stores colors as `(b<<16)|(g<<8)|r`. Returns None if unparseable.
    """
    try:
        if isinstance(color, str):
            val = int(color, 16) if color.lower().startswith("0x") else int(color, 16)
        elif isinstance(color, (int, float)):
            val = int(color)
        else:
            return None
    except (ValueError, TypeError):
        return None
    val &= 0xFFFFFF
    r = val & 0xFF
    g = (val >> 8) & 0xFF
    b = (val >> 16) & 0xFF
    return (r, g, b)


def rgb_hex(rgb: tuple[int, int, int]) -> str:
    return f"#{rgb[0]:02X}{rgb[1]:02X}{rgb[2]:02X}"


# ---------------------------------------------------------------------------
# Session model
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class Item:
    """A channel or mixer track read from FL."""

    kind: str          # "channel" | "mixer"
    index: int
    name: str
    rgb: tuple[int, int, int] | None  # current color, None if unknown

    @property
    def label(self) -> str:
        return f"{self.kind} {self.index}"


@dataclass(frozen=True)
class Change:
    """A proposed rename/recolor for one classified item."""

    item: Item
    category: Category
    rename: bool
    recolor: bool

    @property
    def new_name(self) -> str:
        return self.category.label

    @property
    def new_rgb(self) -> tuple[int, int, int]:
        return self.category.color

    def reasons(self) -> list[str]:
        out: list[str] = []
        if self.rename:
            out.append(f"name {self.item.name!r} -> {self.new_name!r}")
        if self.recolor:
            cur = rgb_hex(self.item.rgb) if self.item.rgb else "?"
            out.append(f"color {cur} -> {rgb_hex(self.new_rgb)}")
        return out


# ---------------------------------------------------------------------------
# Reading the session
# ---------------------------------------------------------------------------
def _ok(resp: Any) -> bool:
    return isinstance(resp, dict) and resp.get("success", False)


def read_channels(transport: Transport) -> list[Item]:
    """Read every channel-rack channel (index, name, color)."""
    resp = transport.send_command("channels.getCount", {"global_count": True})
    if not _ok(resp):
        raise RuntimeError(f"channels.getCount failed: {_err(resp)}")
    count = int(resp.get("count", 0) or 0)
    items: list[Item] = []
    for i in range(count):
        info = transport.send_command(
            "channels.getInfo", {"index": i, "use_global": True}
        )
        if not _ok(info):
            # Skip a channel we couldn't read rather than aborting the whole scan.
            continue
        items.append(
            Item(
                kind="channel",
                index=i,
                name=str(info.get("name", "") or ""),
                rgb=fl_hex_to_rgb(info.get("color")),
            )
        )
    return items


def read_mixer_tracks(transport: Transport) -> list[Item]:
    """Read user-named mixer tracks (skips defaults + Master via track 0 guard)."""
    resp = transport.send_command("mixer.getAllTracks", {"include_empty": False})
    if not _ok(resp):
        raise RuntimeError(f"mixer.getAllTracks failed: {_err(resp)}")
    items: list[Item] = []
    for t in resp.get("tracks", []) or []:
        idx = t.get("index")
        if not isinstance(idx, int) or idx == 0:  # never touch Master
            continue
        # getAllTracks omits color; read it per track for idempotent recolor.
        info = transport.send_command("mixer.getTrackInfo", {"track": idx})
        name = str((info.get("name") if _ok(info) else t.get("name")) or "")
        rgb = fl_hex_to_rgb(info.get("color")) if _ok(info) else None
        items.append(Item(kind="mixer", index=idx, name=name, rgb=rgb))
    return items


def _err(resp: Any) -> str:
    if isinstance(resp, dict):
        return str(resp.get("error", resp))
    return str(resp)


# ---------------------------------------------------------------------------
# Planning
# ---------------------------------------------------------------------------
def plan_changes(items: list[Item]) -> list[Change]:
    """Classify items and emit a Change for each that needs rename and/or recolor."""
    changes: list[Change] = []
    for item in items:
        cat = classify(item.name)
        if cat is None:
            continue
        rename = item.name != cat.label
        recolor = item.rgb != cat.color  # None (unknown) -> recolor
        if not (rename or recolor):
            continue
        changes.append(Change(item, cat, rename=rename, recolor=recolor))
    return changes


def changes_to_ops(changes: list[Change]) -> list[Op]:
    """Turn planned changes into an ordered op list (setName then setColor)."""
    ops: list[Op] = []
    for ch in changes:
        it = ch.item
        if ch.rename:
            if it.kind == "channel":
                ops.append(Op(
                    "channels.setName",
                    {"index": it.index, "name": ch.new_name},
                    f"{it.label} name -> {ch.new_name}",
                ))
            else:
                ops.append(Op(
                    "mixer.setTrackName",
                    {"track": it.index, "name": ch.new_name},
                    f"{it.label} name -> {ch.new_name}",
                ))
        if ch.recolor:
            r, g, b = ch.new_rgb
            if it.kind == "channel":
                ops.append(Op(
                    "channels.setColor",
                    {"index": it.index, "r": r, "g": g, "b": b},
                    f"{it.label} color -> {rgb_hex(ch.new_rgb)}",
                ))
            else:
                ops.append(Op(
                    "mixer.setTrackColor",
                    {"track": it.index, "r": r, "g": g, "b": b},
                    f"{it.label} color -> {rgb_hex(ch.new_rgb)}",
                ))
    return ops


# ---------------------------------------------------------------------------
# Real MIDI transport (self-contained SysEx; copied from session_bootstrap.py)
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

    @property
    def ok_count(self) -> int:
        return sum(1 for r in self.results if r.ok)

    @property
    def fail_count(self) -> int:
        return sum(1 for r in self.results if not r.ok)


def apply_ops(ops: list[Op], transport: Transport) -> ApplyReport:
    """Send each op through the transport, tolerating per-op failures."""
    report = ApplyReport()
    for op in ops:
        try:
            resp = transport.send_command(op.action, op.params)
        except Exception as e:  # transport-level error
            report.results.append(OpResult(op, False, f"transport error: {e}"))
            continue
        if _ok(resp):
            report.results.append(OpResult(op, True, "ok"))
        else:
            report.results.append(OpResult(op, False, _err(resp) or "unknown error"))
    return report


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------
def summarize(items: list[Item], changes: list[Change]) -> dict[str, Any]:
    by_cat: dict[str, int] = {}
    for ch in changes:
        by_cat[ch.category.label] = by_cat.get(ch.category.label, 0) + 1
    changed_idx = {(c.item.kind, c.item.index) for c in changes}
    return {
        "scanned": len(items),
        "changed": len(changes),
        "unchanged": len(items) - len(changed_idx),
        "by_category": dict(sorted(by_cat.items())),
    }


def _print_plan(items: list[Item], changes: list[Change], ops: list[Op]) -> None:
    if not changes:
        print("  (no changes - every classified item already clean; "
              "unrecognised names left untouched)")
    for ch in changes:
        print(f"  {ch.item.label:<12} [{ch.category.label}]  "
              + "; ".join(ch.reasons()))
    s = summarize(items, changes)
    cats = ", ".join(f"{k}:{v}" for k, v in s["by_category"].items()) or "-"
    print(f"\n  scanned {s['scanned']}, changed {s['changed']}, "
          f"unchanged {s['unchanged']} | by category: {cats}")
    print(f"  -> {len(ops)} FL ops")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def _scan(transport: Transport, which: str) -> list[Item]:
    items: list[Item] = []
    if which in ("channels", "both"):
        items += read_channels(transport)
    if which in ("mixer", "both"):
        items += read_mixer_tracks(transport)
    return items


def run(args: argparse.Namespace, transport_factory=MidiTransport) -> int:
    apply = args.apply
    transport = transport_factory()
    try:
        connect = getattr(transport, "connect", None)
        if callable(connect):
            connect()
    except RuntimeError as e:
        print(f"Error: could not connect to FL Studio: {e}", file=sys.stderr)
        print("Tip: FL Studio must be running with the FLStudioMCP controller "
              "enabled.", file=sys.stderr)
        return 3

    try:
        try:
            items = _scan(transport, args.only)
        except RuntimeError as e:
            print(f"Error scanning FL session: {e}", file=sys.stderr)
            return 3
        changes = plan_changes(items)
        ops = changes_to_ops(changes)

        if args.json:
            print(json.dumps({
                "dry_run": not apply,
                "summary": summarize(items, changes),
                "ops": [op.to_dict() for op in ops],
            }, indent=2))
            if not apply:
                return 0

        if not apply:
            print(f"[dry-run] scanning {args.only} -> {len(ops)} ops:\n")
            _print_plan(items, changes, ops)
            print("\n[dry-run] no session was touched. Re-run with --apply to execute.")
            return 0

        if not args.json:
            print(f"Applying janitor to {args.only} -> {len(ops)} ops...\n")
            _print_plan(items, changes, ops)
            print()

        report = apply_ops(ops, transport)
        for r in report.results:
            mark = "ok " if r.ok else "FAIL"
            line = f"  [{mark}] {r.op.desc}"
            if not r.ok:
                line += f"   ({r.detail})"
            print(line)
        print(f"\nDone: {report.ok_count} applied, {report.fail_count} failed.")
        return 0 if report.fail_count == 0 else 1
    finally:
        transport.close()


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="project_janitor.py",
        description="Heuristic auto-name/color for a running FL Studio session.",
    )
    p.add_argument(
        "--apply",
        action="store_true",
        help="Execute rename + recolor ops (default is a dry-run preview).",
    )
    p.add_argument(
        "--only",
        choices=("channels", "mixer", "both"),
        default="both",
        help="Restrict to channel rack, mixer tracks, or both (default: both).",
    )
    p.add_argument(
        "--json",
        action="store_true",
        help="Emit the plan (and, with --apply, still apply) as JSON.",
    )
    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
