# PRD v2 — Qeynos Audio Suite
### Technical design + autonomous execution playbook (source of truth; hardened by adversarial review 2026-07-07)

**Audience: me (Claude), executing autonomously in a loop with NO human available.**
WORK_ORDER.md is historical context only. This file + the §7 checklist are the sole
source of scope, order, and decisions. When reality contradicts this doc, fix the doc
in the same commit.

---

## 0. Resolved decisions — NEVER ask the user

- All WORK_ORDER "open items" are RESOLVED: build order = §7 checklist exactly;
  Phase 3 = all ten ideas; suite name = "Qeynos" (renameable later, irrelevant now);
  GUI = minimal-dark shared theme. OVERSEER stays in Phase 1.
- **Global rule:** if any step requires a judgment call not covered by this doc:
  decide, record the decision as a PRD/DEFERRED.md edit in the same commit, continue.
  Asking the user is never a valid step outcome. "Pause and tell user" does not exist;
  instead write a note to `CHECKPOINTS.md` (the human-checkpoint ledger) and continue.
- Plugin formats: both built always. **FIELD CORRECTION 2026-07-07: the machine runs
  FL Studio 21, which predates CLAP support (2024.1+) — so VST3 is the delivery format
  that actually loads.** The admin junction C:\Program Files\Common Files\VST3\Qeynos →
  distst3 EXISTS (user created it via UAC); build.ps1 auto-installs VST3 through it.
  CLAP installs continue (future-proofing for an FL update).
  FL does NOT scan custom VST3 folders (Image-Line manual: VST3 must be in
  `Program Files\Common Files\VST3`); never rely on a VST3 "extra search path".
- All state files (STATUS.md, LOG, DEFERRED.md, CHECKPOINTS.md) refer to plugins by
  NAME, never by number. Numbering in WORK_ORDER.md is obsolete.
- Never enable nih-plug's `simd` cargo feature (requires nightly; we build on stable).
- Record decisions ONLY as repo file edits — never rely on conversation memory
  surviving compaction.

## 1. Loop contract (the outer loop)

```
ITERATION:
 1. Read STATUS.md. If CURRENT is set → resume at its STEP via §1.6 recovery.
 2. Else pick the FIRST unticked item in §7, top to bottom. Checklist order IS the
    dependency order; never reorder or skip (except the shipped-degraded rule §1.5).
 3. Set CURRENT=<name> STEP=1 in STATUS.md, commit "start(<name>)".
 4. Execute the §1.4 iteration body, updating STATUS.md STEP before each step.
 5. On done: tick §7 checklist, clear CURRENT, append one LOG line to STATUS.md
    (shipped / descoped-what / how-to-test-in-FL — this IS the user's report),
    commit code+tick+STATUS in ONE commit, push.
STOP CONDITIONS (the only ones):
 - all §7 items ticked;
 - toolchain broken and unfixable after 5 attempts → write BLOCKED.md (exact error,
   everything tried), commit, stop;
 - push auth-fails → do NOT stop: set PUSH-PENDING in STATUS.md, keep working locally,
   retry push at each iteration start.
HARD CHECKPOINTS (after OVERSEER+W4/W8, after ASCEND, after CHAMBER, at end):
 - `build.ps1 --all` green, STATUS.md LOG current, everything pushed,
   CHECKPOINTS.md updated with what the human should do/test in FL.
```

### 1.4 Iteration body (one plugin)

```
 1. Re-read this PRD's section for the plugin
 2. Copy plugins/_template → new crate, register in workspace
 3. DSP core first: pure-DSP module + offline harness tests (§4 assertions)
 4. Param layer (nih-plug params + smoothers; ranges from spec, else defaults §1.5)
 5. GUI (suite-core theme; DONE when every param is on screen, theme applied,
    opens under the validator editor test — no aesthetic iteration beyond theme)
 6. ≥5 factory presets (DONE when each loads, differs from default in ≥3 params,
    and its render passes universal assertions; character is my judgment, never ask)
 7. Build gate: powershell -ExecutionPolicy Bypass -File build.ps1 <crate>
    → release build → bundle .clap + .vst3 → clap-validator on .clap →
    pluginval --strictness-level 8 --skip-gui-tests --timeout-ms 120000 on .vst3
    → install .clap to user CLAP dir (+ .vst3 to junction if present)
 8. Render tests write renders/<plugin>/*.wav via the offline harness (cargo test)
    — these are the artifacts the user auditions later. (nih-plug's standalone
    target is realtime-only; it is NOT used for rendering.)
 9. Docs: README table row + docs/<plugin>.md param reference
10. Tick checklist + STATUS.md LOG line, then ONE atomic commit, push
11. Next item. Never two plugins in flight.
```

