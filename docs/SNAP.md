# SNAP — snare / clap generator

## What It Is

A MIDI **instrument** that synthesizes snares and claps from scratch. Built on IMPACT's voice
architecture (mono-ish, last-note priority, phase-continuous declicked retrigger, a length
macro that scales every envelope). A continuous **MODE** knob crossfades three engines — a
tonal snare **body**, a noise-formant **rattle** (snare wires), and a humanized **clap** engine —
from *Snare* through *Hybrid* to *Clap*, so it covers acoustic-ish snares, drum-machine claps,
and everything between.

## Signal Flow

```
note-on ─┬ BODY   : sine/tri @ Tune (140–260 Hz) + fast pitch env (shell knock)  ─┐  (snare
         ├ RATTLE : white noise → 3 parallel BP formants ~800/1.5k/3k + own env   ─┤   engine)
         └ CLAP   : Taps (3–5) humanized noise slaps over Spread (8–30 ms) + 1     ─┘
                    longer tail → BP/LP Tone shaping                              (clap engine)
   → MODE crossfade (Snare ↔ Hybrid ↔ Clap)
   → transient CLICK layer (band-passed noise, level+speed scaled by Snap)
   → Drive (suite waveshaper bank, 2× oversampled)
   → master amp env (Decay = length macro over all envelopes)
   → Width (decorrelated per-channel noise, mono-compatible)
   → soft clip → stereo out
```

## Engines

- **Body** — a phase-continuous sine/tri oscillator at `Tune`, with a fast exponential **pitch**
  envelope from `Tune × 1.9` down to `Tune` (the drum-shell "knock"). Its amplitude is governed
  by the master amp envelope only, so a retrigger is phase-continuous and never steps.
- **Rattle** — white noise through three parallel band-pass formants (~800 Hz / 1.5 kHz / 3 kHz,
  Cytomic SVF) with its own faster decay envelope: the snare wires. `Snap` speeds this envelope up.
- **Clap engine** — at note-on a schedule of `Taps` short noise slaps is laid down, evenly spaced
  across `Spread` ms, each with per-burst **humanized** pre-delay jitter (a per-note-deterministic
  RNG — so a given seed is reproducible), plus **one longer tail burst** just past the spread
  window. All slaps run through a `Tone`-centered band-pass → low-pass tone shaping. With
  `Humanize = 0` the slaps are exactly evenly spaced.
- **Transient click** — a short high-band noise click sits on top of every mode (IMPACT's click
  pattern); `Snap` scales its level.

## Mode blend & width

`Mode` is an equal-power crossfade: 0 = the snare engine (body + rattle), 1 = the clap engine,
0.5 = both (Hybrid). `Body/Noise` balances body against rattle inside the snare engine.

`Width` decorrelates the **noise** layers per channel — each channel is fed `shared + k·(its own
independent noise)`, normalised — while the tonal body stays perfectly mono. At `Width = 0` L and
R are identical (mono); at maximum the noise correlation floors at ≈ 0.61, so the output stays
**mono-compatible** (L/R correlation > 0.5) at any setting. The low-frequency body never
decorrelates.

## Retrigger / declick (IMPACT's recipe)

The master amp envelope always ramps from its **current** value to the new velocity over 1.5 ms,
so a mid-decay retrigger never steps the master gain. The fast noise layers (rattle / clap /
click), which are decayed by mid-note, are faded in over the same 1.5 ms trigger ramp. The body
oscillator is phase-continuous. Net result: click-free retriggers at any point in the decay.

## Signal chain out

Body + rattle + clap + click are summed per channel, driven through the suite waveshaper bank
(`TubeTanh`) at **2× oversampling** (anti-aliased), multiplied by the master amp envelope
(exp decay = `Decay`, which also length-scales every sub-envelope), and soft-clipped. Stereo out,
no audio in, `MidiConfig::Basic`. Key-track (off by default) transposes `Tune` from the MIDI note
(reference note A2 = MIDI 45 reproduces the knob).

## Parameters

| Param | Range | Meaning |
|---|---|---|
| Mode | Snare … Hybrid … Clap | Continuous crossfade of the snare and clap engines |
| Tune | 100–400 Hz | Body fundamental (snare shell pitch) |
| Body/Noise | 0–100 % | Balance of tonal body vs noise rattle in the snare engine |
| Snap | 0–100 % | Transient click level + rattle-envelope speed |
| Decay | 40–1200 ms | Master amp decay; a length macro scaling **all** envelopes (sets the tail) |
| Taps | 3–5 | Number of clap slaps (a longer tail burst is always added) |
| Spread | 8–30 ms | Total clap spread window |
| Humanize | 0–100 % | Per-slap pre-delay jitter (0 = evenly spaced, deterministic) |
| Tone | 0–100 % | Clap/noise band-pass center (≈ 500 Hz … 5 kHz, log) |
| Drive | 0–100 % | Saturation drive into the 2× oversampled waveshaper |
| Width | 0–100 % | Decorrelated-noise stereo width (mono-compatible, > 0.5 correlation) |
| Level | −24…+6 dB | Output trim |
| Key Track | on/off | Transpose Tune from the MIDI note (default off) |

