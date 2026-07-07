# Graph Report - .  (2026-07-07)

## Corpus Check
- Corpus is ~5,785 words - fits in a single context window. You may not need a graph.

## Summary
- 57 nodes · 58 edges · 13 communities (7 shown, 6 thin omitted)
- Extraction: 91% EXTRACTED · 9% INFERRED · 0% AMBIGUOUS · INFERRED: 5 edges (avg confidence: 0.81)
- Token cost: 71,777 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Standalone FX Plugins|Standalone FX Plugins]]
- [[_COMMUNITY_Reverb & Suite Core|Reverb & Suite Core]]
- [[_COMMUNITY_Build Pipeline & Tooling|Build Pipeline & Tooling]]
- [[_COMMUNITY_Distortion Family|Distortion Family]]
- [[_COMMUNITY_Spectral Engine Family|Spectral Engine Family]]
- [[_COMMUNITY_Shared Bus Architecture|Shared Bus Architecture]]
- [[_COMMUNITY_FL Studio Automations|FL Studio Automations]]
- [[_COMMUNITY_Karplus-Strong Instruments|Karplus-Strong Instruments]]
- [[_COMMUNITY_Piano Roll Generators|Piano Roll Generators]]
- [[_COMMUNITY_Vital Preset Generator|Vital Preset Generator]]
- [[_COMMUNITY_Break Chop Script|Break Chop Script]]
- [[_COMMUNITY_Reference Analysis Tool|Reference Analysis Tool]]
- [[_COMMUNITY_Sample Librarian Tool|Sample Librarian Tool]]

## God Nodes (most connected - your core abstractions)
1. `Qeynos Audio Suite` - 11 edges
2. `suite-core (shared lib crate)` - 6 edges
3. `STFT engine (realfft, OLA, Hann)` - 5 edges
4. `GRIT — sidechained distortion` - 5 edges
5. `The Bus (cross-plugin shared state)` - 4 edges
6. `Waveshaper bank (suite-core DSP)` - 4 edges
7. `FDN reverb core (suite-core DSP)` - 4 edges
8. `EMBER — spectral fader / temporal smoother` - 4 edges
9. `SEANCE — ethereal vocal machine` - 4 edges
10. `Qeynos Audio Suite Work Order` - 4 edges

## Surprising Connections (you probably didn't know these)
- `FL Studio MCP server (work order)` --semantically_similar_to--> `FL Studio MCP server`  [INFERRED] [semantically similar]
  WORK_ORDER.md → PRD.md
- `Qeynos Audio Suite` --references--> `Qeynos Audio Suite Work Order`  [EXTRACTED]
  PRD.md → WORK_ORDER.md
- `Tech stack decisions (nih-plug / rustup-gnu / egui / pluginval)` --references--> `nih-plug (Rust plugin framework)`  [EXTRACTED]
  WORK_ORDER.md → PRD.md
- `GRIT — sidechained distortion` --references--> `GRIT (work order ask)`  [EXTRACTED]
  PRD.md → WORK_ORDER.md
- `Tech stack decisions (nih-plug / rustup-gnu / egui / pluginval)` --references--> `pluginval (headless VST3 validator)`  [EXTRACTED]
  WORK_ORDER.md → PRD.md

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Cross-plugin Bus participants (Master/Nodes, mod sources, analyzer)** — prd_the_bus, prd_overseer, prd_nerve, prd_x_ray [EXTRACTED 1.00]
- **Plugins built on the shared STFT/spectral engine** — prd_stft_engine, prd_ember, prd_smudge, prd_carve, prd_grit, prd_seance [EXTRACTED 1.00]
- **Phase 4 FL Studio workflow automations (W1-W8)** — prd_rumble_bassline, prd_break_chop, prd_dark_progression, prd_session_bootstrap, prd_project_janitor, prd_sample_librarian, prd_reference_gap, prd_vitalgen [EXTRACTED 1.00]

## Communities (13 total, 6 thin omitted)

### Community 0 - "Standalone FX Plugins"
Cohesion: 0.22
Nodes (10): ASCEND — tension generator, BANDAID — multiband transient designer, CLEAVE — multi slicer (Slice clone), DRIFT — infinity filter (Sweep clone), Execution Protocol (one plugin at a time), FLYBY — doppler spatializer (Transfer clone), HALT — performance buffer FX, OUROBOROS — recursive processor (Recurse clone) (+2 more)

### Community 1 - "Reverb & Suite Core"
Cohesion: 0.25
Nodes (8): CHAMBER — space simulator (Eigen clone), FDN reverb core (suite-core DSP), IMPACT — kick synth (MIDI instrument), MURMUR — stochastic reverb (Hikari clone), nih-plug (Rust plugin framework), suite-core (shared lib crate), UNDERTOW — kick-to-rumble generator, IMPACT (work order ask)

### Community 2 - "Build Pipeline & Tooling"
Cohesion: 0.29
Nodes (7): build.ps1 (build/bundle/validate/install script), pluginval (headless VST3 validator), _template (hello-gain reference crate), Definition of done (every plugin), Phase 2 — Lese catalog clones, Qeynos Audio Suite Work Order, Tech stack decisions (nih-plug / rustup-gnu / egui / pluginval)

### Community 3 - "Distortion Family"
Cohesion: 0.33
Nodes (7): CARVE — spectral ducker, GRIT — sidechained distortion, Waveshaper bank (suite-core DSP), SHAPESHIFT — morphing distortion (Teuri clone), TRACER — pitch-tracking multiband saturation, GRIT (work order ask), TRACER (work order ask)

### Community 4 - "Spectral Engine Family"
Cohesion: 0.38
Nodes (7): EMBER — spectral fader / temporal smoother, PATINA — analog lo-fi character, SEANCE — ethereal vocal machine, SMUDGE — spectral chaos (Smear clone), STFT engine (realfft, OLA, Hann), WIRE — codec degradation (Codec clone), EMBER (work order ask)

### Community 5 - "Shared Bus Architecture"
Cohesion: 0.50
Nodes (5): NERVE — suite modulation bus, OVERSEER — mastering system (Node + Master), The Bus (cross-plugin shared state), X-RAY — shared analyzer, OVERSEER (work order ask)

### Community 6 - "FL Studio Automations"
Cohesion: 0.50
Nodes (4): FL Studio MCP server, W5 project_janitor.py, W4 session_bootstrap.py, FL Studio MCP server (work order)

## Knowledge Gaps
- **29 isolated node(s):** `_template (hello-gain reference crate)`, `DRIFT — infinity filter (Sweep clone)`, `WIRE — codec degradation (Codec clone)`, `OUROBOROS — recursive processor (Recurse clone)`, `MURMUR — stochastic reverb (Hikari clone)` (+24 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **6 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `suite-core (shared lib crate)` connect `Reverb & Suite Core` to `Standalone FX Plugins`, `Distortion Family`, `Spectral Engine Family`, `Shared Bus Architecture`?**
  _High betweenness centrality (0.377) - this node is a cross-community bridge._
- **Why does `Qeynos Audio Suite` connect `Standalone FX Plugins` to `Reverb & Suite Core`, `Build Pipeline & Tooling`?**
  _High betweenness centrality (0.284) - this node is a cross-community bridge._
- **What connects `Execution Protocol (one plugin at a time)`, `_template (hello-gain reference crate)`, `DRIFT — infinity filter (Sweep clone)` to the rest of the system?**
  _30 weakly-connected nodes found - possible documentation gaps or missing edges._