# PATINA — analog lo-fi character

*Phase 3. A tape/vinyl character chain: wow & flutter, tape saturation, a head-bump low shelf,
azimuth HF skew, dropouts, and a keyed noise layer (hiss / hum / crackle) — all aged together
by a single **AGE** macro.*

PATINA runs a signal through the small imperfections of analog playback gear. The transport
wobbles (**wow & flutter**), the record head gently saturates and lifts the low end
(**Saturation** + **Head Bump**), the two channels drift out of time in the highs
(**Azimuth**), the medium randomly drops out (**Dropouts**), and a bed of **hiss / hum /
crackle** sits underneath — optionally *keyed* to the input so the noise only rises with the
signal. The **AGE** knob scales every one of these together on a curated curve, from a barely-
there patina to a fully destroyed tape.

## What It Is

PATINA is an analog degradation chain — the sound of tired tape and dusty vinyl in one box.
It wobbles the pitch (wow & flutter), saturates and lifts the low end like a record head, drops
out at random, and lays a bed of hiss / hum / crackle underneath, with a single **Age** macro
aging the whole thing on one curve. Neutral it is a bit-exact passthrough, so you can push it
from a barely-there glue to a fully rotted transmission and always dial back to clean.

## Signal flow

```
 in ─ wow/flutter (FracDelay ← 0.4 Hz wow + 8 Hz flutter + slow random walk, one-sided)
    ─ saturation (tape_soft, 2× oversampled; blended vs a clean 15-sample delay)
    ─ head-bump EQ  (low shelf: y = x + (g−1)·LP(x),  60–120 Hz)
    ─ azimuth       (right-channel HF through a 1st-order allpass, blended)
    ─ dropouts      (shared random gain dips, 8 ms-smoothed edges)
    ─ + noise       (hiss + hum(50/60 Hz + harmonics) + crackle,  × key envelope)
    ─ AGE macro     (adds to every section on a curated curve)
    ─ mix (vs latency-matched dry) ─ out
```

### Latency & the exact null

Every section is an **exact identity** when its amount is 0: the wow line reads a constant
integer base delay (a fractional-delay tap at zero fraction returns the stored sample), the
saturation blends the oversampled signal against a *clean* delay line (never the oversampler's
own filtered output) so drive 0 is a pure delay, the head-bump adds `(g−1)·LP` with `g−1 = 0`,
the azimuth adds `amount·(…)` with `amount = 0`, dropouts multiply by a gain primed to 1, and
the noise levels are 0. So with **AGE 0 and every section at 0** the wet path is a *bit-exact*
delay of the input by `LATENCY = 30` samples; the dry path is delayed by the same amount, so
`out = (1−mix)·dry + mix·wet` **nulls exactly** against the latency-matched dry for any Mix.
PATINA reports those 30 samples via `set_latency_samples`, so the host delay-compensates it.

Wow/flutter add delay *on top of* the base (one-sided modulation), so when wow is active the
wet mean-delay exceeds the reported latency; at partial **Mix** that dry/wet detune is an
intended lo-fi flange.

### Keyed noise

