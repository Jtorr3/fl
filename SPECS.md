# SPECS — per-plugin signal flows and DSP designs (v1 §4, preserved verbatim + v2 amendments applied)

> Read the relevant section here at step 1 of every iteration (PRD §1.4).
> Where this file and PRD v2 conflict, PRD v2 wins (esp. WIRE codec plan, EMBER
> fallback rule, TRACER done gate, install/validation gates).

### _template (hello-gain)
Proves: workspace builds, egui window opens, param automates, validators pass,
build.ps1 end-to-end. One gain knob + meter. Also delivers the offline harness +
testsig (PRD §4). Keep forever. Phase 0 GO/NO-GO gate.

---

### GRIT — sidechained distortion
```
main in ─ trim ─ pre-filter(SVF HP/LP) ─┐
                                        ├─ DISTORTION CORE ─ post-filter ─ auto-gain ─ mix ─ out
sidechain in ─ SC filter ─ env follower ┘        ▲
              (focus band)  (att/rel)            └ mode selects how SC drives the core
```
- Mode A: Env→Drive. drive_dB(t) = base + depth × env(t)^curve. Oversampled 4x.
- Mode B: Waveshape-by-SC. dynamic bias/fold: y = shape(x + bias·sc(t)), shapes from
  suite bank (tube/tape/fold/hard).
- Mode C: Spectral. STFT both; per-bin drive ∝ smoothed SC bin magnitude. Reports latency.
- Auto-gain: match post RMS to pre RMS over 300 ms, ±12 dB clamp.
- Params: mode, drive, depth, curve, attack, release, SC focus (freq+width), SC listen,
  shape select, pre/post filter, mix, out. Presets: kick-driven bass grit, vocal spectral
  crush, pad ring-fold, drum bus pump-drive, techno rumble driver.

### EMBER — spectral fader / temporal smoother (Fletcher-style)
```
in ─ STFT(2048, hop 512, Hann) ─ per-bin state machine ─ fitting ─ iSTFT/OLA ─ mix ─ out
              factor-band curves: attack(f), decay(f)  (log-freq spline, UI-editable)
```
- Per bin k: `state[k] += coef(in>state ? atk(f_k) : dec(f_k)) × (in_mag[k] − state[k])`;
  coefs from ms via `1 − exp(−hopTime/τ)`; decay τ up to 60 s ⇒ tails continue after input.
- Phase: input phase while bin active; phase-vocoder advance for generated tails.
- Fitting: spectral envelope (moving avg ~1/3 oct); blend bins toward envelope.
- Freeze = τ→∞. Reports 2048-sample latency.
- Params: factor bands (2 splines), fitting, freeze, gate, tail gain, mix.
- Fallback (PRD §5 rule): magnitude-only + random phase, only after 5 failed attempts
  at the tail assertion.

### IMPACT — kick synth (MIDI instrument)
```
note-on ─ pitch env(f_start→f_end, curve) ─ sine/tri osc ─┐
        ─ click layer: noise burst → BP/HP + transient PCM ├─ mix ─ drive ─ amp env ─ clip ─ out
        ─ sub osc (f_end × ratio)                          ┘
```
- Mono, phase-continuous retrigger + 1.5 ms declick. Pitch env exponential:
  f(t) = f_end + (f_start−f_end)·e^(−t/τ_p), curve morphs τ shape.
- Length macro scales amp decay + pitch τ together. Key-track: MIDI note sets f_end.
- Click: white noise → SVF BP 1–8 kHz, 5–50 ms decay + 3 embedded PCM transients
  (generated offline). Saturation pre-amp-env.
- Presets: 808 long, techno rumble kick, psy snap, house punch, hardstyle distorted.

### TRACER — pitch-tracking multiband saturation
```
in ─┬─ mono sum → decimate ~12 kHz → MPM pitch det (1024) → confidence gate
    │            → median(5) → hysteresis (±35 cents) → slew → f0
    └─ LR4 crossover tree (cutoffs = harmonic multiples of f0, coef-interp @ control rate)
         band1..4: [drive → shaper(bank) → 2x OS → mix/level] → sum → out
```
- Smart Frequency knob: crossover center = f0 × 2^(knob); detents at fundamental /
  2nd / 3rd / body(×4–6) / presence(×8–12). Each crossover pitch-locked or fixed.
