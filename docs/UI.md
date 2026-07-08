# Controls — the shared UI interaction model

Every Qeynos plugin editor is built from the same widgets in `suite_core::ui`, so the
controls behave identically across the suite. This is the "controls" boilerplate the
per-plugin manuals reference. The endgame **PEDAL-UI / CONSOLE v2** theme only re-skins
these widgets — it never changes any interaction described here (SPECS "PEDAL-UI —
LOCKED", guardrail 4).

## Knobs (the standard parameter control)

Continuous and stepped parameters render as a **rotary knob** (arc + tick ring + needle,
label above, live value below):

| Action | Result |
|---|---|
| **Drag up / down** | Increase / decrease. Vertical drag; full range over ~250 px. |
| **Ctrl-drag** (or Shift-drag) | **Fine** adjust — ~10× finer resolution for precise values. |
| **Double-click** | Reset the parameter to its **default**. |
| **Scroll wheel** | Step the value. One detent per notch for stepped params; a small fixed step otherwise. |
| **Click the value text** | Opens a **text field** — type a value and commit (see below). |

Stepped parameters (Int / Enum) use the same knob with **detents** (the value snaps to
each valid step; scroll moves exactly one step). Boolean parameters render as a **toggle
pill** instead of a knob (click to flip).

All edits are modulation-safe: a drag brackets the change with
`begin_set_parameter` / `end_set_parameter`, and values are written through the param's
normalized setter so host automation and modulation stay correct.

## Click-to-type (typing exact values)

Click the value text under any knob to edit it:

- A real text field opens with keyboard focus. The value text is a plain label until you
  click it, so there is **no phantom caret** when you are not editing.
- **Enter** commits — the string is parsed through the parameter's own
  `string_to_value`, so anything the readout shows (e.g. `-6.0 dB`, `440 Hz`, `1/8`,
  `A#3`) parses back.
- **Esc** cancels (no change).
- **Clicking away** commits the current text.

> **FL Studio note.** FL's plugin wrapper can capture computer-keyboard keystrokes for
> its own typing-keyboard-to-piano feature, so typed digits may not reach the plugin.
> If typing does nothing inside FL, toggle **"Allow the plugin to steal keyboard focus"**
> (a.k.a. disabling *Typing keyboard to piano* while the editor is focused) on the
> plugin wrapper. See `CHECKPOINTS.md`. Correctness of the parse/commit path itself is
> covered by unit tests in `suite_core::ui`; the FL wrapper toggle is host-side.

## Uniform window scaling

The whole editor scales as **one unit** (egui zoom / pixels-per-point) rather than
reflowing its layout:

- Content is laid out at a fixed **base logical size** per plugin; resizing the window
  maps to a zoom factor, so text, knobs, and spacing all scale together.
- **Aspect-safe:** zoom is driven by the window width, so content never distorts.
- **Snap points** at **75 % / 100 % / 125 % / 150 %** are surfaced in a small **size menu**
  in the top-right corner (it shows the current percentage). A free resize "clicks into"
  those stops; picking one from the menu locks that zoom until you drag the window again.
- At 100 % nothing clips (the window will not shrink below the base size).
- The chosen size is **persisted** with the plugin state (it rides on the editor's
  `EguiState`, which nih-plug serializes), so it is restored with the project/preset.

### Scaling limitation (host resize API)

`nih_plug_egui` exposes no public API for a plugin to *request* a host window resize from
shared code — only the user's corner-drag changes the window size. So the size menu
snaps the **zoom of the current window**; it cannot grow the OS window on its own. To get
a larger physical window at a snapped zoom, drag the corner (it snaps to the stops). This
is recorded in `DEFERRED.md`.
