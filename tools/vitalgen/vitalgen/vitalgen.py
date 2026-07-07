# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#     "anthropic>=0.40",
#     "pydantic>=2.5",
# ]
# ///
"""vitalgen -- Claude-powered Vital (1.5.x) preset generator.

Architecture (PRD W8): generation is a pure function of
    (schema, base template, description) -> a CONSTRAINED subset filled by Claude.

Claude never emits a whole .vital file. It fills only a bounded set of synthesis
parameters (osc levels/tuning/wavetable frame, filter cutoff/res/routing, env ADSR,
LFO shapes as point lists, FX amounts, macro names). Everything else comes verbatim
from an embedded known-good 1.5.5 preset (base_template.vital), so the output ALWAYS
loads in Vital. Pydantic validates the constrained subset: continuous params are
clamped to Vital's real ranges, enum params are rejected if out of set.

Schema ground truth: parameter ranges from Vital OSS src/common/synth_parameters.cpp;
enum value sets confirmed against user-saved 1.5.5 presets; the base template is a
real user-saved 1.5.5 preset (guaranteed loadable).

CLI:
    vitalgen generate "<description>" [--name X] [--bank B] [-n COUNT] [--out DIR] [--model M]
    vitalgen tweak <preset.vital> "<delta>" [--model M]
    vitalgen validate <preset.vital>        # offline pydantic check only, no API
"""
from __future__ import annotations

import argparse
import copy
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field, ValidationError, field_validator

# --------------------------------------------------------------------------------------
# Paths
# --------------------------------------------------------------------------------------
HERE = Path(__file__).resolve().parent
BASE_TEMPLATE_PATH = HERE / "base_template.vital"
DEFAULT_MODEL = "claude-opus-4-8"

# --------------------------------------------------------------------------------------
# PARAM_SPEC -- the constrained subset the LLM may set.
#   continuous key -> ("range", lo, hi)   -> value clamped into [lo, hi]
#   enum key       -> ("enum", {allowed}) -> value rejected if not in the set
# Ranges: Vital OSS synth_parameters.cpp. Enum sets: confirmed in real 1.5.5 presets.
# --------------------------------------------------------------------------------------
PARAM_SPEC: Dict[str, tuple] = {}


def _rng(key: str, lo: float, hi: float) -> None:
    PARAM_SPEC[key] = ("range", lo, hi)


def _enum(key: str, allowed) -> None:
    PARAM_SPEC[key] = ("enum", set(allowed))


# global / master
_rng("volume", 0.0, 7399.44)
_rng("portamento_time", -10.0, 4.0)
_rng("polyphony", 1.0, 32.0)

# oscillators 1..3
for i in (1, 2, 3):
    _enum(f"osc_{i}_on", (0, 1))
    _rng(f"osc_{i}_level", 0.0, 1.0)
    _rng(f"osc_{i}_transpose", -48.0, 48.0)
    _rng(f"osc_{i}_tune", -1.0, 1.0)
    _rng(f"osc_{i}_wave_frame", 0.0, 256.0)
    _enum(f"osc_{i}_unison_voices", range(1, 17))
    _rng(f"osc_{i}_unison_detune", 0.0, 10.0)
    _rng(f"osc_{i}_pan", -1.0, 1.0)
    _rng(f"osc_{i}_phase", 0.0, 1.0)
    _rng(f"osc_{i}_stereo_spread", -1.0, 1.0)
    _enum(f"osc_{i}_distortion_type", range(0, 12))
    _rng(f"osc_{i}_distortion_amount", 0.0, 1.0)

# filters 1..2 (routing = on/off + which oscillators feed them)
for i in (1, 2):
    _enum(f"filter_{i}_on", (0, 1))
    _rng(f"filter_{i}_cutoff", 8.0, 136.0)
    _rng(f"filter_{i}_resonance", 0.0, 1.0)
    _rng(f"filter_{i}_drive", 0.0, 20.0)
    _rng(f"filter_{i}_mix", 0.0, 1.0)
    _enum(f"filter_{i}_model", range(0, 9))
    _enum(f"filter_{i}_style", range(0, 10))
    _rng(f"filter_{i}_blend", 0.0, 2.0)
    _rng(f"filter_{i}_keytrack", -1.0, 1.0)
    # routing inputs (which sources feed this filter)
    _enum(f"filter_{i}_osc1_input", (0, 1))
    _enum(f"filter_{i}_osc2_input", (0, 1))
    _enum(f"filter_{i}_osc3_input", (0, 1))
    _enum(f"filter_{i}_sample_input", (0, 1))

