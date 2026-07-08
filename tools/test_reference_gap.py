# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""Offline test gate for reference_gap.py (Qeynos W7).

Synthesizes signals (a known-LUFS pair, a kick at a known f0, a bright-boosted
mix, mono vs decorrelated stereo) and checks the analysis lands within tolerance,
plus that the HTML report is generated, self-contained (no CDN), and parseable by
the stdlib HTML parser. No network, no FL.

Run:  uv run --python 3.12 tools\\test_reference_gap.py
"""

from __future__ import annotations

import sys
from html.parser import HTMLParser
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import reference_gap as rg  # noqa: E402

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


def stereo(mono: np.ndarray) -> np.ndarray:
    return np.stack([mono, mono], axis=1).astype(np.float32)


def white(n: int, seed: int, amp: float = 0.1) -> np.ndarray:
    return (np.random.default_rng(seed).standard_normal(n) * amp).astype(np.float32)


# ---------------------------------------------------------------------------
# 1. nearest_note
# ---------------------------------------------------------------------------
def test_nearest_note() -> None:
    print("[nearest_note]")
    name, midi, cents = rg.nearest_note(440.0)
    check("440 Hz -> A4 @ 0c", name == "A4" and midi == 69 and abs(cents) < 0.1,
          f"{name} {midi} {cents}")
    name, _, _ = rg.nearest_note(261.63)
    check("261.63 Hz -> C4", name == "C4", name)
    _, _, cents = rg.nearest_note(448.0)  # slightly sharp of A4 (still nearest A4)
    check("448 Hz -> positive cents on A4", cents > 20, str(cents))


# ---------------------------------------------------------------------------
# 2. kick f0 + tuning
# ---------------------------------------------------------------------------
def test_kick() -> None:
    print("[kick f0]")
    dur = 0.8
    t = np.linspace(0, dur, int(SR * dur), endpoint=False)
    env = np.exp(-t * 8.0)
    kick = (np.sin(2 * np.pi * 50.0 * t) * env).astype(np.float32)
    f0 = rg.detect_kick_f0(kick, SR)
    check("kick f0 ~50 Hz", f0 is not None and abs(f0 - 50.0) <= 2.0, f"got {f0}")

    kt = rg.kick_tuning(kick, SR, key_root="C")
    check("tuning mentions note + key root",
          "Hz" in kt.suggestion and "key root C" in kt.suggestion, kt.suggestion)

    silence = np.zeros(int(SR * 0.5), dtype=np.float32)
    kt2 = rg.kick_tuning(silence, SR)
    check("silence -> no fundamental",
          kt2.f0 is None or "No clear" in kt2.suggestion, kt2.suggestion)


# ---------------------------------------------------------------------------
# 3. LUFS delta
# ---------------------------------------------------------------------------
def test_lufs() -> None:
    print("[lufs]")
    n = int(SR * 5)
    ref = stereo(white(n, 1, 0.1))
    mix = (ref * 2.0).astype(np.float32)  # +6.02 dB louder
    lr = rg.measure_lufs(ref, SR)
    lm = rg.measure_lufs(mix, SR)
    check("ref LUFS finite", np.isfinite(lr), str(lr))
    check("+6dB gain -> ~+6 LU delta", abs((lm - lr) - 6.02) < 0.5, f"delta {lm - lr:.2f}")


# ---------------------------------------------------------------------------
# 4. spectral diff (bright-boosted mix)
# ---------------------------------------------------------------------------
def test_spectral_diff() -> None:
    print("[spectral diff]")
    from scipy import signal
    n = int(SR * 4)
    base = white(n, 2, 0.1)
    hi_src = white(n, 3, 0.1)
    b, a = signal.butter(4, [5000 / (SR / 2), 9000 / (SR / 2)], btype="band")
    hi = signal.lfilter(b, a, hi_src).astype(np.float32)
    ref_bands = rg.third_octave_bands(base, SR)
    mix_bands = rg.third_octave_bands((base + 4.0 * hi).astype(np.float32), SR)
    diff = rg.spectral_diff(ref_bands, mix_bands)
    check("high band brighter than low",
          diff[6300] > diff[100], f"6.3k={diff[6300]:.1f} 100={diff[100]:.1f}")
    check("high band boost positive", diff[6300] > 1.0, f"{diff[6300]:.1f}")


# ---------------------------------------------------------------------------
# 5. stereo width
# ---------------------------------------------------------------------------
def test_width() -> None:
    print("[stereo width]")
    n = int(SR * 3)
    mono = stereo(white(n, 4, 0.1))
    w_mono = rg.stereo_width_by_band(mono, SR)
    check("mono -> ~0 width everywhere",
          max(w_mono.values()) < 0.02, f"max {max(w_mono.values()):.3f}")

    wide = np.stack([white(n, 5, 0.1), white(n, 6, 0.1)], axis=1).astype(np.float32)
    w_wide = rg.stereo_width_by_band(wide, SR)
    mids = [w_wide[fc] for fc in (250, 500, 1000, 2000)]
    check("decorrelated -> wide (~0.5)",
          min(mids) > 0.3, f"mids {[round(m,2) for m in mids]}")


# ---------------------------------------------------------------------------
# 6. HTML report: generated, self-contained, parseable
# ---------------------------------------------------------------------------
class _Parser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.tags = 0

    def handle_starttag(self, tag, attrs):
        self.tags += 1


def test_html() -> None:
    print("[html report]")
    n = int(SR * 3)
    ref = stereo(white(n, 7, 0.1))
    mix = np.stack([white(n, 8, 0.12), white(n, 9, 0.12)], axis=1).astype(np.float32)
    rep = rg.analyze_gap(ref, SR, mix, SR, key_root="C",
                         ref_name="ref.wav", mix_name="mymix.wav")
    doc = rg.build_html_report(rep)

    check("has doctype + svg", doc.lstrip().lower().startswith("<!doctype") and "<svg" in doc)
    for marker in ("Reference Gap", "LUFS", "Spectral balance", "Kick", "Stereo width"):
        check(f"contains {marker!r}", marker in doc)
    # self-contained: no external fetches
    low = doc.lower()
    check("no <script>", "<script" not in low)
    check("no <link>", "<link" not in low)
    check("no https/CDN", "https://" not in low and "cdn" not in low)
    # the only http:// is the SVG xml namespace (not a fetch)
    check("only-http is svg namespace",
          low.count("http://") == low.count("http://www.w3.org/2000/svg"))
    # parseable
    p = _Parser()
    try:
        p.feed(doc)
        parsed_ok = p.tags > 10
    except Exception as e:  # noqa: BLE001
        parsed_ok = False
        print(f"    parse error: {e}")
    check("HTML parses (stdlib)", parsed_ok, f"{p.tags} start tags")

    check("to_dict round-trips key fields",
          rep.to_dict()["lufs_delta"] == round(rep.mix_lufs - rep.ref_lufs, 2))


def main() -> int:
    for t in (test_nearest_note, test_kick, test_lufs, test_spectral_diff,
              test_width, test_html):
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
