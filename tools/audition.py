# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""audition.py — producer-relevant sound-quality analysis of Qeynos render WAVs.

The SOUND-PASS infrastructure tool (PRD §7 "SOUND-PASS", INFRA step 1). Per-plugin
sound-quality agents render every factory preset over genre-appropriate musical
sources (see `suite_core::testsig`) and run this to answer "would a producer keep
this in a real song?" — with numbers, not ears.

Reuses code/patterns from tools/reference_gap.py (pyloudnorm LUFS + welch 1/3-octave
work; numpy pinned <2.3 for wheel availability).

CLI:
  audition.py analyze <wav> [--sine-probe <freq>] [--ref dark_techno|atmos_dnb] [--json]
  audition.py compare <before.wav> <after.wav> [--sine-probe <freq>] [--ref ...] [--json]

Metrics per WAV (mono-safe, stereo-aware):
  1. LUFS-I + true peak (4x-oversampled) + crest factor (peak/RMS dB).
  2. 1/3-octave balance (mean-normalized) vs two genre reference curves, per-band
     deviation report.
  3. Producer flags: MUD / HARSH / BOXY / SUB_WEAK / SUB_HEAVY / DULL.
  4. Click / discontinuity detector (per-sample first-difference outliers vs a
     local 50 ms RMS floor; ignores first/last 20 ms).
  5. DC offset per channel; silence/dropout detector (>80 ms sub -70 dBFS RMS
     inside otherwise-active audio).
  6. Metallic-ringing detector on the tail (last 40%): spectral flatness in
     500 Hz-6 kHz + narrowband modes >12 dB over the local median.
  7. THD character on --sine-probe: harmonic amplitudes rel. fundamental, odd/even
     ratio, inharmonic residual (aliasing indicator).
  8. Stereo: inter-channel correlation + side/mid energy by band.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np

# --------------------------------------------------------------------------- #
# 1/3-octave band centers (ISO 266 nominal), 20 Hz .. 20 kHz  (from reference_gap)
# --------------------------------------------------------------------------- #
THIRD_OCTAVE_CENTERS: tuple[float, ...] = (
    20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630,
    800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000,
    12500, 16000, 20000,
)
_BAND_RATIO = 2.0 ** (1.0 / 6.0)  # half a 1/3-octave, for band edges
_EPS = 1e-12

# numpy>=2.0 renamed trapz -> trapezoid (trapz deprecated); 1.26 only has trapz.
_trapz = np.trapezoid if hasattr(np, "trapezoid") else np.trapz

_NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


# --------------------------------------------------------------------------- #
# Audio IO
# --------------------------------------------------------------------------- #
def load_stereo(path: Path, max_seconds: float = 120.0) -> tuple[np.ndarray, int]:
    """Load audio as (N, C) float32 with 1 or 2 channels. >2 chans -> first two."""
    import soundfile as sf  # noqa: WPS433
    y, sr = sf.read(str(path), dtype="float32", always_2d=True)
    if y.shape[1] > 2:
        y = y[:, :2]
    if max_seconds and len(y) > int(max_seconds * sr):
        y = y[: int(max_seconds * sr)]
    return np.ascontiguousarray(y, dtype=np.float32), int(sr)


def to_mono(data: np.ndarray) -> np.ndarray:
    if data.ndim == 1:
        return data
    return data.mean(axis=1)


def _as_2d(data: np.ndarray) -> np.ndarray:
    return data if data.ndim == 2 else data[:, None]


# --------------------------------------------------------------------------- #
# 1. Loudness / peak / crest
# --------------------------------------------------------------------------- #
def measure_lufs(data: np.ndarray, sr: int) -> float:
    """Integrated loudness (LUFS-I) via pyloudnorm. -inf for silence/too-short."""
    import pyloudnorm as pyln  # noqa: WPS433
    meter = pyln.Meter(sr)
    x = _as_2d(data)
    try:
        return float(meter.integrated_loudness(x))
    except Exception:
        return float("-inf")


def measure_true_peak_db(data: np.ndarray, sr: int, oversample: int = 4) -> float:
    """4x-oversampled inter-sample true-peak estimate, dBTP."""
    from scipy import signal  # noqa: WPS433
    x = _as_2d(data)
    peak = 0.0
    for ch in range(x.shape[1]):
        up = signal.resample_poly(x[:, ch].astype(np.float64), oversample, 1)
        peak = max(peak, float(np.max(np.abs(up))) if len(up) else 0.0)
    # never report below the raw sample peak (guards resample ringing underestimate)
    peak = max(peak, float(np.max(np.abs(x))) if x.size else 0.0)
    return 20.0 * math.log10(peak + _EPS)