# envelopes 1..6 (env_1 = amp env by convention)
for i in range(1, 7):
    _rng(f"env_{i}_delay", 0.0, 1.414)
    _rng(f"env_{i}_attack", 0.0, 2.378)
    _rng(f"env_{i}_hold", 0.0, 1.414)
    _rng(f"env_{i}_decay", 0.0, 2.378)
    _rng(f"env_{i}_sustain", 0.0, 1.0)
    _rng(f"env_{i}_release", 0.0, 2.378)
    _rng(f"env_{i}_attack_power", -20.0, 20.0)
    _rng(f"env_{i}_decay_power", -20.0, 20.0)
    _rng(f"env_{i}_release_power", -20.0, 20.0)

# LFO scalar params (LFO *shape* is set via the lfos[] point-list channel below)
for i in range(1, 9):
    _rng(f"lfo_{i}_frequency", -7.0, 7.0)
    _enum(f"lfo_{i}_sync", range(0, 6))
    _rng(f"lfo_{i}_tempo", 0.0, 12.0)
    _rng(f"lfo_{i}_fade_time", 0.0, 8.0)
    _rng(f"lfo_{i}_delay_time", 0.0, 8.0)
    _rng(f"lfo_{i}_stereo", -1.0, 1.0)
    _enum(f"lfo_{i}_smooth_mode", (0, 1))

# FX chain (amounts + on/off)
for fx in ("distortion", "delay", "reverb", "chorus", "phaser", "flanger", "compressor"):
    _enum(f"{fx}_on", (0, 1))
_rng("distortion_drive", -24.0, 24.0)
_rng("distortion_mix", 0.0, 1.0)
_enum("distortion_type", range(0, 6))
_rng("delay_dry_wet", 0.0, 1.0)
_rng("delay_feedback", -1.0, 1.0)
_rng("reverb_dry_wet", 0.0, 1.0)
_rng("reverb_decay_time", -6.0, 6.0)
_rng("reverb_chorus_amount", 0.0, 1.0)
_rng("reverb_low_shelf_gain", -6.0, 0.0)
_rng("reverb_high_shelf_gain", -6.0, 0.0)
_rng("chorus_dry_wet", 0.0, 1.0)
_rng("phaser_dry_wet", 0.0, 1.0)
_rng("flanger_dry_wet", 0.0, 1.0)
_rng("compressor_mix", 0.0, 1.0)

# macro knob resting positions (names are set via the macros channel)
for i in (1, 2, 3, 4):
    _rng(f"macro_control_{i}", 0.0, 1.0)


# --------------------------------------------------------------------------------------
# Pydantic model for the constrained LLM output
# --------------------------------------------------------------------------------------
class LfoSpec(BaseModel):
    """One LFO shape as a point list (Vital lfos[] entry)."""

    index: int = Field(..., ge=0, le=7)
    name: str = "Custom"
    points: List[float]  # flat [x0,y0,x1,y1,...]; 2*num_points, each clamped 0..1
    powers: Optional[List[float]] = None  # per-point curvature, clamped -20..20
    smooth: bool = False

    @field_validator("points")
    @classmethod
    def _check_points(cls, v: List[float]) -> List[float]:
        if len(v) < 4 or len(v) % 2 != 0:
            raise ValueError(
                "lfo points must be a flat [x,y,...] list of even length >= 4"
            )
        return [min(1.0, max(0.0, float(x))) for x in v]

    @field_validator("powers")
    @classmethod
    def _clamp_powers(cls, v):
        if v is None:
            return v
        return [min(20.0, max(-20.0, float(x))) for x in v]