- Confidence < 0.6 ⇒ freeze crossovers at last-confident. MIDI mode replaces detector.
- Constant-color drive: per-band drive × inverse equal-loudness weight (ISO 226 LUT).
- Time-varying LR4: recompute per 32-sample block, interp states, crossfade pair on
  instability. Done gate: synthetic sliding-saw + synthetic-vocal testsig.

### OVERSEER — mastering system (one library, two plugins)
```
NODE (per track):  in → meter → 4-band EQ → comp → sat → M/S width → trim → meter → out
                                └── slot in same-DLL BUS: meters, params, override area
MASTER (master bus): EQ → 3-band comp (LR4) → lookahead limiter → LUFS meter
                     GUI: grid of live Nodes; writes overrides into Node slots via BUS
```
- Node DSP from suite-core: biquad EQ (LS/2×bell/HS), FF comp (RMS, soft knee), tanh
  sat, M/S width, LUFS-M meter. Master limiter: 2 ms lookahead, 4x OS metering,
  reports latency; integrated LUFS with reset.
- Override badge on Node GUI; local touch steals back. Instance label param ("KICK").
- Ozone hosting: DEFERRED.md only.

---

### DRIFT — infinity filter (Sweep clone)
Shepard-filter illusion: N=6 peak filters spaced one octave apart (log-freq), gliding
up/down at Rate, wrapping at range edges; per-filter gain = raised-cosine window over
log-freq. Params: rate (Hz/BPM), direction, resonance, range lo/hi, peaks, stereo
phase offset, mix. Pure biquads — deliberately first Phase 2 plugin.

### WIRE — codec degradation (Codec clone)
```
in ─ resample 48k ─ [crunch: bit/SR reduce] ─ Opus encode → loss sim (drop/PLC) → decode ─ regen loop ─ out
```
- Codec plan (PRD §5): A = opus_rs (pure Rust enc+dec); B = audiopus + portable CMake;
  C = descope to crunch-only. Link-test before DSP; 3 attempts per plan.
- Params: bitrate 6–128 kbps, packet loss %, bandwidth (NB→FB), voice/music mode,
  FEC, crunch, regen (delay + re-encode feedback), width. 20 ms frames ⇒ report latency.

### OUROBOROS — recursive processor (Recurse clone)
```
in ─ + ─ delay(1 ms–2 s, sync) ─ [slot A ─ slot B ─ slot C] ─ limiter ─ DC block ─┬─ out
     ▲                                                                            │
     └───────────────────────────── × feedback (0–110%) ─────────────────────────┘
```
Slots: pitch shift (granular ±12 st), SVF, freq shifter (Hilbert), saturator,
reverse chunk, bit crush; drag-to-reorder. Freeze = fb 100% + input mute. In-loop
limiter. Params: per-slot amount, delay, feedback, decay-scale, freeze, mix.

### SWARM — mass granulator (Glow clone)
10 s circular buffer; density 1–500 grains/s (poisson or grid-sync); per-grain:
position spray, pitch scatter (free/semitone-quantized), size 10–500 ms, Tukey env,
pan, reverse prob. Sum → optional +12 st shimmer feedback into buffer. Freeze locks
write head. 128-grain cap, steal oldest. Params: density, size, spray, scatter,
quantize, reverse %, shimmer, freeze, width, mix.

### SMUDGE — spectral chaos (Smear clone)
STFT 2048. Per-frame ops, each with amount: scramble (permute bins in ±N neighbor-
hoods), spectral delay (per-1/3-oct band delays on bin frames, feedback), blur
(temporal mag averaging, τ per band), stretch (bin remap ×0.5–2). Chaos macro = slow
S&H randomizing op params. Phase: input phase for scramble/delay; vocoder advance for
blur (EMBER engine reused). Reports latency.

### MURMUR — stochastic reverb (Hikari clone)
FDN 8×8 Householder, re-randomized per onset: spectral-flux onset detector triggers
new draw of delay lengths (within size range), diffusion allpass coefs, per-line
damping color. Two FDN instances ping-pong with 50 ms crossfade (no clicks).
Params: size, decay, color, randomness, onset sensitivity, manual re-roll, freeze, mix.

