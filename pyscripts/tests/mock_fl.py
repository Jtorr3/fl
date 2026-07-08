"""Shared mock `flpianoroll` for the FL piano-roll script offline gates (Qeynos).

FL Studio cannot run its piano-roll scripts headless, so each `.pyscript`'s test
gate imports the script with a MOCK `flpianoroll` module injected into
`sys.modules` before load. This module factors that mock + the `.pyscript`
loader out so BOTH `test_rumble.py` (W1) and `test_breakchop.py` (W2) — and any
future W-series piano-roll script — share ONE implementation.

Contents:
  * `MockNote`   — mirror of flpianoroll.Note (the properties the scripts touch).
  * `MockScore`  — indexable note list + PPQ + timeline selection; add/get/delete.
  * `MockDialog` — ScriptDialog stub: collects AddInput*/serves GetInputValue.
  * `make_mock_flp(ppq)` — a fresh `flpianoroll` module object.
  * `load_pyscript(path, modname, ppq)` — inject a fresh mock into sys.modules
    and load the `.pyscript` (via SourceFileLoader, since `.pyscript` isn't a
    recognized source suffix so spec_from_file_location returns no loader).

W2-BREAK-CHOP note: seed the mock `score` with pre-made notes and set
`.selected = True` on the ones to chop/permute/roll — the script operates on the
SELECTED notes only (fallback: the whole score when nothing is selected).
"""
from __future__ import annotations

import importlib.util
import sys
import types
from importlib.machinery import SourceFileLoader


class MockNote:
    """Mirror of FL's flpianoroll.Note (the properties we touch)."""

    def __init__(self):
        self.number = 0
        self.time = 0
        self.length = 0
        self.velocity = 0.8
        self.pan = 0.5
        self.selected = False
        self.muted = False


class MockScore:
    """Mirror of flpianoroll.score: an indexable note list + PPQ + selection."""

    def __init__(self, ppq=96):
        self.PPQ = ppq
        self._notes = []
        self._selection = (0, -1)   # (start, end); end<=start means "no selection"

    def addNote(self, note):
        self._notes.append(note)

    def getNote(self, i):
        return self._notes[i]

    def deleteNote(self, i):
        del self._notes[i]

    def clearNotes(self, all=True):
        self._notes = []

    @property
    def noteCount(self):
        return len(self._notes)

    def getTimelineSelection(self):
        return self._selection


class MockDialog:
    """Mirror of flpianoroll.ScriptDialog: collects inputs, serves defaults."""

    def __init__(self, title="", description=""):
        self.title = title
        self.description = description
        self._values = {}

    def AddInputCombo(self, name, options, default_index):
        self._values[name] = int(default_index)

    def AddInputKnob(self, name, value, vmin, vmax):
        self._values[name] = float(value)

    def AddInputKnobInt(self, name, value, vmin, vmax):
        self._values[name] = int(value)

    def AddInputCheckbox(self, name, value):
        self._values[name] = bool(value)

    def AddInputText(self, name, value):
        self._values[name] = value

    def SetValue(self, name, value):
        self._values[name] = value

    def GetInputValue(self, name):
        return self._values[name]

    def Execute(self):
        return True


def make_mock_flp(ppq=96):
    """Build a fresh `flpianoroll` module object with an empty score."""
    mod = types.ModuleType("flpianoroll")
    mod.Note = MockNote
    mod.score = MockScore(ppq)
    mod.ScriptDialog = MockDialog
    return mod


def load_pyscript(path, modname, ppq=96):
    """Inject a fresh mock `flpianoroll` and load the `.pyscript` at `path`.

    `.pyscript` isn't a recognized source suffix, so `spec_from_file_location`
    returns a spec with no loader; load through an explicit SourceFileLoader.
    """
    sys.modules["flpianoroll"] = make_mock_flp(ppq)
    loader = SourceFileLoader(modname, str(path))
    spec = importlib.util.spec_from_loader(modname, loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod
