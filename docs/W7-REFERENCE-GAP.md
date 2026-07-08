# W7 — REFERENCE-GAP (`tools/reference_gap.py`)

Compare a professionally-mastered **reference** track against **your mix render**
and get an HTML report of the gaps that drive mixing decisions.

## What it measures

- **LUFS-I** integrated loudness (pyloudnorm) for both files + the delta ("your
  mix is X LU louder/quieter").
- **1/3-octave spectral balance** difference. Each spectrum is normalized to its
  own mean first, so the diff shows *tonal balance* (bright vs dull), not level.
  Bars up = your mix brighter than the reference in that band; down = duller.
- **Stereo width by band** — side/(mid+side) energy per 1/3-octave for both
  files (0 = mono, ~0.5 = wide).
- **Kick fundamental** — dominant low-band peak (30–150 Hz), parabolic-
  interpolated for sub-bin accuracy, mapped to the nearest note + cents, with a
  tuning suggestion. With `--key`, it also gives the semitone move to sit the
  kick on the track's key root.

## Output

A single **self-contained HTML report** — inline CSS + inline SVG charts, **no
external CSS/JS/CDN/fonts** (the only `http://` in the file is the SVG XML
namespace, which is not fetched). Opens offline in any browser, light or dark.

**Design decision — plots are pure inline SVG**, not matplotlib/PNG: keeps the
dependency set light (numpy/scipy/soundfile/pyloudnorm), makes the HTML truly
self-contained without base64-embedding a raster, and stays crisp at any zoom.

## Usage

```powershell
# Compare, writing <mix>_refgap.html next to the mix
uv run --python 3.12 tools\reference_gap.py "refs\pro_master.wav" "renders\my_mix.wav"

# Choose output path + give the track key for kick-tuning advice
uv run --python 3.12 tools\reference_gap.py ref.wav mix.wav --key C --out report.html
```

`--key` accepts a root note (`C`, `F#`, `Bb`) or a key like `Am` (root `A`).

## Notes

- Mono files are handled (width reads ~0). Stereo files with >2 channels are
  truncated to L/R. Analysis is capped at the first 60 s for speed.
- Both files are analyzed at their own sample rate (bands are frequency-based,
  so mismatched rates still compare correctly).

## Offline test gate

`uv run --python 3.12 tools\test_reference_gap.py` — 24 checks, no network/FL.
Synthesized fixtures: `nearest_note` mapping, a 50 Hz kick → f0 within ±2 Hz (+
key-root tuning text, + silence/broadband → "no clear fundamental"), a +6 dB gain
pair → LUFS delta ≈ +6, a high-band-boosted mix → spectral diff positive up top,
mono → ~0 width / decorrelated → ~0.5 width, and the HTML report generated,
self-contained (no `<script>`/`<link>`/CDN), and parseable by the stdlib HTML
parser.