### FLYBY — doppler spatializer (Transfer clone)
Bezier path loop on XY (listener at origin), traversal BPM-synced or Hz. Per block:
source pos → r, θ. Doppler: fractional-delay read (Catmull-Rom, rate-clamped) at
delay=r/c. Distance: gain 1/max(r,r₀); air = one-pole LP, cutoff ∝ 1/r. Pan: equal
power + optional micro-ITD ≤0.6 ms. Params: path (4–8 nodes), speed/sync, size,
doppler amount, air, width, mix.

### CLEAVE — multi slicer (Slice clone)
2-bar rolling buffer; slices via transient detect (spectral flux + backtrack) or grid
(1/8–1/32). Step sequencer 16–64: per step slice index/as-played, gate, reverse,
pitch ±12, roll ×2/3/4, probability, level. Grain-windowed slice reads. Transport-
locked. Pattern randomizer with density. Params: slice mode/sensitivity, lanes,
swing, mix.

### PLUCK — strummer (Strum clone)
Karplus-Strong: delay line + one-pole damp + allpass fine-tune. 6 strings tuned to
chord select / MIDI-held / chromagram key-detect. Strum = staggered excitation
(5–80 ms stride, up/down/alt). Exciter = 500-sample burst of input audio. Embedded
2048-tap body IR. Params: tuning/chord, damp, decay, strum time/dir, body,
velocity→brightness, mix.

### SHAPESHIFT — morphing distortion (Teuri clone)
```
in ─ pre-gain ─ 4x OS ─ [shaper A][B][C][D] ─ bilinear XY blend ─ post LP ─ mix ─ out
```
Corners from suite bank (tube, tape, diode, fold, sine-fold, hard, asym, chebyshev);
y = Σ wᵢ(x,y)·shaperᵢ(gᵢ·x). XY automatable + orbit LFO (rate, shape, radius).

### CHAMBER — space simulator (Eigen clone; hardest, last clone)
Shoebox image-source: room W×D×H, draggable source/listener. ER to order 3
(≈60 images): per image delay r/c, gain (1/r × absorption^bounces), HF damp,
azimuth pan. Late: FDN with RT60 from Sabine, crossfaded after ER window. Moving
source = interpolated delays (doppler free, clamped). Params: dims, materials
(4 presets/wall-group), positions, ER/late balance, distance, mix.
CPU rule (PRD §4): >30% real-time budget → order 2 → order 1 + bigger late field.

---

### CARVE — spectral ducker
STFT main + SC (GRIT mode-C plumbing). Per 1/3-oct band: soft-knee gain reduction
from SC band energy vs threshold; attack/release per band-group; tilt (duck lows vs
highs); max depth. Params: amount, threshold, tilt, att/rel, sensitivity curve,
listen-Δ, mix. Reports latency.

### NERVE — suite modulation bus
4 LFOs (sync/free, 8 shapes), 2 env followers (own input), 2 random S&H, 4 macros →
8 float streams to tier-2 bus. Every suite plugin gets per-param "listen" (source ×
depth × curve) applied at block rate pre-smoother — suite-core feature. FIRST STEP:
retrofit wrapper → rebuild-all → revalidate-all → reinstall-all (PRD §2 API rule).

### HALT — performance buffer FX
4-bar circular buffer. Momentary modes (MIDI/param buttons, 5 ms crossfades):
tape stop (rate 1→0, curve + synced duration), stutter (loop last 1/4–1/64,
optional decay/pitch step per repeat), reverse, half-speed. Retrigger quantize.
Params per mode + global quantize, mix.

### BANDAID — multiband transient designer
LR4 3-band. Per band: transient = fast env (1 ms) − slow env (50 ms); attack region
gain ±12 dB, sustain region gain; 5 ms smoothed application. Params: 2 crossovers,
per-band attack/sustain, output, per-band solo.

### PATINA — analog lo-fi character
```
in ─ wow/flutter (frac delay ← 0.4 Hz wow + 8 Hz flutter + random walk)
   ─ saturation ─ head-bump EQ (LS 60–120 Hz) ─ azimuth (L/R HF phase skew)
   ─ dropouts (random dips) ─ + noise (hiss/hum/crackle, keyed to input env) ─ age macro ─ mix
```
Params: wow, flutter, sat, bump, azimuth, dropout, noise type/level/key, age, mix.

