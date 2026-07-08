# X-RAY — shared cross-plugin spectrum analyzer

*Phase 3. A tier-2 **bus consumer**: every Qeynos plugin publishes its own 32-band output
spectrum to the shared bus, and X-RAY overlays **all of them at once** as colored curves in a
single window — the whole session's spectral balance without a meter on every track.*

X-RAY is two halves:

1. **A publisher baked into the suite.** Each audio plugin now taps its output through a
   32-band analyzer ([`suite_core::spectrum::SpectrumTap`]) and publishes
   `{ spectrum[32], peak, RMS, label, kind }` to its own slot on the tier-2 bus
   (`%TEMP%\qeynos-bus`, the same shared-memory file NERVE uses) at block rate, alloc-free.
2. **The X-RAY plugin.** Audio passes through **bit-exact**; the GUI reads every live slot
   with `Bus::snapshot_live()` and draws one curve per instance on a shared log-frequency /
   dB grid, with a legend, hover-highlight and click-to-solo.

## The spectrum tap — a constant-Q filter bank

`SpectrumTap` is a bank of **32 TPT state-variable bandpass filters** (`suite_core::dsp::Svf`),
log-spaced from **20 Hz to 20 kHz** (≈⅓-octave, `Q ≈ 4.32`). Every output sample is run through
all 32 filters; the squared bandpass output is integrated over the block, and at block end each
band's RMS is one-pole-smoothed to a steady published value (peak and full-band RMS the same).

**Why a per-sample bank, not an FFT?** It is simpler (no windowing/OLA, no ring buffer, no
latency), it needs no allocation, and it is *cheap enough to leave enabled in every plugin*:
32 SVFs × ~10 flops ≈ **320 flops per output sample** ≈ 15 Mflop/s at 48 kHz per instance —
**well under 0.5 % of one core** (asserted loosely in `spectrum::tests::cpu_cost_is_negligible`).
Constant-Q (fractional-octave) bands mean **pink noise reads roughly flat** and white noise
tilts up ~+3 dB/oct, matching a standard ⅓-octave RTA. The band count equals
`bus::NUM_SPECTRUM` so a band maps 1:1 onto a slot field.

## Publishing — the per-plugin retrofit

Publishing is wrapped in `suite_core::spectrum::SpectrumPublisher`, so each plugin's retrofit is
tiny and uniform (the NERVE slot-claim pattern):

- a `spectrum: SpectrumPublisher` field,
- `self.spectrum.init(sample_rate, PluginKind::Generic, "NAME")` in `initialize` (assigns a
  stable **session** bus id — never persisted, per the NERVE CLAP-state-reproducibility rule —
  and claims a slot),
- a `feed`-loop over the output buffer + `self.spectrum.publish()` at the end of `process`,
- `self.spectrum.release()` in `Drop`.

A removed / crashed / bridged-away instance's slot is reclaimed by the bus **heartbeat GC**
(3 s staleness), so the explicit `Drop` release is a promptness nicety, not a correctness
requirement. If the bus can't be mapped, the publisher degrades to a no-op.

### Which plugins publish

The **tractable majority** publish (PRD §1.5 "retrofit what's clean, DEFER the stragglers"):

> ascend · bandaid · carve · chamber · cleave · drift · ember · flyby · grit · halt · impact ·
> murmur · ouroboros · patina · pluck · seance · shapeshift · smudge · snap · swarm · tracer ·
> undertow · voxfit · voxkey · wire — **and X-RAY itself** (its own input, kind `Xray`, when
> **Publish** is on).

**Deferred** (see DEFERRED.md): **OVERSEER** (one bundle exporting two plugins with its own
tier-1 override bus — needs per-plugin care), **NERVE** (already owns a bus slot as a modulation
*source*; it is a transparent utility, not typically analyzed), and **_template** (kept minimal).
These do not publish a spectrum; everything else does.

## The analyzer GUI

- **Overlay** — one polyline per live instance across a log-frequency axis (grid lines at 100 Hz
  / 1 kHz / 10 kHz) and a dB axis (+6 … −96 dB, lines at 0/−24/−48/−72). Curve **color** is a
  golden-angle walk of the slot index, so instances stay visually distinct.
- **Legend** — one row per source: color swatch · **label** · bus id · **peak / RMS dB**.
- **Hover** a legend row → that instance's curve stays bright and **all others dim**.
- **Click** a legend row → **solo-dim** it (persists until clicked again or *clear solo*); hover
  still overrides for the current frame.
- **Freeze** — holds the last snapshot so you can inspect a moment.
- **Publish** — toggles X-RAY publishing its *own* input spectrum (so it can appear as a source);
  off removes it from the bus.
- **Out** — a trim. At **0 dB it is bit-exact** (`out_gain(0.0) == 1.0`), so X-RAY is a
  transparent inline probe.

Params are trivial (two toggles + a trim), so X-RAY **skips the preset bar** by design.

## FL Studio caveat (un-bridged instances)

The bus is one OS-wide shared-memory file, so X-RAY sees any instance that has mapped it —
**including "Make bridged" instances**, since the file is process-independent. The only way an
instance is invisible is if it never publishes (the deferred plugins above) or the host sandbox
blocks `%TEMP%` mapping. In FL, keep the default (un-bridged) and use **Manage plugins → Find
more** after installing so X-RAY and the republished plugins are rescanned. A publisher that is
paused/stopped (not processing) goes stale after **3 s** and drops off the display — that is
expected.

## Done-bar (PRD §4)

*"reads ≥2 live slots' spectra from the bus in a two-instance test."* Covered by
`xray::tests::two_instances_publish_distinct_spectra_and_reader_sees_both`: two bus handles
(two "DLLs") publish a low-band-limited and a high-band-limited noise spectrum through
`SpectrumTap`; the X-RAY reader's `snapshot_live()` sees **both** slots, each with energy
concentrated in the correct half of the band and distinct dominant bands. Passthrough
bit-exactness is `xray::tests::passthrough_is_bit_exact_at_unity`.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Publish | on/off | on | Publish X-RAY's own input spectrum to the bus |
| Freeze | on/off | off | Hold the current display |
| Out | −24…+24 dB | 0 dB | Output trim; **bit-exact passthrough at 0 dB** |
