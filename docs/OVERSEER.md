# OVERSEER — mastering system (one library, two plugins)

One bundle (`overseer.clap` / `overseer.vst3`) exports **two** plugins:

- **Qeynos OVERSEER Node** — a channel strip you put on individual tracks.
- **Qeynos OVERSEER Master** — the mastering bus you put on the master.

Every Node registers a slot on a same-DLL shared bus. The Master's GUI shows a live grid
of all Node instances (label, peak/RMS/LUFS-M meters, key params) and can **override**
each Node's Threshold / Ratio / Drive / Width / Trim remotely. Overridden params show an
`OVR` badge (and a `MASTER OVERRIDE` banner) on the Node GUI; touching the param locally
(GUI or host automation) steals control back (write-wins timestamps, block granularity).

> **Bridging caveat:** the link relies on both plugins living in the same process. FL
> loads same-bitness plugins in-process by default; ticking "Make bridged" on either
> instance severs the link (the plugins still process audio normally).

## Node — signal flow

```
in → meter → 4-band EQ (LS · bell · bell · HS) → FF compressor (RMS, soft knee)
   → tanh saturation → M/S width → trim → meter → mix → out
```

| Param | Range | Notes |
|---|---|---|
| Label | text | instance name shown on the Master grid (persisted, not automatable) |
| Low/High Freq | 20 Hz–20 kHz | shelf corners |
| Low/High Gain | ±24 dB | shelves |
| Bell 1/2 Freq, Gain, Q | 20 Hz–20 kHz, ±24 dB, 0.1–10 | parametric bells |
| Threshold | −60..0 dB | compressor threshold (overridable) |
| Ratio | 1–20:1 | compressor ratio (overridable) |
| Knee | 0–24 dB | soft knee width |
| Attack / Release | 0.1–100 ms / 10–1000 ms | detector ballistics |
| Makeup | ±24 dB | post-comp gain |
| Drive | 0–24 dB | tanh saturation amount (overridable) |
| Width | 0–2 | M/S width, 0 = mono, 1 = unity (overridable) |
| Trim | ±24 dB | output trim (overridable) |
| Mix | 0–100 % | dry/wet; 0 nulls the dry input |

Presets: **Kick Strip**, **Vocal Strip**, **Bus Glue**.

## Master — signal flow

```
in → 4-band EQ → 3-band multiband comp (LR4 splits on TPT SVFs)
   → lookahead limiter (2 ms, brickwall) → LUFS meter → mix → out
```

- The limiter delays audio by its 2 ms lookahead and **reports that latency** to the
  host; the dry path of `Mix` is latency-matched so mix=0 nulls.
- True-peak-style metering approximated with 4x-oversampled peak detection (`TP≈`).
- The LUFS meter is ITU-R BS.1770 (`suite_core::loudness`): K-weighting (shelf + RLB
  high-pass, sample-rate-correct coefficients), momentary 400 ms, short-term 3 s, and
  gated integrated loudness with a GUI **RESET LUFS** button.

| Param | Range | Notes |
|---|---|---|
| EQ (10 params) | as Node | low shelf, 2 bells, high shelf |
| XO Low / XO High | 20 Hz–20 kHz | LR4 crossover frequencies |
| Low/Mid/High Threshold | −60..0 dB | per-band comp |
| Low/Mid/High Ratio | 1–20:1 | per-band comp |
| Low/Mid/High Makeup | ±24 dB | per-band gain |
| Knee / Attack / Release | 0–24 dB / 0.1–100 ms / 10–1000 ms | shared comp ballistics |
| Ceiling | −12..0 dB | limiter output ceiling (brickwall) |
| Lim Release | 10–1000 ms | limiter gain-envelope release |
| Mix | 0–100 % | latency-matched dry/wet |

Presets: **Techno Master**, **Gentle Master**, **Loud & Proud**.

## Done-bar verification (offline tests, `cargo test -p overseer --release`)

1. **Limiter:** +6 dBFS sine into ceiling −1 dBFS → output peak ≤ −0.9 dBFS (and > −2.5,
   i.e. not over-attenuated); plus a sample-continuity check (no clicks) post-settle.
2. **LUFS:** meter reading of a −20 dBFS-RMS 997 Hz sine matches the analytic value from
   the module's own K-filter response within ±0.5 LU (momentary AND integrated); with the
   K-weighting test hook disabled the meter reads −20.0 ±0.1.
3. **Bus round-trip:** Node registers, Master writes an override, the Node's effective
   param reflects it the next block; a local touch steals control back; dropped Nodes GC.

Renders: `renders/OVERSEER/*.wav` (each preset over synthetic kick/vocal/mix signals).
