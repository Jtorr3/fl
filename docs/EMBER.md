# EMBER — spectral fader / temporal smoother

## What It Is

EMBER is a spectral processor that eases each frequency bin's magnitude toward the input with
independent **attack** and **decay** time constants, set per frequency band. Decay reaches 60 s,
so tails keep ringing long after the input stops — from gentle spectral blur to endless washes
and frozen drones, and tails stay tonal (phase-vocoder phase advance) rather than smearing into
noise. It is the suite's atmosphere machine: blooming pads, infinite reverb-like beds, and
key-holdable drones for dark-techno and atmospheric-dnb.

## Signal Flow

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

## Controls

- **Attack breakpoints — Atk 1, Atk 2, Atk 3, Atk 4, Atk 5, Atk 6, Atk 7, Atk 8** — the eight
  log-frequency breakpoints (low → high, 20 Hz → 20 kHz) of the per-band *rise* time constant.
  Short values track the input tightly; long values make notes bloom in slowly. Each 1…2000 ms.
- **Decay breakpoints — Dec 1, Dec 2, Dec 3, Dec 4, Dec 5, Dec 6, Dec 7, Dec 8** — the eight
  log-frequency breakpoints of the per-band *fall* time constant; this is the tail length per
  region, so you can let lows die fast while highs shimmer on. Each 5…60000 ms (up to 60 s tails).
- **Fitting** — blends every bin toward a ~1/3-octave spectral envelope, gluing spectral detail
  into a smooth wash; 0 % keeps detail, 100 % is fully smeared. 0…100 %.
- **Freeze** — captures the current spectrum and holds it forever (τ→∞) as a tonal drone,
  regardless of further input. on/off.
- **Freeze Mix** — while **Freeze** is engaged, how much of the held/frozen texture you hear
  versus the live signal; 100 % is a classic hard freeze, lower values crossfade the live source
  back in. 0…100 %.
- **Gate** — the magnitude threshold that decides input-locked phase (above) vs a phase-vocoder
  generated tail (below); raise it to make tails start sooner. −90…0 dB.
- **Tail Gain** — extra gain applied only to generated-tail bins, to push the ringing tails
  forward or tuck them behind the dry signal. −24…+24 dB.
- **Mix** — dry/wet blend; at 0 % the output nulls against the (delay-compensated) dry input.
  0…100 %.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Atk 1–8 | 1..2000 ms | 20 | Per-band rise time constant (low→high freq) |
| Dec 1–8 | 5..60000 ms | 800 | Per-band fall time constant; up to 60 s tails |
| Fitting | 0..100 % | 0 | Blend bins toward the 1/3-oct spectral envelope |
| Freeze | on/off | off | Hold the captured spectrum (τ→∞) |
| Freeze Mix | 0..100 % | 100 | Held texture vs live signal while Freeze is on |
| Gate | −90..0 dB | −60 | Above → input phase; below → phase-vocoder tail |
| Tail Gain | −24..+24 dB | 0 | Gain applied to generated-tail bins only |
| Mix | 0..100 % | 100 | Dry/wet. At 0 %, output nulls against (delayed) dry. |

The GUI exposes the two 8-band curves (**Atk 1…Atk 8**, **Dec 1…Dec 8**) as slider rows (one
slider per log-frequency band) plus the macro controls and a Freeze toggle.

## Recipes

1. **Dark-Techno Blooming Pad** *(start: Bloom Pad)* — raise the mid/high **Dec** bands (**Dec 5**
   through **Dec 8**) to ~3000–5000 ms while keeping **Dec 1**/**Dec 2** shorter (~600 ms) so the
   sub doesn't wash out. Set **Atk 4**–**Atk 8** ≈ 200 ms for a soft rise, **Fitting** 20 %,
   **Mix** 100 %. Chords swell and hang like a lit-warehouse pad.
2. **Infinite Atmospheric Wash** *(start: Infinite Wash)* — push all **Dec 1…Dec 8** toward
   40000–60000 ms, **Atk 1…Atk 8** ≈ 40 ms, **Fitting** 60 % for a glued cloud, and **Tail Gain**
   +6 dB to bring the endless tail forward. Automate **Mix** up into the breakdown of an
   atmospheric-dnb track for a bottomless corridor.
3. **Key-Held Freeze Drone** *(start: Freeze Drone)* — play a sustained chord, tick **Freeze**,
   then stop the input: the spectrum holds as a steady tonal drone. Set **Freeze Mix** ≈ 70 % so
   a little live source bleeds back in and the drone shifts subtly with the track. Untick to
   release.
4. **Spectral Gate-Fade Vocal Rip** *(start: Spectral Gate-Fade)* — short **Atk 1…Atk 8** (~4 ms),
   medium **Dec** (~400 ms), and lift **Gate** toward −35 dB so quiet vocal tails immediately
   become phase-vocoder ghosts. Blend with **Mix** ≈ 60 % for a haunted, granular vocal tail.

## Factory presets

Bloom Pad · Infinite Wash · Freeze Drone · Spectral Gate-Fade · Fitting Glue.

## Testing in FL

1. Options → Manage plugins → "Find more plugins", then add **Qeynos EMBER** to a
   channel/mixer insert.
2. Load **Bloom Pad** and play a pad or vocal; notes should bloom and sustain past their
   natural release. Raise the **Dec** bands for longer washes.
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
