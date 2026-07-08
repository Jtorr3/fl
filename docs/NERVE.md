# NERVE — suite modulation bus

NERVE is a **modulation source**: it generates 8 continuous control streams (LFOs, envelope
followers, random sample-and-hold, hand macros) and **publishes them to a suite-wide bus** so
that **any parameter of any other Qeynos plugin can "listen" to a stream** and be modulated by
it — even across plugin DLLs and (un-bridged) processes. It is the suite's answer to a global
"mod matrix": one NERVE on any track drives filters, drives, mixes, sizes… everywhere.

NERVE passes audio through **bit-exact** (zero latency) — it is a modulation *tap*, so you can
drop it inline on the track whose signal you want an envelope follower to track.

```
 4 LFOs (8 shapes incl S&H, free/synced) ─┐
 2 env followers (own input) ─────────────┤→ 8 streams ─→ tier-2 BUS ─→ any plugin's MOD section
 2 random S&H ────────────────────────────┤              (%TEMP%\qeynos-bus)
 4 macro knobs ───────────────────────────┘
```

## The 8 streams (fixed layout)

| stream | source | range |
|--------|--------|-------|
| S1..S4 | LFO A..D (+ paired Macro offset) | −1..1 |
| S5..S6 | Env follower A..B (of NERVE's own input) | 0..1 |
| S7..S8 | Random S&H A..B | −1..1 |

The **4 Macro** knobs are bipolar hand controllers **summed into streams S1..S4**: set an LFO's
Depth to 0 and its stream becomes a pure hand-ridden DC modulator (that is the *Macro Desk*
preset). So all four listed source families — LFOs, env followers, S&H, macros — reach the bus
within a bounded, fuzz-safe param set.

### LFOs (A–D)
Each: **Rate** (0.01–20 Hz, or **Sync** to a **Division** 4 bars…1/16 from host tempo), **Shape**
(Sine, Triangle, Saw Up, Saw Down, Square, **S&H** stepped-random, **Smooth Rnd** interpolated
random, **Exp Pulse**), **Depth** 0–100 %.

### Env followers (A–B)
Follow the |input| of the track NERVE sits on. **Attack**/**Release** (ms) + **Depth**. Unipolar.

### Random S&H (A–B)
Free-running stepped random with **Rate** (Hz), a **Slew** glide (0 = hard steps, →1 = slow
morph), and **Depth**. Bipolar.

## How another plugin "listens" (the per-param MOD section)

Every retrofitted plugin gains a collapsible **MOD** section (via `suite_core::ui::mod_section`).
For a modulatable param you pick a **source** (a live NERVE instance on the bus, shown by its
label), one of its **8 signals**, a **depth** (−1..1) and a shaping **curve** (Linear / Unipolar /
Squared / S-Curve). The route is **persisted with the project**.

Modulation is applied **at block rate as an additive, normalized offset**, feeding the plugin's
DSP the modulated value **without touching host parameter state**:

```
modulated_normalized = clamp(base_normalized + depth · curve(signal), 0, 1)
```

The host, its automation and the on-screen knob keep showing the **base** value; the modulation
is a live offset on top, exactly like an internal LFO tool. The plugin's own smoother removes any
block-rate stepping. (Implementation: `suite_core::modlisten::ModRoutes::modulated_float`, applied
where each plugin builds its per-block `configure` settings — the host param is never written from
the audio thread.)

---

# Bus architecture (PRD §3 tier-2)

The suite has two bus tiers. **Tier 1** (`plugins/overseer/src/bus.rs`) is a same-DLL
`OnceLock<Registry>` — genuinely shared only when the linked plugins load into one address space.
**Tier 2** (`suite_core::bus`, built here) is **file-backed shared memory** that links *any*
Qeynos instance in *any* process.

### Shared-memory file
A single fixed-size file at **`%TEMP%\qeynos-bus`** is mapped with `memmap2::MmapMut`
(`PAGE_READWRITE` / shared — the canonical Windows cross-process page-sharing mechanism, no named
kernel object). The file is created **at its full fixed size before mapping and never grown
live**. A header carries a **magic + version + slot count**; a stale or incompatible file (older
layout, different slot count, wrong size) is **recreated** on open. Everything is accessed through
raw pointers into the map as **atomics**.

### Slots (64)
Each plugin instance **claims** one slot (CAS on a `0 = free` instance-id sentinel). A slot holds:
magic-checked identity (instance id, plugin-kind tag, user label), the **8 modulation f32
streams**, a **32-band spectrum + peak/RMS** (published for **X-RAY**, the next consumer), and a
heartbeat.

### Per-slot SEQLOCK
Each slot's payload is guarded by a generation counter: the **writer bumps it to odd, writes,
bumps to even**; a **reader loads it (must be even), reads, re-loads, and retries** on odd/changed.
This gives a wait-free, alloc-free reader that **never observes a torn / mixed-generation
snapshot** (unit-tested with a writer thrashing all 8 signals on a background thread while the
reader asserts they always agree). No lock is ever held in `process()`; publishing and reading are
alloc-free after init.

### GC / liveness
There is no cross-process `Arc` strong-count, so liveness is a **wall-clock heartbeat**: the owner
stamps `last_beat_ms` every block; a slot unbeaten for ~3 s is **stale** and reclaimable, and any
new `claim` GCs it first (CAS the instance id back to free). Crashed / removed / bridged-away
instances therefore never linger.

### What X-RAY will need (next iteration)
X-RAY is the tier-2 **consumer** that renders every live instance's 32-band spectrum. The slot
already carries `spectrum[32]` + `peak`/`rms` and `Bus::publish_spectrum`; the **publishing** side
still needs wiring into the suite-core plugin wrapper (block-rate FFT → `publish_spectrum`), which
is X-RAY's "retrofit suite-core wrapper → rebuild-all" first step (mirrors NERVE's). Readers use
`Bus::snapshot_live()`.

## FL Studio caveat

Cross-plugin modulation requires the plugins to be **un-bridged** (same process) — FL's default is
fine. If you tick **"Make bridged"** on an instance it lands in a separate bridge process; it maps
the same `%TEMP%\qeynos-bus` file, so a **bridged NERVE still publishes** and a bridged listener
still reads (the file is shared OS-wide). The only thing that severs the link is a host that fully
isolates a plugin from the temp filesystem, which FL does not. (Tier-1 OVERSEER, by contrast, goes
dark when bridged — that is why NERVE/X-RAY use tier-2.)

## Instance ids & routes across reloads

A NERVE's bus identity is **session-scoped** (assigned on activation), *not* persisted — a
persisted per-instance random id would make two instances' CLAP states differ and fail the
validator's state-reproducibility test. Consequently **listener routes are session-live**: after
reloading a project, re-point a plugin's MOD source to the NERVE (its label makes this quick). A
future stable-id scheme can lift this without breaking state reproducibility.

## Presets
Slow Swell Bus · 16th Pump · Chaos Pair · Macro Desk · Breathe · Techno Pump 1/8.

<!-- BUILT-IN-MANUALS: canonical sections rendered in-GUI by the '?' button (parsed by suite_core::manual). -->

## What It Is

NERVE is the suite's modulation source: it generates 8 continuous control streams — 4 LFOs, 2
envelope followers, 2 random sample-and-hold, plus 4 hand macros — and publishes them to a
suite-wide bus. Any parameter of any other Qeynos plugin can then "listen" to a stream from its
MOD section, so one NERVE becomes a global mod matrix that moves filters, drives, mixes and sizes
everywhere. It passes audio through bit-exact (zero latency), so drop it inline to follow a track.

## Signal Flow

```
 4 LFOs  (Rate/Sync·Div, Shape, Depth) ─┐
 2 env followers (Env1/Env2 Atk·Rel·Depth, of NERVE's input) ─┤
 2 random S&H (S&H1/S&H2 Rate·Slew·Depth) ────────────────────┤→ 8 streams ─→ tier-2 BUS ─→ any plugin's MOD section
 4 macros (Macro 1–4, summed into S1–S4) ─────────────────────┘        (%TEMP%\qeynos-bus)
```

## Controls

Each LFO A–D shares five controls:

- **Rate** — LFO frequency, 0.01–20 Hz (used when free-running).
- **Sync** — tempo-sync toggle; on = the LFO locks to host tempo and uses **Div** instead of **Rate**.
- **Div** — synced note division (4 bars … 1/16) used when **Sync** is on.
- **Shape** — waveform: Sine / Triangle / Saw Up / Saw Down / Square / S&H / Smooth Rnd / Exp Pulse.
- **Depth** — output amount of that LFO's stream, 0–100 %.

Hand macros (bipolar, summed into streams S1–S4):

- **Macro 1** — hand controller for stream 1, −1…+1.
- **Macro 2** — hand controller for stream 2, −1…+1.
- **Macro 3** — hand controller for stream 3, −1…+1.
- **Macro 4** — hand controller for stream 4, −1…+1.

Envelope followers (follow the level of NERVE's own input):

- **Env1 Atk** — follower A attack, 0.1–200 ms.
- **Env1 Rel** — follower A release, 5–1000 ms.
- **Env1 Depth** — follower A output amount, 0–100 %.
- **Env2 Atk** — follower B attack, 0.1–200 ms.
- **Env2 Rel** — follower B release, 5–1000 ms.
- **Env2 Depth** — follower B output amount, 0–100 %.

Random sample & hold:

- **S&H1 Rate** — generator A step rate, 0.01–20 Hz.
- **S&H1 Slew** — generator A glide between steps, 0–100 % (0 = hard steps).
- **S&H1 Depth** — generator A output amount, 0–100 %.
- **S&H2 Rate** — generator B step rate, 0.01–20 Hz.
- **S&H2 Slew** — generator B glide between steps, 0–100 %.
- **S&H2 Depth** — generator B output amount, 0–100 %.

## Recipes

1. **Dark-techno sidechain pump — "Warehouse Sidechain"** — LFO A **Sync on**, **Div 1/4**,
   **Shape Saw Down**, **Depth 100 %**. In the target plugin's MOD section pick this NERVE, signal
   **S1**, and set a negative depth on its **Mix** or gain → a tight tempo-locked 1/4 pump across the suite.
2. **Atmospheric-DnB drift — "Underwater Cathedral"** — LFO A **Shape Smooth Rnd**, **Rate 0.04 Hz**,
   **Depth 100 %**, with a slow sine on LFO C. Route **S1** to a reverb size or filter cutoff for
   slow, non-repeating movement under pads and Rhodes.
3. **Vocal-rip glitch — "Datamosh"** — **S&H1 Rate 16 Hz**, **S&H1 Slew 0** (hard steps),
   **S&H1 Depth 100 %**, plus a stepped LFO A (**Shape S&H**). Route **S7** to a ripped vocal's
   pitch or filter cutoff for Sewerslvt-style stutter modulation.
