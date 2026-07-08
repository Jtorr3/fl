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
| shift.rs (shared: SEANCE/VOXKEY/VOXFIT) | synth_vocal at f0 = 150…500 Hz through the formant-preserving PV | IMPROVED — the real VOX-suite defect. The cepstral envelope lifter had a **fixed** `fft_size/16` (128-sample) quefrency cutoff, which sits *above* the pitch-period quefrency `sr/f0` for f0 ≳ 262 Hz — so on **high female-range vocals** the "envelope" tracked the harmonic comb and the reapplied envelope smeared inharmonic energy across the output (gurgle/metallic). Fix: **pitch-adaptive lifter** — each frame finds the pitch-period rahmonic in the cepstrum and, when a clear voiced peak is present, pulls the cutoff to `0.75·period` (clamped `[32, fft_size/16]`), always below the pitch peak. Low/textural material keeps the `fft_size/16` default (byte-identical → no SEANCE regression). Regression test `high_f0_formant_shift_does_not_comb`; phase-wrap (TRIAGE) verified still bounded (`synthesis_phase_stays_bounded`). | **Formant-shift inharmonic residual: f0=400 −4.6 → −25.3 dB; f0=500 −6.4 → −15.9 dB (≈20/10 dB cleaner). f0≤300 unchanged (−33 to −36 dB).** Octave-shift path also improved at f0=400 (−14.2 → −20.1 dB). |
| VOXKEY | synth_vocal + vibrato / breathy / two-note glide / high-female (400 Hz) / 20 s sustain, default + 25 presets → audition.py | IMPROVED (via the shared shift.rs fix; no VOXKEY DSP change). VERIFIED the TRIAGE fixes held: retune **glide is zipper-free** (0 click outliers on the hard-snap fifth glide), the **confidence gate does not gurgle** on breathy material (median+hysteresis holds — 0 clicks), pitch tracker showed **no octave errors**. The flagged **high-f0 formant preservation** combing is resolved by the adaptive lifter (Doll/Deep-Throat formant on a 400 Hz vocal). 25 presets kept (all pass the distinctness gate; key-variant hard-snaps are legitimate for a retuner). | glide/breathy renders CLICK 0; high-female formant renders inharmonic per the shift.rs row; METALLIC flags = the vocal's own harmonic series (verified: modes at k·f0), not an artifact. |
| VOXFIT | synth_vocal + sibilant / breathy / high-female / 20 s sustain, default + 22 presets → audition.py | IMPROVED. **De-esser was killing the air**: the sibilant band was `x − low` = *everything* above 5 kHz, so a strong de-ess pulled the 11–18 kHz air down as hard as the sibilance (measured −20 dB on *De-Harsh Rip*, **−33 dB on *Nyquist Sibilance Kill*** — it dulled the whole top). Fix: **3-way complementary split** (low <5 k / sib 5–10 k / air >10 k, `low+sib+air=x`) ducks only the sibilant band; air passes at unity. Also **declicked the per-ess onset** (`gr` step → 0.8 ms one-pole; was 3 click outliers/render, ratio ≤19.5). VERIFIED TRIAGE held: unity-formant **PV bypass nulls −40 dB** (transparent channel tool), TILT/PROX/AIR **settle ~11–30 ms** (block-rate smoother), SIT macro musical. Regression tests: air-preservation added to `deesser_reduces_sibilant_band_only`. 22 presets kept (distinct, on-brief). | **11–18 kHz air during esses: reduced −20/−33 dB → spared (air drop ≪ sibilant drop). De-ess CLICK 3→0.** Sibilant band still fully ducked (5–9 kHz reduced with amount). |
| OVERSEER | deep-dive (see section below) | IMPROVED — judged as a paid Ozone competitor. 3 real DSP fixes + a fully-recalibrated ENRICH assist + a classifier fix, all with regression tests in `plugins/overseer/src/audit.rs`. | See the **OVERSEER deep-dive** checklist below. |

## OVERSEER deep-dive (SOUND-PASS PRD §7 — judged as a PAID mastering competitor)

