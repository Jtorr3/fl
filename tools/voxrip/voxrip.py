# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "librosa>=0.10.1",
#   "soundfile>=0.12",
#   "scipy>=1.11",
# ]
# ///
"""voxrip.py -- acapella extraction + key/tempo conforming (Qeynos W9).

Rip a vocal/lyric out of ANY finished song and conform it to a completely
different track's key and tempo, so a foreign acapella sits in a new production.

Pipeline
--------
  1. SEPARATION  (demucs htdemucs, CPU) -> vocals_raw.wav + instrumental.wav.
     Runs in a SEPARATE `uv run` env (torch CPU wheels) so this script's own
     deps stay light and the offline test gate never needs torch. `--no-separate`
     skips it (the input is already an acapella).
  2. ANALYSIS    BPM (librosa beat tracker, with half/double-time alternates and a
     confidence proxy) + key (chromagram vs Krumhansl-Schmuckler major/minor
     profiles) of BOTH the full song and the isolated vocal.
  3. CONFORM     (when --target-bpm / --target-key given) time-stretch to the
     target BPM and pitch-shift by the MINIMAL semitone move onto the target key.
     Engine: the rubberband CLI portable binary (formant-preserving `-F`), fetched
     into tools/bin/rubberband on demand. Fallback (if rubberband can't be
     fetched/run after 3 attempts): librosa phase-vocoder stretch + resample shift
     (lower quality -- flagged in the REPORT).
  4. OUTPUT      <out>/<song-stem>/ with vocals_raw.wav, vocals_conformed.wav
     (if targets), instrumental.wav, REPORT.md.

Design note -- separation is out-of-process
--------------------------------------------
demucs pulls torch (~200 MB CPU wheel). Putting it in THIS script's PEP 723 deps
would make every analysis run -- and the offline test gate -- download torch.
Instead separation shells out to `uv run --torch-backend cpu --with demucs
python -m demucs ...`, an isolated env uv builds on first use. The analysis and
conform math (this file) depend only on numpy/librosa/soundfile/scipy and are
fully unit-tested without any model weights or network.

Run it
------
uv (at %USERPROFILE%\\.local\\bin\\uv.exe, NOT on PATH) runs this PEP 723 script:

  uv run --python 3.12 C:\\dev\\qeynos-vst-suite\\tools\\voxrip\\voxrip.py song.mp3 \\
      --target-bpm 174 --target-key "F#m" --out ./ripped
"""
from __future__ import annotations

import argparse
import math
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

# numpy is the only import needed for the pure-math helpers (key/tempo). librosa
# and soundfile are imported lazily inside the audio functions so that importing
# this module for the math unit tests is cheap and never triggers a numba JIT.
import numpy as np

# --------------------------------------------------------------------------- #
# Music theory: note names, key parsing, Krumhansl-Schmuckler profiles
# --------------------------------------------------------------------------- #

# Pitch classes, C=0 .. B=11. chroma_cqt bin 0 == C, matching this indexing.
_NOTE_TO_PC = {
    "C": 0, "C#": 1, "DB": 1, "D": 2, "D#": 3, "EB": 3, "E": 4, "FB": 4,
    "E#": 5, "F": 5, "F#": 6, "GB": 6, "G": 7, "G#": 8, "AB": 8, "A": 9,
    "A#": 10, "BB": 10, "B": 11, "CB": 11, "B#": 0,
}
_PC_TO_NAME = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]

# Krumhansl-Schmuckler key profiles (Krumhansl 1990). Index 0 = tonic.
_KS_MAJOR = np.array(
    [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88]
)
_KS_MINOR = np.array(
    [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17]
)


@dataclass(frozen=True)
class Key:
    """A musical key: pitch-class root (C=0) + mode."""

    root: int  # 0..11
    mode: str  # "major" | "minor"

    def __str__(self) -> str:
        return f"{_PC_TO_NAME[self.root % 12]} {self.mode}"

    @property
    def short(self) -> str:
        return f"{_PC_TO_NAME[self.root % 12]}{'m' if self.mode == 'minor' else ''}"


def parse_key(text: str) -> Key:
    """Parse 'Am', 'F#m', 'C', 'Bbmaj', 'G minor' -> Key.

    Rule: a trailing 'm' (not part of 'maj'/'major') or the word 'min'/'minor'
    means minor; otherwise major. Accidentals '#' and 'b' are honoured.
    """
    if text is None:
        raise ValueError("empty key")
    s = text.strip().replace(" ", "")
    if not s:
        raise ValueError("empty key")
    low = s.lower()

    mode = "major"
    # Strip an explicit mode suffix first (longest match wins).
    for suffix, m in (
        ("major", "major"), ("maj", "major"), ("minor", "minor"),
        ("min", "minor"), ("m", "minor"),
    ):
        if low.endswith(suffix) and len(low) > len(suffix):
            mode = m
            s = s[: len(s) - len(suffix)]
            break

    # Now s is the root: a letter + optional accidental(s).
    root_txt = s.strip()
    if not root_txt:
        raise ValueError(f"no root note in key {text!r}")
    letter = root_txt[0].upper()
    accidentals = root_txt[1:].replace("♯", "#").replace("♭", "b")
    norm = letter + accidentals.upper().replace("B", "B")  # keep for map lookup
    key_lookup = (letter + accidentals).upper()
    if key_lookup not in _NOTE_TO_PC:
        # Try letter alone as a fallback (e.g. malformed accidental).
        if letter in _NOTE_TO_PC:
            key_lookup = letter
        else:
            raise ValueError(f"unrecognised root note in key {text!r}")
    return Key(root=_NOTE_TO_PC[key_lookup], mode=mode)


def _minimal_signed(semitones: int) -> int:
    """Wrap a semitone interval into the minimal-magnitude signed value (-5..+6)."""
    x = semitones % 12
    if x > 6:
        x -= 12
    return x


@dataclass
class Transposition:
    """The chosen pitch move plus its octave-wrap partner and any relative-mode
    reinterpretation, for the REPORT."""

    semitones: int  # the chosen, minimal-|st| move
    alternative: int  # the +/-12 octave-wrap partner (same landing pitch class)
    source: Key
    target: Key
    relative_reinterpretation: str | None = None  # human note, if modes differed
    rule: str = ""


def minimal_transposition(source: Key, target: Key) -> Transposition:
    """Minimal semitone move to put `source` material into `target`'s key.

    Rule (documented in every REPORT):
      * Same mode  -> shift = minimal signed distance between the two roots.
      * Different mode -> reinterpret the SOURCE via its relative key so it is
        expressed in the TARGET's mode (minor->its relative major = root+3;
        major->its relative minor = root-3), then take the minimal signed
        distance to the target root. This maps the source scale onto the target
        scale (relative keys share notes), which is what actually makes a foreign
        acapella sit in the new key -- a plain root-to-root match would leave a
        minor vocal clashing over a major track.
      * Either way, the octave-wrap partner (chosen +/- 12) is reported as the
        alternative (e.g. -3 or +9); we pick the smaller |semitones|.
    """
    if source.mode == target.mode:
        eff_root = source.root
        reinterp = None
        rule = (
            "same mode: minimal signed semitone distance between roots "
            f"({source.short} -> {target.short})"
        )
    else:
        if source.mode == "minor":
            eff_root = (source.root + 3) % 12  # relative major of the source
            rel_name = f"{_PC_TO_NAME[eff_root]} major"
        else:
            eff_root = (source.root - 3) % 12  # relative minor of the source
            rel_name = f"{_PC_TO_NAME[eff_root]} minor"
        reinterp = (
            f"modes differ: source {source} reinterpreted as its relative "
            f"{rel_name} (shares the same notes) before matching to {target}"
        )
        rule = (
            "different mode: reinterpret source via its relative key into the "
            "target's mode, then minimal signed distance"
        )

    chosen = _minimal_signed(target.root - eff_root)
    # Octave-wrap partner (same pitch-class landing, +/- one octave).
    partner = chosen - 12 if chosen > 0 else chosen + 12
    # Prefer the smaller magnitude as "chosen"; keep the other as alternative.
    if abs(partner) < abs(chosen):
        chosen, partner = partner, chosen
    return Transposition(
        semitones=chosen,
        alternative=partner,
        source=source,
        target=target,
        relative_reinterpretation=reinterp,
        rule=rule,
    )


