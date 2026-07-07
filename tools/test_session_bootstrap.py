# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
"""Offline test gate for session_bootstrap.py (Qeynos W4).

No FL Studio, no MIDI backend required: the transport is mocked and mido is only
imported lazily inside MidiTransport, so importing the module here is side-effect
free. Run:  uv run --python 3.12 tools\\test_session_bootstrap.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import session_bootstrap as sb  # noqa: E402

FIXTURES = HERE / "tests" / "fixtures"
TEMPLATES = sb.templates_dir_default()

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


# ---------------------------------------------------------------------------
# Mock transport
# ---------------------------------------------------------------------------
class RecordingTransport:
    """Mock Transport: records commands, returns a scripted response per action."""

    def __init__(self, fail_actions: set[str] | None = None) -> None:
        self.sent: list[tuple[str, dict]] = []
        self.fail_actions = fail_actions or set()
        self.closed = False

    def send_command(self, action: str, params: dict) -> dict:
        self.sent.append((action, dict(params)))
        if action in self.fail_actions:
            return {"success": False, "error": "simulated FL error"}
        # mirror the FL MCP server's success envelope
        return {"success": True, "echo": {"action": action, "params": params}}

    def close(self) -> None:
        self.closed = True


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------
def test_shipped_templates_validate() -> None:
    print("[validate shipped templates]")
    for name in ("TECHNO", "DNB"):
        path = TEMPLATES / f"{name}.json"
        check(f"{name}.json exists", path.exists(), str(path))
        try:
            tpl = sb.load_template(path)  # validates
            check(f"{name} validates", True)
            check(f"{name} has mixer tracks", len(tpl["mixer_tracks"]) > 0)
        except Exception as e:  # noqa: BLE001
            check(f"{name} validates", False, repr(e))


def test_op_snapshots() -> None:
    print("[op-list snapshot comparison]")
    for name in ("TECHNO", "DNB"):
        tpl = sb.load_template(TEMPLATES / f"{name}.json")
        ops, skipped = sb.generate_ops(tpl)
        got = {"ops": [o.to_dict() for o in ops], "skipped": skipped}
        fixture = FIXTURES / f"{name}.ops.json"
        check(f"{name} fixture exists", fixture.exists(), str(fixture))
        if fixture.exists():
            want = json.loads(fixture.read_text(encoding="utf-8"))
            check(f"{name} op-list matches snapshot", got == want,
                  "generated op list differs from fixture")


def test_tempo_is_skipped_not_failed() -> None:
    print("[tempo unsupported -> reported, not fatal]")
    tpl = sb.load_template(TEMPLATES / "TECHNO.json")
    ops, skipped = sb.generate_ops(tpl)
    check("tempo produces a skip report", any("tempo" in s for s in skipped), str(skipped))
    check("no op targets a tempo action",
          all("tempo" not in o.action.lower() for o in ops), "")


def test_op_ordering() -> None:
    print("[deterministic op ordering: loop -> name/color -> routing]")
    tpl = sb.load_template(TEMPLATES / "TECHNO.json")
    ops, _ = sb.generate_ops(tpl)
    check("first op is loop mode", ops[0].action == "transport.setLoopMode", ops[0].action)
    # name always immediately precedes its color
    for i, o in enumerate(ops):
        if o.action == "mixer.setTrackColor":
            prev = ops[i - 1]
            check(f"color for track {o.params['track']} preceded by its name",
                  prev.action == "mixer.setTrackName"
                  and prev.params["track"] == o.params["track"], prev.action)
            break
    check("routing ops come last",
          ops[-1].action == "channels.routeToMixer", ops[-1].action)


def test_apply_through_mock() -> None:
    print("[apply through mock transport]")
    tpl = sb.load_template(TEMPLATES / "DNB.json")
    ops, skipped = sb.generate_ops(tpl)
    t = RecordingTransport()
    report = sb.apply_ops(ops, skipped, t)
    check("all ops sent", len(t.sent) == len(ops), f"{len(t.sent)} vs {len(ops)}")
    check("all ops ok", report.fail_count == 0, f"{report.fail_count} failed")
    check("ok_count == len(ops)", report.ok_count == len(ops), "")
    check("skipped carried into report", report.skipped == skipped, "")
    # verify wire shape of a color command
    color_cmds = [p for a, p in t.sent if a == "mixer.setTrackColor"]
    check("color command carries r/g/b ints",
          bool(color_cmds) and all(k in color_cmds[0] for k in ("track", "r", "g", "b")),
          str(color_cmds[:1]))


def test_apply_tolerates_per_op_failure() -> None:
    print("[per-op failure is a warning, not an abort]")
    tpl = sb.load_template(TEMPLATES / "DNB.json")
    ops, skipped = sb.generate_ops(tpl)
    # simulate FL rejecting routing (e.g. channels don't exist yet)
    t = RecordingTransport(fail_actions={"channels.routeToMixer"})
    report = sb.apply_ops(ops, skipped, t)
    n_routes = sum(1 for o in ops if o.action == "channels.routeToMixer")
    check("every op still attempted", len(t.sent) == len(ops), f"{len(t.sent)}/{len(ops)}")
    check("routing failures counted", report.fail_count == n_routes,
          f"{report.fail_count} vs {n_routes}")
    check("non-routing ops still succeeded",
          report.ok_count == len(ops) - n_routes, "")


def test_idempotent_op_generation() -> None:
    print("[idempotency: re-apply yields identical absolute-set ops]")
    tpl = sb.load_template(TEMPLATES / "TECHNO.json")
    ops1, _ = sb.generate_ops(tpl)
    ops2, _ = sb.generate_ops(tpl)
    check("op lists identical across runs",
          [o.to_dict() for o in ops1] == [o.to_dict() for o in ops2], "")
    # applying twice sends the same command sequence (same names/colors/routes)
    t1, t2 = RecordingTransport(), RecordingTransport()
    sb.apply_ops(ops1, [], t1)
    sb.apply_ops(ops1, [], t2)
    check("two applies send identical command streams", t1.sent == t2.sent, "")


def test_color_parsing() -> None:
    print("[color parsing]")
    check("#RRGGBB", sb.parse_color("#8B1E1E") == (139, 30, 30), "")
    check("bare hex", sb.parse_color("8B1E1E") == (139, 30, 30), "")
    check("rgb list", sb.parse_color([139, 30, 30]) == (139, 30, 30), "")
    check("hex roundtrip", sb.color_hex(139, 30, 30) == "#8B1E1E", "")
    for bad in ("#12345", "#GGGGGG", [1, 2], [300, 0, 0], 12345):
        try:
            sb.parse_color(bad)
            check(f"rejects {bad!r}", False, "no error raised")
        except (ValueError, TypeError):
            check(f"rejects {bad!r}", True, "")


def test_validation_errors() -> None:
    print("[template validation rejects malformed input]")
    bad_cases = {
        "missing name": {"mixer_tracks": [{"index": 1, "name": "K"}]},
        "empty name": {"name": " ", "mixer_tracks": [{"index": 1, "name": "K"}]},
        "no mixer_tracks": {"name": "X"},
        "empty mixer_tracks": {"name": "X", "mixer_tracks": []},
        "track index oob": {"name": "X", "mixer_tracks": [{"index": 999, "name": "K"}]},
        "track missing name": {"name": "X", "mixer_tracks": [{"index": 1}]},
        "duplicate index": {"name": "X", "mixer_tracks": [
            {"index": 1, "name": "A"}, {"index": 1, "name": "B"}]},
        "bad loop_mode": {"name": "X", "loop_mode": "verse",
                          "mixer_tracks": [{"index": 1, "name": "K"}]},
        "bad color": {"name": "X", "mixer_tracks": [
            {"index": 1, "name": "K", "color": "nope"}]},
        "bad routing": {"name": "X", "mixer_tracks": [{"index": 1, "name": "K"}],
                        "routing": [{"channel": -1, "mixer_track": 1}]},
    }
    for label, tpl in bad_cases.items():
        try:
            sb.validate_template(tpl)
            check(f"rejects: {label}", False, "no error raised")
        except ValueError:
            check(f"rejects: {label}", True, "")


def test_valid_minimal_template() -> None:
    print("[a minimal valid template passes]")
    tpl = {"name": "Min", "mixer_tracks": [{"index": 1, "name": "KICK"}]}
    try:
        sb.validate_template(tpl)
        ops, skipped = sb.generate_ops(tpl)
        check("minimal validates", True)
        check("minimal -> 1 op (name only, no color)", len(ops) == 1, str(ops))
        check("minimal no skips", skipped == [], str(skipped))
    except Exception as e:  # noqa: BLE001
        check("minimal validates", False, repr(e))


def main() -> int:
    tests = [
        test_shipped_templates_validate,
        test_op_snapshots,
        test_tempo_is_skipped_not_failed,
        test_op_ordering,
        test_apply_through_mock,
        test_apply_tolerates_per_op_failure,
        test_idempotent_op_generation,
        test_color_parsing,
        test_validation_errors,
        test_valid_minimal_template,
    ]
    for t in tests:
        t()
    print(f"\n{_passes} checks passed, {len(_failures)} failed.")
    if _failures:
        print("\nFAILURES:")
        for f in _failures:
            print(f"  - {f}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