Two-part context: a prior **Fable** agent began this audit and crashed mid-fix (usage limit),
leaving the wip checkpoint `1539b10`. That checkpoint added the mud/harsh feature axes to
`suite_core::classify` and the bell-move suggestion fields, and wrote `plugins/overseer/src/audit.rs`
— **but never declared `mod audit`, so the entire audit suite was dead code that had never
compiled or run.** The Opus finisher wired it in, reconciled the incoherences (the audit referenced
`NodeSuggestion` fields that didn't exist; the mud/harsh thresholds were hand-guessed in a comment
and never triggered on real material), and completed the full 10-item checklist. Evidence WAVs in
`renders/_audition/OVERSEER/`; every row is a `#[test]` in `audit.rs`.

| # | Item | Verdict | Evidence / fix (file:line) |
|---|---|---|---|
| 1 | **LIMITER** transparency @ ~3 dB GR on synth_kick_loop; pumping; true-peak/ISP; heavy-GR character | **FIX** (true-peak) + PASS | `audit_limiter_kick_transparency_and_pumping`: @ −3 dB GR, punch envelope −0.19 dB (kept), crest 6.20→4.34, inter-kick ducking −0.20 dB (no pumping). **FIX-1: the brickwall limiter was sample-peak only — an fs/4 π/4 tone (sample −2.93 dBFS, true +0.13 dBTP) sailed 1.13 dB over the −1 ceiling.** Added latency-neutral **4x-oversampled true-peak detection in the sidechain** (`dynamics.rs` `Limiter::process`): the gain is now driven by the inter-sample peak; the oversampler's ~22-sample group delay stays inside the 96-sample lookahead so audio-path latency/PDC/null are unchanged. Canonical fs/4 ISP worst case now **+0.13 dBTP → −0.92 dBTP** (a 1.05 dB improvement, ceiling honored); a drifting near-Nyquist 12 kHz sine lands −0.65 dBTP (small residual, see LIMITATION). Heavy GR (8 dB) on a full mix stays bounded + ceiling-honored. |
| 2 | **EQ honesty** realized vs requested incl. near-Nyquist cramping | **PASS** | `audit_eq_realized_curve_matches_request`: isolated bells hit requested gain within ±0.6 dB (300 Hz −4.0→−4.00, 3.5 k +5→+5.00, 10 k/16 k +6→+6.00); shelves plateau +6→+5.65/+5.52 (documented ±0.75). Near-Nyquist cramping printed as measured. Flat EQ transparent (<0.05 dB). |
| 3 | **DYNAMICS** attack/release realized vs displayed; GR-meter accuracy; program-dependent | **FIX** (attack honesty) + PASS | `audit_comp_attack_release_honesty` + `audit_comp_gr_meter_accuracy`. **FIX-3: the RMS detector was a fixed 10 ms, flooring realized attack at ~15 ms t90 regardless of the knob** (a 0.5 ms setting did nothing). Tied the detector window to the attack (`dynamics.rs` `Compressor::configure`, floor 0.5 ms / cap 10 ms) so a fast attack is actually fast. GR meter exact (−6.02 vs −6.02 measured). LIMITATION: a ~few-ms detector-settling floor remains (see below). |
| 4 | **SATURATION** drive-0 exact bypass; character/aliasing at musical drives | **PASS** | Drive-0 exact bypass held from TRIAGE: `node_drive_floor_bypasses_saturation_exactly` residual **< −96 dB** (better than the −90 requirement). `audit_sat_character_renders` renders 1 kHz probes + reese at 6/12 dB drive (2x-oversampled tanh) for audition.py `--sine-probe` aliasing/THD. |
| 5 | **METERS** LUFS-I + true-peak vs audition.py | **PASS** | `audit_end_to_end_scenario_master` writes `meters.json`; cross-checked against `audition.py analyze` (independent 4x-OS true-peak + BS.1770 LUFS) within the ±0.3 LU / ±0.3 dB bar. |
| 6 | **ENRICH must earn its name** — planted defects move toward reference on every axis | **FIX** (headline) | `audit_enrich_fixes_planted_defects`. The Fable thresholds never fired (measured: muddy bass mud=0.386 vs threshold 0.42; harsh vocal harsh=0.084 vs 0.22; and a starved-sub kick was told to CUT its low end). **Rewrote `suggest_from_features` to be type-aware** (`enrich.rs`): bass-domain sources only ever get their starved sub LIFTED (never cut); mud/harsh thresholds recalibrated on measured values (mud>0.34→−4/−6 dB @300; harsh>0.065→−4/−5 dB @3.5k). Assist now moves MUD, HARSH and SUB toward the clean reference on all three planted axes. |
| 7 | **CLASSIFICATION** on kick/reese/pad/vocal/break; report confusions | **FIX** (reese) + PASS | `audit_classifier_on_musical_sources`. **The detuned-saw reese trips ~22 onsets/s (beating) and was misclassified KICK** (KICK had no upper onset bound; BASS died on its `le(onset,1.5)` gate). Fixed in `suite_core::classify::scores`: KICK onset is now a BAND (1.2–9/s, excludes the 22/s buzz), BASS tolerates the spurious high onset. Kick(1.00)/reese→Bass(1.00)/pad(stereo)→Pad(1.00)/vocal→Vocal(1.00) all correct. **Reported confusion:** the kick-forward synthetic `synth_break` reads as KICK (its low-band beat hit + modest window onset count don't clear the BREAKS gates) — a real dense amen with bright snares/hats classifies correctly. **classify.rs change is workspace-safe** — every existing fixture (kick_train 4/s, sliding-saw bass, vocal, perc, wide-pad) is unaffected (verified against `suite-core` classify fixtures + full workspace test). Note: on a MONO source a pad reads as vocal (width is the separator) — the audit auditions the pad in stereo, matching real use. |
| 8 | **END-TO-END** master a 4-stem synthetic track to −1 dBTP; measurably better than raw sum | **PASS** | `audit_end_to_end_scenario_master`: 4 stems → per-type Nodes → Master (DARK-TECHNO assist 0.3). Gain-staged by iterating the input trim (the chain is compressive; one linear step undershot) into the techno window; TP ceiling honored, crest ≥ 6 dB retained, sub > 2–5 k harsh. audition.py `compare` raw→mastered = IMPROVED. |
| 9 | **PDC** full-chain latency honesty | **PASS** | `audit_pdc_reported_latency_matches_impulse`: Node reported 14 = impulse peak 14; Master reported 96 = peak 96; full chain 110 samples @ 48 k. |
| 10 | **PAPERCUTS** Node/Master GUI/UX (code review, no FL) | **PASS w/ notes** | GUI SUGGEST row now carries the mud/harsh bell moves and APPLY writes them (`lib.rs` `node_enrich_ui`); type-aware suggestion flows from the LEARN commit type. No FL-side testing (headless). |

**LIMITATIONS (honest cost):**
- **Compressor attack floor (item 3):** even with the detector tied to attack, the RMS detector
  imposes a ~few-ms settling floor on the realized attack; sub-1 ms attacks realize a few ms, not
  sub-ms. Cost: OVERSEER is not a true peak/transient-clamp compressor at its fastest setting.
  Removing the floor would need a peak (not RMS) detector path, trading low-frequency detector
  ripple for speed — deferred as out of scope for a channel-strip glue compressor.
- **True-peak limiting (item 1):** ISP detection is 4x-oversampled (matches the metering and
  common streaming-loudness practice). The canonical fs/4 worst case is fully controlled
  (−0.92 dBTP at a −1.0 ceiling), but a drifting near-Nyquist (~12 kHz) full-scale sine leaves a
  ~0.35 dB residual (−0.65 dBTP) — 4x-OS under-reconstructs the true peak of content that close to
  Nyquist. Cost: negligible for real program material (which is rarely full-scale at 12 kHz);
  full control would need 8x+ oversampling (more CPU, no latency change).
- **Break classification (item 7):** a kick-forward breakbeat can read as KICK on a low-band-heavy
  source; the type is a SUGGEST/LEARN starting point the user can pin.
- **Harsh detection margin (item 6):** the harsh-cut trigger (harsh_ratio > 0.065) sits close to a
  clean vocal's baseline (~0.05); a naturally bright-but-clean source could occasionally suggest a
  gentle 3.5 k cut the user can decline (it is a SUGGEST/APPLY ghost, not automatic).
- **Mono pad classification (item 7):** a pad collapsed to mono classifies as VOCAL because stereo
  width is the pad/vocal separator (by design, documented).
