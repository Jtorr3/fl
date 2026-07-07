# Qeynos Audio Suite

A suite of CLAP/VST3 audio plugins (built on [nih-plug](https://github.com/robbert-vdh/nih-plug))
plus FL Studio automation tools, built autonomously. See `PRD.md` for the design and
execution playbook, `STATUS.md` for current progress, and `SPECS.md` for per-plugin
DSP specs.

CLAP bundles install to `%LOCALAPPDATA%\Programs\Common\CLAP\Qeynos\` (per-user, no
admin; FL Studio ≥ 2024.1 scans it). VST3 installs alongside only if the optional
admin junction exists (see `CHECKPOINTS.md`).

## Plugins

| Plugin | Type | Summary | Docs |
|---|---|---|---|
| Qeynos Template | Utility | Hello-gain reference (Phase 0 gate); one smoothed gain + peak meter | — |
| GRIT | Distortion | Sidechained distortion — envelope- and waveshape-driven saturation, 4x oversampled, auto-gain, dry/wet | [docs/GRIT.md](docs/GRIT.md) |

## Building

```powershell
powershell -ExecutionPolicy Bypass -File build.ps1 <crate>   # e.g. grit
powershell -ExecutionPolicy Bypass -File build.ps1 -All
```

Each crate builds (release) → tests → bundles `.clap`+`.vst3` → validates with
clap-validator + pluginval (strictness 8) → installs. Requires `tools/bin/mingw64`
(portable MinGW-w64 binutils; gitignored) — `build.ps1` puts it on PATH automatically.

Offline audition renders are written to `renders/<plugin>/*.wav` by the crate tests.
