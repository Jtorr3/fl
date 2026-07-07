# SMUDGE — spectral chaos (Smear clone)

A short-time Fourier transform (**2048** frame / **512** hop / Hann, `suite_core::stft`, the
EMBER engine) opened up into a chaos engine. Every STFT frame passes through **four spectral
ops in a fixed order** — scramble → spectral delay → blur → smear/stretch — **each with its own
amount and each _exactly_ bypassed when that amount is 0**. A slow **chaos** sample-and-hold
macro randomises the op parameters over time. Reports **2048-sample latency** (the STFT frame
size); the dry path is delay-matched so `mix = 0` nulls cleanly.

```
        ┌────────────────────── STFT 2048 / hop 512 / Hann ──────────────────────┐
 in ──┬─┤ per frame:  ① scramble → ② spectral delay → ③ blur → ④ smear/stretch    ├─ wet ─ safety ─┐
      │ └──────────────────────────── ▲ chaos S&H modulates op params ────────────┘        clip    │
      │                                                                                   (mix)     ▼
      └── delay(2048) ── dry ───────────────────────────────────────────────────────────► out = dry + mix·(wet−dry)
```

## The four ops (fixed order 1 → 4)

Order is fixed and documented: **scramble** first (moves bins around), then **spectral delay**
(echoes those bins across time), then **blur** (temporally averages magnitudes), then
**smear/stretch** (remaps the bin axis). Each reads and writes the complex spectrum in place;
each is skipped entirely — the spectrum is untouched — when its (smoothed) amount is 0, so with
all four at 0 the wet path is the STFT's own identity reconstruction.

### 1 · Scramble
Bins are partitioned into contiguous **neighbourhoods of width `2N+1`** (N = `Scramble Range`
× 48 bins) and **randomly permuted within each neighbourhood** (Fisher–Yates, `suite_core`'s
xorshift `Rng`; DC and Nyquist stay fixed). The permutation is **redrawn every `Scramble Rate`
frames** — **rate 1 = per-frame chaos**, **rate 4–8 = musical** (SPECS). `Scramble` crossfades
the original spectrum toward the permuted one.

A bin permutation is energy-preserving _per frame_, but shuffling bins **decorrelates
overlapping WOLA frames**, so their overlap-add sums in _power_ (×√overlap) instead of
_coherently_ (×overlap) — a fixed **−6 dB** loss at full per-frame scramble. The permuted
content is therefore pre-multiplied by **√(fft/hop) = 2.0** makeup so total energy is
preserved (done-bar 2). At musical redraw rates the reconstruction is coherent and needs no
makeup, but the constant makeup is harmless there (it only slightly over-restores the
already-coherent overlap).

### 2 · Spectral delay
Bins are grouped into **~⅓-octave bands** (30 bands, 20 Hz–20 kHz). Each band has its own
**frame delay** (1…32 frames) read from a **tilt curve**: `Delay Tilt` +1 = lows-short /
highs-long, −1 = the inverse, 0 = flat (all ~16 frames). A per-bin complex ring stores the last
32 frames. Output is **additive**: `out = current + Delay·delayed`. The ring is fed back —
`ring = softlimit(current + Feedback·delayed)` — for cascading echoes; `Delay Feedback` is
capped at 0.95 so the loop always decays, and an in-loop tanh soft-limiter bounds any resonant
bin.

To keep the additive echo (and its feedback tail) **≤ 0 dBFS**, the echoed frame's energy is
normalised **down** to a decaying envelope of the input frame-energy (attenuate-only, no
ducking while input is present; the tail releases over ~1 s). A final wet-path **safety
soft-clip** (exact identity below 0.9, tanh above) is the hard guarantee — it never touches
normal-level reconstruction, so the null/identity tests are unaffected.

