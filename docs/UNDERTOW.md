# UNDERTOW — kick-to-rumble generator

## What It Is

UNDERTOW is a rumble **generator** that sits **ON the kick track**. It passes the dry kick straight
through and, underneath it, builds a kick-derived, **kick-ducked sub-bass rumble** — the long,
breathing low-end tail that defines hard / melodic / hypnotic techno and atmospheric-dnb. The
rumble is mono where it counts, tunable to the key, and it *breathes around* the kick instead of
fighting it.

## Signal Flow

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

## Controls

- **Strip** — how hard the kick's attack/click is stripped before rumble generation, so the rumble
  is built from the body, not the transient. 0–100 %.
- **Drive** — saturation drive into the 2× oversampled waveshaper that thickens the rumble source.
  0–100 %.
- **Size** — scales the FDN delay lengths: small = tight and immediate, large = cavernous and
  diffuse. 0–100 %.
- **Decay** — FDN reverb time (RT60), i.e. the rumble tail length. 0.2–3.0 s.
- **LP Freq** — sub low-pass cutoff; the ceiling of the rumble's frequency range. 90–250 Hz.
- **LP Res** — resonance (Q) of that low-pass; higher adds a bump at the cutoff. 0.5–8.
- **Tune Note** — key-lock note (C0–B2, A1 = 55 Hz) for the resonant peak, so the rumble hums in
  key.
- **Tune Amount** — strength of the resonant bell at the tuned note; 0 % is flat, 100 % rings the
  rumble hard at that pitch. 0–100 %.
- **Duck Depth** — how much the rumble ducks at each kick onset, keyed by the dry kick envelope
  (→ 0–24 dB). This is what makes the rumble breathe. 0–100 %.
- **Duck Release** — how fast the rumble swells back after each kick. 80–300 ms.
- **Rumble** — the rumble output level, summed with the dry kick; the bottom of the range mutes it
  (and the output then nulls against dry). −60…+12 dB.
- **Width** — stereo spread of the rumble content **above 150 Hz** only; the sub stays mono.
  0–100 %.
- **Dry** — the dry-kick level (default 0 dB unity, since the plugin sits on the kick track).
  −60…+6 dB.
- **Out Trim** — final output trim on the summed dry + rumble. −24…+24 dB.

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

## Recipes

1. **Warehouse Rumble Bed** *(start: Warehouse Bed)* — **Strip** ~50 %, **Drive** ~30 %, **Size**
   ~60 %, **Decay** ~1.2 s, **LP Freq** ~140 Hz. Set **Duck Depth** ~55 % with **Duck Release**
   ~160 ms so the rumble ducks hard on each kick and swells back for a rolling hard-techno floor.
   **Rumble** ~−3 dB, **Dry** 0 dB.
2. **Rolling Melodic-Techno Hum** *(start: Hypnotic Wash Low)* — dial **Tune Note** to your track's
   key and push **Tune Amount** ~60 % so the rumble hums the root. **Size** ~65 %, **Decay** ~1.9 s,
   **LP Res** ~2.0 for a little bump. Keep **Duck Depth** ~50 % so it breathes; **Width** ~30 % adds
   air above 150 Hz while the sub stays mono.
3. **Atmospheric-DnB Sub Roller** *(start: Rolling Rumble)* — longer **Decay** (~2.0 s), **Size**
   ~70 %, **Strip** ~55 % to keep only the body, **LP Freq** ~150 Hz. Lower **Duck Depth** (~35 %)
   for a more continuous bed under a half-time break, **Rumble** ~−4 dB.
4. **Distorted Drone Bed** *(start: Distorted Drone Bed)* — crank **Drive** ~85 % for a saturated,
   harmonically-rich rumble, **Decay** ~2.1 s, **Duck Depth** ~40 %. Watch **Out Trim** (pull back
   ~2 dB) so the driven rumble plus dry kick stays under the ceiling.

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
