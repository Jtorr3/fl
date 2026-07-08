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

## Manual (the built-in '?' panel)

Every plugin editor carries a small **`?` button** in the top-right of the header area
(just left of the size menu). It toggles a closable, scrollable **manual panel** rendered
from the plugin's own `docs/<PLUGIN>.md` — one source of truth, readable both on GitHub
and in-GUI.

- The doc is embedded at compile time (`include_str!`) and parsed by
  `suite_core::manual` into `## ` sections; `suite_core::ui::manual_button` renders them.
- Four canonical sections are surfaced in reading order (missing ones are skipped, never a
  panic): **What It Is** (2-3 sentences), **Signal Flow** (the ASCII diagram, monospace),
  **Controls** (every param — what it does *musically*), **Recipes** (concrete workflow
  recipes with real settings, tuned to dark-techno / atmospheric-dnb / vocal-rip work).
- A per-plugin test (`manual::assert_manual_covers_params`) cross-checks that every param's
  display name appears in the Controls section (catches drift) and that Recipes is
  non-empty.

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

## CONSOLE v2 theme (PEDAL-UI)

The suite-wide skin is **CONSOLE v2** — a "console inside a pedal": a machined hardware-pedal
enclosure wrapped around a recessed **amber-CRT telemetry bay**. It is **paint only**: every
interaction above (knob drag / fine-drag / reset / scroll / click-to-type, uniform scaling,
PresetBar, MOD section, the `?` manual) is byte-identical to the plain theme — CONSOLE re-skins
the widgets, it never changes an interaction rule. All of it lives in `suite-core/src/ui.rs`
as cheap egui vector painting (no image assets — the suite stays self-contained).

**How it hooks in (why the retrofit is nearly zero-touch):** every editor already funnels
through `ScaledWindow::show`, `apply_theme`, and the shared `labeled_slider`/toggle widgets.
`ScaledWindow` derives the plugin **slug** from its window id (`qeynos-<slug>-window`), resolves
that plugin's theme prefs, paints the enclosure behind the content, and publishes the effective
`(console, crt_motion)` state on a per-frame egui-memory channel. The paint helpers
(`knob_face`, `toggle_control`, `crt_frame`) read that channel, so a plugin that only calls
`labeled_slider` is re-skinned with **no edits**. The only per-plugin addition is an optional
`crt_lines`/`crt_frame` call to place that plugin's telemetry/visualization into the CRT bay.

**Pieces:**
- **Enclosure** — dark machined body, an amber brand strip, top-highlight/bottom-shadow bevel,
  and four corner screws (`paint_enclosure`, ~14 primitives/frame).
- **CRT bay** — `crt_frame(ui, id, height, add_contents)` gives a recessed bronze-black glass
  panel with faint **static scanlines** and (motion on) a **blinking cursor**; the plugin paints
  amber monospace text / meters / scopes inside. `crt_lines(ui, id, title, &[(label, value)])`
  is the convenience for a titled terminal readout, used by every plugin without a dedicated
  scope. The values shown are honest live state that ALSO appear on the knobs.
- **Knobs** — a tick-ring collar + machined cap (paint only; the widget's hit-testing/value
  logic is untouched). Values stay plain crisp text below the knob (no glow on digits).
- **Toggles** — footswitch-style cap with an amber **LED** when engaged.

**Usability guardrails (SPECS PEDAL-UI — these override aesthetics):**
1. Values are always readable plain text on the knob; no glow blurs digits.
2. **CRT motion (scanline drift / cursor blink) is toggleable and OFF-able**, persisted per
   plugin. System/host reduced-motion is not reliably detectable under baseview, so this reduces
   to the explicit per-plugin toggle.
3. The CRT is **additive** — every value in it is also on a knob; nothing operable lives only in
   the screen.
4. UI-CORE-FIX interactions are untouchable (paint only).
5. Contrast: phosphor `#ffb000` on the glass ≈ 13:1 (body ≥ 4.5:1); dim phosphor ≈ 5.4:1
   (labels ≥ 3:1).
6. Effects are cheap painter ops; the cursor requests only a slow (~8 fps) repaint, so idle GUIs
   stay cheap.

**Settings + THEME-OFF fallback:** the top-right **size menu** (the `NN%` button) gains a THEME
section with **Console skin** and **CRT motion** checkboxes. Both persist per plugin in one
suite-wide file `[MyDocuments]/Qeynos/ui_prefs.json` (keyed by slug — no plugin Params struct
changes). Turning **Console skin** off reverts that plugin wholesale to the plain minimal-dark
look (one code-path switch) for emergency legibility. Defaults: Console **on**, CRT motion **on**.