# --------------------------------------------------------------------------- #
# Tempo conform math
# --------------------------------------------------------------------------- #

def time_ratio(source_bpm: float, target_bpm: float) -> float:
    """rubberband --time ratio (output duration / input duration) to move from
    source_bpm to target_bpm. Faster target -> shorter output -> ratio < 1.

    ratio = source_bpm / target_bpm.  (rubberband --time R makes the output R
    times as long; librosa.effects.time_stretch uses rate = 1/R = target/source.)
    """
    if source_bpm <= 0 or target_bpm <= 0:
        raise ValueError("bpm must be positive")
    return source_bpm / target_bpm


def stretch_rate(source_bpm: float, target_bpm: float) -> float:
    """librosa.effects.time_stretch rate (target/source). Inverse of time_ratio."""
    return 1.0 / time_ratio(source_bpm, target_bpm)


# --------------------------------------------------------------------------- #
# Analysis: BPM + key detection (librosa; imported lazily)
# --------------------------------------------------------------------------- #

@dataclass
class TempoResult:
    bpm: float
    confidence: float  # 0..1 proxy: fraction of local estimates near the pick
    half_time: float
    double_time: float
    alternates: list = field(default_factory=list)


@dataclass
class KeyResult:
    key: Key
    confidence: float  # best correlation score, 0..1-ish
    second: Key
    second_score: float

    @property
    def relative(self) -> Key:
        """The relative major/minor of the detected key (shares its notes)."""
        if self.key.mode == "minor":
            return Key((self.key.root + 3) % 12, "major")
        return Key((self.key.root - 3) % 12, "minor")


def _librosa():
    import librosa  # noqa: WPS433 (lazy)
    return librosa


def detect_bpm(y: np.ndarray, sr: int) -> TempoResult:
    """Estimate BPM with a beat tracker + a confidence proxy and half/double alts.

    A 256-sample hop is used (rather than librosa's 512 default) so the tempogram
    lag bins are fine enough to resolve common tempi to well under 1% -- at
    hop=512 / 22.05 kHz, 120 BPM falls exactly between two lag bins (~117.5 and
    ~123), biasing the estimate by ~2%.
    """
    librosa = _librosa()
    hop = 256
    oenv = librosa.onset.onset_strength(y=y, sr=sr, hop_length=hop)
    tempo_est = librosa.beat.beat_track(onset_envelope=oenv, sr=sr, hop_length=hop)[0]
    bpm = float(np.atleast_1d(tempo_est)[0])

    # Per-frame tempo estimates give a spread we turn into a confidence proxy.
    try:
        dtempo = librosa.feature.rhythm.tempo(
            onset_envelope=oenv, sr=sr, hop_length=hop, aggregate=None
        )
    except AttributeError:  # older/newer librosa layout
        dtempo = librosa.feature.tempo(
            onset_envelope=oenv, sr=sr, hop_length=hop, aggregate=None
        )
    dtempo = np.atleast_1d(np.asarray(dtempo, dtype=float))
    if dtempo.size and bpm > 0:
        conf = float(np.mean(np.abs(dtempo - bpm) <= 0.08 * bpm))
    else:
        conf = 0.0

    # Distinct secondary local tempo (mode of the rounded per-frame estimates).
    alternates: list[float] = []
    if dtempo.size:
        rounded = np.round(dtempo).astype(int)
        vals, counts = np.unique(rounded, return_counts=True)
        for v in vals[np.argsort(-counts)]:
            if abs(float(v) - bpm) > 0.08 * bpm:
                alternates.append(float(v))
            if len(alternates) >= 2:
                break

    return TempoResult(
        bpm=round(bpm, 2),
        confidence=round(conf, 3),
        half_time=round(bpm / 2.0, 2),
        double_time=round(bpm * 2.0, 2),
        alternates=alternates,
    )