class PresetSpec(BaseModel):
    """The full constrained output Claude fills. Everything omitted comes from base."""

    name: Optional[str] = None
    author: Optional[str] = None
    comments: Optional[str] = None
    style: Optional[str] = None
    params: Dict[str, float] = Field(default_factory=dict)
    lfos: List[LfoSpec] = Field(default_factory=list)
    macros: Dict[str, str] = Field(default_factory=dict)

    @field_validator("params")
    @classmethod
    def _validate_params(cls, params: Dict[str, float]) -> Dict[str, float]:
        cleaned: Dict[str, float] = {}
        for key, value in params.items():
            if key not in PARAM_SPEC:
                raise ValueError(
                    f"unknown parameter '{key}' -- not in the constrained schema "
                    f"(the LLM may only set known Vital params)"
                )
            spec = PARAM_SPEC[key]
            try:
                num = float(value)
            except (TypeError, ValueError):
                raise ValueError(f"parameter '{key}' must be numeric, got {value!r}")
            if spec[0] == "range":
                _, lo, hi = spec
                cleaned[key] = min(hi, max(lo, num))  # clamp, never reject
            else:  # enum -- reject if not in the allowed set
                allowed = spec[1]
                ivalue = int(round(num))
                if ivalue not in allowed:
                    lo, hi = min(allowed), max(allowed)
                    raise ValueError(
                        f"parameter '{key}' = {value!r} is not a valid option "
                        f"(allowed integer values {lo}..{hi})"
                    )
                cleaned[key] = float(ivalue)
        return cleaned

    @field_validator("macros")
    @classmethod
    def _validate_macros(cls, macros: Dict[str, str]) -> Dict[str, str]:
        for key in macros:
            if key not in ("macro1", "macro2", "macro3", "macro4"):
                raise ValueError(
                    f"unknown macro key '{key}' (use macro1..macro4)"
                )
        return {k: str(v) for k, v in macros.items()}


# --------------------------------------------------------------------------------------
# Base template + preset building (pure function of schema, base, spec)
# --------------------------------------------------------------------------------------
def load_base_template() -> Dict[str, Any]:
    with open(BASE_TEMPLATE_PATH, "r", encoding="utf-8") as f:
        return json.load(f)


def build_preset(base: Dict[str, Any], spec: PresetSpec) -> Dict[str, Any]:
    """Merge a validated PresetSpec onto the known-good base. Pure, deterministic."""
    preset = copy.deepcopy(base)
    settings = preset["settings"]

    # scalar params (already clamped / enum-checked)
    for key, value in spec.params.items():
        settings[key] = value

    # LFO shapes as point lists
    lfos = settings.setdefault("lfos", [])
    for lfo in spec.lfos:
        while len(lfos) <= lfo.index:
            lfos.append({"name": "Custom", "num_points": 2,
                         "points": [0.0, 0.0, 1.0, 1.0], "powers": [0.0, 0.0],
                         "smooth": False})
        num_points = len(lfo.points) // 2
        powers = lfo.powers if lfo.powers is not None else [0.0] * num_points
        if len(powers) < num_points:
            powers = powers + [0.0] * (num_points - len(powers))
        else:
            powers = powers[:num_points]
        lfos[lfo.index] = {
            "name": lfo.name,
            "num_points": num_points,
            "points": lfo.points,
            "powers": powers,
            "smooth": bool(lfo.smooth),
        }

    # macro names (top-level keys) + resting positions live in params
    for key, macro_name in spec.macros.items():
        preset[key] = macro_name

    # metadata
    if spec.name is not None:
        preset["preset_name"] = spec.name
    if spec.author is not None:
        preset["author"] = spec.author
    if spec.comments is not None:
        preset["comments"] = spec.comments
    if spec.style is not None:
        preset["preset_style"] = spec.style

    return preset