### 3 · Blur
Per bin, the **magnitude** is smoothed by a one-pole `state += coef·(mag − state)` with a **τ
per band** (base `Blur Time` 5–2000 ms, tilted across frequency by `Blur Tilt`). `Blur`
crossfades the instantaneous magnitude toward the averaged one. **Phase** follows the input
until blur dominates: above 50 % the output phase interpolates (along the shortest arc) toward a
**phase-vocoder advance** (per-hop phase increment accumulated forward, EMBER's tail approach),
so a heavily-blurred bin keeps ringing at its tonal frequency instead of smearing.

### 4 · Smear / stretch
The **bin index is remapped** by `Stretch Factor` (0.5–2): output bin `k` reads source bin
`k / factor` with **linear interpolation** between adjacent source bins (a spectral
pitch/stretch illusion). The remapped spectrum is **energy-normalised** back to the original,
and `Stretch` crossfades toward it. Factor > 1 shifts spectral energy upward; < 1 downward.

## Chaos macro
A **sample-and-hold** set of random values is redrawn every `Chaos Rate` frames and scaled by
`Chaos Depth`, modulating the op parameters (redraws are smoothed by the frame-rate one-poles,
so no clicks). **Amount modulation is multiplicative** (a random gain ≤ 1): it can thin a
moving op out, but it **can never lift a base amount of 0**, so the exact-bypass guarantee
survives any chaos setting (verified: full-depth chaos on all-zero amounts still nulls < −60
dB). Chaos also detunes the stretch factor, offsets the delay tilt, and scales the scramble
neighbourhood.

## Smoothing & latency
The four amounts plus stretch factor and delay tilt are **frame-rate one-pole smoothed**
(40 ms), primed on the first configure to avoid a start-up glide; `Mix` is per-sample smoothed.
Latency is the STFT frame size, **2048 samples**, reported via `set_latency_samples`; the dry
path is delayed by the same amount for a clean `mix = 0` null.

## Parameters

| Group | Param | Range | Notes |
|---|---|---|---|
| Scramble | Scramble | 0–100 % | Amount (crossfade toward the permuted spectrum). 0 = exact bypass. |
| Scramble | Scramble Range | 0–100 % | Neighbourhood half-width N (0…48 bins). |
| Scramble | Scramble Rate | 1–32 fr | Frames between permutation redraws. 1 = chaos, 4–8 = musical. |
| Delay | Delay | 0–100 % | Amount (additive echo level). 0 = exact bypass. |
| Delay | Delay Tilt | −1…+1 | Per-band delay curve: +1 lows-short/highs-long, −1 inverse, 0 flat. |
| Delay | Delay Feedback | 0–95 % | In-loop feedback (soft-limited; always decays). |
| Blur | Blur | 0–100 % | Amount (blend toward the temporally-averaged magnitude). 0 = exact bypass. |
| Blur | Blur Time | 5–2000 ms | Base magnitude time constant τ. |
| Blur | Blur Tilt | −1…+1 | τ tilt across frequency (+1 smooths highs more). |
| Stretch | Stretch | 0–100 % | Amount (crossfade toward the remapped spectrum). 0 = exact bypass. |
| Stretch | Stretch Factor | 0.5–2 | Bin-index remap (source bin = k / factor); >1 up, <1 down. |
| Chaos | Chaos Rate | 1–512 fr | Frames between sample-and-hold redraws. |
| Chaos | Chaos Depth | 0–100 % | Modulation depth (multiplicative on amounts ⇒ never lifts a zero). |
| Output | Mix | 0–100 % | Dry/wet. |

## Presets

| Preset | Character |
|---|---|
| Gentle Haze | Light temporal blur + a touch of slow scramble — a soft, drifting veil. |
| Frequency Rain | Fast bin-shuffle + short bright spectral delays — a shimmering scatter of frequencies. |
| Time Smear | Heavy magnitude blur + gentle upward stretch — transients smear into a wash. |
| Chaos Engine | All four ops moving under a deep, slow sample-and-hold chaos macro. |
| Frozen Blur | Near-total blur with very long τ + high-feedback delay — a frozen spectral cloud. |
| Spectral Echoes | Long high-band spectral delays with strong feedback — cascading spectral echoes. |

## Done bar (mechanical, PRD §4 + build brief)

- Universal: no NaN/inf, peak ≤ 0 dBFS, non-silent; `mix = 0` nulls vs the **latency-delayed**
  dry < −80 dB.
- **(1) All op amounts 0 → the wet output nulls against the latency-delayed dry < −60 dB.**
  Stricter than the mix=0 null: it proves each op's amount-0 bypass is _exact_ (the wet path
  collapses to the STFT's identity reconstruction). Also verified under full-depth/full-rate
  chaos (multiplicative amount modulation can't lift a zero).
- **(2) Scramble > 0 → per-frame spectral correlation with the dry drops below 0.9** (measured
  by an independent analysis STFT, aligned by the 4-hop latency) **while total energy stays
  within ±3 dB** (the √overlap makeup restores the permutation's overlap-incoherence loss).
- Delay feedback at max (95 %, amount 100 %) stays finite and ≤ 0 dBFS over 30 s; every op fully
  engaged under full chaos stays finite and ≤ 0 dBFS.

## Try it in FL

Find more plugins → add **Qeynos SMUDGE** on a pad, vocal, drum bus, or full mix. Start with
**Gentle Haze** for a subtle drifting veil, then push **Scramble** with a low **Scramble Rate**
for glitchy bin-shuffle, or **Frequency Rain** for shimmering scattered delays. **Time Smear**
and **Frozen Blur** melt transients into evolving clouds (raise **Blur Time** for longer
smears). **Spectral Echoes** cascades band-delayed echoes — sweep **Delay Tilt** to move the
echo emphasis across the spectrum and **Delay Feedback** for longer tails. Turn up **Chaos
Depth** (with **Chaos Engine**) to let everything drift on its own. **Stretch Factor** away from
1.0 gives a spectral pitch-shift illusion. The host auto-delay-compensates the reported
2048-sample latency; **Mix** blends parallel.
