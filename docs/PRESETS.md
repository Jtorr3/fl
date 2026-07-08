# Presets — factory + user

Every Qeynos plugin ships with **factory presets** and can save your own **user
presets**. Both are the same flat JSON format and both drive the shared **preset bar**
at the top of every editor.

## The preset bar

The bar (`suite_core::ui::PresetBar`) replaces the old per-plugin preset dropdown and is
identical across the whole suite:

- **Dropdown** — one list split into a **FACTORY** section (the presets that ship with
  the plugin) and a **USER** section (yours, loaded from disk). Picking either applies it
  to the live parameters through the host, so the change is automatable and undoable.
- **Dirty dot** — a small amber dot appears next to the dropdown when the current
  parameters have drifted from the preset you loaded (compared with a small tolerance).
  It clears when you load or (re)save a preset.
- **Save** — overwrites the currently-loaded **user** preset in place. Disabled until a
  user preset is loaded (factory presets are read-only).
- **Save As** — opens an inline name field; type a name and press Enter or click Save to
  write a new user preset. Illegal filename characters are stripped automatically.
- **Delete** — removes the loaded **user** preset. It is a **two-click confirm**: the
  button changes to *Confirm?* on the first click.

All of this runs on the GUI thread. Filesystem IO never happens on the audio thread, and
nothing is added to the real-time `process()` path.

## Where the files live

User presets are stored as one JSON file per preset under your **Documents** folder:

```
[Documents]\Qeynos\Presets\<plugin>\<Preset Name>.json
```

`[Documents]` is resolved through the Windows **known-folder API**
(`SHGetKnownFolderPath(FOLDERID_Documents)`), so it follows a **OneDrive-redirected**
Documents folder correctly — on this build machine that resolves to
`C:\Users\<you>\OneDrive\Documents`. It is **never** a hard-coded
`%USERPROFILE%\Documents`.

`<plugin>` is a stable slug per plugin, e.g. `grit`, `ember`, `voxfit`. OVERSEER exports
two plugins from one bundle and uses two folders: **`overseer-node`** and
**`overseer-master`**.

There is also a sample file written by the test suite so you can see the format and
confirm the path resolves:

```
[Documents]\Qeynos\Presets\_selftest\Test Save.json
```

It is safe to delete; it is re-created whenever the disk-tier tests run.

## The JSON format

A preset is a flat JSON object: a `"name"` plus one numeric field per parameter.

**Factory** presets are authored by hand and key their fields by the plugin's *pretty*
preset ids (e.g. `"auto_gain"`, `"post_hp"`):

```json
{ "name": "Kick Bass Grit", "drive": 8.0, "mix": 100.0, "auto_gain": 1.0 }
```

**User** presets are snapshotted generically from the live parameters and key their
fields by the plugin's **nih-plug parameter ids** (e.g. `"autogain"`, `"posthp"`). The
values are *plain* (un-normalized): dB for gains, Hz for cutoffs, `0.0`/`1.0` for
toggles, and the option index for enums.

Both formats load through the same parser. The difference in key spelling is invisible in
use — the bar applies factory presets through each plugin's own mapping and user presets
through the generic parameter map, and the dirty dot works for both by snapshotting the
parameters immediately after a load and comparing against that baseline.

## For plugin authors (how the retrofit works)

`suite_core::presets` provides the disk tier:

- `list_user(plugin_id) -> Result<Vec<Preset>>`
- `save_user(plugin_id, name, &values) -> Result<PathBuf>` (name sanitized, overwrite ok)
- `load_user(plugin_id, name) -> Result<Preset>`
- `delete_user(plugin_id, name) -> Result<()>`
- `sanitize_name(name) -> String` (strips illegal path chars + control chars, collapses
  whitespace, trims dots/spaces; idempotent)

`suite_core::ui` provides the generic parameter bridge and the widget:

- `snapshot_params(&dyn Params) -> BTreeMap<String, f32>` — read every param's plain value
  keyed by param id.
- `apply_values(&dyn Params, &ParamSetter, &values)` — write a value map back through the
  host (begin/set/end bracketed). Exact inverse of `snapshot_params`.
- `PresetBar::new(plugin_id, &factory).show(ui, &*params, setter, apply_factory)` — the
  whole bar in one call. `apply_factory` is the plugin's existing per-preset mapping
  (factory JSON uses pretty keys); user presets are handled generically inside the bar.

Adopting the bar in a new editor is ~8 lines: keep an `Arc<Vec<Preset>>` of factory
presets, keep the plugin's `apply_preset(params, setter, &Preset)`, and call `PresetBar`
where the old preset dropdown was.