# --------------------------------------------------------------------------------------
# Offline validation of a full .vital file (the `validate` subcommand)
# --------------------------------------------------------------------------------------
def validate_preset_file(preset: Dict[str, Any]) -> List[str]:
    """Structural + range sanity check on a full .vital dict. Returns list of errors."""
    errors: List[str] = []
    if not isinstance(preset, dict):
        return ["top level is not a JSON object"]
    if "settings" not in preset or not isinstance(preset["settings"], dict):
        errors.append("missing 'settings' object")
        return errors
    if "synth_version" not in preset:
        errors.append("missing 'synth_version'")
    settings = preset["settings"]
    # structural: lfos/modulations shapes if present
    lfos = settings.get("lfos")
    if lfos is not None:
        if not isinstance(lfos, list):
            errors.append("'lfos' is not an array")
        else:
            for idx, lfo in enumerate(lfos):
                pts = lfo.get("points")
                if pts is None or len(pts) % 2 != 0:
                    errors.append(f"lfo[{idx}] points missing or odd length")
    # range sanity on any known param present
    for key, spec in PARAM_SPEC.items():
        if key not in settings:
            continue
        val = settings[key]
        try:
            num = float(val)
        except (TypeError, ValueError):
            errors.append(f"param '{key}' is non-numeric: {val!r}")
            continue
        if spec[0] == "range":
            _, lo, hi = spec
            if not (lo - 1e-6 <= num <= hi + 1e-6):
                errors.append(f"param '{key}' = {num} outside [{lo}, {hi}]")
        else:
            allowed = spec[1]
            if int(round(num)) not in allowed:
                errors.append(f"param '{key}' = {num} not a valid enum option")
    return errors


# --------------------------------------------------------------------------------------
# Embedded taste / style block -- appended to every generation prompt
# --------------------------------------------------------------------------------------
TASTE_BLOCK = """\
STYLE CONTEXT (bias every sound toward this palette unless the description overrides it):
Dark melodic techno in the vein of KAS:ST and Fjaak -- hypnotic, driving, restrained,
detuned reese/hollow basses, cold metallic stabs, brooding minor-key pads. Atmospheric
drum'n'bass / breakcore in the Cynthoni / Sewerslvt lineage -- grief pads with long
slow attacks and long releases, drowned/underwater leads (heavy filtering, chorus/reverb
wash), hollow detuned reeses, tape-worn melancholy. Prefer minor tonality, low cutoffs
with expressive resonance, slow evolving LFOs, generous reverb/delay for depth, and
subtle unison detune for width. Aim for emotional, cavernous, decayed textures rather
than clean or bright ones."""


def _param_catalog() -> str:
    """Human-readable catalog of settable params for the prompt."""
    lines = []
    for key, spec in PARAM_SPEC.items():
        if spec[0] == "range":
            lines.append(f"  {key}: number in [{spec[1]}, {spec[2]}]")
        else:
            allowed = sorted(spec[1])
            lines.append(f"  {key}: integer, one of {allowed[0]}..{allowed[-1]}")
    return "\n".join(lines)


SYSTEM_PROMPT = """\
You are a sound-design engine for the Vital wavetable synthesizer (version 1.5.x).
You do NOT write a whole preset file. You return ONLY a constrained set of parameter
overrides that will be merged onto a known-good base patch, so the result always loads.

Fill just the parameters that realize the requested sound. Leave everything else out --
omitted parameters keep the base patch's value. Think about signal flow: oscillator
levels/tuning/wavetable frame and unison for the tone and width; filter cutoff (8..136
= MIDI-note scale, ~60 is middle), resonance, drive and model for the character; envelope
1 is the amplitude envelope (attack/decay/sustain/release are on a quartic 0..2.378 scale,
so 1.0 is already a long time); LFOs as point lists for movement; the FX chain (reverb,
delay, chorus, distortion) for space and grit; macro names for performance controls.

Return your answer by calling the emit_vital_preset tool exactly once."""


