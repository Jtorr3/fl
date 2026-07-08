# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "librosa>=0.10.1",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pytest>=8.0",
# ]
# ///
"""Offline test gate for voxrip -- runs WITHOUT demucs weights or any network.

Covers the two stages that ship in the CI path:
  * ANALYSIS  -- BPM + key detection on IN-TEST synthesised fixtures (a click
    track at a known BPM and a harmonic pad at a known key). BPM within +/-2%
    (or a half/double-time alternate); key detected exactly.
  * CONFORM MATH -- key parsing, minimal transposition (incl. the relative-mode
    rule), and stretch-ratio calc, plus the rubberband/demucs command builders.
    The rubberband invocation is MOCKED (no binary, no network).

Separation (demucs) is NOT exercised here -- it is smoke-tested live only when
the model download succeeds; otherwise see CHECKPOINTS.md.

Run either way:
    uv run --python 3.12 test_voxrip.py     # plain-assert runner (the gate)
    uv run --python 3.12 pytest test_voxrip.py
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))

import voxrip  # noqa: E402
from voxrip import Key  # noqa: E402


# --------------------------------------------------------------------------- #
# Fixture synthesis (no external audio files, ever)
# --------------------------------------------------------------------------- #

SR = 22050


def synth_click_track(bpm: float, seconds: float = 8.0, sr: int = SR) -> np.ndarray:
    """A percussive click every beat at `bpm` -- a clean tempo fixture."""
    n = int(seconds * sr)
    y = np.zeros(n, dtype=np.float32)
    period = int(round(sr * 60.0 / bpm))
    click = np.exp(-np.linspace(0, 12, int(0.02 * sr))).astype(np.float32)
    click *= np.sin(2 * np.pi * 2000 * np.arange(click.size) / sr)
    for start in range(0, n - click.size, period):
        y[start:start + click.size] += click
    y += 1e-4 * np.random.default_rng(0).standard_normal(n).astype(np.float32)
    return y


def _tone(pc: int, sr: int, dur: float, amp: float, octave: int = 4) -> np.ndarray:
    freq = 440.0 * 2 ** ((pc - 9) / 12.0) * 2 ** (octave - 4)
    t = np.arange(int(dur * sr)) / sr
    # a couple of harmonics so chroma_cqt has clear energy
    sig = (np.sin(2 * np.pi * freq * t)
           + 0.4 * np.sin(2 * np.pi * 2 * freq * t)
           + 0.2 * np.sin(2 * np.pi * 3 * freq * t))
    env = np.minimum(1.0, np.minimum(t * 40, (dur - t) * 40))
    return (amp * env * sig).astype(np.float32)


def synth_key_pad(root_pc: int, mode: str, seconds: float = 8.0, sr: int = SR) -> np.ndarray:
    """A harmonic pad tonicised on `root_pc` in `mode`, with a loud tonic drone
    so Krumhansl-Schmuckler resolves the relative-key ambiguity correctly."""
    if mode == "minor":
        scale = [0, 2, 3, 5, 7, 8, 10]  # natural minor
        triad = [0, 3, 7]
    else:
        scale = [0, 2, 4, 5, 7, 9, 11]  # major
        triad = [0, 4, 7]
    n = int(seconds * sr)
    y = np.zeros(n, dtype=np.float32)
    # Loud sustained tonic drone (this is what tonicises the key).
    drone = _tone(root_pc, sr, seconds, amp=0.9, octave=3)
    y[: drone.size] += drone
    # Arpeggiate the tonic triad + scale over the duration.
    seq = [root_pc + s for s in triad] + [root_pc + s for s in scale]
    step = seconds / len(seq)
    for i, pc in enumerate(seq):
        note = _tone(pc % 12, sr, step * 1.1, amp=0.4, octave=4)
        start = int(i * step * sr)
        end = min(n, start + note.size)
        y[start:end] += note[: end - start]
    y *= 0.5 / (np.max(np.abs(y)) + 1e-9)
    return y


# --------------------------------------------------------------------------- #
# 1. Key parsing
# --------------------------------------------------------------------------- #

def test_parse_key():
    assert voxrip.parse_key("Am") == Key(9, "minor")
    assert voxrip.parse_key("A") == Key(9, "major")
    assert voxrip.parse_key("C") == Key(0, "major")
    assert voxrip.parse_key("F#m") == Key(6, "minor")
    assert voxrip.parse_key("Bb") == Key(10, "major")
    assert voxrip.parse_key("Bbmaj") == Key(10, "major")
    assert voxrip.parse_key("G minor") == Key(7, "minor")
    assert voxrip.parse_key("c#min") == Key(1, "minor")
    assert voxrip.parse_key("Db major") == Key(1, "major")


def test_parse_key_rejects_garbage():
    for bad in ["", "   ", "H", "Xm"]:
        try:
            voxrip.parse_key(bad)
        except ValueError:
            continue
        raise AssertionError(f"parse_key({bad!r}) should have raised")


# --------------------------------------------------------------------------- #
# 2. Minimal transposition (incl. relative-mode rule)
# --------------------------------------------------------------------------- #

def test_transposition_same_mode():
    # Cm -> Am : both minor. Roots C(0) -> A(9): -3 (or +9); pick -3.
    tr = voxrip.minimal_transposition(Key(0, "minor"), Key(9, "minor"))
    assert tr.semitones == -3, tr.semitones
    assert tr.alternative == 9, tr.alternative
    assert tr.relative_reinterpretation is None


def test_transposition_relative_zero():
    # A minor material into C major: relative keys -> 0 semitones needed.
    tr = voxrip.minimal_transposition(Key(9, "minor"), Key(0, "major"))
    assert tr.semitones == 0, tr.semitones
    assert tr.relative_reinterpretation is not None


def test_transposition_cross_mode():
    # C minor into C major: reinterpret Cm as its relative Eb major (root+3=3),
    # distance to C(0) = -3. A plain root match (0) would leave a clash.
    tr = voxrip.minimal_transposition(Key(0, "minor"), Key(0, "major"))
    assert tr.semitones == -3, tr.semitones
    assert tr.relative_reinterpretation is not None


def test_transposition_major_to_minor():
    # C major into A minor: relative minor of C is A(9); distance A->A = 0.
    tr = voxrip.minimal_transposition(Key(0, "major"), Key(9, "minor"))
    assert tr.semitones == 0, tr.semitones


def test_transposition_minimal_magnitude():
    # Every result must be the minimal-|st| option (<= its wrap partner).
    for s_root in range(12):
        for t_root in range(12):
            for sm in ("major", "minor"):
                for tm in ("major", "minor"):
                    tr = voxrip.minimal_transposition(Key(s_root, sm), Key(t_root, tm))
                    assert abs(tr.semitones) <= abs(tr.alternative)
                    assert -6 <= tr.semitones <= 6
                    # chosen and alternative differ by exactly one octave
                    assert abs(abs(tr.semitones - tr.alternative)) == 12


# --------------------------------------------------------------------------- #
# 3. Stretch-ratio math
# --------------------------------------------------------------------------- #

def test_time_ratio():
    # 100 -> 128 BPM (faster): output shorter, ratio < 1.
    assert abs(voxrip.time_ratio(100, 128) - 100 / 128) < 1e-9
    # librosa rate is the inverse (target/source).
    assert abs(voxrip.stretch_rate(100, 128) - 128 / 100) < 1e-9
    # round-trip
    assert abs(voxrip.time_ratio(120, 120) - 1.0) < 1e-12


def test_time_ratio_rejects_nonpositive():
    for bad in [(0, 120), (120, 0), (-1, 120)]:
        try:
            voxrip.time_ratio(*bad)
        except ValueError:
            continue
        raise AssertionError(f"time_ratio{bad} should have raised")


# --------------------------------------------------------------------------- #
# 4. Krumhansl key scoring from a hand-built chroma (no audio)
# --------------------------------------------------------------------------- #

def test_key_from_chroma_c_major():
    chroma = np.zeros(12)
    for pc, w in ((0, 1.0), (4, 0.7), (7, 0.8), (2, 0.4), (5, 0.4), (9, 0.4), (11, 0.3)):
        chroma[pc] = w
    res = voxrip.key_from_chroma(chroma)
    assert res.key == Key(0, "major"), str(res.key)


def test_key_from_chroma_a_minor():
    chroma = np.zeros(12)
    # A tonic dominant -> A minor, not its relative C major.
    for pc, w in ((9, 1.0), (0, 0.6), (4, 0.7), (2, 0.4), (5, 0.35), (7, 0.4), (11, 0.3)):
        chroma[pc] = w
    res = voxrip.key_from_chroma(chroma)
    assert res.key == Key(9, "minor"), str(res.key)


# --------------------------------------------------------------------------- #
# 5. Analysis on synthesised audio fixtures
# --------------------------------------------------------------------------- #

def _bpm_ok(detected: voxrip.TempoResult, expected: float) -> bool:
    for cand in (detected.bpm, detected.half_time, detected.double_time):
        if abs(cand - expected) <= 0.02 * expected:
            return True
    # a detected alternate is acceptable too
    return any(abs(a - expected) <= 0.02 * expected for a in detected.alternates)


def test_detect_bpm_120():
    y = synth_click_track(120.0)
    res = voxrip.detect_bpm(y, SR)
    assert _bpm_ok(res, 120.0), f"got {res.bpm} (alts {res.alternates})"


def test_detect_bpm_90():
    y = synth_click_track(90.0)
    res = voxrip.detect_bpm(y, SR)
    assert _bpm_ok(res, 90.0), f"got {res.bpm} (alts {res.alternates})"


def test_detect_key_a_minor():
    y = synth_key_pad(9, "minor")
    res = voxrip.detect_key(y, SR)
    assert res.key == Key(9, "minor"), f"got {res.key}"


def test_detect_key_c_major():
    y = synth_key_pad(0, "major")
    res = voxrip.detect_key(y, SR)
    assert res.key == Key(0, "major"), f"got {res.key}"


# --------------------------------------------------------------------------- #
# 6. Command builders + mocked rubberband invocation
# --------------------------------------------------------------------------- #

def test_rubberband_command():
    cmd = voxrip.rubberband_command(
        Path("rb.exe"), Path("in.wav"), Path("out.wav"), t_ratio=0.78125, semitones=-3
    )
    assert "-F" in cmd  # formant preservation
    assert "--time" in cmd and "--pitch" in cmd
    ti = cmd.index("--time")
    assert abs(float(cmd[ti + 1]) - 0.78125) < 1e-6
    pi = cmd.index("--pitch")
    assert abs(float(cmd[pi + 1]) - (-3)) < 1e-6
    assert cmd[-2:] == ["in.wav", "out.wav"]


def test_demucs_command():
    cmd = voxrip.demucs_command(Path("song.mp3"), Path("outdir"), uv="uv")
    assert cmd[:2] == ["uv", "run"]
    assert "--python" in cmd and "3.12" in cmd
    assert cmd[-2:] == ["song.mp3", "outdir"]
    # runs the CPU-torch separation helper (not torch in voxrip's own env)
    assert any(str(c).endswith("voxrip_separate.py") for c in cmd)


def test_separate_helper_pins_cpu_torch():
    text = voxrip.separate_helper().read_text(encoding="utf-8")
    assert "download.pytorch.org/whl/cpu" in text
    assert "--two-stems" in text and "vocals" in text
    assert "htdemucs" in text


def test_conform_rubberband_mocked(monkeypatch):
    calls = {}

    def fake_run(cmd, check=False, timeout=None, **kw):
        calls["cmd"] = cmd
        calls["check"] = check
        Path(cmd[-1]).write_bytes(b"RIFF")  # pretend it produced output

    monkeypatch.setattr(voxrip.subprocess, "run", fake_run)
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "out.wav"
        voxrip.conform_rubberband(
            Path("rb.exe"), Path("in.wav"), out, t_ratio=0.9, semitones=2
        )
    assert calls["check"] is True
    assert "-F" in calls["cmd"]
    assert calls["cmd"][0] == "rb.exe"


# --------------------------------------------------------------------------- #
# Plain-assert runner (the gate, no pytest required)
# --------------------------------------------------------------------------- #

def _run_all() -> int:
    import types

    class _MP:
        """Tiny monkeypatch stand-in for the plain runner."""

        def __init__(self):
            self._undo = []

        def setattr(self, obj, name, val):
            self._undo.append((obj, name, getattr(obj, name)))
            setattr(obj, name, val)

        def undo(self):
            for obj, name, old in reversed(self._undo):
                setattr(obj, name, old)

    tests = [(n, f) for n, f in sorted(globals().items())
             if n.startswith("test_") and isinstance(f, types.FunctionType)]
    passed = failed = 0
    for name, fn in tests:
        mp = _MP()
        try:
            if "monkeypatch" in fn.__code__.co_varnames[: fn.__code__.co_argcount]:
                fn(mp)
            else:
                fn()
            print(f"  PASS  {name}")
            passed += 1
        except Exception as exc:  # noqa: BLE001
            print(f"  FAIL  {name}: {exc}")
            failed += 1
        finally:
            mp.undo()
    print(f"\n{passed} passed, {failed} failed  ({len(tests)} total)")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(_run_all())
