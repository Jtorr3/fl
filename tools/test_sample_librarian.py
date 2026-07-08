# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "librosa>=0.10.1",
#   "soundfile>=0.12",
#   "scipy>=1.11",
# ]
# ///
"""Offline test gate for sample_librarian.py (Qeynos W6).

Synthesizes fixture WAVs in a temp dir (a 120-BPM click loop, a C-major tonal
sample, short one-shots) and exercises the classifier, analysis (BPM within
tolerance, key detected, one-shot -> no BPM), and the full plan/apply/undo
round-trip incl. collision suffixing and idempotency. No network, no FL.

Run:  uv run --python 3.12 tools\\test_sample_librarian.py
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import sample_librarian as sl  # noqa: E402

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


SR = 22050


def _write(path: Path, y: np.ndarray) -> None:
    import soundfile as sf
    sf.write(str(path), y.astype(np.float32), SR)


def make_click_loop(path: Path, bpm: float = 120.0, dur: float = 4.0) -> None:
    rng = np.random.default_rng(0)
    y = np.zeros(int(SR * dur), dtype=np.float32)
    n = int(0.02 * SR)
    env = np.exp(-np.linspace(0, 8, n))
    interval = 60.0 / bpm
    t = 0.0
    while t < dur:
        i = int(t * SR)
        burst = (env * rng.standard_normal(n) * 0.8).astype(np.float32)
        end = min(i + n, len(y))
        y[i:end] += burst[: end - i]
        t += interval
    _write(path, y)


def make_c_major(path: Path, dur: float = 3.0) -> None:
    t = np.linspace(0, dur, int(SR * dur), endpoint=False)
    # C-major triad, tonic-weighted, plus a C octave to anchor the root.
    comps = [(261.63, 1.0), (523.25, 0.5), (329.63, 0.6), (392.0, 0.6)]
    y = sum(a * np.sin(2 * np.pi * f * t) for f, a in comps)
    y *= np.hanning(len(y)) * 0.3
    _write(path, y.astype(np.float32))


def make_oneshot(path: Path, dur: float = 0.3) -> None:
    rng = np.random.default_rng(1)
    n = int(SR * dur)
    env = np.exp(-np.linspace(0, 10, n))
    y = (env * rng.standard_normal(n) * 0.6).astype(np.float32)
    _write(path, y)


# ---------------------------------------------------------------------------
# 1. Classifier + category resolution
# ---------------------------------------------------------------------------
def test_classify() -> None:
    print("[classify]")
    cases = {
        "Kick_01.wav": "kick",
        "deep bassdrum.wav": "kick",   # compound -> kick, not bass
        "Sub Bass C.wav": "bass",
        "Snare_top.wav": "snare",
        "clap_909.wav": "clap",
        "closed_hat.wav": "hat",
        "shaker_perc.wav": "perc",
        "vocal_chop.wav": "vocal",
        "riser_fx.wav": "fx",
        "warm_pad.wav": "synth",
        "drum_loop_128.wav": "loop",
    }
    for name, want in cases.items():
        cat = sl.classify_filename(name)
        check(f"classify {name!r} -> {want}",
              cat is not None and cat.folder == want,
              f"got {cat.folder if cat else None}")
    # duration fallback: unmatched long -> loop, unmatched short -> other
    check("unmatched long -> loop",
          sl.resolve_category("mystery.wav", 3.0).folder == "loop")
    check("unmatched short -> other",
          sl.resolve_category("mystery.wav", 0.4).folder == "other")


# ---------------------------------------------------------------------------
# 2. Token stripping + name building (idempotent rename)
# ---------------------------------------------------------------------------
def test_naming() -> None:
    print("[naming]")
    check("strip key+bpm tokens",
          sl.strip_tokens("Am_128_vocalchop") == "vocalchop",
          sl.strip_tokens("Am_128_vocalchop"))
    check("build name with key+bpm",
          sl.build_new_name("vocalchop", ".wav", "Am", 128) == "Am_128_vocalchop.wav")
    check("build name bpm only",
          sl.build_new_name("break", ".wav", None, 174) == "174_break.wav")
    check("build name no tokens",
          sl.build_new_name("kick", ".wav", None, None) == "kick.wav")
    # idempotent: re-applying tokens to an already-tokened stem is stable
    once = sl.build_new_name("kickloop", ".wav", "C", 120)
    twice = sl.build_new_name(Path(once).stem, ".wav", "C", 120)
    check("re-tokenize is stable", once == twice, f"{once} != {twice}")


# ---------------------------------------------------------------------------
# 3. Analysis on fixtures (BPM tolerance, key detect, one-shot no BPM)
# ---------------------------------------------------------------------------
def test_analysis(tmp: Path) -> None:
    print("[analysis]")
    loop = tmp / "perc_loop.wav"
    tonal = tmp / "synth_chord.wav"
    shot = tmp / "Kick_01.wav"
    make_click_loop(loop, 120.0)
    make_c_major(tonal)
    make_oneshot(shot)

    fl = sl.analyze(loop)
    ok_bpm = fl.bpm is not None and (
        abs(fl.bpm - 120) <= 6 or abs(fl.bpm - 60) <= 3 or abs(fl.bpm - 240) <= 12
    )
    check("click loop BPM ~120 (or half/double)", ok_bpm, f"got {fl.bpm}")

    ft = sl.analyze(tonal)
    check("tonal sample category=synth (tonal)", ft.category == "synth" and ft.tonal)
    check("tonal sample key detected", ft.key is not None, f"got {ft.key}")
    check("tonal key is C or its relative Am",
          ft.key in ("C", "Am"), f"got {ft.key}")

    fs = sl.analyze(shot)
    check("one-shot category=kick", fs.category == "kick")
    check("one-shot (<1.5s) has NO bpm", fs.bpm is None, f"got {fs.bpm}")
    check("one-shot (non-tonal) has NO key", fs.key is None, f"got {fs.key}")


# ---------------------------------------------------------------------------
# 4. plan/apply/undo round-trip + collision + idempotency
# ---------------------------------------------------------------------------
def test_roundtrip(tmp: Path) -> None:
    print("[round-trip]")
    root = tmp / "lib"
    (root / "a").mkdir(parents=True)
    (root / "b").mkdir(parents=True)
    # collision: two identical-named kick one-shots in different subfolders
    make_oneshot(root / "a" / "kick.wav")
    make_oneshot(root / "b" / "kick.wav")
    make_c_major(root / "synth_chord.wav")

    moves, skipped = sl.plan_moves(root, root, recursive=True)
    check("planned 3 moves", len(moves) == 3, f"got {len(moves)}: {[m.dst.name for m in moves]}")

    kick_dsts = sorted(m.dst.name for m in moves if m.features.category == "kick")
    check("collision suffixed (kick.wav + kick_1.wav)",
          kick_dsts == ["kick.wav", "kick_1.wav"], str(kick_dsts))

    originals = {m.src for m in moves}
    report = sl.apply_moves(moves, root)
    check("apply moved all 3", len(report.moved) == 3 and not report.failed)
    check("manifest written", report.manifest_path is not None and report.manifest_path.exists())
    check("kick folder has 2 files",
          len(list((root / "kick").glob("*.wav"))) == 2)
    synths = list((root / "synth").glob("*.wav"))
    check("synth sorted + key-prefixed",
          len(synths) == 1 and sl._KEY_TOKEN.match(synths[0].name) is not None,
          str([p.name for p in synths]))
    check("originals gone", all(not p.exists() for p in originals))

    # idempotency: re-plan the sorted tree -> zero moves
    moves2, _ = sl.plan_moves(root, root, recursive=True)
    check("idempotent re-plan -> 0 moves", len(moves2) == 0, f"got {len(moves2)}")

    # undo -> originals restored, category folders emptied of those files
    ur = sl.undo(report.manifest_path)
    check("undo restored 3", ur.restored == 3 and not ur.failed, str(ur.failed))
    check("undo brought back a/kick.wav", (root / "a" / "kick.wav").exists())
    check("undo brought back b/kick.wav", (root / "b" / "kick.wav").exists())
    check("undo brought back synth_chord.wav", (root / "synth_chord.wav").exists())


def main() -> int:
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        test_classify()
        test_naming()
        test_analysis(tmp)
        test_roundtrip(tmp)
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