def detect_key(y: np.ndarray, sr: int) -> KeyResult:
    """Krumhansl-Schmuckler key finding from a chromagram."""
    librosa = _librosa()
    chroma = librosa.feature.chroma_cqt(y=y, sr=sr)  # (12, frames), bin 0 = C
    return key_from_chroma(chroma.mean(axis=1))


def key_from_chroma(chroma_mean: np.ndarray) -> KeyResult:
    """Score a 12-vector chroma against all 24 keys; return best + runner-up.

    Split out from detect_key so the KS scoring is unit-testable from a
    hand-built chroma vector without synthesising or loading any audio.
    """
    v = np.asarray(chroma_mean, dtype=float)
    if v.shape != (12,):
        raise ValueError("chroma_mean must have 12 elements")
    scores: list[tuple[float, Key]] = []
    for mode, profile in (("major", _KS_MAJOR), ("minor", _KS_MINOR)):
        for root in range(12):
            rp = np.roll(profile, root)  # move tonic weight to pitch class `root`
            score = _pearson(v, rp)
            scores.append((score, Key(root, mode)))
    scores.sort(key=lambda t: t[0], reverse=True)
    best_score, best_key = scores[0]
    second_score, second_key = scores[1]
    return KeyResult(
        key=best_key,
        confidence=round(float(best_score), 3),
        second=second_key,
        second_score=round(float(second_score), 3),
    )


def _pearson(a: np.ndarray, b: np.ndarray) -> float:
    a = a - a.mean()
    b = b - b.mean()
    denom = math.sqrt(float(np.dot(a, a)) * float(np.dot(b, b)))
    if denom == 0:
        return 0.0
    return float(np.dot(a, b) / denom)


# --------------------------------------------------------------------------- #
# Audio IO
# --------------------------------------------------------------------------- #

def load_audio(path: Path, sr: int | None = 22050, mono: bool = True):
    """Load an mp3/wav/flac to a float32 numpy array. Returns (y, sr)."""
    librosa = _librosa()
    y, out_sr = librosa.load(str(path), sr=sr, mono=mono)
    return y, out_sr


def write_wav(path: Path, y: np.ndarray, sr: int) -> None:
    import soundfile as sf
    path.parent.mkdir(parents=True, exist_ok=True)
    data = y.T if (y.ndim == 2 and y.shape[0] < y.shape[1]) else y
    sf.write(str(path), data, sr)


# --------------------------------------------------------------------------- #
# Separation (out-of-process demucs via uv)
# --------------------------------------------------------------------------- #

def uv_path() -> str:
    """Locate uv (not on PATH on the build machine)."""
    explicit = Path(os.path.expandvars(r"%USERPROFILE%\.local\bin\uv.exe"))
    if explicit.exists():
        return str(explicit)
    found = shutil.which("uv")
    return found or "uv"


def separate_helper() -> Path:
    return Path(__file__).resolve().parent / "voxrip_separate.py"


def demucs_command(input_path: Path, out_root: Path, uv: str | None = None) -> list[str]:
    """Build the out-of-process demucs invocation (isolated CPU-torch uv env).

    Runs voxrip_separate.py, whose PEP 723 `[tool.uv]` metadata pins torch to the
    CPU wheel index so no CUDA build is ever downloaded. That helper calls demucs
    with `--two-stems vocals`, yielding exactly vocals.wav + no_vocals.wav (the
    instrumental = mix minus vocals).
    """
    uv = uv or uv_path()
    return [
        uv, "run", "--python", "3.12",
        str(separate_helper()),
        str(input_path),
        str(out_root),
    ]


