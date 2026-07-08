# W9-VOXRIP — acapella extraction + key/tempo conforming

A standalone Python tool (`tools/voxrip/voxrip.py`) that rips a vocal out of any
finished song and conforms it to a completely different track's key and tempo, so
a foreign acapella sits in a new production. First member of the Qeynos **VOX
suite** (with the VOXKEY / VOXFIT plugins).

## Pipeline

| Stage | What it does | Engine |
|---|---|---|
| 1. Separate | `song → vocals_raw.wav + instrumental.wav` | demucs `htdemucs`, `--two-stems vocals` (CPU torch). `--no-separate` skips it |
| 2. Analyse | BPM (+ half/double alts + confidence) and key of **both** the full song and the isolated vocal | librosa beat tracker; chromagram vs Krumhansl-Schmuckler major/minor profiles |
| 3. Conform | Time-stretch vocal to `--target-bpm`, pitch-shift by the minimal semitone move onto `--target-key` | rubberband CLI (`-F` formant-preserving); librosa phase-vocoder fallback |
| 4. Output | `<out>/<song-stem>/` with `vocals_raw.wav`, `vocals_conformed.wav` (if targets), `instrumental.wav`, `REPORT.md` | — |

## CLI

```
uv run --python 3.12 tools\voxrip\voxrip.py <song.(mp3|wav|flac)> \
    [--target-bpm N] [--target-key Am] [--out DIR] [--no-separate] [--force-fallback]
```

`uv` lives at `%USERPROFILE%\.local\bin\uv.exe` (not on PATH — use the absolute path).
The script is a PEP 723 file pinned to Python 3.12.

| Flag | Meaning |
|---|---|
| `--target-bpm N` | conform the vocal to this tempo (time-stretch) |
| `--target-key K` | conform to this key: `Am`, `F#m`, `C`, `Bbmaj`, `G minor`, … |
| `--out DIR` | output base dir (default `<song-dir>/voxrip_out`) |
| `--no-separate` | input is already an acapella — skip demucs |
| `--force-fallback` | skip rubberband, use the librosa phase-vocoder engine |

## Analysis

- **BPM** — `librosa.beat.beat_track` on the onset-strength envelope (256-sample hop
  for fine tempogram resolution). Reports the estimate, a confidence proxy (fraction
  of per-frame local tempo estimates within ±8% of the pick), and half/double-time
  alternates. If confidence is low, the REPORT warns and lists the alternates.
- **Key** — mean chromagram (`chroma_cqt`) correlated (Pearson) against the 24 rotated
  Krumhansl-Schmuckler major/minor profiles; best score wins, runner-up is reported.
  A near-tie (< 0.05) raises an ambiguity warning.

## Key-shift logic

The transposition is the **minimal** semitone move onto the target key:

- **Same mode**: minimal signed distance between the two roots. The ±12 octave-wrap
  partner is reported as the alternative — e.g. C#m → F#m gives **+5** (alt **−7**);
  Cm → Am gives **−3** (alt **+9**).
- **Different mode**: the source key is reinterpreted through its **relative** key into
  the target's mode before matching (minor → relative major = root + 3; major →
  relative minor = root − 3), because relative keys share the same notes and that is
  what actually makes the vocal sit — a plain root-to-root match would leave a minor
  vocal clashing over a major track. Examples: A minor → C major = **0 st** (they are
  relatives); C minor → C major = **−3 st**.

The chosen rule, the shift, its octave alternative, and any relative reinterpretation
are all written into `REPORT.md`.

## Conform engine

- **rubberband CLI** (Breakfast Quay portable Windows build) is fetched on demand into
  `tools/bin/rubberband/` (gitignored) and run with `-F` (formant preservation),
  `--time <src/tgt>`, `--pitch <semitones>`, `-c 6`.
- **Fallback** (if the binary can't be fetched after 3 attempts, or fails to run):
  `librosa.effects.time_stretch` + `pitch_shift` — lower quality (no formant
  preservation); the REPORT records that the fallback was used.

## Separation is out-of-process

demucs pulls torch (~200 MB). To keep the analysis/conform script and its offline
test gate torch-free, separation runs in a **separate uv env** via
`tools/voxrip/voxrip_separate.py`, whose PEP 723 `[tool.uv]` metadata pins
torch/torchaudio to PyTorch's **CPU** wheel index (`download.pytorch.org/whl/cpu`) —
no CUDA download. (torchaudio is capped `<2.8`, since 2.8+ routes `load()` through
torchcodec/FFmpeg; 2.7.x keeps demucs's soundfile loader working unattended.) The
first run downloads torch + the htdemucs weights, then works fully offline.

## Tests (offline gate — no weights, no network)

```
uv run --python 3.12 tools\voxrip\test_voxrip.py     # 19 checks
```

- **Analysis** on in-test synthesised fixtures: a click track at a known BPM (detected
  within ±2% or a half/double alternate) and a tonicised harmonic pad at a known key
  (detected exactly, incl. the A-minor-vs-C-major relative ambiguity).
- **Conform math**: key parsing, minimal transposition (incl. the relative-mode rule,
  fuzzed over all 24×24 key pairs), stretch-ratio calc.
- **Command builders** for rubberband + demucs; the rubberband invocation is mocked.

## Live verification (2026-07-07)

Both live paths were exercised end-to-end on this machine:

- **rubberband**: fetched `rubberband-3.3.0-gpl-executable-windows` and conformed a
  synthetic acapella (C#m/99 BPM → F#m/128 BPM, +5 st, `-F`).
- **demucs**: built the CPU-torch env, downloaded htdemucs weights, and separated a
  track into `vocals_raw.wav` + `instrumental.wav`.

## Related

- Skill: `.claude/skills/voxrip/SKILL.md`
- Next in the VOX suite: **VOXKEY** (vocal retuner plugin) and **VOXFIT** (vocal
  character conformer plugin), which reuse `suite_core::shift` (SEANCE's
  formant-preserving PV engine).