### 1.5 Failure valves (mechanical — replaces all wall-clock rules)

- A **fix attempt** = one edit-build-test cycle targeting one error. Increment
  ATTEMPTS in STATUS.md before each cycle.
- ATTEMPTS on one step reaches **5**, or the same error signature survives **3**
  consecutive attempts → descope that feature (DEFERRED.md entry), reset ATTEMPTS,
  continue. If the whole plugin is the feature → ship a reduced-param stub that
  passes the validators, mark `[x]*` (shipped-degraded) in §7, move on.
- pluginval fails on a framework quirk after 3 attempts → drop to strictness 5,
  record the exact failing check in DEFERRED.md, continue. Below 5 = not done;
  descope features until 5 passes.
- Param range defaults when spec is silent: freq 20–20k log; gain ±24 dB;
  times log-scaled; mix 0–100%. Decide and move on.
- If a crate hard-requires MSVC on windows-gnu after 3 distinct fix attempts:
  drop the crate/feature, DEFERRED.md, note NEEDS-ELEVATION in CHECKPOINTS.md,
  continue. There is NO winget/VS-installer fallback (it requires UAC elevation
  that cannot be granted unattended).

### 1.6 Recovery protocol (mechanical, no judgment)

- **Session start, dirty tree:** never stash/reset. `git add -A && git commit -m
  "wip(<CURRENT>): crash checkpoint at step <STEP>"`. Then re-run the current step's
  verification (build/tests); trust test results over STATUS.md if they disagree,
  and correct STATUS.md.
- **Push fails:** retry once; then PUSH-PENDING in STATUS.md, continue, retry each
  iteration start. Never block on push.
- **Remote diverged:** `git pull --rebase`; on conflict keep LOCAL for `plugins/**`
  and `suite-core/**` (I am sole code author), keep REMOTE for PRD/WORK_ORDER
  (user may edit intent); rebuild + retest before continuing. `--force-with-lease`
  only; never `git reset --hard`.
- **Fresh session entry point:** repo CLAUDE.md mandates: read STATUS.md → read this
  §1 + the CURRENT plugin's spec → `git status` + `git log -5` → reconcile → resume.

## 2. Repo & infrastructure

**Repo location: `C:\dev\qeynos-vst-suite`** — Phase 0 clones it there from the
remote. The OneDrive copy is retired (OneDrive corrupts `.git` under sync contention
and risks MAX_PATH with cargo's deep paths; building inside a sync root is a known
hazard). Remote: **https://github.com/Jtorr3/fl** (gh CLI authed).

```
qeynos-vst-suite/
├── CLAUDE.md  PRD.md  WORK_ORDER.md  README.md  STATUS.md  CHECKPOINTS.md
├── .claude/settings.json     (permission allowlist for unattended runs)
├── Cargo.toml  build.ps1
├── suite-core/   dsp/ ui/ bus/ presets/ testsig/   (see §3, §4)
├── plugins/_template/ + one crate per plugin
├── tools/        (Phase 4 Python; tools/bin/ for pluginval, clap-validator, cmake — gitignored)
├── pyscripts/    docs/    renders/ (gitignored)
```

**Phase 0 bootstrap — exact non-interactive commands, in order:**
1. `git clone https://github.com/Jtorr3/fl.git C:\dev\qeynos-vst-suite`
   then in it: `git config user.name "Jtorr3"`, `git config user.email
   "jason@qeynosholdings.com"`, `git config core.longpaths true`, `gh auth setup-git`
   (routes git pushes through the gh token — no credential-manager popup).
2. `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned -Force` (no admin needed).
3. Rust, fully silent, no MSVC prompt branch:
   `Invoke-WebRequest https://win.rustup.rs/x86_64 -OutFile $env:TEMP\rustup-init.exe`
   `& $env:TEMP\rustup-init.exe -y --default-host x86_64-pc-windows-gnu --default-toolchain stable --profile minimal`
