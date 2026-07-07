# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#     "anthropic>=0.40",
#     "pydantic>=2.5",
#     "pytest>=8.0",
# ]
# ///
"""Offline test suite for vitalgen -- runs WITHOUT an ANTHROPIC_API_KEY.

Run either way:
    uv run test_vitalgen.py          # plain-assert runner (the gate)
    uv run pytest test_vitalgen.py   # pytest

The live API smoke test runs ONLY if ANTHROPIC_API_KEY is set; otherwise it is skipped.
"""
from __future__ import annotations

import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import vitalgen  # noqa: E402
from pydantic import ValidationError  # noqa: E402

FIX = Path(__file__).resolve().parent / "fixtures"


def _load(name):
    return json.loads((FIX / name).read_text(encoding="utf-8"))


def test_base_template_validates():
    """(1) The embedded base template is a valid, loadable .vital."""
    base = vitalgen.load_base_template()
    errs = vitalgen.validate_preset_file(base)
    assert errs == [], f"base template should validate, got: {errs}"
    assert base.get("synth_version", "").startswith("1.5"), base.get("synth_version")


def test_valid_fixture_roundtrips_and_writes():
    """(2) A fixture 'LLM response' validates, clamps, builds, and writes a loadable file."""
    base = vitalgen.load_base_template()
    spec = vitalgen.PresetSpec.model_validate(_load("llm_response_valid.json"))
    preset = vitalgen.build_preset(base, spec)

    # the override actually landed in settings
    assert abs(preset["settings"]["filter_1_cutoff"] - 42.0) < 1e-6
    assert preset["settings"]["osc_1_unison_voices"] == 6.0
    # macro name applied at top level
    assert preset["macro1"] == "REVERB"
    # lfo shape applied
    assert preset["settings"]["lfos"][0]["name"] == "Slow Swell"
    assert preset["settings"]["lfos"][0]["num_points"] == 3
    # metadata
    assert preset["preset_name"] == "Drowned Grief Pad"

    # built preset is a valid, loadable .vital
    errs = vitalgen.validate_preset_file(preset)
    assert errs == [], f"built preset should validate, got: {errs}"

    # physically writes and reads back as JSON
    with tempfile.TemporaryDirectory() as td:
        path = vitalgen.write_preset(preset, Path(td), spec.name)
        assert path.exists()
        reread = json.loads(path.read_text(encoding="utf-8"))
        assert reread["preset_name"] == "Drowned Grief Pad"
        assert vitalgen.validate_preset_file(reread) == []


def test_out_of_range_is_clamped_not_rejected():
    """(3) Out-of-range continuous params clamp to Vital's bounds, never rejected."""
    spec = vitalgen.PresetSpec.model_validate(_load("llm_response_out_of_range.json"))
    p = spec.params
    assert p["osc_1_level"] == 1.0            # clamped to max 1.0
    assert p["filter_1_cutoff"] == 136.0      # clamped to max 136
    assert p["filter_1_resonance"] == 0.0     # clamped to min 0
    assert p["filter_1_drive"] == 20.0        # clamped to max 20
    assert p["env_1_attack"] == 2.378         # clamped to max
    assert p["distortion_drive"] == 24.0      # clamped to max
    assert p["volume"] == 7399.44             # clamped to max

    # and the clamped spec still builds a valid preset
    base = vitalgen.load_base_template()
    preset = vitalgen.build_preset(base, spec)
    assert vitalgen.validate_preset_file(preset) == []


def test_enum_violation_is_rejected_clearly():
    """(4) An out-of-set enum value is rejected with a clear error."""
    raised = False
    try:
        vitalgen.PresetSpec.model_validate(_load("llm_response_enum_violation.json"))
    except ValidationError as exc:
        raised = True
        msg = str(exc)
        assert "filter_1_model" in msg, msg
        assert "valid option" in msg or "allowed" in msg, msg
    assert raised, "enum violation should have raised ValidationError"


def test_unknown_param_is_rejected():
    """Bonus: the LLM cannot smuggle arbitrary keys (constraint enforcement)."""
    raised = False
    try:
        vitalgen.PresetSpec.model_validate({"params": {"not_a_real_param": 1.0}})
    except ValidationError as exc:
        raised = True
        assert "unknown parameter" in str(exc)
    assert raised


def test_live_api_smoke():
    """Live smoke test -- ONLY if ANTHROPIC_API_KEY is set; otherwise skipped."""
    if not os.environ.get("ANTHROPIC_API_KEY"):
        print("  (skipped: ANTHROPIC_API_KEY not set)")
        return "skipped"
    base = vitalgen.load_base_template()
    spec = vitalgen.call_claude("cavernous mid bass, hollow reese", vitalgen.DEFAULT_MODEL)
    preset = vitalgen.build_preset(base, spec)
    assert vitalgen.validate_preset_file(preset) == []
    return "ran"


ALL_TESTS = [
    test_base_template_validates,
    test_valid_fixture_roundtrips_and_writes,
    test_out_of_range_is_clamped_not_rejected,
    test_enum_violation_is_rejected_clearly,
    test_unknown_param_is_rejected,
    test_live_api_smoke,
]


def _run_all() -> int:
    failures = 0
    for t in ALL_TESTS:
        try:
            result = t()
            tag = "SKIP" if result == "skipped" else "PASS"
            print(f"[{tag}] {t.__name__}")
        except Exception as exc:  # noqa: BLE001
            failures += 1
            print(f"[FAIL] {t.__name__}: {exc}")
    print(f"\n{len(ALL_TESTS) - failures}/{len(ALL_TESTS)} passed"
          + (f", {failures} failed" if failures else ""))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(_run_all())
