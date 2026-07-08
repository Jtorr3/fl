# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""reference_gap.py — compare a reference track against your mix render (Qeynos W7).

Given a professionally-mastered REFERENCE track and YOUR mix render, it reports
the gaps that matter for mixing decisions:
  * LUFS-I integrated loudness (pyloudnorm) for both + the delta;
  * 1/3-octave spectral BALANCE difference (each spectrum normalized to its own
    mean so the comparison is tonal, not level) — where your mix is bright/dull
    vs the reference;
  * stereo WIDTH by band (side/total energy per 1/3-octave) for both + delta;
  * KICK fundamental detection (dominant low-band peak, parabolic-interpolated)
    + a tuning suggestion (nearest note + cents, and, if --key is given, the move
    onto the track's key root).

Output is a single self-contained **HTML report** — inline CSS + inline SVG
charts, NO external CSS/JS/CDN/fonts, so it opens offline anywhere.

DESIGN DECISION — plots: PURE INLINE SVG (not matplotlib/PNG). Keeps the
dependency set light (numpy/scipy/soundfile/pyloudnorm), makes the HTML truly
self-contained without base64-embedding a raster, and renders crisply at any
zoom. The charts are simple bar/level diagrams that SVG expresses directly.
"""

from __future__ import annotations

import argparse
import html as _html
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np

# --------------------------------------------------------------------------- #
# 1/3-octave band centers (ISO 266 nominal), 20 Hz .. 20 kHz
# --------------------------------------------------------------------------- #
THIRD_OCTAVE_CENTERS: tuple[float, ...] = (
    20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630,
    800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000,
    12500, 16000, 20000,
)
_BAND_RATIO = 2.0 ** (1.0 / 6.0)  # half a 1/3-octave, for edges

_NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
_PC = {n: i for i, n in enumerate(
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"])}
_PC.update({"DB": 1, "EB": 3, "GB": 6, "AB": 8, "BB": 10})

_EPS = 1e-12

# numpy>=2.0 renamed trapz -> trapezoid (trapz deprecated); 1.26 only has trapz.
_trapz = getattr(np, "trapezoid", getattr(np, "trapz"))


# --------------------------------------------------------------------------- #
# Audio IO
# --------------------------------------------------------------------------- #
def load_stereo(path: Path, max_seconds: float = 60.0) -> tuple[np.ndarray, int]:
    """Load audio as (N, 2) float32. Mono is duplicated to both channels."""
    import soundfile as sf  # noqa: WPS433
    y, sr = sf.read(str(path), dtype="float32", always_2d=True)
    if y.shape[1] == 1:
        y = np.repeat(y, 2, axis=1)
    elif y.shape[1] > 2:
        y = y[:, :2]
    if max_seconds and len(y) > int(max_seconds * sr):
        y = y[: int(max_seconds * sr)]
    return np.ascontiguousarray(y, dtype=np.float32), int(sr)


def to_mono(data: np.ndarray) -> np.ndarray:
    if data.ndim == 1:
        return data
    return data.mean(axis=1)


# --------------------------------------------------------------------------- #
# LUFS
# --------------------------------------------------------------------------- #
def measure_lufs(data: np.ndarray, sr: int) -> float:
    """Integrated loudness (LUFS) via pyloudnorm. -inf for silence/too-short."""
    import pyloudnorm as pyln  # noqa: WPS433
    meter = pyln.Meter(sr)
    x = data if data.ndim == 2 else data[:, None]
    try:
        val = float(meter.integrated_loudness(x))
    except Exception:
        return float("-inf")
    return val


# --------------------------------------------------------------------------- #
# Spectrum (1/3-octave)
# --------------------------------------------------------------------------- #
def _welch_psd(mono: np.ndarray, sr: int, nperseg: int = 8192) -> tuple[np.ndarray, np.ndarray]:
    from scipy import signal  # noqa: WPS433
    nperseg = min(nperseg, len(mono)) if len(mono) else nperseg
    if nperseg < 16:
        nperseg = min(16, len(mono)) or 16
    nfft = max(nperseg, 1 << (int(nperseg - 1).bit_length()))
    f, pxx = signal.welch(mono, fs=sr, nperseg=nperseg, nfft=max(nfft, 4096))
    return f, pxx


def third_octave_bands(mono: np.ndarray, sr: int) -> dict[float, float]:
    """Per-band power in dB (10*log10 of PSD integrated over each band)."""
    f, pxx = _welch_psd(mono, sr)
    out: dict[float, float] = {}
    for fc in THIRD_OCTAVE_CENTERS:
        lo, hi = fc / _BAND_RATIO, fc * _BAND_RATIO
        mask = (f >= lo) & (f < hi)
        power = float(_trapz(pxx[mask], f[mask])) if np.any(mask) else 0.0
        out[fc] = 10.0 * math.log10(power + _EPS)
    return out


def _normalize_bands(bands: dict[float, float]) -> dict[float, float]:
    """Subtract the mean dB so comparisons reflect balance, not overall level."""
    vals = [v for v in bands.values() if math.isfinite(v)]
    mean = sum(vals) / len(vals) if vals else 0.0
    return {k: v - mean for k, v in bands.items()}


def spectral_diff(
    ref_bands: dict[float, float], mix_bands: dict[float, float]
) -> dict[float, float]:
    """mix - ref per band, after normalizing each spectrum to its own mean."""
    r = _normalize_bands(ref_bands)
    m = _normalize_bands(mix_bands)
    return {fc: m[fc] - r[fc] for fc in THIRD_OCTAVE_CENTERS}


# --------------------------------------------------------------------------- #
# Stereo width by band
# --------------------------------------------------------------------------- #
def stereo_width_by_band(data: np.ndarray, sr: int) -> dict[float, float]:
    """Side/(mid+side) energy fraction per 1/3-octave band (0=mono .. ~0.5=wide)."""
    if data.ndim == 1 or data.shape[1] < 2:
        return {fc: 0.0 for fc in THIRD_OCTAVE_CENTERS}
    left = data[:, 0].astype(np.float64)
    right = data[:, 1].astype(np.float64)
    mid = 0.5 * (left + right)
    side = 0.5 * (left - right)
    fm, pm = _welch_psd(mid, sr)
    fs, ps = _welch_psd(side, sr)
    out: dict[float, float] = {}
    for fc in THIRD_OCTAVE_CENTERS:
        lo, hi = fc / _BAND_RATIO, fc * _BAND_RATIO
        mmask = (fm >= lo) & (fm < hi)
        smask = (fs >= lo) & (fs < hi)
        mp = float(_trapz(pm[mmask], fm[mmask])) if np.any(mmask) else 0.0
        sp = float(_trapz(ps[smask], fs[smask])) if np.any(smask) else 0.0
        out[fc] = sp / (mp + sp + _EPS)
    return out


# --------------------------------------------------------------------------- #
# Kick fundamental + tuning
# --------------------------------------------------------------------------- #
def nearest_note(freq: float) -> tuple[str, int, float]:
    """Return (note_name_with_octave, midi, cents_offset) for a frequency."""
    if freq <= 0:
        return ("-", 0, 0.0)
    midi_f = 69.0 + 12.0 * math.log2(freq / 440.0)
    midi = int(round(midi_f))
    cents = round((midi_f - midi) * 100.0, 1)
    name = _NOTE_NAMES[midi % 12] + str(midi // 12 - 1)
    return (name, midi, cents)


def detect_kick_f0(
    mono: np.ndarray, sr: int, fmin: float = 30.0, fmax: float = 150.0
) -> float | None:
    """Dominant low-band fundamental via a fine PSD + parabolic peak interp."""
    if len(mono) < 64 or float(np.max(np.abs(mono))) < 1e-6:
        return None  # too short / silent
    from scipy import signal  # noqa: WPS433
    nperseg = min(len(mono), 16384)
    nfft = 65536
    f, pxx = signal.welch(mono, fs=sr, nperseg=nperseg, nfft=nfft)
    band = (f >= fmin) & (f <= fmax)
    if not np.any(band):
        return None
    idxs = np.where(band)[0]
    sub = pxx[idxs]
    peak = float(np.max(sub))
    med = float(np.median(sub))
    # Require a prominent peak over the band's noise floor; a flat/broadband
    # spectrum (no real low fundamental) has peak ~ median -> return None.
    if peak <= 0.0 or (med > 0.0 and peak < 4.0 * med):
        return None
    k = int(idxs[int(np.argmax(sub))])
    if k <= 0 or k >= len(pxx) - 1:
        return float(f[k])
    # Parabolic interpolation in log-power for sub-bin accuracy.
    ym1, y0, yp1 = (math.log(pxx[k - 1] + _EPS),
                    math.log(pxx[k] + _EPS),
                    math.log(pxx[k + 1] + _EPS))
    denom = (ym1 - 2 * y0 + yp1)
    delta = 0.5 * (ym1 - yp1) / denom if denom != 0 else 0.0
    df = f[1] - f[0]
    return float(f[k] + delta * df)


@dataclass
class KickTuning:
    f0: float | None
    note: str
    cents: float
    suggestion: str


def kick_tuning(mono: np.ndarray, sr: int, key_root: str | None = None) -> KickTuning:
    f0 = detect_kick_f0(mono, sr)
    if f0 is None or f0 <= 0:
        return KickTuning(None, "-", 0.0, "No clear low-frequency fundamental detected.")
    note, midi, cents = nearest_note(f0)
    parts = [f"Kick fundamental ~{f0:.1f} Hz = {note} ({cents:+.1f} cents)."]
    if abs(cents) >= 5.0:
        parts.append(f"Tune {-cents:+.1f} cents to sit exactly on {note}.")
    else:
        parts.append(f"Already within {abs(cents):.1f} cents of {note}.")
    if key_root is not None:
        pc = _root_pc(key_root)
        if pc is not None:
            # nearest frequency in the same octave as f0 whose pitch class == root
            target = _nearest_freq_of_pc(f0, pc)
            tnote, _, _ = nearest_note(target)
            semis = 12.0 * math.log2(target / f0)
            parts.append(
                f"To match key root {key_root.upper()} ({tnote}): shift "
                f"{semis:+.2f} semitones (kick f0 -> {target:.1f} Hz)."
            )
    return KickTuning(f0, note, cents, " ".join(parts))


def _root_pc(root: str) -> int | None:
    return _PC.get(root.strip().upper().replace("♯", "#").replace("♭", "B"))


def _nearest_freq_of_pc(freq: float, pc: int) -> float:
    """Nearest frequency to `freq` whose pitch class is `pc` (C=0)."""
    midi_f = 69.0 + 12.0 * math.log2(freq / 440.0)
    base = round(midi_f)
    best = None
    for cand in range(int(base) - 12, int(base) + 13):
        if cand % 12 == pc:
            f = 440.0 * 2.0 ** ((cand - 69) / 12.0)
            if best is None or abs(math.log2(f / freq)) < abs(math.log2(best / freq)):
                best = f
    return best if best is not None else freq


# --------------------------------------------------------------------------- #
# Analysis bundle
# --------------------------------------------------------------------------- #
@dataclass
class GapReport:
    ref_lufs: float
    mix_lufs: float
    spectral_diff: dict[float, float] = field(default_factory=dict)
    ref_width: dict[float, float] = field(default_factory=dict)
    mix_width: dict[float, float] = field(default_factory=dict)
    kick: KickTuning | None = None
    ref_name: str = "reference"
    mix_name: str = "mix"

    @property
    def lufs_delta(self) -> float:
        return self.mix_lufs - self.ref_lufs

    def to_dict(self) -> dict[str, Any]:
        return {
            "ref_lufs": self.ref_lufs,
            "mix_lufs": self.mix_lufs,
            "lufs_delta": round(self.lufs_delta, 2),
            "spectral_diff": {str(k): round(v, 2) for k, v in self.spectral_diff.items()},
            "kick": {
                "f0": self.kick.f0 if self.kick else None,
                "note": self.kick.note if self.kick else None,
                "cents": self.kick.cents if self.kick else None,
                "suggestion": self.kick.suggestion if self.kick else None,
            },
        }


def analyze_gap(
    ref: np.ndarray, ref_sr: int, mix: np.ndarray, mix_sr: int,
    key_root: str | None = None, ref_name: str = "reference", mix_name: str = "mix",
) -> GapReport:
    ref_mono = to_mono(ref)
    mix_mono = to_mono(mix)
    rep = GapReport(
        ref_lufs=round(measure_lufs(ref, ref_sr), 2),
        mix_lufs=round(measure_lufs(mix, mix_sr), 2),
        spectral_diff=spectral_diff(
            third_octave_bands(ref_mono, ref_sr),
            third_octave_bands(mix_mono, mix_sr),
        ),
        ref_width=stereo_width_by_band(ref, ref_sr),
        mix_width=stereo_width_by_band(mix, mix_sr),
        kick=kick_tuning(mix_mono, mix_sr, key_root),
        ref_name=ref_name,
        mix_name=mix_name,
    )
    return rep


# --------------------------------------------------------------------------- #
# HTML report (self-contained: inline CSS + inline SVG, no CDN)
# --------------------------------------------------------------------------- #
def _fmt_lufs(v: float) -> str:
    return "-inf" if not math.isfinite(v) else f"{v:.1f}"


def _svg_diff_bars(diff: dict[float, float], width: int = 720, height: int = 260) -> str:
    """Horizontal-axis band chart: bars up (brighter) / down (duller) from 0."""
    centers = list(THIRD_OCTAVE_CENTERS)
    n = len(centers)
    pad_l, pad_b, pad_t = 34, 40, 12
    plot_w = width - pad_l - 8
    plot_h = height - pad_b - pad_t
    vmax = max(6.0, max((abs(v) for v in diff.values()), default=6.0))
    mid_y = pad_t + plot_h / 2
    bw = plot_w / n
    parts = [f'<svg viewBox="0 0 {width} {height}" width="100%" '
             f'role="img" aria-label="spectral balance difference by band" '
             f'xmlns="http://www.w3.org/2000/svg">']
    # gridlines at +/- vmax/2 and 0
    for gv in (-vmax, -vmax / 2, 0, vmax / 2, vmax):
        y = mid_y - (gv / vmax) * (plot_h / 2)
        parts.append(f'<line x1="{pad_l}" y1="{y:.1f}" x2="{pad_l + plot_w}" '
                     f'y2="{y:.1f}" class="grid"/>')
        parts.append(f'<text x="{pad_l - 5}" y="{y + 3:.1f}" '
                     f'class="ytick">{gv:+.0f}</text>')
    for i, fc in enumerate(centers):
        v = max(-vmax, min(vmax, diff[fc]))
        x = pad_l + i * bw
        bh = (abs(v) / vmax) * (plot_h / 2)
        y = mid_y - bh if v >= 0 else mid_y
        cls = "up" if v >= 0 else "down"
        parts.append(f'<rect x="{x + 1:.1f}" y="{y:.1f}" width="{bw - 2:.1f}" '
                     f'height="{bh:.1f}" class="{cls}"/>')
        if fc in (50, 200, 1000, 5000, 20000):
            lbl = f"{int(fc/1000)}k" if fc >= 1000 else f"{int(fc)}"
            parts.append(f'<text x="{x + bw/2:.1f}" y="{height - pad_b + 16}" '
                         f'class="xtick">{lbl}</text>')
    parts.append(f'<text x="{pad_l}" y="{height - 6}" class="axis">'
                 f'1/3-octave band (Hz) — bars up = your mix brighter, '
                 f'down = duller vs reference</text>')
    parts.append("</svg>")
    return "".join(parts)


def _svg_width_curve(ref_w: dict[float, float], mix_w: dict[float, float],
                     width: int = 720, height: int = 220) -> str:
    centers = list(THIRD_OCTAVE_CENTERS)
    n = len(centers)
    pad_l, pad_b, pad_t = 34, 40, 12
    plot_w = width - pad_l - 8
    plot_h = height - pad_b - pad_t
    vmax = 0.5

    def pts(w: dict[float, float]) -> str:
        out = []
        for i, fc in enumerate(centers):
            x = pad_l + (i / (n - 1)) * plot_w
            y = pad_t + plot_h - (min(vmax, w[fc]) / vmax) * plot_h
            out.append(f"{x:.1f},{y:.1f}")
        return " ".join(out)

    parts = [f'<svg viewBox="0 0 {width} {height}" width="100%" '
             f'role="img" aria-label="stereo width by band" '
             f'xmlns="http://www.w3.org/2000/svg">']
    for gv in (0.0, 0.25, 0.5):
        y = pad_t + plot_h - (gv / vmax) * plot_h
        parts.append(f'<line x1="{pad_l}" y1="{y:.1f}" x2="{pad_l + plot_w}" '
                     f'y2="{y:.1f}" class="grid"/>')
        parts.append(f'<text x="{pad_l - 5}" y="{y + 3:.1f}" class="ytick">'
                     f'{gv:.2f}</text>')
    parts.append(f'<polyline points="{pts(ref_w)}" class="refline"/>')
    parts.append(f'<polyline points="{pts(mix_w)}" class="mixline"/>')
    for fc in (50, 200, 1000, 5000, 20000):
        i = centers.index(fc)
        x = pad_l + (i / (n - 1)) * plot_w
        lbl = f"{int(fc/1000)}k" if fc >= 1000 else f"{int(fc)}"
        parts.append(f'<text x="{x:.1f}" y="{height - pad_b + 16}" '
                     f'class="xtick">{lbl}</text>')
    parts.append(f'<text x="{pad_l}" y="{height - 6}" class="axis">'
                 f'side/total energy per band (0 = mono, ~0.5 = wide)</text>')
    parts.append("</svg>")
    return "".join(parts)


_CSS = """
:root{color-scheme:light dark}
body{font:14px/1.5 -apple-system,Segoe UI,Roboto,sans-serif;margin:0;padding:2rem;
 background:#0f1216;color:#e7ecf2}
h1{font-size:1.4rem;margin:0 0 .25rem}h2{font-size:1.05rem;margin:1.75rem 0 .5rem;
 border-bottom:1px solid #2a3340;padding-bottom:.25rem}
.sub{color:#8fa0b3;margin:0 0 1.5rem}
.cards{display:flex;gap:1rem;flex-wrap:wrap}
.card{background:#161b22;border:1px solid #2a3340;border-radius:10px;padding:1rem 1.25rem;
 min-width:150px}
.card .k{color:#8fa0b3;font-size:.8rem;text-transform:uppercase;letter-spacing:.04em}
.card .v{font-size:1.6rem;font-weight:600;margin-top:.2rem}
.delta.pos{color:#f0a35e}.delta.neg{color:#6fb2ff}
figure{margin:0;background:#0c1015;border:1px solid #2a3340;border-radius:10px;padding:1rem}
svg .grid{stroke:#243040;stroke-width:1}
svg .up{fill:#f0a35e}svg .down{fill:#6fb2ff}
svg .refline{fill:none;stroke:#8fa0b3;stroke-width:2}
svg .mixline{fill:none;stroke:#f0a35e;stroke-width:2}
svg text{fill:#8fa0b3}svg .ytick,svg .xtick{font-size:11px}svg .axis{font-size:11px}
.legend span{display:inline-block;margin-right:1.25rem;color:#8fa0b3;font-size:.85rem}
.dot{display:inline-block;width:10px;height:10px;border-radius:2px;margin-right:.35rem;
 vertical-align:middle}
.kick{background:#161b22;border:1px solid #2a3340;border-radius:10px;padding:1rem 1.25rem}
@media (prefers-color-scheme:light){body{background:#f6f8fa;color:#1a2230}
 .card,.kick{background:#fff;border-color:#d5dde5}figure{background:#fff;border-color:#d5dde5}
 h2{border-color:#d5dde5}svg text{fill:#5a6b7d}}
"""


def build_html_report(rep: GapReport) -> str:
    delta = rep.lufs_delta
    dcls = "pos" if delta >= 0 else "neg"
    dword = "louder" if delta >= 0 else "quieter"
    esc = _html.escape
    kick_html = esc(rep.kick.suggestion) if rep.kick else "n/a"
    doc = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Reference Gap — {esc(rep.mix_name)} vs {esc(rep.ref_name)}</title>
<style>{_CSS}</style></head><body>
<h1>Reference Gap Report</h1>
<p class="sub">mix <strong>{esc(rep.mix_name)}</strong> vs reference
 <strong>{esc(rep.ref_name)}</strong></p>

<h2>Loudness (LUFS-I)</h2>
<div class="cards">
 <div class="card"><div class="k">Reference</div>
  <div class="v">{_fmt_lufs(rep.ref_lufs)}</div></div>
 <div class="card"><div class="k">Your mix</div>
  <div class="v">{_fmt_lufs(rep.mix_lufs)}</div></div>
 <div class="card"><div class="k">Delta</div>
  <div class="v delta {dcls}">{delta:+.1f} LU</div>
  <div class="k">your mix is {abs(delta):.1f} LU {dword}</div></div>
</div>

<h2>Spectral balance vs reference</h2>
<figure>{_svg_diff_bars(rep.spectral_diff)}</figure>

<h2>Stereo width by band</h2>
<figure>{_svg_width_curve(rep.ref_width, rep.mix_width)}
 <p class="legend"><span><i class="dot" style="background:#8fa0b3"></i>reference</span>
 <span><i class="dot" style="background:#f0a35e"></i>your mix</span></p></figure>

<h2>Kick fundamental &amp; tuning</h2>
<div class="kick">{kick_html}</div>

</body></html>"""
    return doc


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #
def run(args: argparse.Namespace) -> int:
    ref_path = Path(args.reference).expanduser()
    mix_path = Path(args.mix).expanduser()
    for p in (ref_path, mix_path):
        if not p.is_file():
            print(f"Error: {p} not found", file=sys.stderr)
            return 2
    ref, ref_sr = load_stereo(ref_path)
    mix, mix_sr = load_stereo(mix_path)
    rep = analyze_gap(
        ref, ref_sr, mix, mix_sr,
        key_root=args.key,
        ref_name=ref_path.name, mix_name=mix_path.name,
    )

    out = Path(args.out).expanduser() if args.out else mix_path.with_name(
        mix_path.stem + "_refgap.html")
    out.write_text(build_html_report(rep), encoding="utf-8")

    print(f"Reference gap: {mix_path.name} vs {ref_path.name}")
    print(f"  LUFS-I    ref {_fmt_lufs(rep.ref_lufs)}  |  mix "
          f"{_fmt_lufs(rep.mix_lufs)}  |  delta {rep.lufs_delta:+.1f} LU")
    if rep.kick:
        print(f"  Kick      {rep.kick.suggestion}")
    # biggest spectral gaps
    gaps = sorted(rep.spectral_diff.items(), key=lambda kv: -abs(kv[1]))[:3]
    gtxt = ", ".join(f"{int(fc)}Hz {v:+.1f}dB" for fc, v in gaps)
    print(f"  Spectrum  biggest gaps: {gtxt}")
    print(f"\nHTML report -> {out}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="reference_gap.py",
        description="Compare a reference track vs your mix render -> HTML report.",
    )
    p.add_argument("reference", help="Reference (mastered) audio file.")
    p.add_argument("mix", help="Your mix render.")
    p.add_argument("--out", default=None,
                   help="Output HTML path (default: <mix>_refgap.html).")
    p.add_argument("--key", default=None,
                   help="Track key root (e.g. C, F#, Am->A) for kick tuning advice.")
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
