# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "pytest>=8.0",
# ]
# ///
"""Offline test gate for RumbleBassline.pyscript (Qeynos W1).

FL Studio cannot run its piano-roll scripts headless, so this gate imports the
`.pyscript` with a MOCK `flpianoroll` module injected into sys.modules (Note
class + score list + ScriptDialog stub) and exercises the pure generator plus
the FL `apply()` glue.

Asserts (per the work order):
  1. No generated note starts on a kick position (a beat) UNLESS the pattern
     explicitly ghosts it at ghost velocity.
  2. All pitches belong to the selected scale; register bounds (root octave +/-1)
     are respected.
  3. Timing humanize is bounded to +/-5 ticks (and to the requested amount).
  4. Output is deterministic per seed (same seed -> identical; different seed ->
     different when humanize/fills are active).
  5. Base note count (fill_density = 0) matches the pattern math for every
     pattern type.

Run either way (from pyscripts/tests/):
    uv run --python 3.12 test_rumble.py       # plain-assert runner (the gate)
    uv run --python 3.12 pytest test_rumble.py

The mock `flpianoroll` module here is intended to be REUSED by W2-BREAK-CHOP.
"""
from __future__ import annotations

import importlib.util
import sys
import types
from importlib.machinery import SourceFileLoader
from pathlib import Path

PYSCRIPT = Path(__file__).resolve().parent.parent / "RumbleBassline.pyscript"


# --------------------------------------------------------------------------- #
# Mock `flpianoroll` (reusable by W2-BREAK-CHOP)
# --------------------------------------------------------------------------- #

class MockNote:
    """Mirror of FL's flpianoroll.Note (the properties we touch)."""

    def __init__(self):
        self.number = 0
        self.time = 0
        self.length = 0
        self.velocity = 0.8
        self.pan = 0.5
        self.selected = False
        self.muted = False


class MockScore:
    """Mirror of flpianoroll.score: an indexable note list + PPQ + selection."""

    def __init__(self, ppq=96):
        self.PPQ = ppq
        self._notes = []
        self._selection = (0, -1)   # (start, end); end<=start means "no selection"

    def addNote(self, note):
        self._notes.append(note)

    def getNote(self, i):
        return self._notes[i]

    def deleteNote(self, i):
        del self._notes[i]

    def clearNotes(self, all=True):
        self._notes = []

    @property
    def noteCount(self):
        return len(self._notes)

    def getTimelineSelection(self):
        return self._selection


class MockDialog:
    """Mirror of flpianoroll.ScriptDialog: collects inputs, serves defaults."""

    def __init__(self, title="", description=""):
        self.title = title
        self.description = description
        self._values = {}

    def AddInputCombo(self, name, options, default_index):
        self._values[name] = int(default_index)

    def AddInputKnob(self, name, value, vmin, vmax):
        self._values[name] = float(value)

    def AddInputKnobInt(self, name, value, vmin, vmax):
        self._values[name] = int(value)

    def AddInputCheckbox(self, name, value):
        self._values[name] = bool(value)

    def AddInputText(self, name, value):
        self._values[name] = value

    def SetValue(self, name, value):
        self._values[name] = value

    def GetInputValue(self, name):
        return self._values[name]

    def Execute(self):
        return True


def make_mock_flp(ppq=96):
    mod = types.ModuleType("flpianoroll")
    mod.Note = MockNote
    mod.score = MockScore(ppq)
    mod.ScriptDialog = MockDialog
    return mod