The hiss / hum / crackle bed is multiplied by a **key gain** = `(1−Key) + Key·env`, where `env`
is an RMS follower of the input (per the BANDAID handoff: RMS, not peak, so low-frequency
content doesn't ripple the gate). At **Key 0** the noise is a constant floor; at **Key 1** it is
fully gated by the input envelope, so the tape only hisses when the music plays. Hiss and
crackle are decorrelated per channel; hum is correlated (shared phase).

## Parameters

| Param | Range | Notes |
|---|---|---|
| Wow | 0–100 % | 0.4 Hz pitch wobble depth (also scales the slow random walk) |
| Wow Rate | 0.25–4.0× | trims the wow frequency around 0.4 Hz |
| Flutter | 0–100 % | fast (~8 Hz) wobble depth |
| Saturation | 0–100 % | tape soft-clip drive (2× oversampled); 0 = clean |
| Head Bump | 0–100 % | low-shelf boost (→ up to +9 dB) |
| Bump Freq | 60–120 Hz | shelf corner |
| Azimuth | 0–100 % | right-channel HF phase skew (mono-sum HF loss) |
| Dropout Rate | 0–100 % | how often the medium drops out |
| Dropout Depth | 0–100 % | how deep each dip cuts (edges 8 ms-smoothed) |
| Hiss | 0–100 % | filtered white-noise level |
| Hum | 0–100 % | mains hum level (fundamental + 3 harmonics) |
| Crackle | 0–100 % | sparse band-passed pops |
| Hum 60 Hz | toggle | 60 Hz (on) or 50 Hz (off) mains |
| Noise Key | 0–100 % | 0 = constant floor, 1 = noise gated by the input envelope |
| Age | 0–100 % | macro: adds wow/flutter/sat/dropout/noise together |
| Mix | 0–100 % | dry↔wet (0 = latency-matched dry passthrough) |
| Out | ±24 dB | output trim |

**MOD (NERVE):** the **WOW**, **AGE**, and **MIX** targets can listen to a NERVE bus stream.

## Controls

- **Wow** — depth of the slow ~0.4 Hz pitch wobble (also scales the slow random walk), 0–100 %.
- **Wow Rate** — trims the wow frequency around 0.4 Hz, 0.25–4.0×.
- **Flutter** — depth of the fast (~8 Hz) wobble, the jittery cousin of wow, 0–100 %.
- **Saturation** — tape soft-clip drive (2× oversampled); 0 % is clean, higher rounds transients
  and thickens, 0–100 %.
- **Head Bump** — low-shelf lift emulating the record-head resonance (up to ~+9 dB), 0–100 %.
- **Bump Freq** — corner frequency of the head-bump shelf, 60–120 Hz.
- **Azimuth** — right-channel HF phase skew (misaligned tape head); mono-sums to a gentle HF loss,
  0–100 %.
- **Dropout Rate** — how often the medium randomly drops out, 0–100 %.
- **Dropout Depth** — how deep each dropout dip cuts (edges are 8 ms-smoothed, click-free), 0–100 %.
- **Hiss** — level of the filtered white-noise tape hiss, 0–100 %.
- **Hum** — mains hum level (fundamental + 3 harmonics), 0–100 %.
- **Crackle** — level of sparse band-passed vinyl pops, 0–100 %.
- **Hum 60 Hz** — toggles the hum fundamental between 60 Hz (on) and 50 Hz (off) mains.
- **Noise Key** — 0 % holds a constant noise floor; 100 % gates the whole noise bed by the input
  envelope so the tape only hisses when the music plays, 0–100 %.
- **Age** — the master macro: scales wow/flutter, saturation, dropouts and noise together on a
  curated curve, from subtle patina to total decay, 0–100 %.
- **Mix** — dry↔wet blend; 0 % is the latency-matched dry passthrough, 0–100 %.
- **Out** — output trim, ±24 dB.

## Recipes

1. **Warehouse Dust (dark techno)** — load **Bedroom Dub Deck**, push **Age** to ~35 % and drop
   **Mix** to ~70 % across the drum bus. The head bump and wow thicken the low end while the keyed
   **Hiss** breathes with the kick — grime without mud.
2. **Sewer Break (atmospheric dnb)** — start from **Sewer Transmission**, keep **Dropout Rate**
   and **Depth** high and **Flutter** near 0.8 so the break lurches; set **Noise Key** low so the
   crackle bed hangs under the whole thing. A ruined-transmission drum loop.
3. **Ripped Vocal (vocal-rip)** — put PATINA on a bounced vocal with **Warped 78**: **Wow** ~0.5,
   **Wow Rate** slow, **Crackle** up, **Mix** ~80 %. The pitch drift and pops turn a clean take
   into a haunted, half-remembered sample. Back **Age** off toward 0 to recover the dry instantly.

## Presets

Worn Cassette · Dusty Vinyl · Old Console Hum · Broadcast Ghost · Gentle Glue Age ·
Destroyed Tape.

## Done-bars (PRD §4)

1. **Wow** — a 1 kHz sine's demodulated phase track peaks near **0.4 Hz**, and the modulation
   power scales with Wow depth.
2. **Keyed noise** — the output's high-band RMS (a band the input doesn't occupy) rises with the
   input envelope at **Key 1** and stays constant at **Key 0**.
3. **Dropouts** — at high rate/depth the windowed RMS dips well below the baseline, and the max
   sample-delta stays within the input's own (click-free edges).
4. **Null / AGE** — AGE 0 + all sections 0 nulls against the latency-matched dry below −120 dB;
   AGE monotonically raises a composite degradation metric (THD + noise floor + f0-mod depth).

Renders for every preset are written to `renders/PATINA/`.