4. **PATH is not refreshed in running shells**: every subsequent command uses
   `& "$env:USERPROFILE\.cargo\bin\cargo.exe"` or build.ps1, whose first line is
   `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`. Same for uv later
   (`%USERPROFILE%\.local\bin`).
5. Set user env `CARGO_TARGET_DIR=C:\qvs-target` (shared target dir, off OneDrive,
   short path).
6. Validators into `tools/bin/`:
   `gh release download -R Tracktion/pluginval -p "*Windows*"` → `Expand-Archive` →
   `Unblock-File`; same for `robbert-vdh/clap-validator` (Windows binary).
   Unblock-File strips Mark-of-the-Web on every downloaded exe.
7. Preflight checks (report + adapt, never prompt): Smart App Control state (if ON,
   unsigned validators/plugins may be silently blocked — write CHECKPOINTS.md entry
   and attempt anyway); confirm `_template` build artifact survives a Defender scan
   (mingw binaries are a false-positive class): build, wait, re-hash the file.
8. Portable CMake (plan B for WIRE, harmless to have): cmake.org windows zip →
   `Expand-Archive` into tools/bin/cmake. No installer, no admin.
9. nih-plug pinned to a git rev in workspace Cargo.toml. Exports: `nih_export_clap!`
   + `nih_export_vst3!` for every plugin. `cargo xtask bundle` produces both.
10. Build `_template` (hello-gain) end-to-end through the §1.4 step-7 gate.
    **GO/NO-GO GATE:** if windows-gnu cannot produce a passing .clap here, STOP and
    write BLOCKED.md — do not attempt elevation-based fallbacks. This is the single
    riskiest unknown (nih-plug is CI-tested on MSVC, not gnu) and it is resolved in
    hour one, not at plugin 7.