### X-RAY — shared analyzer
Tier-2 bus consumer: renders every live suite instance's 32-band spectrum as colored
overlays + peak/RMS list; hover highlight, click solo-dim. FIRST STEP: same
retrofit/rebuild-all as NERVE (publishing lives in suite-core wrapper).

### CHORALE — resonator bank
12–24 waveguide resonators (KS loops, high feedback, damped) tuned to held MIDI /
scale/chord select / chromagram key-detect. Input excites all; per-resonator gain =
input band energy at its pitch. Params: tuning source, decay, damp, spread (cents),
stereo alternate, wet solo, mix.

### UNDERTOW — kick-to-rumble generator
```
in(kick) ─┬───────────────────────────────────────────┬─ dry ─ + ─ out
          └ transient strip (env-gated tail) ─ sat ─ FDN reverb (small/dark)
            ─ LP 90–250 Hz ─ resonant tune peak (key-lock) ─ ducker (keyed by dry
            kick env, att 1 ms rel 80–300 ms) ─ rumble gain ┘
```
Params: strip, drive, reverb size/decay, LP freq, tune note, duck depth/release,
rumble level, width.

### SEANCE — ethereal vocal machine
```
in ─ pitch shift (±12 st, formant-preserving PV + envelope lift) ─ formant knob
   ─ chopper (synced/random gate patterns, smooth edges) ─ shimmer verb (FDN, +12 st
     in feedback) ─ wash (LP + wow from PATINA core) ─ ducker (keyed to dry) ─ macros ─ mix
```
Macros: Ghost (pitch+formant+wash), Drown (verb size/wet/duck), Chop (density).
Presets: grief pad vox, drowned lead vox, whisper choir, formant ghost.

### ASCEND — tension generator (MIDI/transport instrument)
Host transport → bars-until-target (set drop bar or next 8/16/32 boundary). Sources:
filtered noise + tonal stack (root+5th of key). Over countdown: filter sweep up,
pitch rise 0–24 st, width bloom, volume curve — one tension envelope with curve.
Target bar: optional embedded impact + auto-cut. Downlifter mode = reversed.
Params: key, length, curve, noise/tone balance, rise, impact, sync target.

---

## Phase 4 tool specs

| Tool | Stack | Design |
|---|---|---|
| W1 RumbleBassline.pyscript | flpianoroll | key, density, ghost-vel inputs; offbeat/rolling 16ths avoiding kick collisions; humanize ±vel ±5 ticks |
| W2 BreakChop.pyscript | flpianoroll | permute selected slice-notes, roll inserts, reverse flags, per-step probability |
| W3 DarkProgression.pyscript | flpianoroll | minor/phrygian/harm-minor pools, voice-leading, hypnotic arp emitter, tension presets |
| W4 session_bootstrap.py | FL MCP | template JSON → track names/colors/routing, tempo, loop mode; TECHNO + DNB templates |
| W5 project_janitor.py | FL MCP | heuristic naming from plugin/sample names; category color map; change report |
| W6 sample_librarian.py | librosa/soundfile (py 3.12 via uv) | BPM (onset autocorr) + key (chromagram) → rename {key}_{bpm}_{name}, sort; dry-run default, --apply to write |
| W7 reference_gap.py | pyloudnorm/numpy (py 3.12) | LUFS-I, 1/3-oct diff plot, width by band, kick f0 + tuning suggestion → HTML |
| W8 vitalgen | Python + Claude API | schema from installed-Vital-saved preset diffed vs OSS repo; pydantic validation + range clamps; CLI + Claude Code skill; --tweak iterate; --bank batch; Serum 2 = DEFERRED |


---

## VOX suite — lyric flexibility (user request 2026-07-07)

### W9-VOXRIP — acapella extraction + conforming (Python tool)
Pipeline: `voxrip.py <song.(mp3|wav|flac)> [--target-bpm N] [--target-key Am] [--out DIR]`
1. Stem separation: demucs (htdemucs model) via uv-managed Python 3.12 env; CPU works
   (slow ok, offline tool); outputs vocals.wav + instrumental.wav (+ drums/bass if free).