def measure_crest_db(mono: np.ndarray) -> tuple[float, float, float]:
    """(crest_dB, peak_dB, rms_dB) using the sample peak."""
    if mono.size == 0:
        return (0.0, -math.inf, -math.inf)
    peak = float(np.max(np.abs(mono)))
    rms = float(np.sqrt(np.mean(mono.astype(np.float64) ** 2)))
    peak_db = 20.0 * math.log10(peak + _EPS)
    rms_db = 20.0 * math.log10(rms + _EPS)
    return (peak_db - rms_db, peak_db, rms_db)


# --------------------------------------------------------------------------- #
# 2. Spectrum (1/3-octave), mean-normalized, + genre reference curves
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
    """Per-band power in dB (10*log10 of PSD integrated over each 1/3-oct band)."""
    f, pxx = _welch_psd(mono, sr)
    out: dict[float, float] = {}
    for fc in THIRD_OCTAVE_CENTERS:
        lo, hi = fc / _BAND_RATIO, fc * _BAND_RATIO
        mask = (f >= lo) & (f < hi)
        # Mean PSD over the band x bandwidth. Robust to low bands that catch zero or
        # one Welch bin (narrower than the bin spacing) — those would otherwise be
        # -inf holes that corrupt the mean-normalization; sample the PSD at the band
        # center instead.
        if np.any(mask):
            psd_band = float(np.mean(pxx[mask]))
        else:
            psd_band = float(np.interp(fc, f, pxx))
        power = psd_band * (hi - lo)
        out[fc] = 10.0 * math.log10(power + _EPS)
    return out


def normalize_bands(bands: dict[float, float]) -> dict[float, float]:
    """Subtract the mean dB so comparisons reflect balance, not overall level."""
    vals = [v for v in bands.values() if math.isfinite(v)]
    mean = sum(vals) / len(vals) if vals else 0.0
    return {k: v - mean for k, v in bands.items()}


def _build_curve(anchors: dict[float, float]) -> dict[float, float]:
    """Interpolate anchor (freq->dB) points in log-freq across the 1/3-oct grid,
    then mean-normalize so it is directly comparable to a normalized measurement."""
    afreqs = np.array(sorted(anchors), dtype=np.float64)
    advals = np.array([anchors[f] for f in sorted(anchors)], dtype=np.float64)
    logf = np.log10(afreqs)
    raw: dict[float, float] = {}
    for fc in THIRD_OCTAVE_CENTERS:
        raw[fc] = float(np.interp(math.log10(fc), logf, advals))
    return normalize_bands(raw)


# Dark-techno: strong 40-90 Hz, controlled 200-500, present-but-not-harsh 2-5k,
# gently rolled top. (mean-normalized inside _build_curve)
DARK_TECHNO: dict[float, float] = _build_curve({
    20: 0.0, 40: 4.0, 63: 5.0, 80: 4.0, 125: 1.0, 200: -1.0, 315: -2.0,
    500: -2.0, 800: -1.0, 1250: 0.0, 2000: 1.0, 3150: 1.5, 5000: 1.0,
    8000: -2.0, 12500: -4.0, 20000: -6.0,
})

# Atmospheric d'n'b / breakcore: sub weight (25-60), scooped low-mids (200-600),
# open airy highs.
ATMOS_DNB: dict[float, float] = _build_curve({
    20: 2.0, 31.5: 4.0, 50: 5.0, 80: 2.0, 125: -1.0, 200: -3.0, 315: -4.0,
    500: -3.0, 800: -1.0, 1250: 0.0, 2000: 1.0, 3150: 2.0, 5000: 3.0,
    8000: 4.0, 12500: 4.0, 20000: 3.0,
})

REFERENCE_CURVES: dict[str, dict[float, float]] = {
    "dark_techno": DARK_TECHNO,
    "atmos_dnb": ATMOS_DNB,
}


def band_deviation(norm: dict[float, float], ref: dict[float, float]) -> dict[float, float]:
    """measured_norm - reference_norm per band (positive = more energy than ref)."""
    return {fc: norm[fc] - ref[fc] for fc in THIRD_OCTAVE_CENTERS}


def _band_range_mean(dev: dict[float, float], lo: float, hi: float) -> float:
    vals = [dev[fc] for fc in THIRD_OCTAVE_CENTERS if lo <= fc <= hi]
    return sum(vals) / len(vals) if vals else 0.0


