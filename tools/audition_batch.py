# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "numpy>=1.26,<2.3",
#   "soundfile>=0.12",
#   "scipy>=1.11",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""Batch-audition a folder of WAVs; emit one compact line per preset + key sub-band metrics.

Adds low-band energy readouts (sub 25-60, low 60-120, body 120-250, lomid 250-500)
that the per-preset acceptance in the SOUND-PASS contract needs, plus the flag list.
Usage: audition_batch.py <dir> [--ref dark_techno|atmos_dnb]
"""
from __future__ import annotations
import sys, math, json
from pathlib import Path
import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
import audition as A


def band_energy_db(mono, sr, lo, hi):
    f, pxx = A._welch_psd(mono, sr)
    mask = (f >= lo) & (f < hi)
    if not np.any(mask):
        return -math.inf
    p = float(np.mean(pxx[mask])) * (hi - lo)
    return 10.0 * math.log10(p + A._EPS)


def main():
    d = Path(sys.argv[1])
    ref = "dark_techno"
    if "--ref" in sys.argv:
        ref = sys.argv[sys.argv.index("--ref") + 1]
    rows = []
    for wav in sorted(d.glob("*.wav")):
        data, sr = A.load_stereo(wav)
        rep = A.analyze_wav(data, sr, name=wav.stem, ref=ref)
        mono = A.to_mono(data)
        # absolute band energies (dB) then express relative to the loudest of the 4
        sub = band_energy_db(mono, sr, 25, 60)
        low = band_energy_db(mono, sr, 60, 120)
        body = band_energy_db(mono, sr, 120, 250)
        lomid = band_energy_db(mono, sr, 250, 500)
        hi = band_energy_db(mono, sr, 5000, 16000)
        top = max(sub, low, body, lomid)
        def rel(x):
            return x - top
        # dominant low region label
        labels = {"sub<60": sub, "60-120": low, "120-250": body, "250-500": lomid}
        dom = max(labels, key=labels.get)
        rows.append({
            "name": wav.stem,
            "lufs": None if not math.isfinite(rep.lufs_i) else round(rep.lufs_i, 1),
            "crest": round(rep.crest_db, 1),
            "peak": round(rep.peak_db, 1),
            "dom_low": dom,
            "sub": round(rel(sub), 1),
            "low": round(rel(low), 1),
            "body": round(rel(body), 1),
            "lomid": round(rel(lomid), 1),
            "hi_vs_top": round(hi - top, 1),
            "corr": round(rep.correlation, 2),
            "flags": rep.flags,
        })
    # print table
    print(f"# {d}  (ref={ref})   [sub/low/body/lomid are dB relative to loudest low region]")
    hdr = f"{'preset':28} {'LUFS':>5} {'crest':>5} {'peak':>5} {'dom':>8} {'sub':>5} {'low':>5} {'body':>5} {'lmid':>5} {'hi':>6}  flags"
    print(hdr)
    for r in rows:
        print(f"{r['name'][:28]:28} {str(r['lufs']):>5} {r['crest']:>5} {r['peak']:>5} "
              f"{r['dom_low']:>8} {r['sub']:>5} {r['low']:>5} {r['body']:>5} {r['lomid']:>5} "
              f"{r['hi_vs_top']:>6}  {','.join(r['flags']) if r['flags'] else '-'}")
    if "--json" in sys.argv:
        print(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
