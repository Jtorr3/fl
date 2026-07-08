# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""SOUND-PASS reverb/texture tail scanner. For every WAV in a dir, print the
metallic-ringing verdict (mode count + strongest mode freqs), tail spectral flatness,
stereo correlation, and producer flags. One `uv` invocation for the whole folder.

Usage: tail_scan.py <dir> [--ref atmos_dnb|dark_techno]
"""
from __future__ import annotations
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import audition as A


def main() -> None:
    d = Path(sys.argv[1])
    ref = "atmos_dnb"
    if "--ref" in sys.argv:
        ref = sys.argv[sys.argv.index("--ref") + 1]
    wavs = sorted(d.glob("*.wav"))
    if not wavs:
        print(f"(no wavs in {d})")
        return
    n_metal = 0
    tot_modes = 0
    print(f"{'preset':<34} {'modes':>5} {'flat':>6} {'corr':>6}  flags / mode-freqs")
    print("-" * 100)
    for wav in wavs:
        data, sr = A.load_stereo(wav)
        rep = A.analyze_wav(data, sr, name=wav.stem, ref=ref)
        r = rep.ringing
        nmodes = len(r.modes_hz) if r else 0
        metal = "METAL" if (r and r.metallic) else ""
        if r and r.metallic:
            n_metal += 1
        tot_modes += nmodes
        modes = ",".join(f"{m:.0f}" for m in (r.modes_hz[:6] if r else []))
        flat = f"{r.flatness:.3f}" if r else "  -  "
        flags = " ".join(rep.flags)
        extra = f"{metal} {flags} [{modes}]".strip()
        print(f"{wav.stem:<34} {nmodes:>5} {flat:>6} {rep.correlation:>6.2f}  {extra}")
    print("-" * 100)
    print(f"TOTAL: {len(wavs)} files | metallic presets: {n_metal} | total modes: {tot_modes}")


if __name__ == "__main__":
    main()
