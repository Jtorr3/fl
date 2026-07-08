# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "pytest>=8.0",
# ]
# ///
"""Offline test gate for BreakChop.pyscript (Qeynos W2).

FL Studio cannot run its piano-roll scripts headless, so this gate imports the
`.pyscript` with the SHARED mock `flpianoroll` (pyscripts/tests/mock_fl.py,
factored out of W1's test_rumble.py) and exercises the pure `chop_notes(...)`
generator plus the FL `apply()` glue.

Asserts (per the work order):
  (1) only SELECTED notes are altered - unselected notes stay byte-for-byte;
  (2) total span is preserved (last note end == original last end, exactly, and
      always within one roll subdivision);
  (3) rolls subdivide correctly (roll_count notes tiling the original span, with
      decaying velocity);
  (4) deterministic per seed (same seed identical, different seed differs);
  (5) keep-first-beat leaves the downbeat note in place (byte-for-byte);
  (6) intensity 0 = no changes (output == input).
Plus: PPQ independence, the no-selection whole-score fallback, reverse emulated
as a double-time repeat, and permutation staying within the selection.

Run (from pyscripts/tests/):
    uv run --python 3.12 test_breakchop.py          # plain-assert gate
    uv run --python 3.12 --with pytest pytest -q test_breakchop.py
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mock_fl import load_pyscript  # noqa: E402

PYSCRIPT = Path(__file__).resolve().parent.parent / "BreakChop.pyscript"


def load_module(ppq=96):
    return load_pyscript(PYSCRIPT, "break_chop", ppq)


BC = load_module()


# --------------------------------------------------------------------------- #
# Fixtures: build a sliced break as a contiguous run of note "slices"
# --------------------------------------------------------------------------- #

def make_break(n=16, ppq=96, base_pitch=60, vel=0.8, start=0, step=None):
    """`n` contiguous slices, each `step` ticks (default a 16th), distinct pitch
    per slice so permutation/passthrough is observable."""
    step = step if step is not None else ppq // 4
    return [
        {"number": base_pitch + i, "time": start + i * step,
         "length": step, "velocity": vel}
        for i in range(n)
    ]


def span_end(notes):
    return max(nd["time"] + nd["length"] for nd in notes)


def _chop(notes, ppq=96, **kw):
    params = dict(
        intensity=100, permute_amount=50, roll_chance=30, roll_count=3,
        roll_decay=30, stutter_chance=20, gate_pct=50, reverse_chance=15,
        keep_first_beat=True, humanize_vel=0, seed=7,
    )
    params.update(kw)
    return BC.chop_notes(notes, ppq, **params)


# --------------------------------------------------------------------------- #
# (6) intensity 0 = no changes
# --------------------------------------------------------------------------- #

def test_intensity_zero_is_identity():
    src = make_break(16)
    out = _chop(src, intensity=0, humanize_vel=30, keep_first_beat=False, seed=99)
    assert len(out) == len(src)
    # sort both the same way and compare number/time/length/velocity
    a = sorted((n["number"], n["time"], n["length"], round(n["velocity"], 6))
               for n in src)
    b = sorted((n["number"], n["time"], n["length"], round(n["velocity"], 6))
               for n in out)
    assert a == b
    assert all(n["role"] == "passthrough" for n in out)


# --------------------------------------------------------------------------- #
# (2) span preservation
# --------------------------------------------------------------------------- #

def test_span_preserved_exactly_all_configs():
    src = make_break(16)
    orig_end = span_end(src)
    # sweep a range of operation mixes / seeds, incl. stutter-heavy on the tail
    for seed in range(12):
        for cfg in (
            dict(roll_chance=100, stutter_chance=0, reverse_chance=0),
            dict(roll_chance=0, stutter_chance=100, reverse_chance=0, gate_pct=20),
            dict(roll_chance=0, stutter_chance=0, reverse_chance=100),
            dict(roll_chance=40, stutter_chance=40, reverse_chance=40),
        ):
            out = _chop(src, seed=seed, keep_first_beat=False, **cfg)
            assert span_end(out) == orig_end, (seed, cfg)
            # start is preserved too (nothing emitted before the first slice)
            assert min(n["time"] for n in out) == min(n["time"] for n in src)


def test_span_preserved_with_long_leading_note():
    # the max-end note is NOT the last-in-time: a long slice up front.
    src = [
        {"number": 40, "time": 0, "length": 400, "velocity": 0.9},   # longest
        {"number": 50, "time": 100, "length": 24, "velocity": 0.7},
        {"number": 55, "time": 200, "length": 24, "velocity": 0.7},
    ]
    orig_end = span_end(src)
    for seed in range(8):
        out = _chop(src, seed=seed, keep_first_beat=False, stutter_chance=100,
                    gate_pct=15)
        assert span_end(out) == orig_end, seed


# --------------------------------------------------------------------------- #
# (3) rolls subdivide correctly
# --------------------------------------------------------------------------- #

def test_rolls_subdivide_and_decay():
    for rc in (2, 3, 4):
        src = make_break(8)
        out = _chop(src, roll_chance=100, stutter_chance=0, reverse_chance=0,
                    permute_amount=0, keep_first_beat=False, roll_count=rc,
                    roll_decay=40, humanize_vel=0, seed=3)
        # every slot -> rc retriggers => total = n * rc
        assert len(out) == len(src) * rc
        by_slot = {}
        for n in out:
            by_slot.setdefault(n["slot"], []).append(n)
        for i, slot_notes in by_slot.items():
            slot_notes.sort(key=lambda n: n["time"])
            assert len(slot_notes) == rc
            assert all(n["role"] == "roll" for n in slot_notes)
            # tile exactly across the original slice span
            orig = src[i]
            assert slot_notes[0]["time"] == orig["time"]
            end = slot_notes[-1]["time"] + slot_notes[-1]["length"]
            assert end == orig["time"] + orig["length"]
            # contiguous (no gaps / overlaps)
            for a, b in zip(slot_notes, slot_notes[1:]):
                assert a["time"] + a["length"] == b["time"]
            # velocity decays (non-increasing, and strictly below the first)
            vels = [n["velocity"] for n in slot_notes]
            assert all(x >= y - 1e-9 for x, y in zip(vels, vels[1:]))
            assert vels[-1] < vels[0]


# --------------------------------------------------------------------------- #
# (4) determinism per seed
# --------------------------------------------------------------------------- #

def test_deterministic_same_seed():
    src = make_break(16)
    a = _chop(src, seed=2024, humanize_vel=10)
    b = _chop(src, seed=2024, humanize_vel=10)
    assert a == b


def test_different_seed_differs():
    src = make_break(16)
    a = _chop(src, seed=1, humanize_vel=10)
    b = _chop(src, seed=2, humanize_vel=10)
    assert a != b


# --------------------------------------------------------------------------- #
# (5) keep-first-beat protects the downbeat
# --------------------------------------------------------------------------- #

def test_keep_first_beat_protects_downbeat():
    src = make_break(16, ppq=96)          # slice 0 starts at tick 0 (a bar line)
    for seed in range(10):
        out = _chop(src, keep_first_beat=True, intensity=100, permute_amount=100,
                    roll_chance=100, seed=seed)
        downbeat = [n for n in out if n["time"] == 0]
        # exactly one note on the downbeat, unchanged from the original slice 0
        assert len(downbeat) == 1, seed
        d = downbeat[0]
        assert d["role"] == "passthrough"
        assert d["number"] == src[0]["number"]
        assert d["length"] == src[0]["length"]
        assert abs(d["velocity"] - src[0]["velocity"]) < 1e-9


def test_no_keep_first_beat_allows_downbeat_chop():
    src = make_break(16)
    # with protection off + full roll, the downbeat slot is subdivided
    out = _chop(src, keep_first_beat=False, intensity=100, roll_chance=100,
                stutter_chance=0, reverse_chance=0, permute_amount=0, seed=5)
    downbeat = [n for n in out if n["slot"] == 0]
    assert len(downbeat) > 1


# --------------------------------------------------------------------------- #
# PPQ independence
# --------------------------------------------------------------------------- #

def test_ppq_independence_span_and_grid():
    for ppq in (96, 192, 384, 960):
        src = make_break(16, ppq=ppq)
        orig_end = span_end(src)
        out = _chop(src, ppq=ppq, seed=11, keep_first_beat=False)
        assert span_end(out) == orig_end, ppq
        assert all(n["time"] >= 0 and n["length"] >= 1 for n in out)


# --------------------------------------------------------------------------- #
# Reverse emulation (no real reverse flag -> double-time repeat)
# --------------------------------------------------------------------------- #

def test_reverse_emulated_as_double_time():
    src = make_break(8)
    out = _chop(src, roll_chance=0, stutter_chance=0, reverse_chance=100,
                permute_amount=0, keep_first_beat=False, seed=4)
    rev = [n for n in out if n["role"] == "reverse_dt"]
    assert rev, "reverse should be emulated as reverse_dt when no API flag"
    assert all(not n["reversed"] for n in out)   # no real reverse flag set
    # two retriggers per slot, tiling the slice
    by_slot = {}
    for n in out:
        by_slot.setdefault(n["slot"], []).append(n)
    for i, slot_notes in by_slot.items():
        slot_notes.sort(key=lambda n: n["time"])
        assert len(slot_notes) == 2
        assert slot_notes[0]["time"] == src[i]["time"]
        end = slot_notes[-1]["time"] + slot_notes[-1]["length"]
        assert end == src[i]["time"] + src[i]["length"]


def test_reverse_real_flag_sets_reversed():
    src = make_break(8)
    out = BC.chop_notes(
        src, 96, intensity=100, permute_amount=0, roll_chance=0,
        stutter_chance=0, reverse_chance=100, keep_first_beat=False,
        humanize_vel=0, seed=4, reverse_available=True,
    )
    rev = [n for n in out if n["role"] == "reverse"]
    assert rev
    assert all(n["reversed"] for n in rev)
    # one note per slot, full span
    assert len(out) == len(src)


# --------------------------------------------------------------------------- #
# Permutation stays inside the selection
# --------------------------------------------------------------------------- #

def test_permutation_is_a_rearrangement_within_selection():
    src = make_break(16)
    src_pitches = sorted(n["number"] for n in src)
    # pure permute: no roll/stutter/reverse, no protection, full permute
    out = _chop(src, permute_amount=100, roll_chance=0, stutter_chance=0,
                reverse_chance=0, keep_first_beat=False, intensity=100, seed=8)
    # one note per slot, and the multiset of pitches is unchanged (just reordered)
    assert len(out) == len(src)
    assert sorted(n["number"] for n in out) == src_pitches
    # at least some slot got a different pitch than its original position
    moved = sum(1 for n in out if n["number"] != src[n["slot"]]["number"])
    assert moved > 0


# --------------------------------------------------------------------------- #
# (1) apply() glue: only SELECTED notes altered, unselected byte-for-byte
# --------------------------------------------------------------------------- #

def _seed_score(mod, selected_flags, ppq=96):
    """Populate the mock score with a break; mark selection per `selected_flags`."""
    score = sys.modules["flpianoroll"].score
    Note = sys.modules["flpianoroll"].Note
    src = make_break(len(selected_flags), ppq=ppq)
    for nd, sel in zip(src, selected_flags):
        nt = Note()
        nt.number = nd["number"]
        nt.time = nd["time"]
        nt.length = nd["length"]
        nt.velocity = nd["velocity"]
        nt.selected = sel
        score.addNote(nt)
    return score, src


def _tuples(score, pred=lambda n: True):
    out = []
    for i in range(score.noteCount):
        n = score.getNote(i)
        if pred(n):
            out.append((n.number, n.time, n.length, round(n.velocity, 6)))
    return out


def test_apply_only_touches_selected():
    mod = load_module(ppq=96)
    # slices 4..11 selected, the rest untouched
    flags = [False] * 4 + [True] * 8 + [False] * 4
    score, src = _seed_score(mod, flags)
    unselected_before = _tuples(score, lambda n: not n.selected)

    form = mod.createDialog()
    mod.apply(form)

    # every originally-unselected note is still present, byte-for-byte
    after = _tuples(score)
    from collections import Counter
    ca, cb = Counter(after), Counter(unselected_before)
    for tup, cnt in cb.items():
        assert ca[tup] >= cnt, ("unselected note altered/removed", tup)
    # the score changed (chop produced output) and unselected count is intact
    assert score.noteCount >= len(unselected_before)


def test_apply_no_selection_falls_back_to_whole_score():
    mod = load_module(ppq=96)
    flags = [False] * 16          # nothing selected
    score, src = _seed_score(mod, flags)
    orig_end = span_end(src)
    form = mod.createDialog()
    mod.apply(form)
    assert score.noteCount > 0
    # whole score was chopped, span preserved
    ends = [score.getNote(i).time + score.getNote(i).length
            for i in range(score.noteCount)]
    assert max(ends) == orig_end


def test_apply_deletes_and_rewrites_selected_span():
    mod = load_module(ppq=96)
    flags = [True] * 16
    score, src = _seed_score(mod, flags)
    orig_end = span_end(src)
    form = mod.createDialog()
    mod.apply(form)
    ends = [score.getNote(i).time + score.getNote(i).length
            for i in range(score.noteCount)]
    assert max(ends) == orig_end
    for i in range(score.noteCount):
        n = score.getNote(i)
        assert n.time >= 0 and n.length >= 1
        assert 0.0 <= n.velocity <= 1.0


# --------------------------------------------------------------------------- #
# Empty input
# --------------------------------------------------------------------------- #

def test_empty_input_returns_empty():
    assert BC.chop_notes([], 96) == []


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
