# UNDERTOW — kick-to-rumble generator

A rumble **generator** that sits **ON the kick track**. It passes the dry kick straight through
and, underneath it, builds a kick-derived, **kick-ducked sub-bass rumble** — the long, breathing
low-end tail that defines hard / melodic / hypnotic techno. Taste-tailored for that genre's
low-end: the rumble is mono where it counts, tunable to the key, and it *breathes around* the
kick instead of fighting it.

```
in(kick) ─┬────────────────────────────────────────────────────────────── dry ── + ── out
          └ transient strip (env-gated: strip the click, keep the body)
            → saturation (suite waveshaper bank, 2× oversampled)
            → Fdn8 small/dark  (short delays 8–25 ms × Size, dark damping, RT60 = Decay;
                                 sum stereo pair → mono)
            → LP 90–250 Hz (SVF, resonance)
            → resonant TUNE peak (key-lockable bell at note C0..B2, Amount)
            → ducker (keyed by the DRY kick envelope: att ~1 ms, rel 80–300 ms, Depth)
            → rumble gain → (+ dry)
```

The whole rumble path is **mono below ~150 Hz** (techno low-end stays mono); `Width` only spreads
the FDN's stereo side-content **above** 150 Hz. Zero reported latency — the wet path is a reverb
(a time-smearing effect), so the alignment convention is *"rumble muted → output == dry"* (an
exact null), not a lag-0 coherent peak.

## Signal chain

1. **Transient strip.** A fast and a slow peak envelope follow the kick; where the fast one leads
   (the attack/click), the signal is attenuated by up to `Strip`, so the rumble source is the
   kick's **body**, not its click. In the sustained body the two envelopes converge and the gate
   opens back to unity.
2. **Saturation.** The stripped body is driven through the suite waveshaper bank (`TubeTanh`) at
   **2× oversampling** (anti-aliased), thickening the harmonics that the FDN will smear.
3. **FDN (small / dark).** The reusable `suite_core::fdn::Fdn8` — an 8×8 Householder feedback
   delay network — configured *small and dark*: eight short delay lines (8–25 ms, geometrically
   spread and scaled by `Size`, nudged mutually-prime-ish), heavy damping tilt, dense input
   diffusion, and `RT60 = Decay`. The stereo pair is summed toward **mono** for the sub.
4. **Low-pass.** A resonant TPT state-variable low-pass (`LP Freq` 90–250 Hz, `LP Res`) keeps
   only the sub range.
5. **Resonant tune peak (key-lockable).** A high-Q bell at the fundamental of the chosen `Note`
   (C0..B2; **A1 = 55 Hz**). `Amount` rings the rumble at that pitch so it hums a note in key —
   the melodic-techno move. At full amount the rumble spectrum peaks within ±3 % of the note.
6. **Ducker.** A fast-attack (~1 ms) peak follower on the **dry** kick, normalised by a slow
   peak-tracker so it reaches full `Depth` at every onset regardless of kick level, then recovers
   over `Release` (80–300 ms). The rumble ducks by `Depth` dB at each kick and swells back
   between hits — it breathes *around* the kick.
7. **Output.** `Rumble` level, `Width` (> 150 Hz only), summed with the dry kick, `Out Trim`.

## Parameters

| Param | Range | Meaning |
|---|---|---|
| Strip | 0–100 % | How hard the kick's attack/click is stripped before rumble generation |
| Drive | 0–100 % | Saturation drive into the 2× oversampled waveshaper |
| Size | 0–100 % | Scales the FDN delay lengths (small = tight, large = cavernous) |
| Decay | 0.2–3.0 s | FDN reverb time (RT60) — the rumble tail length |
| LP Freq | 90–250 Hz | Sub low-pass cutoff |
| LP Res | 0.5–8 | Low-pass resonance (Q) |
| Tune Note | C0–B2 | Key-lock note for the resonant peak (A1 = 55 Hz) |
| Tune Amount | 0–100 % | Strength of the resonant bell at the tuned note |
| Duck Depth | 0–100 % | Ducking depth at each kick onset (→ 0–24 dB) |
| Duck Release | 80–300 ms | How fast the rumble recovers after each kick |
| Rumble | −60…+12 dB | Rumble output level (bottom = muted) |
| Width | 0–100 % | Stereo spread of rumble **above 150 Hz** (low-end stays mono) |
| Dry | −60…+6 dB | Dry-kick level (default 0 dB unity — the plugin sits on the kick) |
| Out Trim | −24…+24 dB | Final output trim |

## Presets

Warehouse Bed · Rolling Rumble · Tight Modern Techno · Cavern Floor · Hypnotic Wash Low ·
Distorted Drone Bed.

## Done-bar (offline, mechanical — PRD §4 + build brief)

- **Universal:** no NaN/inf; peak ≤ 0 dBFS; non-silent; *rumble muted → dry* nulls exactly.
- **Ducking:** feeding a 4-on-the-floor `testsig::synth_kick` pattern at 130 BPM and isolating the
  rumble (dry = 0), the rumble envelope **dips by ≥ the Duck Depth (dB)** at each kick onset
  (measured against the un-ducked bed) and **swells back up between hits**.
- **Tune:** with `Note = A1` and high `Amount`, the rumble spectrum peaks **within ±3 % of
  55 Hz** (16 k-point STFT, quadratic-interpolated).
- **Mono low-end:** with `Width` at maximum, the rumble's L/R correlation **below 150 Hz is
  ≥ 0.9** (the sub stays mono).
- **Bounded:** a 30 s render under a hot setting stays finite and bounded.

Renders (auditionable artifacts) are written to `renders/UNDERTOW/` — one full-mix render, one
rumble-only render, and one per factory preset.

## Reuse note

UNDERTOW's rumble tail is built on the suite's shared **`suite_core::fdn::Fdn8`** FDN core (first
introduced by MURMUR); the small/dark preset here — short delays, high damping, dense diffusion,
short RT60 — is the recipe SEANCE and CHAMBER can borrow for tight, dark spaces.