# --------------------------------------------------------------------------------------
# Claude API integration
# --------------------------------------------------------------------------------------
def _emit_tool_schema() -> Dict[str, Any]:
    return {
        "name": "emit_vital_preset",
        "description": (
            "Emit a constrained set of Vital parameter overrides for one preset. "
            "Only set parameters that matter for the requested sound.\n\n"
            "Settable scalar params (params object -- key: allowed value):\n"
            + _param_catalog()
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "preset display name"},
                "author": {"type": "string"},
                "comments": {"type": "string", "description": "short patch note"},
                "style": {
                    "type": "string",
                    "description": "Vital category, e.g. Bass, Pad, Lead, Keys, SFX",
                },
                "params": {
                    "type": "object",
                    "description": "flat map of Vital setting key -> numeric value; "
                    "keys must come from the catalog in this tool's description",
                    "additionalProperties": {"type": "number"},
                },
                "lfos": {
                    "type": "array",
                    "description": "LFO shapes as point lists",
                    "items": {
                        "type": "object",
                        "properties": {
                            "index": {"type": "integer", "minimum": 0, "maximum": 7},
                            "name": {"type": "string"},
                            "points": {
                                "type": "array",
                                "items": {"type": "number"},
                                "description": "flat [x0,y0,x1,y1,...] each 0..1, "
                                "x ascending 0->1",
                            },
                            "powers": {
                                "type": "array",
                                "items": {"type": "number"},
                            },
                            "smooth": {"type": "boolean"},
                        },
                        "required": ["index", "points"],
                    },
                },
                "macros": {
                    "type": "object",
                    "description": "macro1..macro4 -> control name",
                    "additionalProperties": {"type": "string"},
                },
            },
            "required": ["params"],
        },
    }


def call_claude(description: str, model: str, existing: Optional[Dict[str, Any]] = None,
                delta: bool = False) -> PresetSpec:
    """Call Claude, return a validated PresetSpec. Raises RuntimeError on API problems."""
    try:
        import anthropic
    except ImportError as exc:  # pragma: no cover
        raise RuntimeError("anthropic SDK not installed") from exc

    client = anthropic.Anthropic()  # reads ANTHROPIC_API_KEY / ant profile

    if delta and existing is not None:
        cur = json.dumps(existing.get("settings", {}))[:12000]
        user_text = (
            f"Here is the current preset's settings (truncated): {cur}\n\n"
            f"Apply this change and return only the parameters that should differ:\n"
            f"{description}\n\n{TASTE_BLOCK}"
        )
    else:
        user_text = f"Design this sound: {description}\n\n{TASTE_BLOCK}"

    message = client.messages.create(
        model=model,
        max_tokens=4096,
        thinking={"type": "adaptive"},
        system=SYSTEM_PROMPT,
        tools=[_emit_tool_schema()],
        tool_choice={"type": "tool", "name": "emit_vital_preset"},
        messages=[{"role": "user", "content": user_text}],
    )
    tool_input = None
    for block in message.content:
        if getattr(block, "type", None) == "tool_use" and block.name == "emit_vital_preset":
            tool_input = block.input
            break
    if tool_input is None:
        raise RuntimeError("Claude did not return an emit_vital_preset tool call")
    return PresetSpec.model_validate(tool_input)


# --------------------------------------------------------------------------------------
# Output helpers
# --------------------------------------------------------------------------------------
def _known_folder_documents() -> Optional[Path]:
    """Resolve the OneDrive-redirected Documents via the Windows known-folder API."""
    if os.name != "nt":
        return None
    try:
        import ctypes
        from ctypes import wintypes

        _CoTaskMemFree = ctypes.windll.ole32.CoTaskMemFree
        _SHGetKnownFolderPath = ctypes.windll.shell32.SHGetKnownFolderPath
        # FOLDERID_Documents {FDD39AD0-238F-46AF-ADB4-6C85480369C7}
        guid = ctypes.create_string_buffer(16)
        # Use the string GUID via CLSIDFromString
        clsid = (ctypes.c_byte * 16)()
        ctypes.windll.ole32.CLSIDFromString(
            "{FDD39AD0-238F-46AF-ADB4-6C85480369C7}", ctypes.byref(clsid)
        )
        ptr = ctypes.c_wchar_p()
        if _SHGetKnownFolderPath(ctypes.byref(clsid), 0, 0, ctypes.byref(ptr)) == 0:
            path = ptr.value
            _CoTaskMemFree(ptr)
            if path:
                return Path(path)
    except Exception:
        pass
    # fallback
    up = os.environ.get("USERPROFILE")
    return Path(up) / "Documents" if up else None


def resolve_out_dir(out: Optional[str], bank: Optional[str]) -> Path:
    if out:
        return Path(out)
    docs = _known_folder_documents()
    if docs is None:
        return Path.cwd() / "out"
    vital_user = docs / "Vital" / "User"
    return vital_user / bank if bank else vital_user / "Presets"