def is_broadband(norm: dict[float, float], within_db: float = 18.0, min_bands: int = 12) -> bool:
    """True if the signal spans the spectrum (>= min_bands within `within_db` of the
    loudest band). Guards the deficiency flags (SUB_WEAK/DULL) from firing on a lone
    test tone, whose balance is meaningless."""
    vals = [v for v in norm.values() if math.isfinite(v)]
    if not vals:
        return False
    top = max(vals)
    return sum(1 for v in vals if v >= top - within_db) >= min_bands


# --------------------------------------------------------------------------- #
# 3. Producer flags
# --------------------------------------------------------------------------- #
@dataclass
class ProducerFlag:
    name: str
    detail: str
    value: float


def producer_flags(
    norm: dict[float, float], ref: dict[float, float], thresh_db: float = 4.0,
    dull_collapse_db: float = 10.0,
) -> list[ProducerFlag]:
    """Producer balance-defect flags.

    All flags are gated behind is_broadband(): you cannot judge tonal balance on a
    lone test tone (its balance is meaningless), so a pure sine raises nothing.

    MUD / HARSH / BOXY are *absolute* excess flags — a band range sitting > thresh_db
    above the signal's own mean-normalized average is genuinely too hot, regardless
    of genre. SUB_WEAK / SUB_HEAVY are genre-relative (sub weight differs by genre),
    so they compare against the mean-normalized reference curve. DULL is an absolute
    high-frequency collapse (air bands far below the mix average).
    """
    flags: list[ProducerFlag] = []
    if not is_broadband(norm):
        return flags
    dev = band_deviation(norm, ref)

    mud = _band_range_mean(norm, 200.0, 500.0)
    if mud > thresh_db:
        flags.append(ProducerFlag("MUD", f"200-500 Hz +{mud:.1f} dB over mix avg", mud))

    harsh = _band_range_mean(norm, 2000.0, 5000.0)
    if harsh > thresh_db:
        flags.append(ProducerFlag("HARSH", f"2-5 kHz +{harsh:.1f} dB over mix avg", harsh))

    boxy = _band_range_mean(norm, 300.0, 800.0)
    if boxy > thresh_db:
        flags.append(ProducerFlag("BOXY", f"300-800 Hz +{boxy:.1f} dB over mix avg", boxy))

    sub = _band_range_mean(dev, 25.0, 60.0)
    if sub > thresh_db:
        flags.append(ProducerFlag("SUB_HEAVY", f"25-60 Hz +{sub:.1f} dB over ref", sub))
    elif sub < -thresh_db:
        flags.append(ProducerFlag("SUB_WEAK", f"25-60 Hz {sub:.1f} dB under ref", sub))

    dull = _band_range_mean(norm, 8000.0, 20000.0)
    if dull < -dull_collapse_db:
        flags.append(ProducerFlag("DULL", f">8 kHz {dull:.1f} dB below mix avg", dull))

    return flags


# --------------------------------------------------------------------------- #
# 4. Click / discontinuity detector
# --------------------------------------------------------------------------- #
@dataclass
class ClickReport:
    count: int
    worst_time_s: float
    worst_ratio: float


def detect_clicks(
    mono: np.ndarray, sr: int, thresh_ratio: float = 8.0, edge_ms: float = 20.0,
    window_ms: float = 50.0,
) -> ClickReport:
    """First-difference outliers vs a local (50 ms) RMS floor of the difference.

    A click is a single-sample jump far larger than the local sample-to-sample
    motion. Ignores the first/last `edge_ms`.
    """
    if mono.size < 8:
        return ClickReport(0, 0.0, 0.0)
    d = np.abs(np.diff(mono.astype(np.float64)))
    # local RMS floor of the difference via boxcar of d^2
    win = max(8, int(window_ms * 1e-3 * sr))
    kernel = np.ones(win) / win
    local_ms = np.convolve(d * d, kernel, mode="same")
    local_rms = np.sqrt(local_ms) + 1e-9
    ratio = d / local_rms
    edge = int(edge_ms * 1e-3 * sr)
    lo, hi = edge, max(edge, len(ratio) - edge)
    if hi <= lo:
        return ClickReport(0, 0.0, 0.0)
    span = ratio[lo:hi]
    hits = np.where(span > thresh_ratio)[0]
    if hits.size == 0:
        return ClickReport(0, 0.0, 0.0)
    worst_local = int(hits[int(np.argmax(span[hits]))])
    worst_idx = worst_local + lo
    return ClickReport(int(hits.size), worst_idx / sr, float(span[worst_local]))


