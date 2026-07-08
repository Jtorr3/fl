# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "librosa>=0.10.1",
#   "soundfile>=0.12",
#   "scipy>=1.11",
# ]
# ///
"""sample_librarian.py — analyze, rename and sort a sample library (Qeynos W6).

Scans a directory tree of audio samples and, for each file:
  * detects BPM (librosa onset-strength autocorrelation beat tracker) — only for
    files longer than 1.5 s (one-shots have no meaningful tempo);
  * detects musical key (chromagram vs Krumhansl-Schmuckler major/minor profiles,
    import-copied from the W9 voxrip tool so this file stays self-contained) —
    only for TONAL categories (bass/vocal/synth/pad/loop);
  * classifies into a category folder (kick/snare/clap/hat/perc/bass/vocal/fx/
    loop/synth/other) from filename keywords, refined by a duration heuristic;
  * renames to `{key}_{bpm}_{origname}` (whichever tokens apply) and moves it
    into `<dest>/<category>/`.

DRY-RUN IS THE DEFAULT — nothing is touched until you pass `--apply`. Moves never
overwrite (a name collision gets a `_1`, `_2`… suffix). Every applied run writes
an **undo manifest** (JSON list of moves); `--undo <manifest>` replays it in
reverse to restore the original layout. The rename is idempotent: existing
leading key/bpm tokens are stripped before re-prefixing, so re-running a sorted
library is a no-op.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable

import numpy as np

# --------------------------------------------------------------------------- #
# Music theory — Krumhansl-Schmuckler key finding (import-copied from voxrip.py)
# --------------------------------------------------------------------------- #
_PC_TO_NAME = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]

_KS_MAJOR = np.array(
    [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88]
)
_KS_MINOR = np.array(
    [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17]
)


@dataclass(frozen=True)
class Key:
    root: int  # 0..11, C=0
    mode: str  # "major" | "minor"

    @property
    def short(self) -> str:
        return f"{_PC_TO_NAME[self.root % 12]}{'m' if self.mode == 'minor' else ''}"

    def __str__(self) -> str:
        return f"{_PC_TO_NAME[self.root % 12]} {self.mode}"


@dataclass(frozen=True)
class KeyResult:
    key: Key
    confidence: float


def _pearson(a: np.ndarray, b: np.ndarray) -> float:
    a = a - a.mean()
    b = b - b.mean()
    denom = math.sqrt(float(np.dot(a, a)) * float(np.dot(b, b)))
    if denom == 0:
        return 0.0
    return float(np.dot(a, b) / denom)


def key_from_chroma(chroma_mean: np.ndarray) -> KeyResult:
    v = np.asarray(chroma_mean, dtype=float)
    if v.shape != (12,):
        raise ValueError("chroma_mean must have 12 elements")
    scores: list[tuple[float, Key]] = []
    for mode, profile in (("major", _KS_MAJOR), ("minor", _KS_MINOR)):
        for root in range(12):
            rp = np.roll(profile, root)
            scores.append((_pearson(v, rp), Key(root, mode)))
    scores.sort(key=lambda t: t[0], reverse=True)
    best_score, best_key = scores[0]
    return KeyResult(key=best_key, confidence=round(float(best_score), 3))


# --------------------------------------------------------------------------- #
# Category system — filename keywords + tonal flag
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class Category:
    folder: str
    tonal: bool
    keywords: tuple[str, ...]


# First match wins (compound resolution: bassdrum -> kick, not bass).
CATEGORIES: tuple[Category, ...] = (
    Category("kick", False, (
        "kick", "kicks", "kik", "bd", "bassdrum", "bass drum",
    )),
    Category("snare", False, (
        "snare", "snares", "snr", "rimshot", "rim",
    )),
    Category("clap", False, ("clap", "claps")),
    Category("hat", False, (
        "hat", "hats", "hihat", "hihats", "hi hat", "hh", "ohh", "chh",
        "open hat", "closed hat", "cymbal", "cymbals", "ride", "crash",
    )),
    Category("perc", False, (
        "perc", "percs", "percussion", "tom", "toms", "conga", "bongo",
        "shaker", "shakers", "tambourine", "tamb", "cowbell", "clave",
        "woodblock", "snap", "click", "rimshot",
    )),
    Category("bass", True, (
        "bass", "sub", "808", "reese", "wobble", "bassline",
    )),
    Category("vocal", True, (
        "vox", "vocal", "vocals", "voc", "acapella", "acappella", "acap",
        "adlib", "adlibs", "verse", "chorus", "choir", "harmony",
    )),
    Category("fx", False, (
        "fx", "sfx", "riser", "uplifter", "downlifter", "sweep", "impact",
        "whoosh", "foley", "noise", "transition", "boom", "drop", "glitch",
        "riser",
    )),
    Category("synth", True, (
        "synth", "lead", "pluck", "arp", "stab", "pad", "atmos", "ambient",
        "drone", "texture", "string", "strings", "keys", "piano", "bell",
        "chord", "melody", "saw",
    )),
    Category("loop", True, (
        "loop", "break", "groove", "beat", "drumloop", "top", "fill",
    )),
)

TONAL_FOLDERS = {c.folder for c in CATEGORIES if c.tonal}

# Duration heuristics
BPM_MIN_SEC = 1.5    # below this: a one-shot, no tempo
LOOP_MIN_SEC = 2.0   # unlabeled files this long or longer default to "loop"

AUDIO_EXTS = {".wav", ".aif", ".aiff", ".flac", ".mp3", ".ogg"}

_MANIFEST_STEM = "sample_librarian_undo"


def _normalize(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", name.lower()).strip()


_KEYWORD_RES: tuple[tuple[Category, tuple[re.Pattern[str], ...]], ...] = tuple(
    (cat, tuple(re.compile(rf"\b{re.escape(kw)}\b") for kw in cat.keywords))
    for cat in CATEGORIES
)


def classify_filename(name: str) -> Category | None:
    norm = _normalize(name)
    if not norm:
        return None
    for cat, patterns in _KEYWORD_RES:
        for pat in patterns:
            if pat.search(norm):
                return cat
    return None


def resolve_category(name: str, duration: float) -> Category:
    """Keyword category if any, else duration fallback (loop vs one-shot)."""
    cat = classify_filename(name)
    if cat is not None:
        return cat
    if duration >= LOOP_MIN_SEC:
        return _CAT_BY_FOLDER["loop"]
    return _CAT_BY_FOLDER["other"]


_CAT_BY_FOLDER: dict[str, Category] = {c.folder: c for c in CATEGORIES}
# synthetic fallback bucket (no keywords)
_CAT_BY_FOLDER["other"] = Category("other", False, ())


# --------------------------------------------------------------------------- #
# Filename token stripping (idempotent rename)
# --------------------------------------------------------------------------- #
_KEY_TOKEN = re.compile(r"^(?:[A-G](?:#|b)?m?)_", re.IGNORECASE)
_BPM_TOKEN = re.compile(r"^\d{2,3}_")


def strip_tokens(stem: str) -> str:
    """Remove any leading key_/bpm_ tokens we may have added on a prior run."""
    changed = True
    while changed:
        changed = False
        for pat in (_KEY_TOKEN, _BPM_TOKEN):
            m = pat.match(stem)
            if m:
                stem = stem[m.end():]
                changed = True
    return stem


def build_new_name(orig_stem: str, ext: str, key: str | None, bpm: int | None) -> str:
    base = strip_tokens(orig_stem) or orig_stem
    tokens: list[str] = []
    if key:
        tokens.append(key)
    if bpm is not None:
        tokens.append(str(bpm))
    tokens.append(base)
    return "_".join(tokens) + ext


# --------------------------------------------------------------------------- #
# Audio analysis
# --------------------------------------------------------------------------- #
def _librosa():
    import librosa  # noqa: WPS433 (lazy)
    return librosa


def load_audio(path: Path) -> tuple[np.ndarray, int, float]:
    """Load an audio file to mono float, returning (y, sr, duration_seconds)."""
    import soundfile as sf  # noqa: WPS433 (lazy)
    y, sr = sf.read(str(path), dtype="float32", always_2d=False)
    if y.ndim > 1:
        y = y.mean(axis=1)
    y = np.ascontiguousarray(y, dtype=np.float32)
    duration = len(y) / float(sr) if sr else 0.0
    return y, int(sr), duration


def detect_bpm(y: np.ndarray, sr: int) -> int | None:
    """Beat-track BPM via onset-strength autocorrelation. None on failure."""
    librosa = _librosa()
    hop = 256
    try:
        oenv = librosa.onset.onset_strength(y=y, sr=sr, hop_length=hop)
        tempo = librosa.beat.beat_track(onset_envelope=oenv, sr=sr, hop_length=hop)[0]
        bpm = float(np.atleast_1d(tempo)[0])
    except Exception:
        return None
    if not math.isfinite(bpm) or bpm <= 0:
        return None
    return int(round(bpm))


def detect_key(y: np.ndarray, sr: int) -> KeyResult | None:
    librosa = _librosa()
    try:
        chroma = librosa.feature.chroma_cqt(y=y, sr=sr)  # (12, frames), bin0 = C
    except Exception:
        return None
    if chroma.size == 0:
        return None
    return key_from_chroma(chroma.mean(axis=1))


@dataclass(frozen=True)
class Features:
    duration: float
    category: str
    tonal: bool
    bpm: int | None
    key: str | None
    key_conf: float | None


def analyze(
    path: Path,
    loader: Callable[[Path], tuple[np.ndarray, int, float]] = load_audio,
) -> Features:
    """Analyze one file into Features (category, bpm, key)."""
    y, sr, duration = loader(path)
    cat = resolve_category(path.name, duration)
    bpm = None
    key = None
    key_conf = None
    if duration >= BPM_MIN_SEC and len(y) > 0:
        bpm = detect_bpm(y, sr)
        if cat.tonal:
            kr = detect_key(y, sr)
            if kr is not None:
                key = kr.key.short
                key_conf = kr.confidence
    return Features(
        duration=round(duration, 3),
        category=cat.folder,
        tonal=cat.tonal,
        bpm=bpm,
        key=key,
        key_conf=key_conf,
    )


# --------------------------------------------------------------------------- #
# Planning
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class Move:
    src: Path
    dst: Path
    features: Features

    def to_dict(self) -> dict[str, Any]:
        return {
            "from": str(self.src),
            "to": str(self.dst),
            "category": self.features.category,
            "bpm": self.features.bpm,
            "key": self.features.key,
            "duration": self.features.duration,
        }


def iter_audio_files(root: Path, recursive: bool) -> list[Path]:
    globber = root.rglob("*") if recursive else root.glob("*")
    out = [
        p for p in sorted(globber)
        if p.is_file() and p.suffix.lower() in AUDIO_EXTS
        and not p.name.startswith(_MANIFEST_STEM)
    ]
    return out


def _uniquify(dst: Path, claimed: set[str]) -> Path:
    """Return a collision-free destination (adds _1/_2… before the extension).

    Considers both already-claimed targets in THIS plan and files already on
    disk (case-insensitively, since Windows filesystems are case-insensitive).
    """
    def taken(p: Path) -> bool:
        return str(p).lower() in claimed or p.exists()

    if not taken(dst):
        claimed.add(str(dst).lower())
        return dst
    stem, ext = dst.stem, dst.suffix
    i = 1
    while True:
        cand = dst.with_name(f"{stem}_{i}{ext}")
        if not taken(cand):
            claimed.add(str(cand).lower())
            return cand
        i += 1


def plan_moves(
    root: Path,
    dest: Path,
    recursive: bool = True,
    loader: Callable[[Path], tuple[np.ndarray, int, float]] = load_audio,
) -> tuple[list[Move], list[str]]:
    """Analyze every audio file under `root`, planning a move into `dest`/<cat>/.

    A file already at its target path (same category folder + canonical name) is
    skipped, making the whole operation idempotent.
    """
    moves: list[Move] = []
    skipped: list[str] = []
    claimed: set[str] = set()
    for path in iter_audio_files(root, recursive):
        try:
            feats = analyze(path, loader=loader)
        except Exception as e:  # unreadable / corrupt file
            skipped.append(f"{path.name}: unreadable ({e})")
            continue
        new_name = build_new_name(path.stem, path.suffix, feats.key, feats.bpm)
        target_dir = dest / feats.category
        ideal = target_dir / new_name
        if ideal.resolve() == path.resolve():
            continue  # already sorted + canonically named
        dst = _uniquify(ideal, claimed)
        moves.append(Move(src=path, dst=dst, features=feats))
    return moves, skipped


# --------------------------------------------------------------------------- #
# Apply / undo
# --------------------------------------------------------------------------- #
@dataclass
class ApplyReport:
    moved: list[Move] = field(default_factory=list)
    failed: list[tuple[Move, str]] = field(default_factory=list)
    manifest_path: Path | None = None


def apply_moves(moves: list[Move], dest: Path, write_manifest: bool = True) -> ApplyReport:
    import shutil

    report = ApplyReport()
    done: list[dict[str, Any]] = []
    for mv in moves:
        try:
            mv.dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(mv.src), str(mv.dst))
        except Exception as e:
            report.failed.append((mv, str(e)))
            continue
        report.moved.append(mv)
        done.append({"from": str(mv.src), "to": str(mv.dst)})

    if write_manifest and done:
        dest.mkdir(parents=True, exist_ok=True)
        ts = time.strftime("%Y%m%d_%H%M%S")
        mpath = dest / f"{_MANIFEST_STEM}_{ts}.json"
        with mpath.open("w", encoding="utf-8") as f:
            json.dump({"created": ts, "moves": done}, f, indent=2)
        report.manifest_path = mpath
    return report


@dataclass
class UndoReport:
    restored: int = 0
    failed: list[str] = field(default_factory=list)


def undo(manifest_path: Path) -> UndoReport:
    """Replay a manifest in reverse: move every 'to' back to its 'from'."""
    import shutil

    with manifest_path.open("r", encoding="utf-8") as f:
        data = json.load(f)
    report = UndoReport()
    for entry in reversed(data.get("moves", [])):
        src = Path(entry["to"])
        dst = Path(entry["from"])
        try:
            if not src.exists():
                report.failed.append(f"missing: {src}")
                continue
            dst.parent.mkdir(parents=True, exist_ok=True)
            final = dst
            if final.exists():
                i = 1
                while final.exists():
                    final = dst.with_name(f"{dst.stem}_restored{i}{dst.suffix}")
                    i += 1
            shutil.move(str(src), str(final))
            report.restored += 1
        except Exception as e:
            report.failed.append(f"{src} -> {dst}: {e}")
    return report


# --------------------------------------------------------------------------- #
# Reporting
# --------------------------------------------------------------------------- #
def _summary(moves: list[Move]) -> dict[str, int]:
    by_cat: dict[str, int] = {}
    for mv in moves:
        by_cat[mv.features.category] = by_cat.get(mv.features.category, 0) + 1
    return dict(sorted(by_cat.items()))


def _print_plan(moves: list[Move], skipped: list[str], dest: Path) -> None:
    for mv in moves:
        f = mv.features
        tags = []
        if f.key:
            tags.append(f"key={f.key}")
        if f.bpm is not None:
            tags.append(f"{f.bpm}bpm")
        tags.append(f"{f.duration}s")
        rel = mv.dst.relative_to(dest) if _is_relative(mv.dst, dest) else mv.dst
        print(f"  {mv.src.name}  ->  {rel}   [{', '.join(tags)}]")
    if skipped:
        print("\n  skipped:")
        for s in skipped:
            print(f"    - {s}")
    cats = ", ".join(f"{k}:{v}" for k, v in _summary(moves).items()) or "-"
    print(f"\n  {len(moves)} moves | by category: {cats}")


def _is_relative(p: Path, base: Path) -> bool:
    try:
        p.relative_to(base)
        return True
    except ValueError:
        return False


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #
def cmd_sort(args: argparse.Namespace) -> int:
    root = Path(args.directory).expanduser().resolve()
    if not root.is_dir():
        print(f"Error: {root} is not a directory", file=sys.stderr)
        return 2
    dest = Path(args.dest).expanduser().resolve() if args.dest else root

    moves, skipped = plan_moves(root, dest, recursive=not args.no_recursive)

    if args.json and not args.apply:
        print(json.dumps({
            "dry_run": True,
            "dest": str(dest),
            "summary": _summary(moves),
            "moves": [m.to_dict() for m in moves],
            "skipped": skipped,
        }, indent=2))
        return 0

    if not args.apply:
        print(f"[dry-run] {root} -> {dest}\n")
        _print_plan(moves, skipped, dest)
        print("\n[dry-run] nothing moved. Re-run with --apply to execute.")
        return 0

    print(f"Sorting {root} -> {dest}\n")
    _print_plan(moves, skipped, dest)
    print()
    report = apply_moves(moves, dest)
    for mv, err in report.failed:
        print(f"  [FAIL] {mv.src.name}: {err}")
    print(f"\nDone: {len(report.moved)} moved, {len(report.failed)} failed.")
    if report.manifest_path:
        print(f"Undo manifest: {report.manifest_path}")
        print(f"  restore with: sample_librarian.py undo \"{report.manifest_path}\"")
    return 0 if not report.failed else 1


def cmd_undo(args: argparse.Namespace) -> int:
    mpath = Path(args.manifest).expanduser().resolve()
    if not mpath.is_file():
        print(f"Error: manifest {mpath} not found", file=sys.stderr)
        return 2
    report = undo(mpath)
    for f in report.failed:
        print(f"  [FAIL] {f}")
    print(f"Restored {report.restored} file(s), {len(report.failed)} failed.")
    return 0 if not report.failed else 1


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="sample_librarian.py",
        description="Analyze, rename ({key}_{bpm}_name) and sort a sample library.",
    )
    sub = p.add_subparsers(dest="command", required=True)

    ps = sub.add_parser("sort", help="Scan a directory, plan/apply renames + sorting.")
    ps.add_argument("directory", help="Root of the sample tree to scan.")
    ps.add_argument("--apply", action="store_true",
                    help="Actually move/rename files (default is a dry-run preview).")
    ps.add_argument("--dest", default=None,
                    help="Destination root for category folders (default: in place).")
    ps.add_argument("--no-recursive", action="store_true",
                    help="Only scan the top level, not subfolders.")
    ps.add_argument("--json", action="store_true",
                    help="Emit the dry-run plan as JSON.")
    ps.set_defaults(func=cmd_sort)

    pu = sub.add_parser("undo", help="Replay an undo manifest to restore layout.")
    pu.add_argument("manifest", help="Path to a sample_librarian_undo_*.json manifest.")
    pu.set_defaults(func=cmd_undo)
    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
