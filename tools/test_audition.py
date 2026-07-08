# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""Offline test gate for audition.py (Qeynos SOUND-PASS infra).

Synthesizes fixtures and checks the analysis lands where a producer would expect.
No network, no FL, no external audio files.

  * clean sine                          -> no flags
  * sine + one hard sample step         -> click detected at that time
  * pink noise + a +8 dB 300 Hz bell    -> MUD flag
  * a decaying feedback-comb tail        -> ringing modes near the comb frequencies
  * sine through tanh                    -> odd-dominant THD, no inharmonics
  * sine @ 0.5 amp, 4-bit quantized      -> inharmonic residual flagged (aliasing)

Run:  uv run --python 3.12 tools\\test_audition.py
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import audition as au  # noqa: E402

_failures: list[str] = []
_passes = 0
SR = 48000


def check(name: str, cond: bool, detail: str = "") -> None:
    global _passes
    if cond:
        _passes += 1
        print(f"  ok   {name}")
    else:
        _failures.append(f"{name}: {detail}")
        print(f"  FAIL {name}  {detail}")


def sine(freq: float, amp: float, secs: float, sr: int = SR) -> np.ndarray:
    t = np.arange(int(secs * sr)) / sr
    return (amp * np.sin(2 * np.pi * freq * t)).astype(np.float32)


def stereo(mono: np.ndarray) -> np.ndarray:
    return np.stack([mono, mono], axis=1).astype(np.float32)


# ---------------------------------------------------------------------------
# 1. clean sine -> no flags
# ---------------------------------------------------------------------------
def test_clean_sine() -> None:
    print("[clean sine -> no flags]")
    # 220 Hz: below the 500 Hz-6 kHz ringing band, not in a mud/harsh range, and a
    # lone tone so the (gated) deficiency flags stay silent.
    x = sine(220.0, 0.5, 2.0)
    rep = au.analyze_wav(stereo(x), SR, name="clean_sine")
    check("no flags at all", rep.flags == [], f"got {rep.flags}")
    check("no clicks", rep.click.count == 0, str(rep.click))
    check("no ringing modes", not rep.ringing.metallic, str(rep.ringing.modes_hz))
    check("dc ~ 0", all(abs(v) < 1e-3 for v in rep.dc), str(rep.dc))
    check("true-peak under 0 dBTP", rep.true_peak_db < 0.5, str(rep.true_peak_db))
    check("crest ~ 3 dB (sine)", 2.0 < rep.crest_db < 4.5, str(rep.crest_db))


# ---------------------------------------------------------------------------
# 2. sine + one hard sample step -> click detected
# ---------------------------------------------------------------------------
def test_click() -> None:
    print("[sine + hard step -> click]")
    x = sine(220.0, 0.4, 1.0).copy()
    idx = len(x) // 2
    # a hard single-sample discontinuity
    x[idx:] += 0.6
    rep = au.analyze_wav(stereo(x), SR, name="click")
    check("click detected", rep.click.count >= 1, str(rep.click))
    check("worst click near mid", abs(rep.click.worst_time_s - 0.5) < 0.02,
          f"{rep.click.worst_time_s:.4f}s")
    check("CLICK flag present", "CLICK" in rep.flags, str(rep.flags))

    # control: same sine without the step -> no click
    clean = sine(220.0, 0.4, 1.0)
    rc = au.detect_clicks(clean, SR)
    check("clean sine -> no click", rc.count == 0, str(rc))


# ---------------------------------------------------------------------------
# 3. pink noise + a +8 dB 300 Hz bell -> MUD
# ---------------------------------------------------------------------------
def _pink(n: int, seed: int) -> np.ndarray:
    # White shaped to -3 dB/oct (1/sqrt(f) in frequency) so the 1/3-oct baseline is
    # flat; DC removed. This is the "shaped white noise" the fixture spec calls for.
    w = np.random.default_rng(seed).standard_normal(n)
    X = np.fft.rfft(w)
    f = np.fft.rfftfreq(n, 1.0 / SR)
    f[0] = f[1]
    X = X / np.sqrt(f)
    X[0] = 0.0  # kill DC
    y = np.fft.irfft(X, n=n)
    y = y - y.mean()
    return (y / (np.max(np.abs(y)) + 1e-9) * 0.2).astype(np.float32)


def test_mud() -> None:
    print("[pink + 300 Hz bell -> MUD]")
    from scipy import signal
    n = int(SR * 4)
    x = _pink(n, 11)
    # +8 dB low-mid bell centered at 300 Hz (broad Q — real mud is broadband)
    w0 = 300.0 / (SR / 2)
    Q = 0.7
    A = 10 ** (8.0 / 40.0)
    w0r = np.pi * w0
    alpha = np.sin(w0r) / (2 * Q)
    cw = np.cos(w0r)
    b = [1 + alpha * A, -2 * cw, 1 - alpha * A]
    a = [1 + alpha / A, -2 * cw, 1 - alpha / A]
    belled = signal.lfilter(b, a, x).astype(np.float32)
    belled = belled / (np.max(np.abs(belled)) + 1e-9) * 0.5
    rep = au.analyze_wav(stereo(belled), SR, name="mud", ref="dark_techno")
    check("MUD flag present", "MUD" in rep.flags, f"flags={rep.flags}")
    # control: the un-belled pink should not be MUD
    rep0 = au.analyze_wav(stereo(x / (np.max(np.abs(x)) + 1e-9) * 0.5), SR, name="flat")
    check("flat pink -> no MUD", "MUD" not in rep0.flags, f"flags={rep0.flags}")