# --------------------------------------------------------------------------- #
# 5. DC offset + silence/dropout
# --------------------------------------------------------------------------- #
def dc_offsets(data: np.ndarray) -> list[float]:
    x = _as_2d(data)
    return [float(np.mean(x[:, ch].astype(np.float64))) for ch in range(x.shape[1])]


@dataclass
class DropoutReport:
    count: int
    first_time_s: float
    longest_ms: float


def detect_dropouts(
    mono: np.ndarray, sr: int, floor_db: float = -70.0, min_ms: float = 80.0,
    hop_ms: float = 10.0, win_ms: float = 20.0,
) -> DropoutReport:
    """Runs >min_ms below floor_db RMS that sit *inside* otherwise-active audio
    (active content both before and after the run)."""
    if mono.size < int(win_ms * 1e-3 * sr):
        return DropoutReport(0, 0.0, 0.0)
    hop = max(1, int(hop_ms * 1e-3 * sr))
    win = max(hop, int(win_ms * 1e-3 * sr))
    x = mono.astype(np.float64)
    n_frames = 1 + (len(x) - win) // hop
    if n_frames <= 0:
        return DropoutReport(0, 0.0, 0.0)
    rms_db = np.empty(n_frames)
    for i in range(n_frames):
        seg = x[i * hop: i * hop + win]
        rms_db[i] = 10.0 * math.log10(float(np.mean(seg * seg)) + _EPS)
    active = rms_db >= floor_db
    if not active.any():
        return DropoutReport(0, 0.0, 0.0)
    first_active = int(np.argmax(active))
    last_active = n_frames - 1 - int(np.argmax(active[::-1]))
    min_frames = max(1, int(math.ceil(min_ms / hop_ms)))
    count = 0
    first_time = 0.0
    longest = 0.0
    i = first_active
    while i <= last_active:
        if not active[i]:
            j = i
            while j <= last_active and not active[j]:
                j += 1
            run = j - i
            # inside active audio: bounded by active frames on both sides
            if run >= min_frames and i > first_active and j <= last_active:
                count += 1
                if count == 1:
                    first_time = (i * hop) / sr
                longest = max(longest, run * hop_ms)
            i = j
        else:
            i += 1
    return DropoutReport(count, first_time, longest)


# --------------------------------------------------------------------------- #
# 6. Metallic-ringing detector (tail)
# --------------------------------------------------------------------------- #
@dataclass
class RingingReport:
    flatness: float
    modes_hz: list[float]

    @property
    def metallic(self) -> bool:
        return len(self.modes_hz) >= 3


def detect_ringing_modes(
    mono: np.ndarray, sr: int, tail_frac: float = 0.40,
    lo_hz: float = 500.0, hi_hz: float = 6000.0, prom_db: float = 12.0,
) -> RingingReport:
    """On the last `tail_frac` of the file: spectral flatness in [lo,hi] + narrowband
    modes >prom_db above the local median (comb / metallic-FDN symptom)."""
    n = mono.size
    if n < 256:
        return RingingReport(1.0, [])
    tail = mono[int(n * (1.0 - tail_frac)):].astype(np.float64)
    if tail.size < 256 or float(np.max(np.abs(tail))) < 1e-7:
        return RingingReport(1.0, [])
    f, pxx = _welch_psd(tail, sr, nperseg=min(len(tail), 16384))
    band = (f >= lo_hz) & (f <= hi_hz)
    if band.sum() < 8:
        return RingingReport(1.0, [])
    p = pxx[band]
    fb = f[band]
    # spectral flatness = geomean / mean
    logp = np.log(p + _EPS)
    flatness = float(math.exp(float(np.mean(logp))) / (float(np.mean(p)) + _EPS))
    pdb = 10.0 * np.log10(p + _EPS)
    df = f[1] - f[0]
    half = max(2, int(round(300.0 / df)))  # ~ +/-300 Hz local-median window
    modes: list[float] = []
    for i in range(1, len(pdb) - 1):
        a, b = max(0, i - half), min(len(pdb), i + half + 1)
        med = float(np.median(pdb[a:b]))
        if pdb[i] - med > prom_db and pdb[i] >= pdb[i - 1] and pdb[i] > pdb[i + 1]:
            modes.append(float(fb[i]))
    # de-duplicate near-adjacent modes (< 40 Hz apart)
    merged: list[float] = []
    for m in modes:
        if not merged or (m - merged[-1]) > 40.0:
            merged.append(m)
    return RingingReport(flatness, merged)