**Install targets (per build):**
- CLAP → `%LOCALAPPDATA%\Programs\Common\CLAP\Qeynos\` (per-user, no admin, FL scans it).
- VST3 → `C:\Program Files\Common Files\VST3\Qeynos\` ONLY if that junction/folder
  exists and is writable (one-time optional human admin step, listed in
  CHECKPOINTS.md); otherwise skip silently.
- Any path involving Documents resolves via
  `[Environment]::GetFolderPath('MyDocuments')` (Documents is OneDrive-redirected
  on this machine; never hardcode `%USERPROFILE%\Documents`).

**Human checkpoint ledger (`CHECKPOINTS.md`):** everything only a human can do lands
here and the loop continues past it: FL "Manage plugins → find more" rescan after new
installs; listening to renders/; optional admin VST3 junction
(`mklink /J "C:\Program Files\Common Files\VST3\Qeynos" "C:\dev\qeynos-vst-suite\dist\vst3"`);
optional MSVC Build Tools install; in-FL GUI/DPI spot-check (nih_plug_egui needs
OpenGL; FL has known plugin-DPI quirks — validator editor tests don't catch a black
window). GUI verification in FL is ALWAYS a checkpoint entry, never a loop step.

**Unattended permissions:** `.claude/settings.json` in the repo allowlists cargo /
rustup / git / gh / powershell build+download commands. The loop should be launched
in bypass-permissions mode regardless; the allowlist is defense-in-depth.

**suite-core API rule:** any public-API or wrapper change ⇒ `cargo build --workspace
--release` + re-run build.ps1 gate (validate + install) for EVERY completed plugin
before ticking the current item. NERVE and X-RAY explicitly include a
"retrofit + rebuild-all + revalidate-all" step. `build.ps1 --all` exists from Phase 0.

## 3. Cross-plugin architecture — "the Bus"

(unchanged in design; constraints verified)
- Tier 1 (same-DLL): OVERSEER Node + Master exported from ONE library —
  `nih_export_vst3!`/`nih_export_clap!` accept multiple plugin types (verified in
  nih-plug source; CHANGELOG 2023-09-03). Shared `static BUS: Mutex<Registry>`.
  Caveat documented in README: FL loads same-bitness plugins in-process by default,
  but a user-ticked "Make bridged" degrades tier 1 → tier 2 is the fallback.
- Tier 2 (cross-DLL/cross-process): file-backed shared memory via `memmap2`
  (`MmapRaw` over a fixed-size file in `%TEMP%\qeynos-bus`; canonical Windows
  cross-process mechanism — no named kernel mapping needed). Fixed-layout slots,
  per-slot seqlock, atomics only, heartbeat = block counter, GC dead slots on init.
  File is created at fixed size before mapping and never grown live.
- Block-granularity only (~1–10 ms); Master-written overrides bypass host
  automation/undo — Node GUI badges them; local touch steals back.

**Suite-wide conventions:** smoothed params; 2–4x oversampling on nonlinear stages;
latency reported for FFT/lookahead plugins; DC blocker + soft-clip guard on feedback
paths; crossfaded bypass; 44.1–192k.

## 4. Offline verification harness (Phase 0 deliverable, in suite-core)

`suite-core/testsig` — generated-in-code signals ONLY (no external audio files,
ever): impulse; 1 kHz sine; 20 Hz–20 kHz log chirp; white/pink noise bursts;
synthetic kick (IMPACT's own math); **synthetic vocal** = sawtooth + 5 Hz vibrato
through 3 formant band-passes; sliding-pitch saw (808 stand-in); MIDI event script
type (timestamped note on/off); fake-transport struct (tempo, playhead, bar pos)
for ASCEND/CLEAVE/HALT.

`render_offline(dsp, input, midi, transport) -> Vec<f32>` runs the pure-DSP module
block-by-block inside `cargo test` and writes `renders/<plugin>/*.wav` via `hound`.

**Universal assertions (every plugin, every render):** no NaN/inf; peak ≤ 0 dBFS;
RMS > −60 dBFS (non-silence); mix=0 nulls against dry < −80 dB.

**Per-plugin mechanical assertions (the "done" bar — no listening required):**

| Plugin | Assert |
|---|---|
| GRIT | THD higher during SC pulses than between; auto-gain holds post-RMS within ±1 dB of pre |
| EMBER | noise burst→silence @ τ=10 s: tail at +2 s > −40 dBFS, frame-RMS monotone ↓ (±1 dB); freeze: tail RMS flat ±1 dB over 5 s |
| IMPACT | STFT f0 starts within 10% of f_start, ends within 5% of f_end; retrigger mid-decay: no step > declick threshold |
| TRACER | sliding saw: band-1 centroid tracks f0 ±1 semitone; white noise: crossovers frozen over 1 s |
| OVERSEER | +6 dBFS sine: limiter output ≤ ceiling +0.1 dB; LUFS meter ±0.5 LU vs known reference signal |
| DRIFT | dominant spectral peak strictly advances and wraps; spectra at t, t+period/N correlate |
| WIRE | 6 kbps output correlates with input less than 128 kbps output does; measured latency == reported ±1 block |
| OUROBOROS | 110% feedback, 30 s: peak ≤ 0 dBFS, no NaN, last-5 s RMS stable |
| SWARM | onset count scales monotonically with density at 3 settings |
| SMUDGE | all amounts 0: null < −60 dB; scramble > 0: frame correlation vs dry < 0.9 |
| MURMUR | two impulses 2 s apart: tail cross-correlation < 0.9; RT60 within ±25% of setting |
| FLYBY | sine input: periodic f0 deviation present; L/R RMS ratio crosses 1.0 each traversal |
| CLEAVE | 120 BPM grid: output onsets on grid ±5 ms |
| PLUCK | C-major trigger: spectral peaks at chord fundamentals ±10 cents; tail decays > 20 dB over decay setting |
| SHAPESHIFT | XY at corner A nulls against shaper A alone < −60 dB |
| CHAMBER | first reflection = direct r/c ±1 sample; late RT60 ±25% of Sabine prediction |
| CARVE | gain reduction present only in SC-active bands |
| NERVE | published mod signal measurably modulates a listening plugin's param (round-trip test) |
| HALT | tape-stop: f0 glides to < 50 Hz; stutter: loop period == division ±1 ms |
| BANDAID | per-band transient peak ratio moves with attack gain |
| PATINA | wow: f0 modulation at ~0.4 Hz measurable; noise keyed: noise floor rises with input env |
| X-RAY | reads ≥2 live slots' spectra from the bus in a two-instance test |
| CHORALE | resonator peaks at tuned pitches ±10 cents |
| UNDERTOW | rumble envelope dips ≥ duck-depth at each kick onset |
| SEANCE | +12 st shift doubles f0 ±20 cents; chop gate periods match pattern |
| ASCEND | spectral centroid rises monotonically over countdown; impact lands on target bar ±5 ms |

CHAMBER CPU rule: bench mean process() per 512-sample block @48 kHz in release mode;
> 30% of real-time budget → descope reflection order 3→2; still over → order 1 +
bigger late field.

## 5. Plugin technical specs

*(Signal flows, parameters, and DSP designs are unchanged from v1 except where noted —
see git history for v1. Amendments from adversarial review:)*

- **_template**: gains the §4 harness + testsig as part of its deliverable. Its gate
  is the Phase 0 GO/NO-GO (§2 step 10).
- **GRIT / EMBER / IMPACT / TRACER / OVERSEER / DRIFT / OUROBOROS / SWARM / SMUDGE /
  MURMUR / FLYBY / CLEAVE / PLUCK / SHAPESHIFT / CHAMBER / CARVE / NERVE / HALT /
  BANDAID / PATINA / X-RAY / CHORALE / UNDERTOW / SEANCE / ASCEND**: as specced in v1
  (flow diagrams preserved below in §6), with these deltas:
  - **EMBER fallback rule:** take the magnitude-only fallback iff the phase-vocoder
    tail assertion fails after 5 attempts; the fallback's bar is the universal
    assertions only.
  - **TRACER done gate:** synthetic sliding-saw + synthetic-vocal testsig (no
    external stems exist).
  - **WIRE codec plan (rewritten):** plan A = `opus_rs` (pure-Rust encoder+decoder,
    v0.1.x, published 2026-06; no C, no CMake). Plan B = `audiopus` built with the
    portable CMake zip from tools/bin (audiopus_sys requires CMake on Windows).
    Plan C (valve) = descope Opus; ship WIRE as bitcrush/SR-reduce/"crunch" only.
    Note: `opus-embedded` is decoder-only — NOT an option. Before writing WIRE DSP,
    build a 10-line encode/decode link-test; 3 failed attempts per plan → next plan.
  - **OVERSEER:** Node+Master one-library export verified feasible. Ozone hosting
    remains DEFERRED.md.
  - **NERVE / X-RAY:** each begins with "retrofit suite-core wrapper → rebuild-all →
    revalidate-all → reinstall-all" as an explicit checklist step (§2 API rule).
- nih-plug capabilities verified: aux sidechain inputs (`AudioIOLayout.aux_input_ports`),
  full MIDI, `set_latency_samples`, multi-plugin export, xtask bundling. Stable Rust
  OK without `simd` feature. Framework is in maintenance mode — pin the rev, expect
  no upstream fixes.

## 6. Signal-flow specs

The complete per-plugin signal flows, parameter lists, presets, and DSP algorithm
choices live in **SPECS.md** (v1 content preserved with v2 amendments applied).
Read the relevant SPECS.md section at step 1 of every iteration.

## 7. Canonical checklist (order = value-if-interrupted + dependencies; names are IDs)

Phase 0
- [x] BOOTSTRAP (toolchain, workspace, harness+testsig, validators, _template GO/NO-GO)

Phase 1 — priority plugins
- [x]* GRIT   - [x] EMBER   - [x] IMPACT   - [x] TRACER   - [x] OVERSEER

Quick wins (non-Rust, immediately usable)
- [x] W8-VITALGEN   - [x] W4-SESSION-BOOTSTRAP
- [x] **HARD CHECKPOINT 1** (2026-07-07): remediation of 7 confirmed adversarial-review
  findings — GRIT/TRACER parallel-mix comb + PDC latency (measured group delay, matched
  dry delay, set_latency_samples), OVERSEER smoother application, OVERSEER Node
  saturation aliasing (2x OS), loudness realtime cost (momentary-only + O(bins) gating),
  suite-wide denormal FTZ/DAZ. `cargo test --workspace --release` + `build.ps1 -All`
  green; shared partial-mix alignment regression helper wired into GRIT + TRACER.

Phase 2a — clones
- [x] DRIFT - [x] WIRE - [x] OUROBOROS - [x] SWARM - [x] SMUDGE - [x] MURMUR

Taste-tailored (deps satisfied: IMPACT, MURMUR-FDN, EMBER/SMUDGE engines)
- [x] UNDERTOW - [x] SNAP - [x] SEANCE - [x] ASCEND

VOX suite (user request 2026-07-07: rip lyrics from other songs, make them fit
anything — key/tempo/character conforming; SEANCE's formant-preserving shift engine
is the shared core, built immediately before)
- [x] W9-VOXRIP - [x] VOXKEY - [x] VOXFIT
- [x] UI-CORE-FIX (knobs not sliders, uniform window scaling, working click-to-type — suite-wide retrofit; user-reported defects 2026-07-07)
- [x] W1-RUMBLE-BASSLINE - [x] W2-BREAK-CHOP - [x] W3-DARK-PROGRESSION
**HARD CHECKPOINT**

Pulled forward from POLISH (user priority 2026-07-07):
- [ ] PRESET-SYSTEM (suite-wide user preset save/load + retrofit all plugins)
- [ ] OVERSEER-ENRICH (per-Node instrument type context + thematic preset banks)

Phase 2b — remaining clones
- [ ] FLYBY - [ ] CLEAVE - [ ] PLUCK - [ ] SHAPESHIFT - [ ] CHAMBER
**HARD CHECKPOINT**

Phase 3 — remainder
- [ ] CARVE - [ ] NERVE - [ ] HALT - [ ] BANDAID - [ ] PATINA - [ ] X-RAY - [ ] CHORALE

Phase 4 — remaining automations
- [ ] W5-PROJECT-JANITOR - [ ] W6-SAMPLE-LIBRARIAN - [ ] W7-REFERENCE-GAP

POLISH phase (user feedback 2026-07-07; PRESET-SYSTEM + OVERSEER-ENRICH pulled
forward to after HARD CHECKPOINT 2 — see above)
- [ ] PRESET-EXPANSION (deep factory banks, 15-30 purpose-named presets per plugin)
- [ ] BUILT-IN-MANUALS (in-GUI usage manual panel per plugin, embedded from docs/)
- [ ] PEDAL-UI (LOCKED: CONSOLE v2 — pedal + amber CRT terminal; usability guardrails in SPECS override aesthetics)
**FINAL CHECKPOINT**

## 8. Model & token policy (user directive 2026-07-07)

- **Opus is the default engine.** Launch the loop session on Opus
  (`claude --model opus` in C:\dev\qeynos-vst-suite, bypass-permissions mode).
  All routine iteration work — crate scaffolding, DSP implementation, params, GUI,
  presets, docs, test writing, build/fix cycles, Phase 4 tools — runs on Opus.
  Subagents spawned for parallel/mechanical work default to Opus too (haiku is fine
  for pure-mechanical text chores like README rows).
- **Fable escalation valve (inline with §1.5 attempt counters):** when the SAME error
  signature survives 3 Opus attempts, spawn ONE Fable subagent
  (`model: "fable"`) scoped to that specific problem, apply its fix, drop back to
  Opus. Also Fable-eligible without waiting for failures: the four hardest specced
  problems — EMBER's phase-vocoder tail, TRACER's time-varying LR4 stability,
  the Bus seqlock layout, OVERSEER's limiter. Nothing else.
- **Ultracode (multi-agent workflows): most important administrative tasks ONLY.**
  Whitelist — exactly these, nothing else qualifies:
  1. Phase 0 GO/NO-GO verdict (adversarial verification that the toolchain gate
     genuinely passed, since everything rides on it);
  2. each HARD CHECKPOINT in §7 (parallel re-validation sweep of all shipped
     plugins + adversarial review of the phase, before declaring it green);
  3. the FINAL CHECKPOINT (full-suite audit).
  Never use workflows for building individual plugins. Log every ultracode use in
  STATUS.md with a one-line justification.
- Record token-notable events (a plugin that burned >3 escalations, a checkpoint
  sweep) in the STATUS.md LOG so the user can audit spend cold.

Phase 4 notes: Python via uv (`uv python install 3.12`; PEP 723 headers pin
`requires-python = ">=3.12,<3.13"` — librosa/numba on 3.14 are bleeding-edge).
W1–W5 need FL running with the MCP controller: if `fl_connection_status` fails,
build the tool against recorded fixtures, mark live verification in CHECKPOINTS.md,
move on. W8 validates generated presets against a preset SAVED BY THE INSTALLED
Vital (1.5.x) — the OSS repo tracks ~1.0.7; diff both before trusting the schema.
Verify Vital is installed (and locate its preset dir via known-folder Documents)
as a W8 preflight.