2. Source analysis: BPM (librosa beat track) + key (chromagram, Krumhansl profiles) of
   BOTH the full song and the isolated vocal; report confidence.
3. Conform (if targets given): time-stretch vocal to target BPM (rubberband CLI —
   portable binary into tools/bin, formant-preserving mode -F) and pitch-shift by the
   minimal semitone move onto the target key (report the chosen transposition and the
   relative mode option, e.g. target Am from source Cm -> -3 or +9; pick min |st|).
4. Output folder: <song>/vocals_raw.wav, vocals_conformed.wav, instrumental.wav,
   REPORT.md (source bpm/key, target, shift chosen, confidence warnings).
Done bar (offline tests, no separation model in CI path): analysis + conform stages
tested on synthesized fixtures (known-BPM click+vocal synth at known key -> detected
within tolerance; conform hits target BPM ±0.5% and key shift exact); separation is
smoke-tested live only if demucs weights download succeeds — else CHECKPOINTS entry.

### VOXKEY — vocal retuner (plugin)
```
in -> PitchTracker (suite_core::pitch, mono vocal) -> target note = nearest in
Key/Scale (root + scale-mask params, common scales + chromatic) -> shift ratio ->
formant-preserving pitch shift (SEANCE engine from suite-core) -> out
```
- Retune speed (0 = hard snap / autotune artifact, up to 400 ms glide), amount (0-100%),
  humanize (random cents drift), formant preserve on/off + formant offset (st),
  MIDI override mode (held note = target, ignores scale), dry/wet.
- Done bar: synthetic vocal gliding across a fifth -> output f0 quantizes to the
  selected scale within +-15 cents at retune speed 0 (measured via suite_core::pitch);
  formant preservation: spectral-envelope peak positions unchanged +-5% while f0 moves
  >= 3 st; mix=0 null (latency-compensated).

### VOXFIT — vocal character conformer (plugin)
```
in -> formant shift (+-5 st, pitch-independent, PV envelope-lift engine) -> de-esser
(split 5-9 kHz, compression keyed on sibilant energy) -> harshness tamer (dynamic bell
2-5 kHz) -> tilt EQ (+-6 dB/oct pivot 1 kHz) -> proximity (low-mid shelf 200-400 Hz) ->
air (shelf 12 kHz) -> output trim -> out.  "Sit" macro sweeps a curated combination.
```
- Purpose: make a foreign acapella sit in a completely different production.
- Done bar: formant shift +3 st moves spectral-envelope peaks by ~2^(3/12) ratio while
  measured f0 stays +-10 cents; de-esser reduces 5-9 kHz band energy on synthetic
  sibilant bursts (noise bursts through HP) by an amount consistent with its threshold
  while leaving the vowel band (<2 kHz) within +-1 dB; universal assertions + mix null.


---

## POLISH phase (user feedback 2026-07-07)

### PRESET-SYSTEM — suite-wide user presets
- suite-core::presets grows a disk tier: user presets in
  [MyDocuments]/Qeynos/Presets/<plugin>/<name>.json (known-folder API, NOT
  %USERPROFILE%\Documents literal). Same flat JSON as factory presets.
- GUI preset bar widget in suite-core::ui used by every plugin: factory + user
  sections in one dropdown, Save / Save As (text field), Delete (user only),
  dirty-state dot when params diverge from loaded preset.
- Filesystem IO NEVER on the audio thread (load applies via param setter events on
  the GUI thread; save reads a params snapshot).
- Retrofit: every shipped plugin adopts the bar; suite-core API rule applies
  (rebuild-all + revalidate-all). Done bar: round-trip test (save -> mutate ->
  load -> params restored exactly); a GUI-less unit test on the disk tier; name
  sanitization (illegal path chars); overwrite-safe.

### PRESET-EXPANSION — deep factory banks (user: "tons and tons of good presets
will make these plugins feel real")
- Target: 15-30 factory presets PER PLUGIN (instruments and complex FX at the high
  end; simple utilities may stop at 12), organized in categories shown as sections
  in the preset bar (e.g. GRIT: Kick-Driven / Vocal / Bus / Extreme; EMBER: Pads /
  Fades / Freezes / Rhythmic).