# --------------------------------------------------------------------------- #
# 7. THD character (sine probe)
# --------------------------------------------------------------------------- #
@dataclass
class ThdReport:
    f0: float
    thd_db: float
    harmonics_db: list[tuple[int, float]]  # (k, dB rel fundamental)
    odd_even_ratio: float
    inharmonic_db: float                   # worst inharmonic bin, dB rel fundamental
    aliasing: bool                         # inharmonic residual > -60 dB rel fund

    def to_dict(self) -> dict[str, Any]:
        return {
            "f0": round(self.f0, 3),
            "thd_db": round(self.thd_db, 2),
            "harmonics_db": [[k, round(v, 2)] for k, v in self.harmonics_db],
            "odd_even_ratio_db": round(self.odd_even_ratio, 2),
            "inharmonic_db": round(self.inharmonic_db, 2),
            "aliasing": self.aliasing,
        }


def thd_analysis(
    mono: np.ndarray, sr: int, probe_hz: float, alias_thresh_db: float = -60.0,
) -> ThdReport:
    """Harmonic amplitudes rel. fundamental, odd/even ratio, and inharmonic residual.

    Inharmonic energy above `alias_thresh_db` rel. the fundamental flags aliasing
    (e.g. bitcrush/quantization foldback), which harmonic distortion alone won't.
    """
    from scipy import signal  # noqa: WPS433
    x = mono.astype(np.float64)
    n = x.size
    if n < 1024 or probe_hz <= 0:
        return ThdReport(probe_hz, -math.inf, [], 0.0, -math.inf, False)
    w = signal.windows.hann(n)
    # coherent-gain compensation not needed (all ratios rel. fundamental)
    spec = np.abs(np.fft.rfft(x * w))
    freqs = np.fft.rfftfreq(n, 1.0 / sr)
    df = freqs[1] - freqs[0]
    excl = max(3, int(round(1.5 / df)))  # +/- bins to attribute to a peak

    def peak_near(target: float) -> tuple[int, float]:
        k = int(round(target / df))
        a, b = max(0, k - excl), min(len(spec), k + excl + 1)
        if b <= a:
            return (k, 0.0)
        j = a + int(np.argmax(spec[a:b]))
        return (j, float(spec[j]))

    f0_bin, a1 = peak_near(probe_hz)
    f0 = float(freqs[f0_bin]) if f0_bin < len(freqs) else probe_hz
    a1 = max(a1, _EPS)

    nyq = sr * 0.5
    harmonics: list[tuple[int, float]] = []
    claimed = np.zeros(len(spec), dtype=bool)
    for j in range(max(0, f0_bin - excl), min(len(spec), f0_bin + excl + 1)):
        claimed[j] = True
    harm_energy_odd = 0.0
    harm_energy_even = 0.0
    total_harm_sq = 0.0
    k = 2
    while k * f0 < 0.95 * nyq and k <= 40:
        j, ak = peak_near(k * f0)
        for jj in range(max(0, j - excl), min(len(spec), j + excl + 1)):
            claimed[jj] = True
        rel_db = 20.0 * math.log10(ak / a1 + _EPS)
        harmonics.append((k, rel_db))
        total_harm_sq += (ak / a1) ** 2
        if k % 2 == 0:
            harm_energy_even += (ak / a1) ** 2
        else:
            harm_energy_odd += (ak / a1) ** 2
        k += 1

    thd_db = 10.0 * math.log10(total_harm_sq + _EPS)
    odd_even = 10.0 * math.log10((harm_energy_odd + _EPS) / (harm_energy_even + _EPS))

    # inharmonic residual: strongest un-claimed bin above a low-freq guard
    guard = freqs >= (f0 * 0.5)
    resid = spec.copy()
    resid[claimed] = 0.0
    resid[~guard] = 0.0
    worst = float(np.max(resid)) if resid.size else 0.0
    inharm_db = 20.0 * math.log10(worst / a1 + _EPS)
    aliasing = inharm_db > alias_thresh_db
    return ThdReport(f0, thd_db, harmonics, odd_even, inharm_db, aliasing)


