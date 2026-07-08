# SOUND-PASS — suite-wide sound-quality audition

Post-completion verification pass (PRD §7 "SOUND-PASS"). The question for every
plugin: **"Does this warrant being used in an actual song? Is this good?"** — answered
by analysis (no human ears in the loop; the user auditions the before/after WAV pairs
last).

## Method

1. **INFRA (step 1 — done).** Shared audition infrastructure:
   - `tools/audition.py` — producer-relevant WAV analysis (`uv`-runnable, PEP 723).
     `analyze <wav> [--sine-probe f] [--ref dark_techno|atmos_dnb] [--json]` and
     `compare <before.wav> <after.wav> [...]`. Metrics: LUFS-I / true-peak / crest;
     1/3-octave balance vs the two genre reference curves + per-band deviation;
     producer flags (MUD / HARSH / BOXY / SUB_WEAK / SUB_HEAVY / DULL); click /
     discontinuity; DC offset; silence/dropout; metallic-ringing modes on tails;
     THD character + inharmonic/aliasing residual on a sine probe; stereo correlation
     + side/mid width by band.
   - `suite_core::testsig` musical audition sources — `synth_kick_loop`, `synth_reese`,
     `synth_break`, `synth_pad` (+ the existing `synth_vocal`), deterministic, 48 kHz.

2. **PER PLUGIN (steps 2+, one plugin per item — filled in below).** Render EVERY
   factory preset + the default state over genre-appropriate musical sources; run
   `audition.py`; judge "would a producer keep this in a real song?"; fix what falls
   short — preset param retunes, internal voicing (default curves, output tilts,
   diffusion/mod for metallic reverbs, oversampling where aliasing is audible in the
   analysis) — **never breaking the null / latency / alloc contracts**; re-render and
   require measurably-better metrics before commit.

3. **DELIVERABLE.** `renders/_audition/<PLUGIN>/` before+after WAV pairs for the user's
   ears, plus this file's verdict table. Gates: per-crate tests per fix; the full
   `cargo test --workspace --release` + `build.ps1 -All` at the end.

## Running the infra

```
# analyze one render (dark-techno reference curve by default)
uv run --python 3.12 tools/audition.py analyze renders/<PLUGIN>/<preset>.wav

# aliasing / THD character on a tone render
uv run --python 3.12 tools/audition.py analyze <wav> --sine-probe 1000 --json

# before/after a fix (verdict: IMPROVED / REGRESSED / MIXED / UNCHANGED)
uv run --python 3.12 tools/audition.py compare before.wav after.wav

# infra self-test
uv run --python 3.12 tools/test_audition.py
```

Musical sources (Rust, `suite_core::testsig`; render offline via each crate's harness):

```
synth_kick_loop(bpm: f32, bars: usize, sr: f32) -> Vec<f32>   // four-on-floor, 55 Hz
synth_reese(f0: f32, seconds: f32, sr: f32)     -> Vec<f32>   // detuned dual-saw bed
synth_break(bpm: f32, bars: usize, sr: f32)     -> Vec<f32>   // amen-ish break
synth_pad(root_hz: f32, seconds: f32, sr: f32)  -> Vec<f32>   // minor-triad pad
synth_vocal(freq: f32, len: usize, sr: f32)     -> Vec<f32>   // (existing)
```

## Verdict table

Verdict legend: **GOOD-AS-IS** · **IMPROVED** (what changed) · **LIMITATION** (why +
what it'd take). One row per plugin, filled in by the per-plugin pass.

| Plugin | Source(s) | Verdict | What changed / why | Key metric delta |
|---|---|---|---|---|
| _(pending per-plugin pass)_ | | | | |
