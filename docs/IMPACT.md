# IMPACT — kick drum synth (MIDI instrument)

## What It Is

IMPACT is a mono, last-note-priority kick-drum synthesizer. A note-on drives an exponential
pitch envelope into a phase-continuous sine/triangle body oscillator, layered with a band-passed
noise click, one of three embedded PCM transients, and a sub oscillator, then saturated,
shaped by an amp envelope, and clipped. It covers the full range from deep 808 sub-kicks and
atmospheric-dnb booms to punchy house kicks and distorted hardstyle/techno stompers, and
retriggers are phase-continuous so a kick can be re-fired mid-decay without a click.

## Signal Flow

```
note-on ─ pitch env(f_start→f_end, curve) ─ sine/tri body osc ─┐
        ─ click: white noise → SVF band-pass + embedded PCM     ├─ mix ─ drive ─ amp env ─ clip ─ out
        ─ sub osc (f_end × ratio)                               ┘
```

This is a **MIDI instrument**: no audio input, stereo (or mono) output, `MidiConfig::Basic`
note input. It emits `ProcessStatus::KeepAlive` so its tail rings out after the note.

## How it works

- **Pitch envelope** — `f(t) = f_end + (f_start − f_end)·e^(−t/τ_p)`. The **Pitch Curve**
  control warps the shape by raising the normalized envelope to a power (0.5 = pure
  exponential; lower = a faster initial drop, higher = holds then drops). This is the
  characteristic kick "pitch drop" from the initial beater click down to the body tone.
- **Body oscillator** — a phase-accumulated **sine** morphing to **triangle** via the
  **Tone** control. Retriggers keep the running phase (no reset), so no waveform
  discontinuity.
- **Click layer** — white noise through an SVF **band-pass** (1–8 kHz) with its own fast
  (5–50 ms) decay, for the beater/attack transient.
- **Embedded PCM transients** — three short click samples (**Tick / Snap / Knock**)
  synthesized entirely offline in `build.rs` and baked into the binary as `const` arrays
  (no external files). Each is windowed to start and end at zero.
- **Sub oscillator** — a sine at `f_end × Sub Ratio`, for added low-end weight.
- **Drive** — the mixed signal is saturated through the suite waveshaper bank
  (**Tube / Tape / Fold / Hard**) *before* the amp envelope, so the saturation character is
  independent of the envelope stage.
- **Amp envelope** — an exponential decay with its own curve morph; the **Length** macro
  scales the amp decay and the pitch τ together for a single "shorter/longer" gesture.
- **Retrigger declick** — the amp envelope always ramps from its *current* value to the new
  velocity over 1.5 ms, and the click/transient layers are faded in over the same 1.5 ms, so
  a mid-decay retrigger introduces no sample-to-sample step beyond a normal onset's slew.
- **Key track** — when enabled, the incoming MIDI note sets `f_end` (A1 = 55 Hz); off, the
  kick is pitch-fixed and the note only triggers it.
- **Output** — soft (`tanh`-style) or hard clip, then a trim, with a safety ceiling below
  0 dBFS.

## Controls

- **Pitch Start** — the frequency the pitch envelope starts at; higher gives a more pronounced
  beater "click" at the top of the drop. 30…2000 Hz.
- **Pitch End** — the settled body frequency the pitch drops to (the perceived kick note).
  20…400 Hz.
- **Pitch Decay** — how fast the pitch falls from start to end (τ_p, scaled by **Length**);
  short = a snappy tick, long = a sliding 808 drop. 1…500 ms.
- **Pitch Curve** — morphs the shape of the pitch drop, from a fast initial plunge to a hold-then-
  drop. 0…1.
- **Length** — master macro that scales the amp decay and pitch τ together for one "shorter /
  longer" gesture. 0.1…4.0 ×.
- **Amp Decay** — the amp envelope time constant, i.e. how long the kick sustains (also scaled by
  **Length**). 20…3000 ms.
- **Amp Curve** — morphs the amp decay shape from a fast initial drop to a longer hold. 0…1.
- **Tone** — morphs the body oscillator from pure sine (0 %) toward triangle (100 %), adding
  upper-harmonic edge. 0…100 %.
- **Drive** — pre-envelope saturation drive through the waveshaper bank; the grit/weight control.
  0…100 %.
- **Drive Shape** — the waveshaper curve used by **Drive**: **Tube / Tape / Fold / Hard**.
- **Soft Clip** — output clipping mode: soft (tanh) for rounded loudness or hard for aggressive
  edge. on/off.
- **Click Level** — amount of the band-passed noise click layer (beater attack). 0…100 %.
- **Click Decay** — decay time of the noise click; longer smears the attack. 5…50 ms.
- **Click Freq** — center frequency of the click band-pass; higher = a tickier, more forward top.
  1000…8000 Hz.
- **Transient** — which embedded PCM transient layers on top: **Off / Tick / Snap / Knock**.
- **Transient Level** — amount of the selected PCM transient. 0…100 %.
- **Sub Level** — amount of the sub oscillator for extra low-end weight. 0…100 %.
- **Sub Ratio** — the sub oscillator frequency as a fraction of `f_end` (sub = f_end × ratio).
  0.25…1.0.
