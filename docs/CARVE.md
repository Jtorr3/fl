# CARVE — spectral ducker (Trackspacer clone)

A **sidechain** carves its own frequencies out of the main signal. Instead of ducking the whole
broadband level (a classic sidechain compressor), CARVE measures **where** in the spectrum the
sidechain has energy and cuts **only the main's matching bands**, opening a frequency-matched
pocket so the two signals stop fighting for the same range.

Two short-time Fourier transforms (**2048** frame / **512** hop / Hann, `suite_core::stft` — the
EMBER/SMUDGE engine) run in lockstep per sample:

- a mono **sidechain** analysis STFT measures per-**⅓-octave-band** energy and, at each frame
  boundary, updates a shared per-band gain table;
- one **main** STFT per channel multiplies its bins by the (just-updated) per-band gains and
  resynthesises (iSTFT / overlap-add).

Because all STFTs share the 2048/512 geometry and advance together, the sidechain frame fires on
the same sample as the main frames — the SC callback writes the gain table first, the main
callbacks read it (SMUDGE's primary/secondary split-borrow pattern). Reports **2048-sample
latency**; the dry path is delay-matched so `mix = 0` nulls cleanly.

```
 sidechain (mono-sum) ── STFT 2048/512 ── per ⅓-oct band energy ─┐
                                                                 │  soft-knee(energy vs Threshold)
                                                                 │  × Amount × Max Depth × Tilt
                                                                 │  → Attack/Release smooth @ hop rate
                                                                 ▼
                                            per-band gain table  ├──────────────┐
                                                                                ▼
 main L/R ──┬── STFT 2048/512 ── bins ×= interpolated per-band gain ── iSTFT ── wet ─┐
            │                                                                (mix)   ▼
            └── delay(2048) ── dry ─────────────────────────────────────────────────► out = dry + mix·(wet−dry)
```

## How the reduction is computed

Per sidechain frame, for each of **30 ⅓-octave band groups** (20 Hz – 20 kHz):

1. **Band energy** — sum of the group's normalised bin magnitudes squared, in dB. Magnitude is
   scaled by `4/N` so a full-scale in-band tone reads ≈ 0 dB, making **Threshold** roughly a
   dBFS-referenced control.
2. **Soft-knee excess** — `over = softknee(energy_dB − Threshold, knee)`: zero below the knee,
   linear above it, quadratic across the knee width.
3. **Depth fraction** — `frac = clamp(over / span, 0, 1)`. **Sensitivity** sets both the knee
   width (12 dB gentle → 2 dB tight) and the excess **span** needed to reach full depth (30 dB
   gentle → 8 dB tight), so higher Sensitivity ducks harder from less sidechain energy.
4. **Depth** — `depth_dB = Max Depth × Amount × frac × tilt_weight(band)`.
5. **Attack / Release** — the per-band reduction envelope moves toward `depth_dB` with the
   **Attack** coefficient when the cut is deepening and **Release** when it lets go (one-pole at
   the **hop rate**, ≈ 93 frames/s @ 48 kHz).

The per-band linear gain `10^(−env_dB/20)` is then **interpolated across band edges** (log-freq
lerp between adjacent group centres) so the applied gain is smooth across bins — no spectral
steps. When no band has any reduction (silent sidechain) the whole per-bin multiply is **skipped**
→ the main path is the STFT's exact identity reconstruction (this is what makes the SC-silent
null hold).

### Tilt

**Tilt** biases the cut toward one end of the spectrum without changing the maximum depth: the
per-band weight stays in `[1 − |Tilt|, 1]`. **Tilt < 0** cuts the **lows** more (spares the
highs); **Tilt > 0** cuts the **highs** more (spares the lows); 0 is flat. Handy for a
kick-vs-bass duck (tilt down, so only the low pocket ducks) versus a de-masking air tuck (tilt
up).

### Listen modes

- **Off** — normal carved output (`out = dry + Mix·(wet − dry)`).
- **Sidechain** — passes the sidechain straight through, so you can audition exactly what is
  controlling the duck.
