# IMPACT — kick drum synth (MIDI instrument)

A mono, last-note-priority kick synthesizer. A note-on drives an exponential **pitch
envelope** into a phase-continuous sine/triangle body oscillator, layered with a
band-passed noise **click**, one of three **embedded PCM transients**, and a **sub**
oscillator. The mix is saturated through the suite waveshaper bank, shaped by an
exponential **amp envelope**, and clipped. Retriggers are phase-continuous with a 1.5 ms
declick ramp, so a kick can be re-fired mid-decay without a click.

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

## Factory presets

808 Long · Techno Rumble Kick · Psy Snap · House Punch · Hardstyle Distorted.

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