def separate(input_path: Path, work: Path, timeout: int = 3600) -> tuple[Path, Path]:
    """Run demucs; return (vocals_wav, instrumental_wav). Raises on failure."""
    out_root = work / "demucs_out"
    out_root.mkdir(parents=True, exist_ok=True)
    cmd = demucs_command(input_path, out_root)
    print(f"[voxrip] separating (demucs htdemucs, CPU): {' '.join(cmd)}", flush=True)
    subprocess.run(cmd, check=True, timeout=timeout)
    # demucs writes <out_root>/htdemucs/<track>/{vocals,no_vocals}.wav
    stem = input_path.stem
    base = out_root / "htdemucs" / stem
    vocals = base / "vocals.wav"
    instrumental = base / "no_vocals.wav"
    if not vocals.exists() or not instrumental.exists():
        # Fall back to searching (demucs sanitises some track names).
        found_v = list(out_root.rglob("vocals.wav"))
        found_i = list(out_root.rglob("no_vocals.wav"))
        if found_v:
            vocals = found_v[0]
        if found_i:
            instrumental = found_i[0]
    if not vocals.exists():
        raise FileNotFoundError(f"demucs produced no vocals stem under {out_root}")
    return vocals, instrumental


# --------------------------------------------------------------------------- #
# rubberband CLI: fetch + invoke (with librosa fallback)
# --------------------------------------------------------------------------- #

# Breakfast Quay ships portable Windows command-line builds. Newest first.
_RUBBERBAND_URLS = [
    "https://breakfastquay.com/files/releases/rubberband-3.3.0-gpl-executable-windows.zip",
    "https://breakfastquay.com/files/releases/rubberband-3.2.1-gpl-executable-windows.zip",
    "https://breakfastquay.com/files/releases/rubberband-2.0.2-gpl-executable-windows.zip",
]


def rubberband_dir() -> Path:
    return Path(__file__).resolve().parents[1] / "bin" / "rubberband"


def find_rubberband() -> Path | None:
    """Return a usable rubberband.exe path if one is already present."""
    d = rubberband_dir()
    if d.exists():
        for exe in d.rglob("rubberband*.exe"):
            return exe
    onpath = shutil.which("rubberband")
    return Path(onpath) if onpath else None


def _unblock_file(path: Path) -> None:
    """Strip Mark-of-the-Web (Zone.Identifier ADS) so the exe runs unprompted."""
    try:
        ads = str(path) + ":Zone.Identifier"
        if os.path.exists(ads):
            os.remove(ads)
    except OSError:
        pass
    # PowerShell Unblock-File is the canonical strip; best-effort.
    try:
        subprocess.run(
            ["powershell", "-NoProfile", "-Command", f"Unblock-File -Path '{path}'"],
            check=False, timeout=30,
        )
    except (OSError, subprocess.SubprocessError):
        pass


def fetch_rubberband(attempts: int = 3) -> Path | None:
    """Download + unzip a portable rubberband.exe into tools/bin/rubberband.

    Returns the exe path, or None after `attempts` failed downloads (the caller
    then uses the librosa fallback). Each URL counts as one attempt.
    """
    existing = find_rubberband()
    if existing:
        return existing
    dest = rubberband_dir()
    dest.mkdir(parents=True, exist_ok=True)
    tried = 0
    for url in _RUBBERBAND_URLS:
        if tried >= attempts:
            break
        tried += 1
        try:
            print(f"[voxrip] fetching rubberband ({tried}/{attempts}): {url}", flush=True)
            with tempfile.NamedTemporaryFile(suffix=".zip", delete=False) as tmp:
                zpath = Path(tmp.name)
            req = urllib.request.Request(url, headers={"User-Agent": "voxrip/1.0"})
            with urllib.request.urlopen(req, timeout=120) as resp, open(zpath, "wb") as fh:
                shutil.copyfileobj(resp, fh)
            with zipfile.ZipFile(zpath) as zf:
                zf.extractall(dest)
            zpath.unlink(missing_ok=True)
            exe = find_rubberband()
            if exe:
                _unblock_file(exe)
                print(f"[voxrip] rubberband ready: {exe}", flush=True)
                return exe
        except (OSError, urllib.error.URLError, zipfile.BadZipFile) as exc:
            print(f"[voxrip] rubberband fetch attempt {tried} failed: {exc}", flush=True)
            continue
    return None


