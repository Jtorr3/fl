# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
"""Offline test gate for project_janitor.py (Qeynos W5).

No FL Studio, no MIDI backend required: the transport is mocked and mido is only
imported lazily inside MidiTransport, so importing the module here is side-effect
free. Run:  uv run --python 3.12 tools\\test_project_janitor.py
"""

from __future__ import annotations

import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import project_janitor as pj  # noqa: E402

_failures: list[str] = []
_passes = 0


def check(name: str, cond: bool, detail: str = "") -> None:
    global _passes
    if cond:
        _passes += 1
        print(f"  ok   {name}")
    else:
        _failures.append(f"{name}: {detail}")
        print(f"  FAIL {name}  {detail}")


def rgb_to_fl_hex(rgb: tuple[int, int, int]) -> str:
    r, g, b = rgb
    return f"0x{((b << 16) | (g << 8) | r):06x}"


# ---------------------------------------------------------------------------
# Mock transport: scripts read responses, records writes
# ---------------------------------------------------------------------------
class MockFL:
    """A scripted FL session for the transport protocol.

    channels: list of (name, rgb-or-None)   -> indices 0..n-1
    mixer:    dict index -> (name, rgb-or-None)  (index 0 should be Master)
    """

    def __init__(
        self,
        channels: list[tuple[str, tuple[int, int, int] | None]] | None = None,
        mixer: dict[int, tuple[str, tuple[int, int, int] | None]] | None = None,
    ) -> None:
        self.channels = channels or []
        self.mixer = mixer or {}
        self.sent: list[tuple[str, dict]] = []
        self.closed = False

    def connect(self) -> None:
        pass

    def send_command(self, action, params):  # noqa: ANN001
        self.sent.append((action, dict(params)))
        if action == "channels.getCount":
            return {"success": True, "count": len(self.channels)}
        if action == "channels.getInfo":
            i = params["index"]
            name, rgb = self.channels[i]
            color = rgb_to_fl_hex(rgb) if rgb is not None else "0x565148"
            return {"success": True, "index": i, "name": name, "color": color}
        if action == "mixer.getAllTracks":
            tracks = [{"index": i, "name": self.mixer[i][0]}
                      for i in sorted(self.mixer)]
            return {"success": True, "tracks": tracks}
        if action == "mixer.getTrackInfo":
            t = params["track"]
            name, rgb = self.mixer[t]
            color = rgb_to_fl_hex(rgb) if rgb is not None else "0x565148"
            return {"success": True, "index": t, "name": name, "color": color}
        if action in ("channels.setName", "channels.setColor",
                      "mixer.setTrackName", "mixer.setTrackColor"):
            return {"success": True}
        return {"success": False, "error": f"unexpected action {action}"}

    def writes(self) -> list[tuple[str, dict]]:
        write_actions = {"channels.setName", "channels.setColor",
                         "mixer.setTrackName", "mixer.setTrackColor"}
        return [(a, p) for a, p in self.sent if a in write_actions]

    def close(self) -> None:
        self.closed = True


# ---------------------------------------------------------------------------
# 1. Classifier — >=10 cases incl. ambiguous -> unchanged (None)
# ---------------------------------------------------------------------------
def test_classifier() -> None:
    print("[classifier]")
    classified = {
        "kick": "KICK",
        "Deep Kick 02": "KICK",
        "BD": "KICK",
        "bassdrum": "KICK",          # compound: KICK beats BASS
        "Bass Drum": "KICK",
        "Sub Bass": "BASS",
        "808": "BASS",
        "Reese": "BASS",
        "Clap": "CLAP",
        "Snare Top": "SNARE",
        "Closed Hat": "HAT",
        "OH Ride": "HAT",
        "Perc Loop": "PERC",
        "Shaker": "PERC",
        "Lead Pluck": "LEAD",
        "Warm Pad": "PAD",
        "Vocal Chop": "VOX",
        "Adlib": "VOX",
        "Riser FX": "FX",
        "Uplifter": "FX",
    }
    for name, want in classified.items():
        cat = pj.classify(name)
        check(f"classify {name!r} -> {want}",
              cat is not None and cat.label == want,
              f"got {cat.label if cat else None}")

    ambiguous = [
        "", "Channel 1", "Insert 5", "Sampler", "Audio 3", "Loop",
        "Full Mix", "brass", "subtle", "My Track", "Master",
    ]
    for name in ambiguous:
        check(f"classify {name!r} -> None (untouched)",
              pj.classify(name) is None,
              f"got {pj.classify(name)}")


# ---------------------------------------------------------------------------
# 2. Color parsing
# ---------------------------------------------------------------------------
def test_color_parse() -> None:
    print("[color parse]")
    # FL stores (b<<16)|(g<<8)|r; (214,40,40) -> 0x2828d6
    check("fl_hex_to_rgb 0x2828d6 -> (214,40,40)",
          pj.fl_hex_to_rgb("0x2828d6") == (214, 40, 40),
          str(pj.fl_hex_to_rgb("0x2828d6")))
    check("fl_hex_to_rgb round-trips every category color",
          all(pj.fl_hex_to_rgb(rgb_to_fl_hex(c.color)) == c.color
              for c in pj.CATEGORIES))
    check("fl_hex_to_rgb junk -> None", pj.fl_hex_to_rgb(None) is None)


