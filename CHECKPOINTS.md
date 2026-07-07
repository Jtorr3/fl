# CHECKPOINTS — human-only actions (the loop writes here and continues; nothing blocks on these)

## Before launching the loop (optional but recommended)
- [ ] Launch Claude Code in bypass-permissions mode for the run (the repo's
      .claude/settings.json allowlist is defense-in-depth, not sufficient alone).
- [ ] OPTIONAL (admin, one time, enables VST3 alongside CLAP): open an elevated
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
- [ ] FL Studio: Options → Manage plugins → "Find more plugins" after new installs
      (FL never auto-detects new plugins).
- [ ] Audition `renders/<plugin>/*.wav` — automated assertions check math, not taste.
- [ ] Spot-check each plugin GUI inside FL (OpenGL/DPI quirks aren't machine-testable).
- [ ] Delete the orphaned GitHub repo Jtorr3/qeynos-vst-suite (my token lacks
      delete_repo scope). Also decide if Jtorr3/fl should be private (it is public).

## Toolchain note (informational — the loop handles it, but a fresh clone won't)
- `tools/bin/` is gitignored, including `tools/bin/mingw64` (portable MinGW-w64
  binutils, winlibs 16.1.0-ucrt). This is REQUIRED to build: the rustup windows-gnu
  toolchain ships `dlltool` but no assembler, so raw-dylib import libraries fail
  without it. `build.ps1` prepends `tools\bin\mingw64\bin` to PATH automatically.
  If you re-provision the machine, re-download winlibs into `tools/bin/mingw64`
  (or any full MinGW-w64 providing as.exe/dlltool.exe/ld.exe on PATH).