def _slug(name: str) -> str:
    s = re.sub(r"[^A-Za-z0-9 _-]", "", name).strip().replace(" ", "_")
    return s or "Qeynos_Preset"


def write_preset(preset: Dict[str, Any], out_dir: Path, name: str) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{_slug(name)}.vital"
    with open(path, "w", encoding="utf-8") as f:
        json.dump(preset, f)
    return path


# --------------------------------------------------------------------------------------
# Subcommands
# --------------------------------------------------------------------------------------
def cmd_generate(args) -> int:
    base = load_base_template()
    out_dir = resolve_out_dir(args.out, args.bank)
    count = max(1, args.count)
    written: List[Path] = []
    for n in range(count):
        try:
            spec = call_claude(args.description, args.model)
        except Exception as exc:
            print(f"error: generation failed: {exc}", file=sys.stderr)
            return 2
        default_name = args.name or _default_name(args.description)
        if count > 1:
            default_name = f"{default_name} {n + 1}"
        if spec.name is None:
            spec.name = default_name
        preset = build_preset(base, spec)
        errs = validate_preset_file(preset)
        if errs:
            print(f"error: built preset failed validation: {errs}", file=sys.stderr)
            return 2
        path = write_preset(preset, out_dir, spec.name)
        written.append(path)
        print(f"wrote {path}")
    return 0


def cmd_tweak(args) -> int:
    src = Path(args.preset)
    with open(src, "r", encoding="utf-8") as f:
        existing = json.load(f)
    try:
        spec = call_claude(args.delta, args.model, existing=existing, delta=True)
    except Exception as exc:
        print(f"error: tweak failed: {exc}", file=sys.stderr)
        return 2
    preset = build_preset(existing, spec)
    errs = validate_preset_file(preset)
    if errs:
        print(f"error: tweaked preset failed validation: {errs}", file=sys.stderr)
        return 2
    out_dir = src.parent
    stem = src.stem
    name = spec.name or f"{stem}_tweaked"
    path = out_dir / f"{_slug(name)}.vital"
    with open(path, "w", encoding="utf-8") as f:
        json.dump(preset, f)
    print(f"wrote {path}")
    return 0


def cmd_validate(args) -> int:
    src = Path(args.preset)
    try:
        with open(src, "r", encoding="utf-8") as f:
            preset = json.load(f)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"INVALID: could not read/parse: {exc}", file=sys.stderr)
        return 1
    errs = validate_preset_file(preset)
    if errs:
        print(f"INVALID: {src}")
        for e in errs:
            print(f"  - {e}")
        return 1
    print(f"OK: {src} ({preset.get('synth_version', '?')}, "
          f"{len(preset.get('settings', {}))} settings)")
    return 0


def _default_name(description: str) -> str:
    words = re.findall(r"[A-Za-z0-9]+", description)[:3]
    return " ".join(w.capitalize() for w in words) or "Qeynos Preset"


# --------------------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------------------
def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="vitalgen",
        description="Claude-powered Vital preset generator (Qeynos suite W8).",
    )
    sub = p.add_subparsers(dest="cmd", required=True)

    g = sub.add_parser("generate", help="generate preset(s) from a description")
    g.add_argument("description")
    g.add_argument("--name", default=None)
    g.add_argument("--bank", default=None, help="subfolder under Vital/User/")
    g.add_argument("-n", "--count", type=int, default=1)
    g.add_argument("--out", default=None, help="output directory (overrides Vital dir)")
    g.add_argument("--model", default=DEFAULT_MODEL)
    g.set_defaults(func=cmd_generate)

    t = sub.add_parser("tweak", help="modify an existing preset by a delta description")
    t.add_argument("preset")
    t.add_argument("delta")
    t.add_argument("--model", default=DEFAULT_MODEL)
    t.set_defaults(func=cmd_tweak)

    v = sub.add_parser("validate", help="offline pydantic/structure check (no API)")
    v.add_argument("preset")
    v.set_defaults(func=cmd_validate)
    return p


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