- **Delta (Δ)** — outputs the **carved residual**: what is being *removed* from the main. Because
  the per-bin gain `g` and its complement `1 − g` partition the spectrum, the normal output and
  the Δ output **sum back to the dry** — a fast, honest way to tune the threshold/sensitivity by
  ear (turn the carve up until the Δ has just the sidechain's shape in it).

## Parameters

| Param | Range | Notes |
|---|---|---|
| Amount | 0–100 % | Overall duck depth (scales every band's reduction). Smoothed. |
| Max Depth | 0–24 dB | Maximum reduction at full sidechain energy. |
| Threshold | −90 – 0 dB | Sidechain band level (≈ dBFS) at which ducking begins (soft-knee centre). |
| Sensitivity | 0–100 % | Knee width + excess span. 0 = gentle/wide, 1 = aggressive/narrow. |
| Tilt | −1 … +1 | Bias the cut toward lows (−) or highs (+). |
| Attack | 1–50 ms | Reduction-envelope attack (how fast the duck engages). |
| Release | 20–500 ms | Reduction-envelope release (how fast it lets go). |
| Listen | Off / Sidechain / Delta | Monitoring / output mode (see above). |
| Mix | 0–100 % | Dry/wet. `Mix = 0` passes the latency-matched dry through exactly. |
| Out | −24 … +24 dB | Output trim. Smoothed. |

The plugin exposes a **stereo (or mono) aux sidechain input**; route the ducking source (a kick,
a vocal, a music bed) into it. If nothing is connected the sidechain reads as silence and CARVE
is transparent. The GUI shows a live **per-band reduction meter** (14 bars) so you can see where
the carve is landing.

## Verification (done-bars)

All in `plugins/carve/src/tests.rs`, plus the universal render assertions:

1. **Reduction only in sidechain bands** — main = full-spectrum pink noise, sidechain =
   band-limited noise 500 Hz–2 kHz: in-band groups (700–1500 Hz) are cut ≥ `Max Depth − 3` dB;
   clearly out-of-band groups (< 150 Hz, > 6 kHz) stay within **±1 dB** of dry.
2. **SC-silent null** — silent sidechain, `Mix = 1`: the carved wet path nulls against the
   latency-delayed dry below **−60 dB** (the honest STFT round-trip bound — the WOLA identity is
   not bit-exact); with `Mix = 0` the delayed-dry null is below **−80 dB**.
3. **Δ + normal = dry** — the Δ-listen output and the normal output sum to the dry within **1 dB**
   of energy (the `g` / `1−g` partition).
4. **Attack / release tracking** — a gated (on→off) band-limited sidechain drives the reduction
   envelope; the measured 10 %→63 % (attack) and 90 %→37 % (release) intervals match the settings
   within **±50 %**.

Plus a tilt-bias test (Tilt < 0 cuts lows more, Tilt > 0 cuts highs more) and an extremes fuzz
(max depth / min threshold / extreme times stay finite and ≤ 0 dBFS).

### STFT null quality

The STFT is a windowed WOLA round-trip, so the identity reconstruction is accurate to about
**−60 dB**, not bit-exact — hence the SC-silent wet-path null uses a **−60 dB** bound. The
`Mix = 0` path taps the *delayed dry directly* (not through the STFT), so it nulls below **−80 dB**
regardless. When no band is ducking, the per-bin gain multiply is skipped entirely, so the wet
path is exactly the STFT's own identity output — CARVE adds no coloration of its own beyond the
transform.

## Presets

Vocal Space · Kick Vs Bass · Master Bus Tuck · Aggressive Carve · Gentle Glue Duck · Delta
Inspector. Renders in `renders/CARVE/` (pink main × band-limited sidechain pulse train).

## Using it in FL Studio

Put **Qeynos CARVE** on the track you want carved (a pad, a music bed, a bass). Route the
ducking source into CARVE's **sidechain** input (in FL: send the kick/vocal to the sidechain via
the mixer, or use the plugin's sidechain routing). Load **Kick Vs Bass** on a bass to punch a
hole for the kick, **Vocal Space** on a music bed under a vocal, **Master Bus Tuck** / **Gentle
Glue Duck** for subtle glue. Set **Threshold** so the duck only triggers on the loud parts of the
sidechain, **Sensitivity** and **Max Depth** for how hard, **Tilt** to aim the cut, and
**Attack/Release** for the feel. Flip **Listen = Delta** to hear exactly what's being removed
while you dial it in; **Mix = 0** is an exact bypass.
