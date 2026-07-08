# BANDAID — multiband transient designer

*Phase 3. A 3-band (LR4) transient shaper: independently boost or cut the attack and the
sustain of the lows, mids, and highs.*

BANDAID splits the incoming signal into three bands with minimum-phase Linkwitz-Riley
crossovers and, in each band, measures a **transient signal** = a fast (1 ms) envelope minus a
slow (50 ms) envelope. Where the fast envelope overshoots the slow one the sound is *attacking*
(an onset); everywhere else it is *sustaining* (the body and tail). Two per-band controls —
**Attack** and **Sustain**, each ±12 dB — set how much that band's onsets and body are lifted
or tightened. Use it to make a kick punch harder, dry up a boomy room tail, take the *tss* off
hats, add snap to a drum bus, or let a pad bloom.

## Signal flow

```
        ┌─ LP(xLow) ─────────────────────────────── low  ─┐
 in ──┬─┤                                                  │
      │ └─ HP(xLow) ─┬─ LP(xHigh) ─────────────────  mid  ─┤   per band:
      │              └─ HP(xHigh) ─────────────────  high ─┤     detector = fastEnv(1ms) − slowEnv(50ms)
      │                                                     │     att_w = rising overshoot, sus_w = 1 − att_w
      │                                                     │     g_b = dB→lin(attack·att_w + sustain·sus_w), 5 ms-smoothed
      │                                                     │
 dry ─┴────────────────────────────────────────────────────┴─► out = x + mix · Σ_b (g_b − 1)·band_b
                                                                 (solo: out = Σ_soloed g_b·band_b)
                                                        · Out trim · ±0.999 ceiling
```

### Why it nulls at neutral

An LR4 split-then-sum is *allpass-flat* — unity magnitude but a 360° phase lag — so a naive
`Σ g_b·band_b` recombination would **not** cancel against the dry input. BANDAID instead adds
only the per-band **difference** its shaping makes: `out = x + Σ (g_b − 1)·band_b`. When a
band's gain is 0 dB, `g_b = 1`, so its term is **exactly zero** — with all attack/sustain
controls at 0 the output is the input bit-for-bit. This is the "neutral nulls to the input"
guarantee (verified to < −80 dB against a pink+chirp broadband signal), and it holds no matter
how the crossovers are set. **Mix = 0** likewise returns the dry input exactly. Zero latency.

### The transient detector

Per band, two peak envelope followers run on the band signal: a **fast** one (1 ms attack, so
it catches onsets) and a **slow** one (a lagging 40 ms attack). Their difference, normalised,
is the **attack weight** `att_w` (high on a rising onset, near zero on steady or decaying
material); the **sustain weight** is `1 − att_w` (the SPL-style split, so a steady tone is
fully "sustain"). The per-band gain is `attack_dB·att_w + sustain_dB·sus_w`, converted to
linear and smoothed over ~5 ms so it never zippers or clicks. The **Detector** knob scales both
envelope times together (faster ↔ slower response).

## Parameters

| Param | Range | Notes |
|---|---|---|
| Xover Low | 20 – 800 Hz | Low↔mid crossover (LR4, 24 dB/oct). |
| Xover High | 800 – 8000 Hz | Mid↔high crossover. Sanitised so it always stays above Xover Low. |
| Low / Mid / High **Attack** | ±12 dB | Gain applied to that band's **onsets** (transient attack region). |
| Low / Mid / High **Sustain** | ±12 dB | Gain applied to that band's **body / tail** (sustain region). |
| Low / Mid / High **Solo** | on/off | Audition just that band's **shaped** output (bypasses the dry). Multiple solos sum. |
| Detector | 0.5 – 2.0 | Scales the fast/slow envelope times (`<1` faster, `>1` slower). |
| Mix | 0 – 100 % | Dry↔shaped blend. `0 %` = exact bypass. |
| Out | ±24 dB | Output trim. |

All parameters are smoothed / block-rate and have typed value entry (click a value to type it).
**Low Attack**, **High Attack**, and **Mix** are exposed as NERVE **MOD** targets.

## Presets

Punchier Kick · Tighter Room · Soften Hats · Drum Bus Snap · Pad Bloom · Full Squash-Reverse.
(Per-band solo is live audition state and is never stored in a preset.)

## Done-bar tests (PRD §4)

Verified offline in `cargo test -p bandaid` (renders written to `renders/BANDAID/`):

1. **Neutral nulls** — all attack/sustain 0, solos off → residual vs input < −80 dB (parallel-
   delta / allpass-flat proof); **Mix = 0** nulls even with extreme shaping.
2. **Targeted attack** — on a synthetic kick + tonal-pad mix, **low-band Attack +12 dB** raises
   the LOW band's onset-to-sustain ratio while the mid and high band ratios stay within ±1 dB.
3. **Targeted sustain** — **mid-band Sustain −12 dB** lowers the MID band's inter-onset RMS only
   (low/high within ±1 dB).
4. **Monotonic sweep** — an attack sweep (−12 / 0 / +12 dB) yields a monotonically increasing
   LOW-band onset-to-sustain ratio.

Plus: solo isolates a single band, all six presets pass the universal assertions (finite,
≤ 0 dBFS, non-silent), and a degenerate/maxed fuzz setting stays finite and bounded.

## Using it in FL Studio

Put **Qeynos BANDAID** on a drum, bus, or full mix. Load **Punchier Kick** and nudge **Low
Attack** up for more thump, or **Low Sustain** down for a tighter kick. **Tighter Room** and
**Drum Bus Snap** work on a drum bus; **Soften Hats** tames harsh hi-hats (High Attack down);
**Pad Bloom** lifts the sustain of pads/keys. Set the two **Xover** knobs so each band covers
the range you want to shape, tap **Solo** on a band to hear exactly what it contains, and pull
**Mix** back to blend the effect under the dry. Neutral settings are a transparent bypass.
