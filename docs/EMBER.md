# EMBER — spectral fader / temporal smoother

A spectral processor that eases each frequency bin's magnitude toward the input with
independent **attack** and **decay** time constants. Decay reaches 60 s, so tails keep
ringing long after the input stops — from gentle spectral blur to endless washes and
frozen drones. Tails stay tonal via a phase-vocoder phase advance rather than smearing.

```
in ─ STFT(2048, hop 512, Hann) ─ per-bin state machine ─ fitting ─ iSTFT/OLA ─ mix ─ out
              factor-band curves: attack(f), decay(f)  (log-freq, 8 editable breakpoints)
```

Backed by the alloc-free streaming STFT engine `suite_core::stft` (window-sum-compensated
overlap-add, latency == FFT size). EMBER reports **2048 samples** of latency; the dry path
is delay-compensated so `Mix = 0` nulls against the input.

## How it works

Per bin `k`, once per STFT frame (hop time `T = 512 / sr ≈ 10.7 ms`):

```
coef  = 1 − exp(−T / τ)          τ = attack(f_k) if in_mag > state else decay(f_k)
state[k] += coef · (in_mag[k] − state[k])
```

- **Attack / Decay factor bands** — `attack(f)` and `decay(f)` are log-frequency curves
  defined by 8 editable breakpoints each (20 Hz → 20 kHz), interpolated smoothly in
  log-time. A slow attack blooms; a long decay sustains; per-band decay lets lows fade
  while highs shimmer on (or vice-versa).
- **Phase strategy** — while a bin's input magnitude is above the **Gate**, output phase
  locks to the measured input phase and the per-hop phase increment is recorded. Once the
  bin falls silent (a *generated tail*), phase is advanced by that recorded increment — a
  phase-vocoder advance — so the tail rings coherently at the bin's tonal frequency.
- **Fitting** — blends each bin toward a ~1/3-octave moving-average spectral envelope,
  smoothing spectral detail into a glued wash.
- **Freeze** — sets τ→∞ (coef 0): the current spectrum is captured and held indefinitely
  as a tonal drone, regardless of further input.
- **Tail Gain** — extra gain on generated-tail bins only, to push tails forward or back.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Attack 1–8 | 1..2000 ms | 20 | Per-band rise time constant (low→high freq) |
| Decay 1–8 | 5..60000 ms | 800 | Per-band fall time constant; up to 60 s tails |
| Fitting | 0..100 % | 0 | Blend bins toward the 1/3-oct spectral envelope |
| Freeze | on/off | off | Hold the captured spectrum (τ→∞) |
| Gate | −90..0 dB | −60 | Above → input phase; below → phase-vocoder tail |
| Tail Gain | −24..+24 dB | 0 | Gain applied to generated-tail bins only |
| Mix | 0..100 % | 100 | Dry/wet. At 0 %, output nulls against (delayed) dry. |

The GUI exposes the two 8-band curves as slider rows (one slider per log-frequency band)
plus the macro controls and a Freeze toggle.

## Factory presets

Bloom Pad · Infinite Wash · Freeze Drone · Spectral Gate-Fade · Fitting Glue.

## Testing in FL

1. Options → Manage plugins → "Find more plugins", then add **Qeynos EMBER** to a
   channel/mixer insert.
2. Load **Bloom Pad** and play a pad or vocal; notes should bloom and sustain past their
   natural release. Raise **Decay** bands for longer washes.
3. Play a sustained sound, tick **Freeze**, then stop the input — the spectrum holds as a
   steady drone. Untick to release.

EMBER adds 2048 samples of latency (reported to the host for delay compensation).

Offline audition renders (1 s pink-noise burst → 2 s tail, one per preset — Freeze Drone
captures during the burst then holds) are in `renders/EMBER/`.

## Freeze Mix

**Freeze Mix** (0–100%, default 100%) works alongside the **Freeze** toggle. Freeze stays a
toggle; Freeze Mix sets how much of the held/frozen texture you hear versus the live signal
while Freeze is engaged. At 100% it is the classic hard freeze (unchanged); lower it to blend
the live source back in so the freeze is a smooth crossfade rather than a sudden jump. The
blend is smoothed (~15 ms) and only active while Freeze is on.