def rubberband_command(
    exe: Path, in_wav: Path, out_wav: Path, t_ratio: float, semitones: float
) -> list[str]:
    """Build the rubberband CLI call: formant-preserving, --time + --pitch."""
    return [
        str(exe),
        "-F",                       # preserve formants (natural vocal timbre)
        "--time", f"{t_ratio:.9f}",  # output duration / input duration
        "--pitch", f"{semitones:.6f}",
        "-c", "6",                  # highest-quality crispness for vocals
        str(in_wav),
        str(out_wav),
    ]


def conform_rubberband(
    exe: Path, in_wav: Path, out_wav: Path, t_ratio: float, semitones: float,
    timeout: int = 900,
) -> None:
    cmd = rubberband_command(exe, in_wav, out_wav, t_ratio, semitones)
    print(f"[voxrip] conform (rubberband): {' '.join(cmd)}", flush=True)
    subprocess.run(cmd, check=True, timeout=timeout)


def conform_librosa(
    in_wav: Path, out_wav: Path, t_ratio: float, semitones: float
) -> None:
    """Fallback conform: phase-vocoder time-stretch + resample pitch-shift.

    Lower quality than rubberband (no formant preservation), used only if the
    rubberband binary can't be fetched/run.
    """
    librosa = _librosa()
    y, sr = load_audio(in_wav, sr=None, mono=False)
    rate = 1.0 / t_ratio  # time_stretch rate = target/source = 1/time_ratio

    def _proc(mono: np.ndarray) -> np.ndarray:
        out = mono
        if abs(rate - 1.0) > 1e-4:
            out = librosa.effects.time_stretch(out, rate=rate)
        if abs(semitones) > 1e-4:
            out = librosa.effects.pitch_shift(out, sr=sr, n_steps=semitones)
        return out

    if y.ndim == 1:
        out = _proc(y)
    else:
        chans = [_proc(y[c]) for c in range(y.shape[0])]
        n = min(len(c) for c in chans)
        out = np.stack([c[:n] for c in chans], axis=0)
    write_wav(out_wav, out, sr)


# --------------------------------------------------------------------------- #
# REPORT
# --------------------------------------------------------------------------- #

def _tempo_md(label: str, t: TempoResult) -> str:
    alts = ", ".join(f"{a:g}" for a in t.alternates) if t.alternates else "none"
    return (
        f"- **{label} BPM:** {t.bpm:g} "
        f"(confidence {t.confidence:.2f}; half-time {t.half_time:g}, "
        f"double-time {t.double_time:g}; other local estimates: {alts})\n"
    )


def _key_md(label: str, k: KeyResult) -> str:
    return (
        f"- **{label} key:** {k.key} "
        f"(score {k.confidence:.2f}; runner-up {k.second} @ {k.second_score:.2f}; "
        f"relative {k.relative})\n"
    )