# ---------------------------------------------------------------------------
# 4. decaying feedback-comb tail -> ringing modes near comb freqs
# ---------------------------------------------------------------------------
def test_ringing() -> None:
    print("[comb tail -> ringing modes]")
    n = int(SR * 2.0)
    # feedback comb: y[n] = x[n] + g*y[n-D]; D=96 -> resonances at k*500 Hz.
    # Excited continuously with low-level noise so the resonances persist through
    # the tail (a metallic FDN under signal) — the symptom the detector targets.
    D = 96
    g = 0.985
    rng = np.random.default_rng(5)
    x = (rng.standard_normal(n) * 0.05).astype(np.float64)
    y = np.zeros(n, dtype=np.float64)
    for i in range(n):
        y[i] = x[i] + (g * y[i - D] if i >= D else 0.0)
    y = (y / (np.max(np.abs(y)) + 1e-9) * 0.7).astype(np.float32)
    rep = au.analyze_wav(stereo(y), SR, name="comb")
    check("metallic ringing flagged", rep.ringing.metallic,
          f"modes={rep.ringing.modes_hz}")
    # comb resonances are multiples of SR/D = 500 Hz; at least one detected mode
    # should sit within 60 Hz of a 500 Hz multiple.
    spacing = SR / D
    near = [m for m in rep.ringing.modes_hz
            if abs(m - round(m / spacing) * spacing) < 60.0]
    check("modes align to comb spacing", len(near) >= 3,
          f"spacing={spacing:.0f} modes={[round(m) for m in rep.ringing.modes_hz]}")


# ---------------------------------------------------------------------------
# 5. sine through tanh -> odd-dominant THD, no inharmonics
# ---------------------------------------------------------------------------
def test_thd_tanh() -> None:
    print("[tanh -> odd THD, no aliasing]")
    f0 = 500.0
    x = np.tanh(3.0 * sine(f0, 0.9, 1.0).astype(np.float64)).astype(np.float32)
    rep = au.analyze_wav(stereo(x), SR, name="tanh", probe_hz=f0)
    t = rep.thd
    check("THD present", t.thd_db > -30.0, f"{t.thd_db:.1f} dB")
    check("odd-dominant", t.odd_even_ratio > 6.0, f"odd/even {t.odd_even_ratio:+.1f} dB")
    check("no aliasing flag", not t.aliasing, f"inharm {t.inharmonic_db:.1f} dB")
    check("ALIASING not in flags", "ALIASING" not in rep.flags, str(rep.flags))


# ---------------------------------------------------------------------------
# 6. sine @ 0.5 amp, 4-bit quantized -> inharmonic residual flagged
# ---------------------------------------------------------------------------
def test_quantize_aliasing() -> None:
    print("[4-bit quantize -> inharmonic residual]")
    f0 = 997.0  # non-round: quantization harmonics fold to inharmonic bins
    x = sine(f0, 0.5, 1.0).astype(np.float64)
    levels = 16.0  # 4-bit
    xq = (np.round(x * (levels / 2)) / (levels / 2)).astype(np.float32)
    rep = au.analyze_wav(stereo(xq), SR, name="quant4", probe_hz=f0)
    t = rep.thd
    check("inharmonic residual above -60 dB", t.inharmonic_db > -60.0,
          f"inharm {t.inharmonic_db:.1f} dB")
    check("aliasing flagged", t.aliasing, f"inharm {t.inharmonic_db:.1f} dB")
    check("ALIASING in flags", "ALIASING" in rep.flags, str(rep.flags))


# ---------------------------------------------------------------------------
# 7. compare verdict sanity
# ---------------------------------------------------------------------------
def test_compare() -> None:
    print("[compare verdict]")
    from scipy import signal
    n = int(SR * 3)
    rng = np.random.default_rng(21)
    base = (rng.standard_normal(n) * 0.1).astype(np.float32)
    # "before" has a harsh 3 kHz boost; "after" removes it -> IMPROVED
    w0 = 3000.0 / (SR / 2)
    b, a = signal.iirpeak(w0, 1.0)
    harsh = signal.lfilter([x * 3 for x in b], a, base).astype(np.float32)
    before = au.analyze_wav(stereo(harsh + base), SR, name="before")
    after = au.analyze_wav(stereo(base), SR, name="after")
    cmp = au.CompareReport(before, after)
    check("verdict is a known token",
          cmp.verdict in ("IMPROVED", "REGRESSED", "MIXED", "UNCHANGED"), cmp.verdict)
    d = cmp.deltas()
    check("deltas has lufs_i key", "lufs_i" in d, str(d.keys()))
    # round-trip json-able
    check("to_dict round-trips verdict",
          cmp.to_dict()["verdict"] == cmp.verdict)


def main() -> int:
    for t in (test_clean_sine, test_click, test_mud, test_ringing,
              test_thd_tanh, test_quantize_aliasing, test_compare):
        t()
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