# --------------------------------------------------------------------------- #
# 8. Stereo
# --------------------------------------------------------------------------- #
def inter_channel_correlation(data: np.ndarray) -> float:
    if data.ndim == 1 or data.shape[1] < 2:
        return 1.0
    left = data[:, 0].astype(np.float64)
    right = data[:, 1].astype(np.float64)
    if np.std(left) < 1e-9 or np.std(right) < 1e-9:
        return 1.0
    return float(np.corrcoef(left, right)[0, 1])


def stereo_width_by_band(data: np.ndarray, sr: int) -> dict[float, float]:
    """Side/(mid+side) energy fraction per 1/3-octave band (from reference_gap)."""
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
# Analysis bundle
# --------------------------------------------------------------------------- #
@dataclass
class AuditionReport:
    name: str
    sr: int
    channels: int
    lufs_i: float
    true_peak_db: float
    crest_db: float
    peak_db: float
    rms_db: float
    ref_name: str
    bands_norm: dict[float, float] = field(default_factory=dict)
    deviation: dict[float, float] = field(default_factory=dict)
    dev_dark: dict[float, float] = field(default_factory=dict)
    dev_dnb: dict[float, float] = field(default_factory=dict)
    prod_flags: list[ProducerFlag] = field(default_factory=list)
    click: ClickReport | None = None
    dc: list[float] = field(default_factory=list)
    dropout: DropoutReport | None = None
    ringing: RingingReport | None = None
    thd: ThdReport | None = None
    correlation: float = 1.0
    width: dict[float, float] = field(default_factory=dict)

    @property
    def flags(self) -> list[str]:
        """The single flat list of all raised flags (defect + tonal)."""
        out = [f.name for f in self.prod_flags]
        if self.click and self.click.count > 0:
            out.append("CLICK")
        if self.dropout and self.dropout.count > 0:
            out.append("DROPOUT")
        if any(abs(v) > 1e-3 for v in self.dc):
            out.append("DC_OFFSET")
        if self.ringing and self.ringing.metallic:
            out.append("METALLIC_RINGING")
        if self.true_peak_db > 0.0:
            out.append("TRUE_PEAK_OVER")
        if self.thd and self.thd.aliasing:
            out.append("ALIASING")
        return out

    @property
    def deviation_sum(self) -> float:
        return float(sum(abs(v) for v in self.deviation.values()))

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "sr": self.sr,
            "channels": self.channels,
            "lufs_i": None if not math.isfinite(self.lufs_i) else round(self.lufs_i, 2),
            "true_peak_db": round(self.true_peak_db, 2),
            "crest_db": round(self.crest_db, 2),
            "peak_db": round(self.peak_db, 2),
            "rms_db": round(self.rms_db, 2),
            "ref": self.ref_name,
            "deviation_sum": round(self.deviation_sum, 1),
            "deviation": {str(k): round(v, 2) for k, v in self.deviation.items()},
            "flags": self.flags,
            "producer_flags": [
                {"name": f.name, "detail": f.detail, "value": round(f.value, 2)}
                for f in self.prod_flags
            ],
            "click": None if not self.click else {
                "count": self.click.count,
                "worst_time_s": round(self.click.worst_time_s, 4),
                "worst_ratio": round(self.click.worst_ratio, 2),
            },
            "dc": [round(v, 6) for v in self.dc],
            "dropout": None if not self.dropout else {
                "count": self.dropout.count,
                "first_time_s": round(self.dropout.first_time_s, 4),
                "longest_ms": round(self.dropout.longest_ms, 1),
            },
            "ringing": None if not self.ringing else {
                "flatness": round(self.ringing.flatness, 4),
                "metallic": self.ringing.metallic,
                "modes_hz": [round(m, 1) for m in self.ringing.modes_hz],
            },
            "thd": None if not self.thd else self.thd.to_dict(),
            "correlation": round(self.correlation, 4),
        }


def analyze_wav(
    data: np.ndarray, sr: int, name: str = "wav",
    ref: str = "dark_techno", probe_hz: float | None = None,
) -> AuditionReport:
    ref_curve = REFERENCE_CURVES.get(ref, DARK_TECHNO)
    mono = to_mono(data)
    bands = third_octave_bands(mono, sr)
    norm = normalize_bands(bands)
    crest, peak_db, rms_db = measure_crest_db(mono)
    rep = AuditionReport(
        name=name,
        sr=sr,
        channels=_as_2d(data).shape[1],
        lufs_i=measure_lufs(data, sr),
        true_peak_db=measure_true_peak_db(data, sr),
        crest_db=crest,
        peak_db=peak_db,
        rms_db=rms_db,
        ref_name=ref,
        bands_norm=norm,
        deviation=band_deviation(norm, ref_curve),
        dev_dark=band_deviation(norm, DARK_TECHNO),
        dev_dnb=band_deviation(norm, ATMOS_DNB),
        prod_flags=producer_flags(norm, ref_curve),
        click=detect_clicks(mono, sr),
        dc=dc_offsets(data),
        dropout=detect_dropouts(mono, sr),
        ringing=detect_ringing_modes(mono, sr),
        thd=thd_analysis(mono, sr, probe_hz) if probe_hz else None,
        correlation=inter_channel_correlation(data),
        width=stereo_width_by_band(data, sr),
    )
    return rep