# ---------------------------------------------------------------------------
# 3. Planning: rename+recolor / rename-only / recolor-only / idempotent
# ---------------------------------------------------------------------------
def test_planning() -> None:
    print("[planning]")
    kick = pj.CATEGORY_BY_LABEL["KICK"]

    # full change: bad name + bad color
    it = pj.Item("channel", 0, "kick_deep", (255, 255, 255))
    ch = pj.plan_changes([it])
    check("dirty kick -> 1 change, rename+recolor",
          len(ch) == 1 and ch[0].rename and ch[0].recolor)

    # rename only: wrong name, right color
    it = pj.Item("channel", 1, "kick_deep", kick.color)
    ch = pj.plan_changes([it])
    check("right-color kick -> rename only",
          len(ch) == 1 and ch[0].rename and not ch[0].recolor)

    # recolor only: right name, wrong color
    it = pj.Item("channel", 2, "KICK", (1, 2, 3))
    ch = pj.plan_changes([it])
    check("right-name kick -> recolor only",
          len(ch) == 1 and not ch[0].rename and ch[0].recolor)

    # idempotent: canonical name + canonical color -> no change
    it = pj.Item("channel", 3, "KICK", kick.color)
    check("clean kick -> 0 changes", pj.plan_changes([it]) == [])

    # ambiguous -> untouched
    it = pj.Item("channel", 4, "Sampler", (255, 255, 255))
    check("ambiguous name -> 0 changes", pj.plan_changes([it]) == [])


# ---------------------------------------------------------------------------
# 4. Op-list snapshot (order + params for channel vs mixer)
# ---------------------------------------------------------------------------
def test_op_snapshot() -> None:
    print("[op snapshot]")
    clap = pj.CATEGORY_BY_LABEL["CLAP"]
    items = [
        pj.Item("channel", 5, "clap_909", (255, 255, 255)),
        pj.Item("mixer", 3, "Clap Bus", (255, 255, 255)),
    ]
    ops = pj.changes_to_ops(pj.plan_changes(items))
    got = [(o.action, o.params) for o in ops]
    r, g, b = clap.color
    want = [
        ("channels.setName", {"index": 5, "name": "CLAP"}),
        ("channels.setColor", {"index": 5, "r": r, "g": g, "b": b}),
        ("mixer.setTrackName", {"track": 3, "name": "CLAP"}),
        ("mixer.setTrackColor", {"track": 3, "r": r, "g": g, "b": b}),
    ]
    check("op list matches snapshot", got == want, f"\n   got={got}\n   want={want}")


# ---------------------------------------------------------------------------
# 5. read_* against the mock (Master guard, color read-back)
# ---------------------------------------------------------------------------
def test_reads() -> None:
    print("[reads]")
    fl = MockFL(
        channels=[("kick_deep", (255, 255, 255)), ("Sampler", None)],
        mixer={0: ("Master", None), 3: ("Clap Bus", (255, 255, 255))},
    )
    chans = pj.read_channels(fl)
    check("read 2 channels", len(chans) == 2)
    check("channel color parsed", chans[0].rgb == (255, 255, 255))
    tracks = pj.read_mixer_tracks(fl)
    check("mixer read skips Master (track 0)",
          len(tracks) == 1 and tracks[0].index == 3)


# ---------------------------------------------------------------------------
# 6. End-to-end run() dry-run then --apply via the mock (no FL, no MIDI)
# ---------------------------------------------------------------------------
def _args(**kw):
    import argparse
    ns = argparse.Namespace(apply=False, only="both", json=False)
    for k, v in kw.items():
        setattr(ns, k, v)
    return ns


def test_end_to_end() -> None:
    print("[end-to-end run()]")
    bass = pj.CATEGORY_BY_LABEL["BASS"]

    def factory():
        return MockFL(
            channels=[
                ("kick_deep", (255, 255, 255)),   # KICK: rename + recolor -> 2 ops
                ("Sub Bass", bass.color),         # BASS: rename only        -> 1 op
                ("Sampler", None),                # ambiguous                -> 0
            ],
            mixer={0: ("Master", None), 3: ("Clap Bus", (255, 255, 255))},  # 2 ops
        )

    # dry-run: NOTHING written
    dry = factory()
    rc = pj.run(_args(apply=False), transport_factory=lambda: dry)
    check("dry-run returns 0", rc == 0)
    check("dry-run writes nothing", dry.writes() == [], str(dry.writes()))
    check("dry-run closed transport", dry.closed)

    # apply: exactly 5 write ops
    live = factory()
    rc = pj.run(_args(apply=True), transport_factory=lambda: live)
    writes = live.writes()
    check("apply returns 0", rc == 0)
    check("apply emits 5 write ops", len(writes) == 5, f"got {len(writes)}: {writes}")
    actions = [a for a, _ in writes]
    check("apply renames kick channel",
          ("channels.setName", {"index": 0, "name": "KICK"}) in writes)
    check("apply recolors clap mixer track",
          any(a == "mixer.setTrackColor" and p["track"] == 3 for a, p in writes))
    check("apply does NOT recolor the right-colored bass",
          actions.count("channels.setColor") == 1,  # only the kick recolors
          str(actions))

    # idempotent: re-running run() over the now-clean session -> 0 writes.
    # (Simulate by feeding a clean session.)
    clean = MockFL(
        channels=[("KICK", pj.CATEGORY_BY_LABEL["KICK"].color)],
        mixer={0: ("Master", None)},
    )
    pj.run(_args(apply=True), transport_factory=lambda: clean)
    check("idempotent: clean session -> 0 writes", clean.writes() == [])


def main() -> int:
    for t in (
        test_classifier,
        test_color_parse,
        test_planning,
        test_op_snapshot,
        test_reads,
        test_end_to_end,
    ):
        t()
    print()
    if _failures:
        print(f"FAILED {len(_failures)} / {_passes + len(_failures)}:")
        for f in _failures:
            print(f"  - {f}")
        return 1
    print(f"PASSED all {_passes} checks.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
