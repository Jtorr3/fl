# W8-VITALGEN — Claude-powered Vital preset generator

A Python tool (not a Rust plugin) that generates [Vital](https://vital.audio) 1.5.x
synth presets from natural-language descriptions using the Claude API, then validates
them offline with pydantic so the output always loads.

Location: `tools/vitalgen/vitalgen/`
- `vitalgen.py` — CLI (PEP 723 header: `anthropic`, `pydantic`; Python 3.12)
- `base_template.vital` — embedded known-good 1.5.5 base patch (a real user-saved preset)
- `fixtures/` — checked-in "LLM response" fixtures used by the offline tests
- `test_vitalgen.py` — offline test suite (runs without an API key)

## Design — generation is a pure function

`build_preset(schema, base_template, spec)` is deterministic. Claude fills only a
**constrained subset** of parameters (a `PresetSpec`); everything else is copied
verbatim from the embedded base patch. Claude is never allowed to emit a whole
`.vital` file, so a malformed or partial model response can never produce an
unloadable preset.

```
description ──▶ Claude (tool-use, constrained schema) ──▶ PresetSpec (pydantic)
                                                              │ clamp + enum-check
                base_template.vital ──▶ build_preset ◀────────┘
                                            │
                                            ▼
                                     validate_preset_file ──▶ write .vital
```

### The constrained subset (`PARAM_SPEC`)

| Group | Params | Validation |
|---|---|---|
| Oscillators 1–3 | on, level, transpose, tune, wave_frame, unison voices/detune, pan, phase, stereo spread, distortion type/amount | continuous clamped; `on`, `unison_voices`, `distortion_type` are enums |
| Filters 1–2 | on, cutoff (8–136), resonance, drive, mix, model, style, blend, keytrack, per-source routing inputs | continuous clamped; on/model/style/routing are enums |
| Envelopes 1–6 | delay, attack, hold, decay, sustain, release + curve powers | clamped (attack/decay/release on Vital's quartic 0–2.378 scale) |
| LFOs 1–8 | frequency, sync, tempo, fade/delay time, stereo, smooth mode; **shape as a point list** | scalars clamped/enum; shape points clamped 0–1 |
| FX chain | distortion / delay / reverb / chorus / phaser / flanger / compressor on + amounts | on = enum; amounts clamped |
| Macros | macro1–4 names + resting positions | names free text; positions clamped 0–1 |

Continuous params are **clamped** into range (never rejected); enum params are
**rejected** with a clear error if the value is not in the allowed set. Ranges come
from Vital OSS `src/common/synth_parameters.cpp`; enum value sets and the two ranges
that OSS under-states (`wave_frame` up to 256, `lfo frequency` bipolar) were corrected
against user-saved 1.5.5 presets on the build machine.

### Embedded taste block

Every generation prompt appends a fixed style context: dark melodic techno
(KAS:ST / Fjaak) and atmospheric dnb / breakcore (Cynthoni / Sewerslvt) — grief pads,
hollow reeses, drowned leads, minor tonality, low resonant cutoffs, slow LFOs.

## CLI

```
vitalgen generate "<description>" [--name X] [--bank B] [-n COUNT] [--out DIR] [--model M]
vitalgen tweak <preset.vital> "<delta description>" [--model M]
vitalgen validate <preset.vital>          # offline pydantic/structure check, no API
```

Run via uv (Python 3.12 pinned):
```
uv run --python 3.12 tools/vitalgen/vitalgen/vitalgen.py generate "cavernous mid bass" --bank Qeynos
```

`generate`/`tweak` read `ANTHROPIC_API_KEY` (model default `claude-opus-4-8`).
Output location: `--out DIR`, else `Documents\Vital\User\<bank>\`, else
`Documents\Vital\User\Presets\`. Documents is resolved via the Windows known-folder
API (OneDrive-redirected on the build machine).

## Tests (the gate — no API key required)

```
uv run --python 3.12 tools/vitalgen/vitalgen/test_vitalgen.py
```

1. base template validates
2. a fixture "LLM response" round-trips through validation + clamping and writes a
   loadable `.vital`
3. an out-of-range fixture is clamped (not rejected)
4. an enum-violation fixture is rejected with a clear error
5. (bonus) unknown parameter keys are rejected
6. live API smoke test — runs only if `ANTHROPIC_API_KEY` is set, else skipped

## Schema provenance

- Base template: a real user-saved **1.5.5** preset (`PAD_-_Miasma`) — guaranteed
  loadable. The installed Vital is 1.5.5; the OSS repo tracks ~1.0.7, so ranges were
  cross-checked against local 1.5.5 presets, which are the primary evidence.
- Parameter ranges: Vital OSS `synth_parameters.cpp`, corrected with local 1.5.5 data.

Serum 2 preset generation is out of scope — see `DEFERRED.md`.