## Controls

- **Mode** — continuous crossfade of the two engines, from Snare (0 %) through Hybrid (50 %) to
  Clap (100 %); the single macro that turns the box from a snare into a clap.
- **Tune** — body/shell fundamental pitch, 100–400 Hz. Low = deep sub-snare knock, high = tight
  metallic shell.
- **Body/Noise** — balance of tonal body vs noise rattle inside the snare engine, 0–100 %.
- **Snap** — transient click level plus rattle-envelope speed, 0–100 %. Higher = sharper attack.
- **Decay** — master length macro that scales every envelope, 40–1200 ms. Sets the tail.
- **Taps** — number of clap slaps (a longer tail burst is always added on top), 3–5.
- **Spread** — total clap spread window the slaps are laid across, 8–30 ms.
- **Humanize** — per-slap pre-delay jitter, 0–100 % (0 = perfectly even, deterministic).
- **Tone** — clap/noise band-pass centre (~500 Hz … 5 kHz, log), 0–100 %.
- **Drive** — saturation into the 2× oversampled waveshaper, 0–100 %.
- **Width** — decorrelated-noise stereo width, 0–100 % (mono-compatible; body stays mono).
- **Level** — output trim, −24 … +6 dB.
- **Key Track** — transpose Tune from the incoming MIDI note (on/off, default off).

## Recipes

1. **Concrete Techno Backbeat** — start from *Concrete Snap*: Mode 20 %, Tune 200 Hz,
   Body/Noise 40 %, Snap 70 %, Decay 130 ms, Drive 30 %, Width 25 %, Level −1 dB. A tight, dry,
   dark-techno snare that sits on the 2 and 4 without washing the mix. Nudge Drive to 45 % for a
   *Blown Rimshot*-style mean edge.
2. **DnB Break Crack** — from *DnB Crack* / *Break Rattle*: Mode 45 %, Tune 220 Hz,
   Body/Noise 60 %, Snap 80 %, Decay 180 ms, Taps 4, Spread 16 ms, Humanize 35 %, Tone 75 %,
   Drive 45 %, Width 45 %. An aggressive breakbeat snare with rattle bite; raise Humanize to
   45 % and Spread to 18 ms for the looser *Break Rattle* atmosphere.
3. **Warehouse Clap Layer** — from *Warehouse Clap*: Mode 90 %, Tune 150 Hz, Body/Noise 90 %,
   Snap 55 %, Decay 420 ms, Taps 5, Spread 30 ms, Humanize 60 %, Width 75 %, Level −2 dB. A huge
   reverberant techno clap to stack over a four-on-the-floor; drop Tone to 40 % for *Clap Layer
   Dark*.
4. **Snap Weld Click** — from *Snap Click Layer*: Mode 35 %, Snap 100 %, Decay 100 ms,
   Spread 10 ms, Width 25 %, Level −3 dB. A max-snap transient click to weld under any acoustic
   snare or kick for attack without adding body.

## Presets

Rimshot Knock · Wet Techno Clap · DnB Crack · Gunshot Layer · 90s Machine Clap · Airy Top Snare.

## Done-bar (offline, mechanical — PRD §4 + build brief)

- **Universal:** no NaN/inf; peak ≤ 0 dBFS; non-silent (per channel, every preset render).
- **Clap onsets:** in **Clap mode** (blend = 1) with `Humanize = 0`, the render's onset count
  equals **`Taps + 1`** (the slaps + the tail) within the spread window. Onsets are counted by an
  envelope-peak method (à la SWARM): a 0.3 ms-attack / 2 ms-release peak follower → a hysteresis
  level gate (set 0.40·peak, reset 0.22·peak) with a 4 ms refractory; analysed over the spread +
  15 ms guard window. Asserted at Taps ∈ {3, 4} (spacing 10 / 7.5 ms); Taps = 5 at 30 ms spread is
  6 ms spacing, where slaps intentionally overlap into fewer resolvable onsets.
- **Tone → centroid:** in Clap mode the accumulated-magnitude **spectral centroid** rises
  monotonically across `Tone` = 0.1 / 0.5 / 0.9.
- **Retrigger click-free:** a mid-decay retrigger's worst sample-to-sample step stays within the
  declick bound of a no-retrigger baseline (IMPACT's exact test recipe).
- **Decay → RT:** the measured tail length (−20 dB time) scales with `Decay` across two settings.
- **Width mono-compatible:** at `Width = 1` the L/R Pearson correlation stays > 0.5.

Renders (auditionable stereo artifacts) are written to `renders/SNAP/` — one per factory preset.

## Reuse note

SNAP keeps all its DSP **local to the crate** (body osc, noise formant bank, clap scheduler,
decorrelated-noise width) — nothing is added to `suite-core`, so no suite-wide rebuild/revalidate
is required. It reuses `suite_core::dsp` (`Svf`, `Shaper`, `Oversampler2x`, `tape_soft`,
`EnvFollower`, `ScopedFtz`), `suite_core::testsig::Rng`, and `suite_core::stft` (in tests).