# --------------------------------------------------------------------------- #
# compare
# --------------------------------------------------------------------------- #
@dataclass
class CompareReport:
    before: AuditionReport
    after: AuditionReport

    @property
    def verdict(self) -> str:
        b, a = self.before, self.after
        nb, na = set(b.flags), set(a.flags)
        fixed = nb - na
        introduced = na - nb
        dev_delta = a.deviation_sum - b.deviation_sum  # negative = closer to ref
        better = len(fixed) + (1 if dev_delta < -1.0 else 0)
        worse = len(introduced) + (1 if dev_delta > 1.0 else 0)
        if better > 0 and worse == 0:
            return "IMPROVED"
        if worse > 0 and better == 0:
            return "REGRESSED"
        if better == 0 and worse == 0:
            return "UNCHANGED"
        return "MIXED"

    def deltas(self) -> dict[str, Any]:
        b, a = self.before, self.after
        def d(x: float, y: float) -> float:
            if not (math.isfinite(x) and math.isfinite(y)):
                return float("nan")
            return round(y - x, 2)
        return {
            "lufs_i": d(b.lufs_i, a.lufs_i),
            "true_peak_db": d(b.true_peak_db, a.true_peak_db),
            "crest_db": d(b.crest_db, a.crest_db),
            "deviation_sum": round(a.deviation_sum - b.deviation_sum, 1),
            "correlation": round(a.correlation - b.correlation, 4),
            "click_count": (a.click.count if a.click else 0) - (b.click.count if b.click else 0),
            "flags_fixed": sorted(set(b.flags) - set(a.flags)),
            "flags_introduced": sorted(set(a.flags) - set(b.flags)),
        }

    def to_dict(self) -> dict[str, Any]:
        return {
            "verdict": self.verdict,
            "before": self.before.to_dict(),
            "after": self.after.to_dict(),
            "deltas": self.deltas(),
        }


# --------------------------------------------------------------------------- #
# Text rendering
# --------------------------------------------------------------------------- #
def _fmt_db(v: float) -> str:
    return "-inf" if not math.isfinite(v) else f"{v:+.1f}"


def print_report(rep: AuditionReport) -> None:
    print(f"AUDITION  {rep.name}  ({rep.sr} Hz, {rep.channels} ch, ref={rep.ref_name})")
    lufs = "-inf" if not math.isfinite(rep.lufs_i) else f"{rep.lufs_i:.1f}"
    print(f"  Loudness   LUFS-I {lufs}   true-peak {rep.true_peak_db:+.2f} dBTP   "
          f"crest {rep.crest_db:.1f} dB  (peak {rep.peak_db:.1f} / rms {rep.rms_db:.1f})")
    print(f"  Balance    deviation-sum {rep.deviation_sum:.1f} dB vs {rep.ref_name}")
    worst = sorted(rep.deviation.items(), key=lambda kv: -abs(kv[1]))[:4]
    print("             worst bands: " +
          ", ".join(f"{int(fc)}Hz {v:+.1f}" for fc, v in worst))
    if rep.prod_flags:
        for f in rep.prod_flags:
            print(f"  ! {f.name:<12} {f.detail}")
    if rep.click and rep.click.count:
        print(f"  ! CLICK        {rep.click.count} outlier(s), worst @ "
              f"{rep.click.worst_time_s:.3f}s (ratio {rep.click.worst_ratio:.1f})")
    if any(abs(v) > 1e-3 for v in rep.dc):
        print("  ! DC_OFFSET    " + ", ".join(f"ch{ i } {v:+.5f}" for i, v in enumerate(rep.dc)))
    if rep.dropout and rep.dropout.count:
        print(f"  ! DROPOUT      {rep.dropout.count} run(s), first @ "
              f"{rep.dropout.first_time_s:.3f}s, longest {rep.dropout.longest_ms:.0f}ms")
    if rep.ringing and rep.ringing.metallic:
        modes = ", ".join(f"{m:.0f}" for m in rep.ringing.modes_hz[:8])
        print(f"  ! METALLIC     flatness {rep.ringing.flatness:.3f}, modes @ {modes} Hz")
    if rep.true_peak_db > 0.0:
        print(f"  ! TRUE_PEAK    {rep.true_peak_db:+.2f} dBTP over 0")
    if rep.thd:
        t = rep.thd
        harm = ", ".join(f"h{k}:{v:.0f}" for k, v in t.harmonics_db[:6])
        print(f"  THD        f0 {t.f0:.1f} Hz   THD {t.thd_db:.1f} dB   "
              f"odd/even {t.odd_even_ratio:+.1f} dB   inharm {t.inharmonic_db:.1f} dB"
              f"{'  [ALIASING]' if t.aliasing else ''}")
        print(f"             harmonics {harm}")
    if rep.channels >= 2:
        print(f"  Stereo     inter-channel corr {rep.correlation:+.3f}")
    if not rep.flags:
        print("  = no flags")


