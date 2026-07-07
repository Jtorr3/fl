# GRIT — sidechained distortion

A saturation stage whose character is shaped by a sidechain signal. Feed the main
input the sound to distort and route a sidechain (kick, drum bus, vocal, …) to GRIT's
aux input; the sidechain's envelope in a focus band drives the distortion.

```
main in ─ trim ─ pre-filter(SVF HP/LP) ─┐
                                        ├─ DIST CORE ─ post-filter ─ auto-gain ─ mix ─ out
sidechain in ─ SC focus BP ─ env follower┘        ▲
                                                  └ mode selects how SC drives the core
```

Nonlinear stages run at **4x oversampling** (polyphase halfband, `suite_core::dsp::Oversampler4x`).
Output is hard-ceilinged at ±0.999 (≤ 0 dBFS) as a safety guard.

## Modes

- **A · Env-Drive** — the sidechain envelope raises the drive amount:
  `drive_dB(t) = drive + depth·36dB · env(t)^curve`. More sidechain → more harmonics.
- **B · Waveshape** — the sidechain envelope injects a dynamic bias into the
  waveshaper: `y = shape(x·drive + depth·2·env(t))`. The bias shifts the operating
  point, pumping even-harmonic character with the sidechain. A DC blocker follows.
- **C · Spectral** — *deferred* (see `DEFERRED.md`); not selectable in this build.

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| Mode | A · Env-Drive / B · Waveshape | A | How the sidechain drives the core |
| Shape | Tube / Tape / Fold / Hard | Tube | Waveshaper from the suite bank |
| Trim | −24..+24 dB | 0 | Input trim (wet path only; dry stays pristine) |
| Drive | 0..48 dB | 12 | Base drive into the shaper |
| Depth | 0..100 % | 50 | Sidechain modulation amount |
| Curve | 0.25..4.0 | 1.0 | Envelope shaping exponent (Mode A) |
| Attack | 0.1..200 ms | 5 | Sidechain envelope attack |
| Release | 5..2000 ms | 120 | Sidechain envelope release |
| SC Focus | 20..20000 Hz | 100 | Sidechain focus-band center |
| SC Width | 0.2..4.0 oct | 1.5 | Focus-band bandwidth (→ bandpass Q) |
| SC Listen | on/off | off | Monitor the sidechain focus band |
| Pre HP | 20..2000 Hz | 20 | Pre-distortion high-pass |
| Pre LP | 200..20000 Hz | 20000 | Pre-distortion low-pass |
| Post HP | 20..2000 Hz | 20 | Post-distortion high-pass |
| Post LP | 200..20000 Hz | 20000 | Post-distortion low-pass |
| Auto-Gain | on/off | on | Match post-RMS to pre-RMS over 300 ms (±12 dB clamp) |
| Mix | 0..100 % | 100 | Dry/wet. At 0 %, output nulls against dry. |
| Out | −24..+24 dB | 0 | Output trim |

## Factory presets

Kick Bass Grit · Vocal Crush · Pad Ring-Fold · Drum Bus Pump-Drive · Techno Rumble Driver.

## Testing in FL

1. Options → Manage plugins → "Find more plugins", then add **Qeynos GRIT** to a
   channel/mixer insert.
2. Route a kick (or drum bus) to GRIT's **sidechain** input (FL: use the plugin
   wrapper's sidechain routing on the mixer — send the kick track to the GRIT track
   and pick it as the sidechain source).
3. Start with **Kick Bass Grit**; the distortion should pump with the kick. Toggle
   **SC Listen** to hear the focus band the envelope is tracking.

Offline audition renders (1 kHz sine main + pulsed sidechain, one per preset) are in
`renders/GRIT/`.