def load_module(ppq=96):
    """Load the .pyscript with a fresh mock flpianoroll injected."""
    sys.modules["flpianoroll"] = make_mock_flp(ppq)
    # `.pyscript` isn't a recognized source suffix, so load via an explicit loader.
    loader = SourceFileLoader("rumble_bassline", str(PYSCRIPT))
    spec = importlib.util.spec_from_loader("rumble_bassline", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


RB = load_module()

PATTERNS = RB.PATTERNS
SCALES = RB.SCALES
F1 = RB.note_name_to_midi(5, 1)   # F, octave 1


def _gen(pattern="rolling_16ths", scale="minor", bars=2, ppq=96, **kw):
    params = dict(
        root_note=F1, scale=scale, pattern=pattern, bars=bars, ppq=ppq,
        note_length_pct=75, ghost_vel=35, accent_vel=95,
        humanize_vel=8, humanize_ticks=3, fill_density=20, seed=7,
    )
    params.update(kw)
    return RB.generate_bassline(**params)


# --------------------------------------------------------------------------- #
# (1) Kick-collision rule
# --------------------------------------------------------------------------- #

def test_no_accent_on_kick_and_onbeat_notes_are_ghosts():
    ghost_ceiling = 0.35 + 1e-9   # ghost_vel = 35%
    for pattern in PATTERNS:
        for scale in SCALES:
            notes = _gen(pattern=pattern, scale=scale, bars=3, seed=42,
                         humanize_ticks=0, fill_density=30)
            for n in notes:
                on_kick = (n["nominal_time"] % 96) == 0
                assert on_kick == n["on_beat"], (pattern, n)
                if on_kick:
                    # allowed ONLY if ghosted at ghost velocity
                    assert n["ghost"], ("accent/base note on kick", pattern, n)
                    assert n["nominal_velocity"] <= ghost_ceiling, (pattern, n)
                if n["accent"]:
                    assert not on_kick, ("accent landed on kick", pattern, n)


def test_offbeat_and_broken_never_touch_a_beat():
    # These two patterns must have ZERO notes on beats (not even ghosts).
    for pattern in ("offbeat_8ths", "broken"):
        notes = _gen(pattern=pattern, bars=4, humanize_ticks=0, fill_density=0)
        assert all((n["nominal_time"] % 96) != 0 for n in notes), pattern


# --------------------------------------------------------------------------- #
# (2) Scale membership + register bounds
# --------------------------------------------------------------------------- #

def test_pitches_in_scale_and_register_bounds():
    for pattern in PATTERNS:
        for scale, offsets in SCALES.items():
            notes = _gen(pattern=pattern, scale=scale, bars=4, seed=3,
                         fill_density=100, passing_prob=1.0)  # force passing tones
            # rolling_16ths occupies every slot, so it has no gaps to fill.
            if pattern != "rolling_16ths":
                assert any(n["fill"] for n in notes), \
                    ("fills should appear at density 100", pattern)
            for n in notes:
                assert (n["number"] - F1) % 12 in offsets, (scale, n)
                assert F1 - 12 <= n["number"] <= F1 + 12, ("register", n)


# --------------------------------------------------------------------------- #
# (3) Timing humanize bound
# --------------------------------------------------------------------------- #

def test_timing_humanize_bounded():
    for ht in (0, 3, 5):
        for pattern in PATTERNS:
            for seed in (0, 1, 99, 1234):
                notes = _gen(pattern=pattern, bars=5, seed=seed,
                             humanize_ticks=ht, fill_density=40)
                for n in notes:
                    dev = abs(n["time"] - n["nominal_time"])
                    assert dev <= ht, (pattern, ht, dev, n)
                    assert dev <= 5
                    assert n["time"] >= 0


def test_humanize_ticks_clamped_to_five():
    # requesting more than 5 must still stay within +/-5
    notes = _gen(pattern="rolling_16ths", bars=4, seed=5, humanize_ticks=50)
    for n in notes:
        assert abs(n["time"] - n["nominal_time"]) <= 5


# --------------------------------------------------------------------------- #
# (4) Determinism per seed
# --------------------------------------------------------------------------- #

def test_deterministic_same_seed():
    for pattern in PATTERNS:
        a = _gen(pattern=pattern, bars=6, seed=2024, fill_density=50)
        b = _gen(pattern=pattern, bars=6, seed=2024, fill_density=50)
        assert a == b, pattern


def test_different_seed_differs():
    # with humanize + fills active, different seeds should diverge
    a = _gen(pattern="rolling_16ths", bars=8, seed=1, fill_density=50)
    b = _gen(pattern="rolling_16ths", bars=8, seed=2, fill_density=50)
    assert a != b


# --------------------------------------------------------------------------- #
# (5) Note-count math
# --------------------------------------------------------------------------- #

def test_base_note_count_matches_math():
    expected_per_bar = {
        "offbeat_8ths": 4,
        "rolling_16ths": 16,
        "gallop": 12,
        "broken": 6,
    }
    for pattern in PATTERNS:
        for bars in (1, 2, 4, 8):
            notes = _gen(pattern=pattern, bars=bars, fill_density=0)
            assert len(notes) == expected_per_bar[pattern] * bars, (pattern, bars)
            assert len(notes) == RB.expected_note_count(pattern, bars)


def test_fills_add_only_ghost_notes_in_gaps():
    base = _gen(pattern="broken", bars=4, seed=8, fill_density=0)
    filled = _gen(pattern="broken", bars=4, seed=8, fill_density=100)
    assert len(filled) > len(base)
    extras = [n for n in filled if n["fill"]]
    for n in extras:
        assert n["ghost"] and not n["accent"]
        # a fill never lands on a beat (gaps are non-beat 16ths)
        assert (n["nominal_time"] % 96) != 0


# --------------------------------------------------------------------------- #
# Multiple PPQ resolutions
# --------------------------------------------------------------------------- #

def test_various_ppq_resolutions():
    for ppq in (96, 192, 384, 960):
        notes = _gen(pattern="gallop", bars=2, ppq=ppq, humanize_ticks=0,
                     fill_density=0)
        step16 = ppq // 4
        for n in notes:
            assert n["nominal_time"] % step16 == 0, ppq
        assert len(notes) == 12 * 2


# --------------------------------------------------------------------------- #
# FL apply() glue via the mock
# --------------------------------------------------------------------------- #

def test_apply_writes_notes_to_score():
    mod = load_module(ppq=96)
    form = mod.createDialog()
    # dialog defaults: F, octave 1, Minor, Offbeat 8ths, Bars=2, fills=20, seed=1
    mod.apply(form)
    score = sys.modules["flpianoroll"].score
    assert score.noteCount >= 4 * 2   # offbeat 8ths base = 8, plus any fills
    root = mod.note_name_to_midi(5, 1)
    for i in range(score.noteCount):
        n = score.getNote(i)
        assert root - 12 <= n.number <= root + 12
        assert n.time >= 0
        assert n.length >= 1
        assert 0.0 <= n.velocity <= 1.0


def test_apply_respects_timeline_selection_offset():
    mod = load_module(ppq=96)
    score = sys.modules["flpianoroll"].score
    score._selection = (960, 1920)   # a 10-beat-in selection
    form = mod.createDialog()
    mod.apply(form)
    assert score.noteCount > 0
    assert min(score.getNote(i).time for i in range(score.noteCount)) >= 960


# --------------------------------------------------------------------------- #
# Plain-assert runner
# --------------------------------------------------------------------------- #

def _main():
    tests = [v for k, v in sorted(globals().items())
             if k.startswith("test_") and callable(v)]
    failures = 0
    for t in tests:
        try:
            t()
            print("PASS %s" % t.__name__)
        except AssertionError as e:
            failures += 1
            print("FAIL %s: %s" % (t.__name__, e))
        except Exception as e:  # noqa: BLE001
            failures += 1
            print("ERROR %s: %r" % (t.__name__, e))
    print("\n%d/%d passed" % (len(tests) - failures, len(tests)))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_main())