def print_compare(cmp: CompareReport) -> None:
    print(f"COMPARE  {cmp.before.name}  ->  {cmp.after.name}")
    print()
    print("--- BEFORE ---")
    print_report(cmp.before)
    print()
    print("--- AFTER ---")
    print_report(cmp.after)
    print()
    d = cmp.deltas()
    print("--- DELTAS (after - before) ---")
    print(f"  LUFS-I        {d['lufs_i']:+}")
    print(f"  true-peak     {d['true_peak_db']:+} dB")
    print(f"  crest         {d['crest_db']:+} dB")
    print(f"  deviation-sum {d['deviation_sum']:+} dB")
    print(f"  correlation   {d['correlation']:+}")
    print(f"  click-count   {d['click_count']:+}")
    if d["flags_fixed"]:
        print(f"  flags fixed:      {', '.join(d['flags_fixed'])}")
    if d["flags_introduced"]:
        print(f"  flags introduced: {', '.join(d['flags_introduced'])}")
    print()
    print(f"VERDICT: {cmp.verdict}")


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #
def _cmd_analyze(args: argparse.Namespace) -> int:
    path = Path(args.wav).expanduser()
    if not path.is_file():
        print(f"Error: {path} not found", file=sys.stderr)
        return 2
    data, sr = load_stereo(path)
    rep = analyze_wav(data, sr, name=path.name, ref=args.ref, probe_hz=args.sine_probe)
    if args.json:
        print(json.dumps(rep.to_dict(), indent=2))
    else:
        print_report(rep)
    return 0


def _cmd_compare(args: argparse.Namespace) -> int:
    bp = Path(args.before).expanduser()
    ap = Path(args.after).expanduser()
    for p in (bp, ap):
        if not p.is_file():
            print(f"Error: {p} not found", file=sys.stderr)
            return 2
    bd, bsr = load_stereo(bp)
    ad, asr = load_stereo(ap)
    b = analyze_wav(bd, bsr, name=bp.name, ref=args.ref, probe_hz=args.sine_probe)
    a = analyze_wav(ad, asr, name=ap.name, ref=args.ref, probe_hz=args.sine_probe)
    cmp = CompareReport(b, a)
    if args.json:
        print(json.dumps(cmp.to_dict(), indent=2))
    else:
        print_compare(cmp)
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="audition.py",
        description="Producer-relevant sound-quality analysis of Qeynos render WAVs.",
    )
    sub = p.add_subparsers(dest="cmd", required=True)

    pa = sub.add_parser("analyze", help="Analyze a single WAV.")
    pa.add_argument("wav")
    pa.add_argument("--sine-probe", type=float, default=None,
                    help="Probe frequency (Hz) for THD/aliasing analysis.")
    pa.add_argument("--ref", choices=sorted(REFERENCE_CURVES), default="dark_techno",
                    help="Genre reference curve for balance flags (default dark_techno).")
    pa.add_argument("--json", action="store_true", help="Emit JSON.")
    pa.set_defaults(func=_cmd_analyze)

    pc = sub.add_parser("compare", help="Compare before.wav vs after.wav.")
    pc.add_argument("before")
    pc.add_argument("after")
    pc.add_argument("--sine-probe", type=float, default=None)
    pc.add_argument("--ref", choices=sorted(REFERENCE_CURVES), default="dark_techno")
    pc.add_argument("--json", action="store_true")
    pc.set_defaults(func=_cmd_compare)
    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
