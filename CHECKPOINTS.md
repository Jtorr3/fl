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
- [ ] FL Studio: Options → Manage plugins → "Find more plugins" after new installs
      (FL never auto-detects new plugins).
- [ ] Audition `renders/<plugin>/*.wav` — automated assertions check math, not taste.
- [ ] Spot-check each plugin GUI inside FL (OpenGL/DPI quirks aren't machine-testable).
- [ ] Delete the orphaned GitHub repo Jtorr3/qeynos-vst-suite (my token lacks
      delete_repo scope). Also decide if Jtorr3/fl should be private (it is public).