- **Key Track** — when on, the incoming MIDI note sets `f_end` (A1 = 55 Hz) so the kick is playable
  as a tuned tom/bass; off, the note only triggers a fixed-pitch kick. on/off.
- **Out** — output trim after the clip stage. −24…+6 dB.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Pitch Start | 30..2000 Hz | 220 | Pitch-envelope start frequency |
| Pitch End | 20..400 Hz | 55 | Pitch-envelope end / body frequency |
| Pitch Decay | 1..500 ms | 45 | Pitch τ_p (scaled by Length) |
| Pitch Curve | 0..1 | 0.5 | Morphs the pitch-drop shape |
| Length | 0.1..4.0 × | 1.0 | Macro: scales amp decay + pitch τ together |
| Amp Decay | 20..3000 ms | 400 | Amp envelope time constant (scaled by Length) |
| Amp Curve | 0..1 | 0.5 | Morphs the amp decay shape |
| Tone | 0..100 % | 0 | Body oscillator sine → triangle |
| Drive | 0..100 % | 0 | Pre-envelope waveshaper drive |
| Drive Shape | Tube/Tape/Fold/Hard | Tube | Waveshaper bank selection |
| Soft Clip | on/off | on | Output stage: soft (tanh) vs hard clip |
| Click Level | 0..100 % | 25 | Band-passed noise click amount |
| Click Decay | 5..50 ms | 12 | Click envelope decay |
| Click Freq | 1000..8000 Hz | 3500 | Click band-pass center |
| Transient | Off/Tick/Snap/Knock | Off | Embedded PCM transient variant |
| Transient Level | 0..100 % | 50 | PCM transient amount |
| Sub Level | 0..100 % | 0 | Sub oscillator amount |
| Sub Ratio | 0.25..1.0 | 0.5 | Sub frequency = `f_end × ratio` |
| Key Track | on/off | off | MIDI note sets `f_end` (A1 = 55 Hz) |
| Out | −24..+6 dB | 0 | Output trim |

## Recipes

1. **Atmospheric-DnB 808 Sub** *(start: 808 Long)* — **Pitch Start** ≈ 160 Hz, **Pitch End** 45 Hz,
   **Pitch Decay** ~60 ms, **Length** ~2.0 and **Amp Decay** long (~1500 ms) for a sliding sub that
   sustains under a break. Add **Sub Level** ~40 % (**Sub Ratio** 0.5), keep **Drive** low, enable
   **Key Track** so you can play the sub-bass melody from the keyboard.
2. **Hard-Techno Rumble Kick** *(start: Techno Rumble Kick)* — **Pitch Start** ~300 Hz, **Pitch End**
   ~50 Hz, **Pitch Decay** short, **Amp Decay** medium, then push **Drive** ~60 % with **Drive Shape**
   = Hard and **Soft Clip** off for a saturated stomp. Pair with UNDERTOW on the same track for the
   rumble tail.
3. **Punchy House Kick** *(start: House Punch)* — **Pitch Start** ~240 Hz, **Pitch End** ~55 Hz,
   **Pitch Decay** ~25 ms, **Click Level** ~40 % with **Click Freq** ~3.5 kHz and **Transient** = Tick
   at **Transient Level** ~50 % for a forward beater attack. Short **Length** (~0.7) keeps it tight.
4. **Distorted Hardstyle Stomp** *(start: Hardstyle Distorted)* — **Tone** ~40 % for triangle edge,
   **Drive** ~80 % with **Drive Shape** = Fold, **Soft Clip** off, **Pitch End** ~70 Hz. Use
   **Transient** = Knock at high **Transient Level** and pull **Out** back a couple dB to leave
   headroom for the distortion.

## Verification (offline done-bar)

- **Pitch track** — an STFT-measured f0 track (streaming `suite_core::stft` + quadratic peak
  interpolation) starts within 10 % of `f_start` and ends within 5 % of `f_end`.
- **Retrigger declick** — a mid-decay retrigger's worst sample-to-sample step stays within
  the declick bound of an otherwise identical no-retrigger render (a phase-reset or
  envelope-jump would step by a large fraction of full scale; the phase-continuous, ramped
  retrigger does not).

IMPACT's own kick math is also exposed suite-wide as `suite_core::testsig::synth_kick`
(with a `KickSpec`) for later plugins' tests (e.g. UNDERTOW's kick-duck).

## Testing in FL

1. Options → Manage plugins → "Find more plugins", then add **Qeynos IMPACT** to a channel.
2. Load a preset (e.g. **808 Long** or **House Punch**) and play notes; each note fires a
   kick. Enable **Key Track** to tune `f_end` from the keyboard.
3. Fire rapid repeated notes to hear the phase-continuous, declicked retrigger.

Offline audition renders (one 1.5 s note per preset) are in `renders/IMPACT/`.

## Factory presets

808 Long · Techno Rumble Kick · Psy Snap · House Punch · Hardstyle Distorted.
