# CHECKPOINTS — human-only actions (the loop writes here and continues; nothing blocks on these)

## Before launching the loop (optional but recommended)
- [ ] Launch Claude Code in bypass-permissions mode for the run (the repo's
      .claude/settings.json allowlist is defense-in-depth, not sufficient alone).
- [x] DONE 2026-07-07 (was required after all — FL 21 has no CLAP): open an elevated
      prompt and run:
      `mklink /J "C:\Program Files\Common Files\VST3\Qeynos" "C:\dev\qeynos-vst-suite\dist\vst3"`
      Without this, plugins ship as CLAP only — FL Studio ≥ 2024.1 scans CLAP fine.
- [ ] OPTIONAL insurance: install VS Build Tools (C++ workload) manually if you want
      an MSVC fallback to exist. The loop will never try to install it itself.

## During / after the run (whenever you're at the machine)
- [ ] **W8-VITALGEN (shipped 2026-07-07): open Vital, load the Qeynos bank preset.**
      A fixture-built preset is already written to
      `Documents\Vital\User\Qeynos\Drowned_Grief_Pad.vital` (real 1.5.5 base +
      constrained overrides, passes offline `vitalgen validate`). Open Vital 1.5.5 ->
      browser -> User -> Qeynos -> load it to confirm it opens without error and sounds
      like a slow-attack drowned pad. (GUI load is human-only; the loop can't launch
      Vital's GUI.)
- [ ] **W8-VITALGEN: set ANTHROPIC_API_KEY and smoke-test live generation.** No key
      was set on the build machine, so the live Claude API path was not exercised
      (all offline tests pass). Set `ANTHROPIC_API_KEY`, then run:
      `uv run --python 3.12 tools\vitalgen\vitalgen\vitalgen.py generate "cavernous mid bass" --bank Qeynos`
      (uv is at `%USERPROFILE%\.local\bin\uv.exe`, not on PATH). The offline
      `test_vitalgen.py` live smoke test also auto-runs once the key is present.

- [ ] **W9-VOXRIP (shipped 2026-07-07): audition ripped/conformed vocals.** The tool is
      a standalone Python CLI (no FL rescan needed). Both live paths were verified on
      the build machine: demucs (htdemucs, CPU) separated a track into
      `vocals_raw.wav` + `instrumental.wav`, and rubberband (`rubberband-3.3.0` portable,
      `-F` formant-preserving) conformed a synthetic acapella (C#m/99 BPM → F#m/128 BPM,
      +5 st). Run it on a REAL song and listen:
      `uv run --python 3.12 tools\voxrip\voxrip.py "C:\path\song.mp3" --target-bpm 174 --target-key "F#m" --out .\ripped`
      (uv at `%USERPROFILE%\.local\bin\uv.exe`, not on PATH). First run downloads torch
      (~200 MB, CPU) + htdemucs weights; both cache afterwards. Audition
      `ripped\<song>\vocals_conformed.wav` over the target track and read `REPORT.md`
      for the detected BPM/key + chosen transposition. The rubberband binary self-installs
      into `tools\bin\rubberband\` (gitignored) on first conform.
- [ ] **BOOTSTRAP (done 2026-07-07): FL rescan needed.** "Qeynos Template" CLAP is
      installed at `%LOCALAPPDATA%\Programs\Common\CLAP\Qeynos\_template.clap`.
      In FL: Options → Manage plugins → "Find more plugins", then load
      "Qeynos Template" to confirm the GUI opens (OpenGL) and the gain knob works.
- [ ] **GRIT (shipped 2026-07-07): FL rescan + GUI/sidechain spot-check.** "Qeynos GRIT"
      CLAP installed at `%LOCALAPPDATA%\Programs\Common\CLAP\Qeynos\grit.clap`. In FL:
      Find more plugins → add GRIT to a track, route a kick to its **sidechain** input,
      load the "Kick Bass Grit" preset, confirm the GUI opens (all params + preset combo
      + SC Listen / Auto-Gain toggles) and the distortion pumps with the kick.
- [ ] **OVERSEER (shipped 2026-07-07): FL rescan + two-plugin link check.** ONE bundle
      (`overseer.clap`) installs BOTH "Qeynos OVERSEER Node" and "Qeynos OVERSEER Master".
      In FL: Find more plugins → put **Node** on 2–3 tracks (set each LABEL in its GUI,
      e.g. "KICK"), put **Master** on the master track. The Master GUI's NODES grid
      should list every Node live with meters; drag a Node's THRESH/DRIVE slider there →
      the Node shows an `OVR` badge and its sound changes; touch the same param on the
      Node → control steals back. IMPORTANT: leave "Make bridged" UNTICKED on all
      OVERSEER instances (bridging severs the same-process link; audio still works).
      Limiter check: hot signal into Master, ceiling −1 dB → output never clips past it.
- [ ] **W4-SESSION-BOOTSTRAP (shipped 2026-07-07): live-apply smoke test.** Offline
      gate is green (47 checks) but the live apply could NOT be run: on the build
      machine `fl_connection_status` reported "connected" (the loopMIDI port opens),
      but every real FL command timed out — FL Studio was not running with the
      FLStudioMCP controller actually responding. To verify live: start FL Studio,
      enable the FLStudioMCP controller on the loopMIDI port (port enabled in BOTH
      MIDI Input and Output with the same number), open/confirm a session, then run:
      `uv run --python 3.12 tools\session_bootstrap.py apply TECHNO`
      (uv at `%USERPROFILE%\.local\bin\uv.exe`, not on PATH). Expect mixer tracks
      1–11 to be renamed KICK/RUMBLE/BASS/PERC/HATS/ATMOS/LEAD/CHORD/FX/REVERB/DELAY
      and recolored (dark scheme), loop mode → pattern, and channels 0–8 routed to
      tracks 1–9. It only names/colors/routes (non-destructive) and is idempotent.
      Routing ops for channels that don't exist in the rack yet will report as
      warnings — that's expected. Preview any time without FL via `apply TECHNO
      --dry-run`. (`tempo` is intentionally not applied — no MCP command exists.)
- [ ] **HARD CHECKPOINT (after ASCEND, 2026-07-07): audition the transport-synced riser in FL.**
      ASCEND is the suite's first transport-reading instrument, so the one thing offline tests
      cannot prove is that FL actually feeds it tempo + bar position. In FL: "Find more plugins",
      then add **Qeynos ASCEND** as an instrument. Load **Riser 8 Dark**, PLAY the transport, and
      confirm: (a) the GUI's COUNTDOWN readout counts down the bars-remaining in sync with the
      playhead, (b) the sound sweeps up over 8 bars and drops an **impact** on the downbeat of the
      target bar with a clean **auto-cut**, re-arming each 8 bars. Try **Riser 16 Wide** (longer,
      wider), **Sub Boom Drop** (big low drop), **Downlifter 8** (reversed fall after the drop),
      **Noise Swell Short** (no impact). Then STOP the transport and click **TRIGGER** (or play a
      note; tick **Key Track** to pitch it) to confirm free-run works standalone over **Free
      Length** seconds. If FL swallows the momentary Trigger via keyboard, use automation or the
      knob. (build.ps1 ascend is GREEN — clap-validator + pluginval s8 pass; this listen is the
      human confirmation that the host transport wiring behaves.) Renders in renders/ASCEND/.
- [ ] **HARD CHECKPOINT 1 (2026-07-07): re-test GRIT/TRACER parallel (dry/wet) mix in FL.**
      The comb-filtering + wrong-PDC defects were fixed: GRIT (4x OS, 22-sample latency)
      and TRACER (per-band 2x OS, 14-sample latency) now delay-compensate their dry paths
      and report `set_latency_samples`, and OVERSEER Node now reports 14 samples (its
      saturation is 2x-oversampled). Reinstalled CLAPs are at
      `%LOCALAPPDATA%\Programs\Common\CLAP\Qeynos\`. In FL: put GRIT (or TRACER) on a
      track, set **MIX ≈ 50%** with a light/neutral wet, and confirm the parallel blend
      sounds full (no hollow comb) and that FL's automatic plugin-delay-compensation keeps
      it phase-aligned with dry sibling tracks. Also sanity-check OVERSEER Node at partial
      MIX. (Latency/alignment is asserted offline; the FL listen is the human confirmation.)
- [ ] FL Studio: Options → Manage plugins → "Find more plugins" after new installs
      (FL never auto-detects new plugins). NEW: **Qeynos SEANCE** (ethereal vocal machine).
- [ ] SEANCE GUI/listen: add on a vocal/lead/pad; try presets (Grief Pad Vox, Drowned
      Lead, Whisper Choir, Formant Ghost, Chopped Ether, Sunken Chorus); confirm Pitch/
      Formant move independently, chopper locks to tempo, and the ducker swells between
      phrases. Host should auto-comp +2048-sample latency. Renders in renders/SEANCE/.
- [ ] Freeze Mix spot-check (EMBER / MURMUR / OUROBOROS / SWARM): engage FREEZE, then
      pull the new FREEZE MIX slider below 100% — the live source should blend back in
      under the frozen tail (100% = the previous instant-freeze behavior). GUI-only add.
- [ ] Audition `renders/<plugin>/*.wav` — automated assertions check math, not taste.
- [ ] Spot-check each plugin GUI inside FL (OpenGL/DPI quirks aren't machine-testable).
- [ ] Delete the orphaned GitHub repo Jtorr3/qeynos-vst-suite (my token lacks
      delete_repo scope). Also decide if Jtorr3/fl should be private (it is public).
- [ ] **W1-RUMBLE-BASSLINE (shipped 2026-07-07): run the piano-roll script in FL.**
      Copied to `Documents\Image-Line\FL Studio\Settings\Piano roll scripts\RumbleBassline.pyscript`
      (alongside ComposeWithLLM). Offline gate passes 12/12 via a mock `flpianoroll`,
      but FL's piano-roll script engine cannot be driven headless — so verify live:
      open the Piano roll on a bass channel, **Tools → Scripting → Rumble Bassline**,
      confirm the dialog shows all inputs (root/octave/scale/pattern/bars/lengths/
      velocities/humanize/fills/seed), click OK, and check the generated notes sit
      between the kicks (offbeats accented, on-beat notes ghosted). Try each pattern
      (Offbeat 8ths / Rolling 16ths / Gallop / Broken). Notes APPEND (clear the roll
      first for a fresh pattern) and start at the timeline selection if one is set.
      If the script doesn't appear in the menu, FL may need a rescan / restart, or the
      Documents redirect (OneDrive) may point elsewhere than FL's configured user data
      folder — check FL's *File settings → user data folder* and copy it there.
- [ ] **W2-BREAK-CHOP (shipped 2026-07-07): run the piano-roll script in FL.**
      Copied to `Documents\Image-Line\FL Studio\Settings\Piano roll scripts\BreakChop.pyscript`
      (alongside RumbleBassline + ComposeWithLLM; byte-for-byte copy verified). Offline
      gate passes 16/16 via the shared mock `flpianoroll`, but FL can't run piano-roll
      scripts headless — so verify live: open the Piano roll on a channel with a sliced
      break (Fruity Slicer slice-notes) or any note run, **select** the notes to chop
      (Ctrl+A = all), **Tools → Scripting → Break Chop**, confirm the dialog shows all
      inputs (intensity / permute / roll chance-count-decay / stutter chance-gate /
      reverse chance / keep-first-beat / humanize / seed), click OK. Check: only the
      SELECTED notes are rewritten (unselected untouched); the chop re-fills the SAME
      span (loop point intact); the downbeat stays put with Keep-first-beat on; rolls
      are rapid decaying retriggers; reverse renders as a fast double-time repeat (FL's
      note API has no reverse flag — documented). Same Seed → identical result. If the
      script isn't in the menu, rescan/restart FL or check FL's *File settings → user
      data folder* matches the OneDrive-redirected Documents path.
- [ ] **W3-DARK-PROGRESSION (shipped 2026-07-07): run the piano-roll script in FL.**
      Copied to `Documents\Image-Line\FL Studio\Settings\Piano roll scripts\DarkProgression.pyscript`
      (alongside RumbleBassline + BreakChop + ComposeWithLLM; byte-for-byte copy
      verified). Offline gate passes 19/19 (44 asserts) via the shared mock
      `flpianoroll`, but FL can't run piano-roll scripts headless — so verify live:
      open the Piano roll on a **pad/keys channel**, **Tools → Scripting → Dark
      Progression**, confirm the dialog shows all inputs (root/octave, scale
      [natural minor / phrygian / harmonic minor], progression preset [Dark Pop /
      Hypnotic / Tension / Wander / Random], bars-per-chord, total bars, voicing
      [triad / 7th / add9], voice-leading toggle, arp [off/up/down/up-down/random],
      arp rate / octave span / gate %, suspension %, velocity base + humanize,
      timing humanize, seed), click OK. Check: chords are **in-key** and land on the
      bars-per-chord grid; with **Voice leading ON** the chords move by small
      inversions (hold hands) instead of jumping to root position; the **arp** (when
      not Off) sits **above** the pad, follows the chord tones, and locks to the
      8th/16th grid with the gate shortening each note. Notes **APPEND** at the
      timeline selection start (or tick 0) — clear the roll (Ctrl+A, Delete) first
      for a fresh progression. Same **Seed** → identical result; try the **Tension**
      preset with the **Phrygian** scale for the bII colour. If the script isn't in
      the menu, rescan/restart FL or check FL's *File settings → user data folder*
      matches the OneDrive-redirected Documents path.

## Toolchain note (informational — the loop handles it, but a fresh clone won't)
- `tools/bin/` is gitignored, including `tools/bin/mingw64` (portable MinGW-w64
  binutils, winlibs 16.1.0-ucrt). This is REQUIRED to build: the rustup windows-gnu
  toolchain ships `dlltool` but no assembler, so raw-dylib import libraries fail
  without it. `build.ps1` prepends `tools\bin\mingw64\bin` to PATH automatically.
  If you re-provision the machine, re-download winlibs into `tools/bin/mingw64`
  (or any full MinGW-w64 providing as.exe/dlltool.exe/ld.exe on PATH).

## UI-CORE-FIX — GUI interaction (verify in FL; headless gate skips GUI tests)
- [ ] **Knobs + typing (every plugin).** Open any Qeynos plugin editor in FL. Confirm the
      new **rotary knobs**: drag up/down to change, **Ctrl-drag** for fine (~10×),
      **double-click** to reset to default, **scroll** to step. Then **click a value
      readout** and type an exact value (e.g. a dB/Hz/note), press **Enter** — it should
      commit; **Esc** cancels; clicking away commits. (Parse/commit + scale math are
      unit-tested in `suite_core::ui`; pluginval runs `--skip-gui-tests`, so the live
      window/keyboard behavior is only verifiable by hand in FL.)
- [ ] **FL keyboard focus toggle (if typing does nothing).** FL's wrapper can swallow
      computer-keyboard keys for *Typing keyboard to piano*. If typed digits don't reach a
      Qeynos knob's text field, on the plugin wrapper title bar enable **"Allow the plugin
      to steal keyboard focus"** (turn off *Typing keyboard to piano* while the editor is
      focused). Once flipped, Enter-to-commit should work. (docs/UI.md documents this.)
- [ ] **Uniform scaling.** Drag a plugin window's bottom-right corner — the whole editor
      should **zoom as one unit** (no layout reflow), snapping near **75/100/125/150 %**.
      The corner **size menu** (top-right, shows current %) lists the snap points. Close and
      reopen the project — the chosen size should be restored. NOTE: the menu snaps the
      current window's zoom; to enlarge the OS window use the corner drag (DEFERRED.md
      documents the missing host-resize API).

## HARD CHECKPOINT 3 — Phase 2b re-validation (2026-07-08)
- [ ] **Rescan FL for the Phase 2b clones.** All five rebuilt + reinstalled by the checkpoint
      sweep: Options → Manage plugins → Find more plugins → verify **Qeynos FLYBY / CLEAVE /
      PLUCK / SHAPESHIFT / CHAMBER** load. (CLEAVE got an audio-thread crash fix — the transient
      slicer could panic on ≥128 detected onsets; PLUCK's body IR is now the full 2048-tap spec
      body and its MIDI tuning path no longer allocates on the audio thread.)
- [ ] **CLEAVE stress spot-check (the fixed blocker).** On a busy percussion loop at a slow
      tempo (e.g. 60–70 BPM), set Slice Mode = **Transient** and Sensitivity to max — the old
      build could hard-crash FL's audio thread here; the fixed build must keep chopping (slices
      cap at 128).
- [ ] **Preset names.** Saving a preset named `NUL`, `CON`, `COM5` etc. now lands on disk as
      `NUL_` / `CON_` / `COM5_` instead of silently vanishing into a Windows device name.

## FINAL CHECKPOINT (2026-07-08) — full-suite verify in FL

### PEDAL-UI — CONSOLE v2 skin + THEME prefs
- [ ] **Skin renders in FL.** Reopen any few Qeynos plugins → each is a dark pedal enclosure
      (machined body, amber brand strip flush between the top screws, corner screws, tick-ringed
      amber knobs, LED footswitch toggles) with a recessed amber CRT bay showing that plugin's
      live telemetry. Confirm nothing paints over the preset bar when you **scroll a tall editor**
      (e.g. NERVE at 150 % zoom — the clip-escape bug is fixed) and the CRT cursor no longer sits
      on top of the last telemetry row.
- [ ] **Size-menu THEME toggles (try on ~2 plugins).** Click the top-right **NN%** menu → THEME:
      toggle **CRT motion** (kills the cursor blink / freezes animated scopes to ~8 fps idle) and
      **Console skin** (instant plain-dark fallback — the enclosure/CRT vanish, all knobs/values
      still work). Both should apply immediately.
- [ ] **Prefs persist across reopen.** Turn Console skin OFF on one plugin and CRT motion OFF on
      another, close both editors, reopen → each comes back with your setting. (Stored in
      `Documents\Qeynos\ui_prefs.json`; the write is now atomic + merge-safe, so two open editors
      no longer clobber each other and a corrupt file won't wipe everyone's prefs.)

### BUILT-IN-MANUALS — the '?' panel
- [ ] **Open the '?' on a few plugins** (top-right of every editor) → a closable/scrollable
      manual window (What It Is / Signal Flow / Controls / Recipes) should render, and every
      control on the panel should be described in the Controls section.

### SUITE-TRIAGE — re-test the fixed "broken" behaviors in FL
The SUITE-TRIAGE fix pass repaired 13 behaviors the audit called broken/weak. Spot-check them
on real material (all rebuilt/reinstalled):
- [ ] **NERVE → GRIT drive** — route a NERVE LFO (S1) to GRIT's DRIVE and hear it pump in time.
- [ ] **CLEAVE 1-bar loop** — feed a 1-bar drum loop, transport playing → slices lock to the
      step grid, no audio-thread crash at max Sensitivity / Transient mode.
- [ ] **IMPACT / SNAP key-track** — play a bassline/melody in → the tuned layer tracks the note.
- [ ] **VOXFIT knob response** — knobs move the sound as labeled (no dead/no-op controls).
- [ ] **SEANCE grid chop** — the transport-locked chopper cuts on the grid.
- [ ] **Freeze toggles** — freeze/hold controls (X-RAY Freeze, and any plugin buffer-freeze)
      hold the display/buffer as expected.

## NERVE — suite modulation bus (2026-07-08)
- [ ] **Rescan FL** (Options → Manage plugins → Find more plugins) → verify **Qeynos NERVE**
      loads (CLAP + VST3 both installed; every other Qeynos plugin was also rebuilt/reinstalled
      by the retrofit's rebuild-all).
- [ ] **Cross-plugin modulation smoke test.** Put **NERVE** on any track, label it, load
      **Techno Pump 1/8** with the transport playing. Open **GRIT** (or any retrofitted plugin)
      on another track → expand the **MOD** section under its preset bar → route DRIVE to the
      NERVE instance, signal S1, depth ~0.4 → the drive should audibly pump at 1/8ths while the
      knob (and host automation) stays at its base value.
- [ ] **Bridging caveat (informational):** cross-plugin modulation runs over a shared-memory
      file (`%TEMP%\qeynos-bus`), so plugins should be **un-bridged** (FL default). Bridged
      instances still map the same OS-wide file, but keeping the suite un-bridged is the
      supported configuration (and what tier-1 OVERSEER already requires).
- [ ] **Session-scoped routes:** MOD routes point at a NERVE *session* identity — after
      reloading a project, re-pick the source in each MOD row (labels make this quick). This is
      deliberate (a persisted random id breaks CLAP state reproducibility); a stable-id scheme
      is a candidate follow-up.
- [ ] **Not every plugin is a modulation target (by design).** OVERSEER (Node + Master),
      EMBER, SEANCE, VOXFIT and VOXKEY have **no MOD section** — they never grew the deferred
      NERVE *listener* layer, so they cannot receive NERVE modulation (they have no MOD row to
      route into). Everything with a MOD section under its preset bar is a valid target; these
      five are not. Nothing to do here — noted so you don't hunt for a MOD section that isn't there.

## X-RAY — shared cross-plugin analyzer (2026-07-08)
- [ ] **Rescan FL** (Options → Manage plugins → Find more plugins) → verify **Qeynos X-RAY**
      loads (CLAP + VST3 both installed; every other Qeynos plugin was rebuilt/reinstalled by
      the publishing retrofit's rebuild-all).
- [ ] **Overlay smoke test.** Put **X-RAY** on the master (or any bus) and 2–3 other Qeynos
      plugins (e.g. GRIT on a bass, PATINA on drums, MURMUR on a send) on playing tracks. Open
      X-RAY's window → one colored curve per live instance should appear on the log-freq grid,
      with a legend row (name · bus id · peak/RMS) for each. **Hover** a legend row → the other
      curves dim; **click** it → solo-dim persists; **Freeze** → the display holds.
- [ ] **Publishers go stale correctly:** stop the transport (FL keeps processing, curves stay)
      then delete/bypass a publishing plugin → its curve should drop off within ~3 s
      (heartbeat GC), not linger.
- [ ] **Bridging caveat (informational):** the spectrum bus is the same OS-wide shared file as
      NERVE's (`%TEMP%\qeynos-bus`), so even bridged instances publish into it — but keep the
      suite un-bridged (FL default) as the supported configuration. OVERSEER / NERVE / _template
      do not publish spectra (DEFERRED.md).

## W5 — PROJECT-JANITOR (2026-07-08)
- [ ] **Live scan/apply (FL was NOT live when W5 shipped).** `fl_get_channel_count`
      returned -1 and `fl_get_all_channels` timed out this session (the MCP
      `fl_connection_status` can report connected when it is not — verify with a real
      read command, as the loop did). W5 shipped fixture-verified only (52 offline
      checks green). When FL is running with the FLStudioMCP controller (loopMIDI
      enabled in BOTH MIDI Input and Output at the same port): open a messy project,
      run `uv run --python 3.12 tools\project_janitor.py` to preview the rename/recolor
      plan, sanity-check the classifications, then `--apply`. Confirm channels + mixer
      tracks get canonical names (KICK/SNARE/HAT/PERC/BASS/VOX/PAD/LEAD/FX/CLAP) and
      category colors, Master is untouched, and a second `--apply` reports 0 ops
      (idempotent).

## W6 — SAMPLE-LIBRARIAN (2026-07-08)
- [ ] **Optional real-library audition (no hardware dependency — fully offline-tested).**
      W6 is a pure file tool; its 37-check offline gate exercises real BPM/key analysis on
      synthesized WAVs + a tmp-dir plan/apply/undo round-trip, so this is a nice-to-have, not
      a blocker. When convenient, point it at a real pack:
      `uv run --python 3.12 tools\sample_librarian.py sort "D:\Samples\unsorted"` (dry-run) to
      eyeball categories/BPM/key, then `--apply`. Confirm the undo manifest restores the tree
      (`sample_librarian.py undo <manifest>`). First run downloads librosa/soundfile into a uv
      env (~26 pkgs).

## W7 — REFERENCE-GAP (2026-07-08)
- [ ] **Optional real-track audition (no hardware dependency — fully offline-tested + a
      real end-to-end CLI run was verified this session).** W7 is a pure analysis tool; its
      24-check offline gate covers LUFS/spectrum/width/kick-f0 on synthesized fixtures and the
      self-contained HTML output, and a real run over synthesized ref/mix WAVs produced a valid
      7.4 KB report (kick 54.7 Hz≈A1, LUFS delta +4.7 LU, low-band spectral gaps flagged). When
      convenient, run it on a real reference vs one of your renders under renders\:
      `uv run --python 3.12 tools\reference_gap.py "<reference>.wav" "renders\<mix>.wav" --key <root>`
      and open the generated `*_refgap.html`. First run downloads pyloudnorm/numpy/scipy/soundfile
      into a uv env.

## SUITE-TRIAGE (2026-07-08) — CUT DECISIONS NEEDED (nothing was uninstalled)
The Fable audit (docs/TRIAGE-2026-07-08.md + docs/triage/) flagged 5 plugins as
CUT-CANDIDATES for YOUR call. All stay installed until you decide; PEDAL-UI skins
them last. Reply in-session or delete their dist/ + VST3 entries yourself:
- [ ] MURMUR - third reverb in the suite (CHAMBER + SEANCE + FL stock); its per-onset
      re-randomized room is subtle at default mix. Cut or keep as texture-verb?
- [ ] ASCEND - auto-fires a riser every 8/16/32 bars from bar 0 while playing and
      cannot be triggered mid-playback; samples/Vital patches beat it. Cut, or
      redesign as a MIDI-triggered one-shot?
- [ ] PLUCK - superseded by CHORALE (same KS core, sympathetic + wet solo); its only
      unique trick is the staggered strum. Cut?
- [ ] SHAPESHIFT - 4th+ distortion (GRIT/TRACER/PATINA/WIRE + FL Distructor); XY
      orbit is the only novelty. Cut? (Its mirrored-XY bug gets fixed regardless.)
- [ ] NERVE - FL Peak/Envelope Controller do cross-plugin modulation natively AND
      persist with the project; NERVE routes die on reload by design. The id-collision
      bug gets fixed regardless (X-RAY shares the bus). Cut NERVE the plugin, or keep
      as the suite's LFO brain and accept re-picking routes per session?

## SOUND-PASS diagnostic findings (2026-07-08 PM, surfaced during the audition pass)
- [x] **RESOLVED 2026-07-08 — OVERSEER Master theme stuck on "Generic" + ASSIST/SUGGEST-ONLY inert when no Node instances are placed.** Root cause (diagnosed read-only): the Master inferred its theme ONLY from live OVERSEER *Node* reports on the bus; with just the Master on the mix bus and no Nodes on individual tracks, infer_theme() returned Generic (classify.rs ~922) → theme_assist_targets(Generic) = all-zero moves → ASSIST knob and SUGGEST-ONLY did nothing by construction. FIX (shipped): the Master now falls back to inferring the theme from its OWN mix-bus analysis when no Nodes report — new `suite_core::classify::infer_theme_from_mix(&FeatureSummary, &MixAnalysis)` classifies dark-techno/dnb-breaks/ambient/house from the mix's own sub-weight/tilt/onset/sustain/width + transport tempo (the FeatureSummary the Master already computes each block; no per-sample cost). Both Master call sites (`master.rs::update_assist`, `lib.rs::master_enrich_ui`) route to it only when `nodes.is_empty()`; the Node-present path (`infer_theme`) is unchanged (Nodes refine it). Gated on a higher confidence floor (`MIX_FALLBACK_CONF_FLOOR = 0.5`) so a murky mix stays Generic rather than guess wrong. Regression: `overseer::audit::audit_master_theme_fallback_from_mix` (kick+reese mix → DARK-TECHNO@1.00, break+pad@174 → DNB-BREAKS@0.60, assist non-zero, ASSIST moves output toward ref, Node path unchanged) + `suite-core classify::tests::mix_fallback_*`. Gates GREEN: `cargo test --workspace --release` 0 failures + `build.ps1 -All` 30/30. **FL human-verify (I can't run FL):** put a **Qeynos OVERSEER Master** ALONE on the master bus of a dark techno mix (kick + reese/sub, NO Node instances on tracks), open its editor — the THEME readout should now show **DARK-TECHNO** (with a confidence %) instead of GENERIC, the ASSIST knob should audibly shape the master (raise it to ~40%), and the SUGGEST card should list non-zero moves. Then drop a **Node** on the kick — the Master should switch to the (refined) Node-report inference. On an ambiguous/quiet mix the Master may correctly stay GENERIC.
- [ ] **RE-VERIFY 2026-07-08 — OVERSEER Master gain-staging + LEARN-THEME fix (drop Master alone → LOUDER/CLEANER not quieter; LEARN THEME → non-Generic + audible ASSIST).** Two user-reported defects fixed (rebuilt/reinstalled by the DSP commit): (1) enabling the Master at default no longer degrades the mix — it now has a **maximizer Drive** (0–18 dB, default 6) that lifts loudness toward the ceiling ahead of the limiter, plus gentle-glue comp defaults and a wet DC blocker. In FL: put **Qeynos OVERSEER Master** alone on a bus/master over a normal mix (~−16…−12 LUFS in), toggle **Mix** 0↔100 % (or bypass) — enabled should be **louder and cleaner, NOT quieter/duller** (measured on synth mixes: kick+reese −14.2→−9.0 LUFS-I, break+pad −18.3→−11.9 LUFS-I, true-peak held at the −1 dBTP ceiling, crest preserved). The new **DRIVE** knob sits in the LIMITER/OUTPUT row (0 dB = neutral/off; raise for more loudness). (2) **LEARN THEME** now locks a real theme on characterful material: play the fullest ~12 s and press **LEARN THEME** on a dark-techno (kick+reese) or dnb (break) mix — the THEME readout should lock **DARK-TECHNO / DNB-BREAKS** (not GENERIC) and the ASSIST knob + SUGGEST card should have non-zero moves (the mix-fallback confidence floor was lowered 0.5→0.42 and dnb separation retuned; a genuinely ambiguous/murky mix may still stay GENERIC by design). Verify a NEUTRALIZED Master (Drive 0, flat EQ, ratio-1 comp, high ceiling, Mix 0) still passes clean.
- [ ] **FL Studio MCP scripts (separate repo `!!!!FLSTUDIOMCP(cluade)`, NOT the VST suite):** the 5 modified files are your in-progress SysEx-protocol rewrite (migrating off the JSON-file handshake since FL 21's sandbox blocks file access), 801/604 uncommitted. All 5 pass py_compile (no syntax break) — the issue is runtime/integration in the half-finished rewrite (device OnSysEx decoder vs server frame encoder agreement, loopMIDI port matching, or frame reassembly). Needs FL running with the controller enabled for live round-trip testing to finish. Out of scope for the VST SOUND-PASS; flagged so it isn't lost.