def write_report(
    out_dir: Path,
    input_path: Path,
    song_tempo: TempoResult | None,
    song_key: KeyResult | None,
    vox_tempo: TempoResult,
    vox_key: KeyResult,
    target_bpm: float | None,
    target_key: Key | None,
    trans: Transposition | None,
    t_ratio: float | None,
    engine: str | None,
    warnings: list[str],
    separated: bool,
) -> Path:
    lines: list[str] = []
    lines.append(f"# VOXRIP report -- {input_path.name}\n")
    lines.append("")
    lines.append("## Source analysis\n")
    if song_tempo and song_key:
        lines.append(_tempo_md("Full-song", song_tempo))
        lines.append(_key_md("Full-song", song_key))
    else:
        lines.append("- Full-song analysis skipped (--no-separate; input treated as an acapella).\n")
    lines.append(_tempo_md("Vocal", vox_tempo))
    lines.append(_key_md("Vocal", vox_key))
    lines.append("")

    lines.append("## Conform\n")
    if target_bpm or target_key:
        if target_bpm:
            lines.append(
                f"- **Target BPM:** {target_bpm:g} "
                f"(from vocal {vox_tempo.bpm:g}; "
                f"time ratio = {t_ratio:.4f}, i.e. rate {1.0 / t_ratio:.4f}x)\n"
                if t_ratio else f"- **Target BPM:** {target_bpm:g}\n"
            )
        else:
            lines.append("- **Target BPM:** (none -- tempo unchanged)\n")
        if target_key and trans:
            lines.append(f"- **Target key:** {target_key}\n")
            lines.append(
                f"- **Transposition chosen:** {trans.semitones:+d} semitones "
                f"(octave-wrap alternative: {trans.alternative:+d})\n"
            )
            lines.append(f"- **Key-shift rule:** {trans.rule}\n")
            if trans.relative_reinterpretation:
                lines.append(f"- **Relative-mode note:** {trans.relative_reinterpretation}\n")
        else:
            lines.append("- **Target key:** (none -- pitch unchanged)\n")
        lines.append(f"- **Engine:** {engine}\n")
    else:
        lines.append("- No targets given -- analysis only (no conformed vocal written).\n")
    lines.append("")

    lines.append("## Outputs\n")
    lines.append("- `vocals_raw.wav` -- isolated vocal"
                 + (" (demucs htdemucs)\n" if separated else " (input passed through)\n"))
    if target_bpm or target_key:
        lines.append("- `vocals_conformed.wav` -- key/tempo-conformed vocal\n")
    if separated:
        lines.append("- `instrumental.wav` -- accompaniment (mix minus vocals)\n")
    lines.append("")

    lines.append("## Warnings\n")
    if warnings:
        for w in warnings:
            lines.append(f"- {w}\n")
    else:
        lines.append("- none\n")

    report = out_dir / "REPORT.md"
    report.write_text("".join(lines), encoding="utf-8")
    return report


# --------------------------------------------------------------------------- #
# Orchestration / CLI
# --------------------------------------------------------------------------- #

