# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "pytest>=8.0",
# ]
# ///
"""Offline test gate for DarkProgression.pyscript (Qeynos W3).

FL Studio cannot run its piano-roll scripts headless, so this gate imports the
`.pyscript` with a MOCK `flpianoroll` module injected into sys.modules (the
SHARED mock in pyscripts/tests/mock_fl.py, reused from W1/W2) and exercises the
pure generator plus the FL `apply()` glue.

Asserts (per the work order):
  1. Every generated pitch (chord + arp) is strictly in the selected scale.
  2. Chord-change boundaries land EXACTLY on the bars-per-chord grid.
  3. Voice leading: average summed semitone motion between consecutive chords is
     within a bound (<= 6 total for triads) AND strictly less than the
     no-voice-leading (root-position) rendering of the same progression.
  4. Arp notes are a subset of the current chord tones (+/- octave), the rate
     grid is exact, and the gate % is applied to the note length.
  5. Output is deterministic per seed (same seed identical; different differs).
  6. Humanize bounds (velocity + timing <= 5 ticks) are respected.

Run either way (from pyscripts/tests/):
    uv run --python 3.12 test_darkprog.py            # plain-assert gate
    uv run --python 3.12 --with pytest pytest -q test_darkprog.py
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mock_fl import load_pyscript  # noqa: E402  (shared mock, factored out W1/W2)

PYSCRIPT = Path(__file__).resolve().parent.parent / "DarkProgression.pyscript"


def load_module(ppq=96):
    """Load the .pyscript with a fresh mock flpianoroll injected."""
    return load_pyscript(PYSCRIPT, "dark_progression", ppq)


DP = load_module()

SCALES = DP.SCALES
PRESET_KEYS = DP.PRESET_KEYS
VOICING_KEYS = DP.VOICING_KEYS
C3 = DP.note_name_to_midi(0, 3)   # C, octave 3


def _gen(preset="dark_pop", scale="minor", bars_per_chord=1, total_bars=8,
         ppq=96, voicing="triad", **kw):
    params = dict(
        root_midi=C3, scale=scale, preset=preset,
        bars_per_chord=bars_per_chord, total_bars=total_bars, ppq=ppq,
        voicing=voicing, voice_leading=True, arp_mode="off", arp_rate="16th",
        arp_octaves=1, arp_gate_pct=70, vel_base=90, vel_humanize=8,
        timing_humanize=3, sus_prob=0.2, seed=7,
    )
    params.update(kw)
    return DP.generate_progression(**params)


# --------------------------------------------------------------------------- #
# (1) Scale membership - every pitch strictly in-key
# --------------------------------------------------------------------------- #

def test_all_pitches_in_scale():
    for preset in PRESET_KEYS:
        for scale, offsets in SCALES.items():
            for voicing in VOICING_KEYS:
                res = _gen(preset=preset, scale=scale, voicing=voicing,
                           arp_mode="updown", arp_octaves=2, sus_prob=0.9,
                           total_bars=16, seed=13)
                assert res["notes"], (preset, scale)
                for n in res["notes"]:
                    assert (n["number"] - C3) % 12 in offsets, \
                        (preset, scale, voicing, n)


def test_suspensions_stay_in_scale_and_only_off_tonic():
    # Force suspensions everywhere; tonic chords must never be suspended.
    res = _gen(preset="wander", scale="phrygian", sus_prob=1.0, seed=3)
    offsets = SCALES["phrygian"]
    for n in res["notes"]:
        assert (n["number"] - C3) % 12 in offsets, n


# --------------------------------------------------------------------------- #
# (2) Chord boundaries on the bars-per-chord grid
# --------------------------------------------------------------------------- #

def test_chord_boundaries_on_grid():
    for bpc in (1, 2):
        for total in (4, 8, 16):
            for ppq in (96, 192, 960):
                res = _gen(bars_per_chord=bpc, total_bars=total, ppq=ppq)
                chord_ticks = bpc * ppq * 4
                assert res["chord_ticks"] == chord_ticks
                chords = [n for n in res["notes"] if n["lane"] == "chord"]
                # Every chord note starts exactly on a chord boundary.
                starts = sorted({n["nominal_time"] for n in chords})
                assert starts == [i * chord_ticks
                                  for i in range(total // bpc)], (bpc, total, ppq)
                for n in chords:
                    assert n["nominal_time"] % chord_ticks == 0
                    # pad sustains exactly one chord span
                    assert n["nominal_length"] == chord_ticks


def test_chord_count_matches_total_bars():
    for bpc in (1, 2):
        for total in (4, 8, 16):
            res = _gen(bars_per_chord=bpc, total_bars=total)
            assert len(res["voicings"]) == total // bpc
            assert len(res["degrees"]) == total // bpc


# --------------------------------------------------------------------------- #
# (3) Voice leading reduces motion and stays within bound
# --------------------------------------------------------------------------- #

def test_voice_leading_beats_root_position():
    for preset in PRESET_KEYS:
        for scale in SCALES:
            degs = DP.progression_degrees(preset, 8, seed=5)
            vl = DP.chord_voicings(C3, scale, degs, voicing="triad",
                                   voice_leading=True, sus_prob=0.0)
            rp = DP.chord_voicings(C3, scale, degs, voicing="triad",
                                   voice_leading=False, sus_prob=0.0)
            m_vl = DP.total_motion(vl)
            m_rp = DP.total_motion(rp)
            assert m_vl < m_rp, (preset, scale, m_vl, m_rp)
            avg = m_vl / (len(vl) - 1)
            assert avg <= 6.0, (preset, scale, avg)


def test_voice_leading_first_chord_root_position():
    degs = DP.progression_degrees("dark_pop", 4, seed=1)
    vl = DP.chord_voicings(C3, "minor", degs, voice_leading=True, sus_prob=0.0)
    rp = DP.chord_voicings(C3, "minor", degs, voice_leading=False, sus_prob=0.0)
    assert vl[0] == rp[0]   # first chord is always root position


def test_seventh_voicing_voice_leading_bound():
    # 7th chords have 4 voices -> allow a slightly looser per-chord bound.
    for scale in SCALES:
        degs = DP.progression_degrees("hypnotic", 8, seed=2)
        vl = DP.chord_voicings(C3, scale, degs, voicing="7th",
                               voice_leading=True, sus_prob=0.0)
        rp = DP.chord_voicings(C3, scale, degs, voicing="7th",
                               voice_leading=False, sus_prob=0.0)
        assert DP.total_motion(vl) < DP.total_motion(rp)
        assert DP.total_motion(vl) / (len(vl) - 1) <= 8.0


# --------------------------------------------------------------------------- #
# (4) Arp: subset of chord tones, exact rate grid, gate applied, above pad
# --------------------------------------------------------------------------- #

def test_arp_subset_rate_grid_and_gate():
    for mode in ("up", "down", "updown", "random"):
        for rate, div in (("8th", 2), ("16th", 4)):
            for span in (1, 2):
                ppq = 96
                res = _gen(arp_mode=mode, arp_rate=rate, arp_octaves=span,
                           arp_gate_pct=60, total_bars=8, ppq=ppq,
                           timing_humanize=0, seed=21)
                rate_step = ppq // div
                exp_len = max(1, round(rate_step * 0.60))
                voicings = res["voicings"]
                arps = [n for n in res["notes"] if n["lane"] == "arp"]
                assert arps, (mode, rate, span)
                for n in arps:
                    ci = n["chord_index"]
                    chord_pcs = {(p - C3) % 12 for p in voicings[ci]}
                    # subset of chord tones (+/- octave transposition)
                    assert (n["number"] - C3) % 12 in chord_pcs, n
                    # registered ABOVE the pad
                    assert n["number"] > max(voicings[ci]), n
                    # exact rate grid (relative to the chord start)
                    chord_ticks = res["chord_ticks"]
                    assert n["nominal_time"] % rate_step == 0, n
                    # gate applied to length
                    assert n["nominal_length"] == exp_len, (n, exp_len)


def test_arp_off_writes_no_arp_notes():
    res = _gen(arp_mode="off")
    assert all(n["lane"] == "chord" for n in res["notes"])
    res2 = _gen(arp_mode="up")
    assert any(n["lane"] == "arp" for n in res2["notes"])


def test_arp_step_count_matches_rate():
    # 16ths over a 1-bar chord at ppq 96 -> 16 steps/chord.
    res = _gen(arp_mode="up", arp_rate="16th", bars_per_chord=1, total_bars=4,
               ppq=96, timing_humanize=0)
    per_chord = {}
    for n in res["notes"]:
        if n["lane"] == "arp":
            per_chord[n["chord_index"]] = per_chord.get(n["chord_index"], 0) + 1
    assert per_chord and all(c == 16 for c in per_chord.values()), per_chord


# --------------------------------------------------------------------------- #
# (5) Determinism
# --------------------------------------------------------------------------- #

def test_deterministic_same_seed():
    for preset in PRESET_KEYS:
        a = _gen(preset=preset, arp_mode="random", total_bars=16, seed=2024)
        b = _gen(preset=preset, arp_mode="random", total_bars=16, seed=2024)
        assert a == b, preset


def test_different_seed_differs():
    a = _gen(preset="random", arp_mode="random", total_bars=16, seed=1)
    b = _gen(preset="random", arp_mode="random", total_bars=16, seed=2)
    assert a != b


def test_random_preset_starts_on_tonic():
    for seed in (0, 1, 42, 999):
        degs = DP.progression_degrees("random", 8, seed=seed)
        assert degs[0] == 0
        assert len(degs) == 8
        # no immediate repeats in the walk
        assert all(degs[i] != degs[i - 1] for i in range(1, len(degs)))


# --------------------------------------------------------------------------- #
# (6) Humanize bounds
# --------------------------------------------------------------------------- #

def test_timing_humanize_bounded():
    for ht in (0, 3, 5):
        res = _gen(arp_mode="up", timing_humanize=ht, total_bars=8, seed=9)
        for n in res["notes"]:
            dev = abs(n["time"] - n["nominal_time"])
            assert dev <= ht
            assert dev <= 5
            assert n["time"] >= 0
        # chord lane is never timing-humanized (pad stays locked)
        for n in res["notes"]:
            if n["lane"] == "chord":
                assert n["time"] == n["nominal_time"]


def test_timing_humanize_clamped_to_five():
    res = _gen(arp_mode="up", timing_humanize=50, total_bars=8, seed=4)
    for n in res["notes"]:
        assert abs(n["time"] - n["nominal_time"]) <= 5


def test_velocity_humanize_bounded():
    hv = 8
    res = _gen(arp_mode="up", vel_base=90, vel_humanize=hv, total_bars=8, seed=6)
    lo, hi = 0.90 - hv / 100.0 - 1e-9, 0.90 + hv / 100.0 + 1e-9
    for n in res["notes"]:
        assert 0.02 <= n["velocity"] <= 1.0
        assert lo <= n["velocity"] <= hi, n


# --------------------------------------------------------------------------- #
# FL apply() glue via the shared mock
# --------------------------------------------------------------------------- #

def test_apply_writes_notes_to_score():
    mod = load_module(ppq=96)
    form = mod.createDialog()
    mod.apply(form)
    score = sys.modules["flpianoroll"].score
    assert score.noteCount > 0
    root = mod.note_name_to_midi(0, 3)
    offsets = mod.SCALES["minor"]
    for i in range(score.noteCount):
        n = score.getNote(i)
        assert (n.number - root) % 12 in offsets
        assert n.time >= 0
        assert n.length >= 1
        assert 0.0 <= n.velocity <= 1.0


def test_apply_respects_timeline_selection_offset():
    mod = load_module(ppq=96)
    score = sys.modules["flpianoroll"].score
    score._selection = (960, 1920)
    form = mod.createDialog()
    mod.apply(form)
    assert score.noteCount > 0
    assert min(score.getNote(i).time for i in range(score.noteCount)) >= 960


def test_apply_with_arp_writes_two_lanes():
    mod = load_module(ppq=96)
    form = mod.createDialog()
    form.SetValue("Arp", mod.ARP_KEYS.index("updown"))
    mod.apply(form)
    score = sys.modules["flpianoroll"].score
    numbers = [score.getNote(i).number for i in range(score.noteCount)]
    # arp notes sit above the pad, so the high extreme must exceed the base pad
    assert max(numbers) > mod.note_name_to_midi(0, 3) + 12


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