- Naming = purpose-driven, evocative, genre-aware (dark techno + atmospheric dnb
  taste profile): 'Warehouse Thump', 'Last Train Home', 'Drowned Ghost Sit' — never
  'Preset 12' or settings descriptions.
- Quality gate per preset (mechanical): loads, differs meaningfully (>=4 params from
  default AND >=2 params from every other preset in its plugin — no near-duplicates),
  render passes universal assertions; renders kept in renders/<plugin>/presets/ for
  human audition.
- Process: one pass over every shipped plugin after PRESET-SYSTEM lands (factory
  presets ride the same disk format). Batchable: one commit per plugin.

### OVERSEER-ENRICH — instrument context + thematic banks
- Node gains an Instrument Type param (enum: KICK, BASS, RUMBLE, PERC, HATS, SNARE,
  BREAKS, VOCAL, PAD, LEAD, ATMOS, FX, BUS, MASTER-ish). Type drives:
  (a) context-tuned defaults (EQ band starting freqs, comp time constants, sat amount,
      width defaults — e.g. KICK: mono-below-120Hz width default, fast comp;
      VOCAL: gentle knee, presence-tilted EQ bands),
  (b) metering context (KICK shows fundamental-region level; VOCAL shows presence/sibilance),
  (c) Master grid shows type badge + type-colored strip per Node.
- Thematic preset banks per type, >= 6 per common type (KICK/BASS/VOCAL/PAD/PERC/BUS),
  named by purpose not settings: e.g. KICK: 'Warehouse Thump', 'Rumble Bed Glue',
  'Psy Click Forward'; VOCAL: 'Drowned Ghost Sit', 'Upfront Dark Pop', 'Tape Choir Bed';
  PAD: 'Grief Wash', 'Afterlife Wide'. Banks live as factory presets tagged by type;
  preset bar filters by the Node's current type.
- Done bar: type switch applies documented defaults (test asserts a KICK-vs-VOCAL
  default diff table); every bank preset loads + passes universal render assertions;
  Master grid displays type badges (validator editor test only).

### BUILT-IN-MANUALS — in-GUI usage manual per plugin
- Every plugin GUI gets a '?' button opening a manual panel (egui window/side panel,
  scrollable, closable): sections = What It Is (2-3 sentences), Signal Flow (the
  SPECS ASCII diagram rendered monospace), Controls (every param: name, range, what
  it does musically — not just technically), Recipes (>=3 concrete workflow recipes
  tuned to the user's genres, e.g. GRIT: 'Kick-driven rumble distortion: route kick
  to sidechain, Mode A, drive 8, focus 60-120 Hz...').
- Content source: extend each docs/<plugin>.md with these sections and embed at
  compile time (include_str! + a tiny section parser in suite-core::ui::manual) —
  one source of truth, readable both on GitHub and in-GUI.
- Done bar: manual opens under validator editor test for every plugin; every param
  listed in the manual exists in the param set (test cross-checks names); recipes
  section non-empty.

### PEDAL-UI — modern stompbox theme (endgame, after all plugins exist)
- suite-core::ui v2: pedal-style visual language — textured dark panel, chunky
  rotary knobs with position indicator + value readout on hover, plugin-accent
  color per pedal (GRIT rust-orange, EMBER ember-red, WIRE circuit-teal, ...),
  LED indicator (activity/clip), footswitch-style bypass toggle, recessed screw
  corners, consistent header (logo + plugin name + preset bar) and footer (in/out
  meters). Pure egui painting (no image assets -> stays self-contained; rounded
  rects, gradients, shadows via egui painter).
- Rollout: build the widget set + apply to _template as the reference pedal, then
  retrofit every plugin (mechanical: replace param rows with pedal layout groups).
  One plugin per commit; build.ps1 -All at the end (suite-core API rule).
- Done bar: every plugin opens under validator editor test with the new theme; all
  params still reachable; knob drag/reset/fine-drag work (manual FL check ->
  CHECKPOINTS). No aesthetic iteration loops beyond the defined language — the
  spec above IS the design decision.