def run(args: argparse.Namespace) -> int:
    input_path = Path(args.song).expanduser().resolve()
    if not input_path.exists():
        print(f"[voxrip] input not found: {input_path}", file=sys.stderr)
        return 2

    out_base = Path(args.out).expanduser().resolve() if args.out else input_path.parent / "voxrip_out"
    out_dir = out_base / input_path.stem
    out_dir.mkdir(parents=True, exist_ok=True)

    warnings: list[str] = []
    target_bpm = float(args.target_bpm) if args.target_bpm else None
    target_key = parse_key(args.target_key) if args.target_key else None

    with tempfile.TemporaryDirectory(prefix="voxrip_") as tmp:
        work = Path(tmp)
        vocals_raw = out_dir / "vocals_raw.wav"
        separated = False

        # 1. SEPARATION -------------------------------------------------------
        if args.no_separate:
            print("[voxrip] --no-separate: treating input as an acapella", flush=True)
            y, sr = load_audio(input_path, sr=None, mono=False)
            write_wav(vocals_raw, y, sr)
        else:
            try:
                v_wav, i_wav = separate(input_path, work)
                shutil.copyfile(v_wav, vocals_raw)
                shutil.copyfile(i_wav, out_dir / "instrumental.wav")
                separated = True
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired,
                    FileNotFoundError, OSError) as exc:
                warnings.append(
                    f"Separation failed ({exc}); treating input as the vocal. "
                    "Install demucs / check the uv env, then re-run without --no-separate."
                )
                print(f"[voxrip] separation failed: {exc}", file=sys.stderr, flush=True)
                y, sr = load_audio(input_path, sr=None, mono=False)
                write_wav(vocals_raw, y, sr)

        # 2. ANALYSIS ---------------------------------------------------------
        vy, vsr = load_audio(vocals_raw, sr=22050, mono=True)
        vox_tempo = detect_bpm(vy, vsr)
        vox_key = detect_key(vy, vsr)

        song_tempo = song_key = None
        if separated:
            sy, ssr = load_audio(input_path, sr=22050, mono=True)
            song_tempo = detect_bpm(sy, ssr)
            song_key = detect_key(sy, ssr)

        if vox_tempo.confidence < 0.5:
            warnings.append(
                f"Low BPM confidence ({vox_tempo.confidence:.2f}) on the vocal; "
                f"consider the half/double-time alternates "
                f"({vox_tempo.half_time:g}/{vox_tempo.double_time:g})."
            )
        if vox_key.confidence - vox_key.second_score < 0.05:
            warnings.append(
                f"Key is ambiguous: {vox_key.key} vs {vox_key.second} "
                f"({vox_key.confidence:.2f} vs {vox_key.second_score:.2f})."
            )

        # 3. CONFORM ----------------------------------------------------------
        engine = None
        t_ratio = None
        trans = None
        if target_bpm or target_key:
            t_ratio = time_ratio(vox_tempo.bpm, target_bpm) if target_bpm else 1.0
            if target_key:
                trans = minimal_transposition(vox_key.key, target_key)
                semis = float(trans.semitones)
            else:
                semis = 0.0
            conformed = out_dir / "vocals_conformed.wav"

            exe = None
            if not args.force_fallback:
                exe = fetch_rubberband(attempts=3)
            if exe is not None:
                try:
                    conform_rubberband(exe, vocals_raw, conformed, t_ratio, semis)
                    engine = f"rubberband CLI ({exe.name}, -F formant-preserving)"
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as exc:
                    warnings.append(
                        f"rubberband ran but failed ({exc}); used librosa phase-vocoder "
                        "fallback (lower quality, no formant preservation)."
                    )
                    conform_librosa(vocals_raw, conformed, t_ratio, semis)
                    engine = "librosa phase-vocoder fallback (rubberband run failed)"
            else:
                if args.force_fallback:
                    engine_reason = "rubberband skipped (--force-fallback)"
                else:
                    warnings.append(
                        "rubberband binary could not be fetched after 3 attempts; used "
                        "librosa phase-vocoder fallback (lower quality, no formant preservation)."
                    )
                    engine_reason = "rubberband unavailable"
                conform_librosa(vocals_raw, conformed, t_ratio, semis)
                engine = f"librosa phase-vocoder fallback ({engine_reason})"

        # 4. REPORT -----------------------------------------------------------
        report = write_report(
            out_dir, input_path, song_tempo, song_key, vox_tempo, vox_key,
            target_bpm, target_key, trans, t_ratio, engine, warnings, separated,
        )

    print(f"[voxrip] done -> {out_dir}", flush=True)
    print(f"[voxrip] report: {report}", flush=True)
    for w in warnings:
        print(f"[voxrip] WARNING: {w}", flush=True)
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="voxrip",
        description="Rip a vocal from a song and conform it to a new key/tempo.",
    )
    p.add_argument("song", help="input song (.mp3/.wav/.flac)")
    p.add_argument("--target-bpm", type=float, default=None,
                   help="conform the vocal to this BPM")
    p.add_argument("--target-key", default=None,
                   help="conform the vocal to this key, e.g. Am, F#m, C, Bbmaj")
    p.add_argument("--out", default=None,
                   help="output base dir (default: <song-dir>/voxrip_out)")
    p.add_argument("--no-separate", action="store_true",
                   help="skip demucs; input is already an acapella")
    p.add_argument("--force-fallback", action="store_true",
                   help="skip rubberband; use the librosa fallback (testing/offline)")
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
