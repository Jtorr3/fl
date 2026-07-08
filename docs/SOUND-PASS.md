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
| IMPACT | note render per preset (1.5 s, vel 1.0) → audition.py, ref dark_techno | IMPROVED — FULL RE-AUTHORING (25 → 16). User: "completely useless… especially kick/snare". Audition proved it: the whole old **Distorted** category had its energy at **250-500 Hz with no sub** (MUD/BOXY/DULL — honky mid-kicks) and the psy/clicky/tom rows were off-genre filler. New bank = 5 use-archetypes judged on output audio: **Warehouse Rumble** (KAS:ST, 50-55 Hz sub-dominant, saturated tail), **Wave 808** (Akiaura/agonyOST tuned deep-sub), **DnB Punch** (Cynthoni, crest 9-11), **Deep Sub Roller** (near-sine layers, low crest by design), **Character Drive** (aggression with the sub kept — Tube/Tape not Fold, low `fend`, sub floor). DSP fix: added a **~5 Hz output DC blocker** (regression test `deep_sub_kick_has_no_dc_offset`) — the deep-sub presets left ~0.003-0.005 DC. | **DC_OFFSET 8→0, CLICK 4→0, TRUE_PEAK 2→0, MUD 6→2, BOXY 6→2, DULL 6→2.** Every kick now low-band-dominant (was 4 kicks centered at 250-500 Hz). Remaining flags: 2 presets (Memphis Sub 808 / Industrial Thud) carry SUB_HEAVY/MUD/BOXY/DULL — the deep-808 & distorted archetypes (a sub-heavy bandlimited kick reads that way vs a full-range curve; SUB_HEAVY = the intended octave-down sub, DULL = a kick has no cymbal top). METALLIC 21→15 = the tune's harmonic overtone series (verified: modes at integer multiples of `fend`), not inharmonic FDN ringing — the near-clean Sine Sub Layer flags nothing. |
| SNAP | note render per preset (1.2 s stereo) → audition.py, ref atmos_dnb | IMPROVED — FULL RE-AUTHORING (24 → 15). Audition proved the old claps piled energy into **200-800 Hz (MUD/BOXY — boxy, not crisp)** and several snares were **dull** (HF 30-40 dB down — no crack). New bank = 5 archetypes: **DnB Snare** (Cynthoni, 200 Hz body + bright noise, real crack HF), **Breakcore Snap** (driven, tight, short), **Wave Clap** (Akiaura, wide + bright — `tone` raised 0.4→0.6-0.68 to kill the boxiness), **Techno Rim** (KAS:ST high-tuned body-forward), **Texture Layer** (rattle tops). DSP fix: **note-off declick ramp** (regression test `note_off_deactivation_is_click_free`) — the hard `active=false` cut stepped the running noise floor to zero (a sub -74 dBFS click the audition caught on the short presets, 0.9-1.1 s into the dead tail). | **BOXY 8→0, MUD 6→0, CLICK 4→0.** Claps crisp not boxy; snares carry crack HF (hi_vs_top −22 to −26 dB, was −37). Remaining: HARSH×4 (bright claps/rattle — the intended crispness, not mud), SUB_WEAK×4 (a snare/clap is not a sub instrument), METALLIC×7 = odd-harmonic series of the tuned body (verified: neuro tune 220 → modes 660/1100/1540/1980; noise-dominated presets don't flag it) — the snare's tonal body, not FDN ringing. |
| GRIT | reese bed + kick-loop sidechain | IMPROVED | DSP verified clean (no aliasing, musical env-follower pump, auto-gain flat). Replaced off-brief fizzy "Nyquist Screech" with glossy-dark 808 destroyer "Sub Detonator"; fixed "Concrete Slam" true-peak over. 18→18. | 1 kHz probe through hottest preset: inharm −94.9 dB (4x OS ample); auto-gain within 0.4 LU across 3–18 dB drive; Concrete Slam +0.26 → clean dBTP |
| UNDERTOW | techno kick loop | IMPROVED | Low end already meets the KAS:ST bar (40–90 Hz dominant, mud 20–34 dB down; duck breathes, tails natural). Pruned 2 duplicate presets, declipped/re-leveled 7 over-hot presets into a consistent ~−12 dBFS core. 19→17. | eliminated two +0.7 dBFS clippers; ~18 dB RMS spread → 7 fixed presets at −11.7…−12.9 dBFS; devsum toward-ref −21.6…−55.7 |
| TRACER | reese bed + sliding-808 glide | IMPROVED | Fixed systemic saturation-injected DC (up to −0.06 on hot Reese) with a wet-only ~10 Hz DC blocker; pitch-tracked crossover verified zipper-free on a glide. Cut 1 filler, trimmed 4 clipping presets. 21→20. | DC_OFFSET cleared on all 20 shipped presets, both sources; gliding-sine ≤4 click hits (crossover clean); null/latency contracts intact |
| PATINA | pad + break | IMPROVED | Degradation confirmed musical (0 clicks, no harsh digital edge; wow/flutter smears resonances). Fixed true-peak clipping on 5 hot presets via Out trims; replaced redundant "Barely There" with distinct "Worn-In Glue". 18→18. | 5 presets +0.03…+1.30 → −0.2…−0.5 dBTP; bank min pairwise distance 0.17 → 0.31 |
| WIRE | vocal + break | IMPROVED | Resolved the P2 dropout re-entry click — post-concealment frame ramped up from zero over ~2.7 ms in `dsp::run_frame` (drop-free path byte-identical). Pruned 2 filler presets, trimmed 2 hot presets. 22→20. | worst vocal click ratio 24–40 → 10–15 across the loss bank; TRUE_PEAK_OVER cleared on 3 presets; loss-free presets byte-identical (no clean-path regression) |
