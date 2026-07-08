# Template — hello-gain reference

The Phase-0 reference plugin every Qeynos crate is copied from. Not a musical tool —
it exists to prove the pipeline (theme, one smoothed param, peak meter, CLAP+VST3).

## What It Is

A single smoothed gain stage with a peak meter — the minimal plugin the whole suite is
built on. Use it as a plain trim/utility, or as the starting point when reading how a
Qeynos plugin is wired.

## Signal Flow

```
in ─ gain (smoothed) ─ out
                └─ peak meter (GUI)
```

## Controls

- **Gain** — output level, −60…+24 dB, smoothed so moves never click. At 0 dB the output
  nulls bit-for-bit against the input; below 0 it attenuates, above 0 it boosts.

## Recipes

1. **Clean trim** — drop **Gain** a few dB on a too-hot channel; the smoother keeps
   automation moves click-free.
2. **Null check** — set **Gain** to 0 dB to confirm transparent passthrough (used as the
   template's own unity-gain null test).
3. **Reference read** — open it beside any shipped plugin to see the shared header, preset
   bar, and '?' manual button in their simplest form.
