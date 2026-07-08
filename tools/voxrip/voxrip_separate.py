# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#   "demucs>=4.0",
#   "torch>=2.1,<2.8",
#   "torchaudio>=2.1,<2.8",
#   "soundfile>=0.12",
# ]
#
# [tool.uv.sources]
# torch = [{ index = "pytorch-cpu" }]
# torchaudio = [{ index = "pytorch-cpu" }]
#
# [[tool.uv.index]]
# name = "pytorch-cpu"
# url = "https://download.pytorch.org/whl/cpu"
# explicit = true
# ///
"""voxrip_separate.py -- out-of-process demucs stem separation for voxrip (W9).

Kept SEPARATE from voxrip.py so torch (a ~200 MB wheel) lives only in demucs's
own uv env and never touches the light analysis/conform script or its offline
test gate. The `[tool.uv]` block above pins torch/torchaudio to PyTorch's CPU
index (`download.pytorch.org/whl/cpu`) so `uv run` never pulls a CUDA build --
CPU is fine for an offline tool (slow is acceptable). torchaudio is capped at
<2.8 because 2.8+ routes `torchaudio.load` through torchcodec (which needs a
system FFmpeg); 2.7.x keeps demucs's classic soundfile/sox loader working
unattended on Windows.

Usage (driven by voxrip.py):
    uv run --python 3.12 voxrip_separate.py <input-song> <output-root>

Produces <output-root>/htdemucs/<track>/{vocals,no_vocals}.wav via
`--two-stems vocals` (vocals + the accompaniment = mix minus vocals).
"""
from __future__ import annotations

import runpy
import sys


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: voxrip_separate.py <input-song> <output-root>", file=sys.stderr)
        return 2
    input_song, out_root = sys.argv[1], sys.argv[2]
    # Re-exec demucs's CLI inside this resolved (CPU-torch) env.
    sys.argv = [
        "demucs",
        "-n", "htdemucs",
        "--two-stems", "vocals",
        "-o", out_root,
        input_song,
    ]
    runpy.run_module("demucs", run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
