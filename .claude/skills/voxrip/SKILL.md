---
name: voxrip
description: >-
  Rip a vocal/acapella out of any finished song and conform it to a different
  track's key and tempo. Use when the user wants to steal lyrics/vocals from one
  song and make them fit a completely different production (key/tempo matching),
  extract stems (vocals + instrumental) from an mp3/wav/flac, or analyse a song's
  BPM and key. Part of the Qeynos VOX suite (W9).
---

# voxrip — acapella extraction + key/tempo conforming

A Python CLI (`tools/voxrip/voxrip.py`) that:

1. **Separates** a song into `vocals_raw.wav` + `instrumental.wav` with demucs
   (htdemucs, CPU). `--no-separate` skips this when the input is already an acapella.
2. **Analyses** BPM (librosa beat tracker, with half/double-time alternates + a
   confidence proxy) and key (chromagram vs Krumhansl-Schmuckler profiles) of BOTH
   the full song and the isolated vocal.
3. **Conforms** the vocal to a target BPM (time-stretch) and target key (minimal
   semitone pitch-shift) using the formant-preserving **rubberband** CLI (falls
   back to a librosa phase-vocoder if the binary can't be fetched).
4. Writes `<out>/<song-stem>/` with the stems, `vocals_conformed.wav` (if targets
   given), and a `REPORT.md`.

## Running it

PEP 723 script pinned to Python 3.12. Run with `uv` (at `%USERPROFILE%\.local\bin\uv.exe`,
NOT on PATH — use the absolute path):

```
uv run --python 3.12 C:\dev\qeynos-vst-suite\tools\voxrip\voxrip.py <song> [options]
```

### Examples

- Rip a vocal and conform it to a 174 BPM F# minor DnB track:
  ```
  uv run --python 3.12 ...\voxrip.py "song.mp3" --target-bpm 174 --target-key "F#m" --out .\ripped
  ```
- Analyse + separate only (no conform), report BPM/key of song and vocal:
  ```
  uv run --python 3.12 ...\voxrip.py "song.wav" --out .\ripped
  ```
- Input is already an acapella — skip separation, just conform:
  ```
  uv run --python 3.12 ...\voxrip.py "acapella.wav" --no-separate --target-bpm 128 --target-key Am
  ```

## Options

- `--target-bpm N` — conform the vocal to this tempo.
- `--target-key K` — conform to this key (`Am`, `F#m`, `C`, `Bbmaj`, `G minor`, …).
- `--out DIR` — output base dir (default `<song-dir>/voxrip_out`).
- `--no-separate` — input is already an acapella (skip demucs).
- `--force-fallback` — skip rubberband, use the librosa phase-vocoder engine.

## Key-shift rule (also written into every REPORT)

- **Same mode** (e.g. C#m → F#m): shift = minimal signed semitone distance between
  roots; the octave-wrap partner is reported as the alternative (e.g. +5 or −7).
- **Different mode**: the source key is reinterpreted via its *relative* key into the
  target's mode (minor→relative major = root+3; major→relative minor = root−3) before
  matching, so the source scale lands on the target scale. A minor material into C
  major → 0 st (relative keys); C minor into C major → −3 st.

## Notes for agents

- Separation runs in a **separate uv env** (`voxrip_separate.py`) that pins torch to
  the **CPU** wheel index — the main script and its offline tests never touch torch.
- First separation run downloads torch (~200 MB) + the htdemucs weights; it is slow
  on CPU but fully offline afterwards.
- The rubberband binary is fetched on demand into `tools/bin/rubberband/` (gitignored).
- Offline tests: `uv run --python 3.12 tools\voxrip\test_voxrip.py` (no weights/network).
