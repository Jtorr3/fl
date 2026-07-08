# OVERSEER — mastering system (one library, two plugins)

One bundle (`overseer.clap` / `overseer.vst3`) exports **two** plugins:

- **Qeynos OVERSEER Node** — a channel strip you put on individual tracks.
- **Qeynos OVERSEER Master** — the mastering bus you put on the master.

Every Node registers a slot on a same-DLL shared bus. The Master's GUI shows a live grid
of all Node instances (label, peak/RMS/LUFS-M meters, key params) and can **override**
each Node's Threshold / Ratio / Drive / Width / Trim remotely. Overridden params show an
`OVR` badge (and a `MASTER OVERRIDE` banner) on the Node GUI; touching the param locally
(GUI or host automation) steals control back (write-wins timestamps, block granularity).

> **Bridging caveat:** the link relies on both plugins living in the same process. FL
> loads same-bitness plugins in-process by default; ticking "Make bridged" on either
> instance severs the link (the plugins still process audio normally).

## What It Is

OVERSEER is a two-part channel-strip-and-mastering system that understands your session. Drop a
**Node** on every track — each is a full EQ / compressor / saturation / width strip that also
*listens* to its input and auto-classifies the instrument (kick, bass, vocal, pad…). Drop the
**Master** on the mix bus — it reads all the Nodes, infers the session's theme (dark-techno,
dnb-breaks, ambient…), and can remotely override any Node while assisting the master EQ, multiband
comp and limiter toward that theme.

## Signal Flow

```
 NODE   in → meter → 4-band EQ → FF comp (RMS, soft knee) → tanh drive → M/S width → trim → mix → out
                     └─ feature extractor ─► classify ─► type + features ─┐
                                                                          ▼ same-DLL bus
 MASTER in → 4-band EQ → 3-band MB comp (LR4) → lookahead limiter (2 ms) → LUFS → mix → out
              ▲ theme assist ◄─ infer theme ◄── reads every live Node ────┘
              └─ can push overrides (Threshold/Ratio/Drive/Width/Trim) back to each Node
```

(Detailed per-editor chains follow.)

## Node — signal flow

```
in → meter → 4-band EQ (LS · bell · bell · HS) → FF compressor (RMS, soft knee)
   → tanh saturation → M/S width → trim → meter → mix → out
```

| Param | Range | Notes |
|---|---|---|
| Label | text | instance name shown on the Master grid (persisted, not automatable) |
| Low/High Freq | 20 Hz–20 kHz | shelf corners |
| Low/High Gain | ±24 dB | shelves |
| Bell 1/2 Freq, Gain, Q | 20 Hz–20 kHz, ±24 dB, 0.1–10 | parametric bells |
| Threshold | −60..0 dB | compressor threshold (overridable) |
| Ratio | 1–20:1 | compressor ratio (overridable) |
| Knee | 0–24 dB | soft knee width |
| Attack / Release | 0.1–100 ms / 10–1000 ms | detector ballistics |
| Makeup | ±24 dB | post-comp gain |
| Drive | 0–24 dB | tanh saturation amount (overridable) |
| Width | 0–2 | M/S width, 0 = mono, 1 = unity (overridable) |
| Trim | ±24 dB | output trim (overridable) |
| Mix | 0–100 % | dry/wet; 0 nulls the dry input |

Presets: **Kick Strip**, **Vocal Strip**, **Bus Glue**.

## Master — signal flow

```
in → 4-band EQ → 3-band multiband comp (LR4 splits on TPT SVFs)
   → lookahead limiter (2 ms, brickwall) → LUFS meter → mix → out
```

- The limiter delays audio by its 2 ms lookahead and **reports that latency** to the
  host; the dry path of `Mix` is latency-matched so mix=0 nulls.
- True-peak-style metering approximated with 4x-oversampled peak detection (`TP≈`).
- The LUFS meter is ITU-R BS.1770 (`suite_core::loudness`): K-weighting (shelf + RLB
  high-pass, sample-rate-correct coefficients), momentary 400 ms, short-term 3 s, and
  gated integrated loudness with a GUI **RESET LUFS** button.

| Param | Range | Notes |
|---|---|---|
| EQ (10 params) | as Node | low shelf, 2 bells, high shelf |
| XO Low / XO High | 20 Hz–20 kHz | LR4 crossover frequencies |
| Low/Mid/High Threshold | −60..0 dB | per-band comp |
| Low/Mid/High Ratio | 1–20:1 | per-band comp |
| Low/Mid/High Makeup | ±24 dB | per-band gain |
| Knee / Attack / Release | 0–24 dB / 0.1–100 ms / 10–1000 ms | shared comp ballistics |
| Ceiling | −12..0 dB | limiter output ceiling (brickwall) |
| Lim Release | 10–1000 ms | limiter gain-envelope release |
| Mix | 0–100 % | latency-matched dry/wet |

Presets are **theme-tagged** thematic banks (see OVERSEER-ENRICH below).

## OVERSEER-ENRICH — auto-classification, LEARN, theme assist, thematic banks

OVERSEER doesn't need to be *told* what a track is — it listens.

### Instrument auto-classification (Node)
- Each Node runs a lightweight, allocation-free **feature extractor** on its own input
  (`suite_core::classify`): rolling ~4 s stats of low-band (<120 Hz) ratio, spectral
  centroid + tilt, onset rate + crest, pitch confidence + pitched-frame ratio, a 5–9 kHz
  sibilance ratio, a sustain estimate and stereo width — via a cheap SVF filterbank, two
  envelope followers and the suite pitch tracker. It never alters the audio.
- A **rule/score classifier** maps those features to `(InstrumentType, confidence)`:
  KICK, BASS, RUMBLE, PERC, HATS, SNARE, BREAKS, VOCAL, PAD, LEAD, ATMOS, FX, BUS.
  Below a confidence margin the type stays GENERIC (no false confidence on noise/silence).
- **Instrument Type** param: `Auto` (default, follows the classifier) or a concrete type
  (pins it). The Node header shows the guessed type + a confidence %; the preset bar filters
  the factory bank to the current type.
- Nodes publish their features + effective type over the same-DLL **Bus** for the Master.

### LEARN (deliberate capture — both plugins)
- **Node LEARN:** press, play the track's most representative ~8 s; the Node captures a
  focused feature window (progress shown), then **commits**: the type is locked (overriding
  drift), context-tuned defaults are applied, and **ghost suggestions** (low-shelf move from
  the measured low-band excess; comp threshold/ratio from the crest factor) appear with an
  **APPLY** button. The lock + suggestions persist with the project. The window captures
  *exactly* N seconds — the committed type matches what played during the window even if a
  different sound follows.
- **Master LEARN THEME:** press, play the fullest ~12 s; the Master captures its mix
  analysis while reading every live Node, then **locks** the inferred theme and freezes the
  assist targets. A summary card shows the theme, per-track types, and the assist moves.

### Session-theme inference + assist (Master)
- The Master aggregates the live Nodes' types/features with its own mix analysis (transport
  tempo, spectral tilt, onset density, dynamic range) into a **THEME**: DARK-TECHNO,
  DNB-BREAKS, AMBIENT, HOUSE-GROOVE, or GENERIC (with confidence, shown on the GUI).
- **Master-alone fallback:** the Master no longer needs Nodes to have an opinion. When there
  are **no** OVERSEER Node instances reporting on the bus (just the Master on the mix bus —
  the common setup), it infers the theme from its **own mix-bus analysis** — the sub-weight,
  spectral tilt, onset rate, sustain and width it already extracts from its input each block,
  plus the transport tempo (`classify::infer_theme_from_mix`). So dropping a Master on a dark
  kick-and-reese mix reads **DARK-TECHNO** and the ASSIST knob + SUGGEST moves come alive with
  no Nodes placed. Placing Nodes **refines** it: whenever any Node reports, the Master switches
  back to the richer per-instrument `infer_theme` path (Nodes carry per-track context the summed
  mix can't). Mix-only inference is held to a higher confidence floor than the Node path, so an
  ambiguous mix stays GENERIC (advisory only) rather than guess wrong.
- **ASSIST** knob (0 = display only, default **30 %**) scales how far theme-derived targets
  nudge the master EQ tilt, multiband-comp character (glue vs punch) and limiter drive.
  **SUGGEST-ONLY** keeps the theme advisory. Assist is a **bit-exact identity at strength 0**
  — with assist at 0 the audio path is unchanged from pre-enrich (verified by a null test).
- The Master grid + summary card badge each Node with its type (type-colored). The Master
  preset bar filters the factory bank by the inferred theme.

### Context-tuned defaults (per type)
Selecting a type (or committing a LEARN) applies documented starting settings — e.g. KICK =
mono-low width + fast comp; VOCAL = gentle knee + presence bands; PAD = wide + slow glue;
PERC = high-passed + fast/bright. (`enrich::context_defaults`.)

### Thematic factory banks
- Node: ≥6 purpose-named presets per common type (KICK/BASS/VOCAL/PAD/PERC/BUS) — e.g.
  *Warehouse Thump*, *Rumble Bed Glue*, *Drowned Ghost Sit*, *Grief Wash*, *Warehouse Tops*,
  *Drum Bus Glue* — tagged by `category`.
- Master: theme banks (*Warehouse Master*, *Neurofunk Master*, *Ambient Bed*, *House Punch*,
  *Gentle Master*, …) tagged by theme.

## Done-bar verification (offline tests, `cargo test -p overseer --release`)

1. **Limiter:** +6 dBFS sine into ceiling −1 dBFS → output peak ≤ −0.9 dBFS (and > −2.5,
   i.e. not over-attenuated); plus a sample-continuity check (no clicks) post-settle.
2. **LUFS:** meter reading of a −20 dBFS-RMS 997 Hz sine matches the analytic value from
   the module's own K-filter response within ±0.5 LU (momentary AND integrated); with the
   K-weighting test hook disabled the meter reads −20.0 ±0.1.
3. **Bus round-trip:** Node registers, Master writes an override, the Node's effective
   param reflects it the next block; a local touch steals control back; dropped Nodes GC.
4. **Classifier fixtures** (`suite_core::classify` + in-process bus): synth_kick train →
   KICK, sustained/sliding saw → BASS, synth_vocal → VOCAL, noise-burst train → PERC, wide
   chord pad → PAD (all above the confidence margin); white noise/silence stay below margin.
5. **LEARN window:** captures exactly N samples (fake transport) and the committed type
   matches the fixture played during the window even when a different fixture follows.
6. **Theme:** kick + rumble + pad Node streams at 130 BPM through the Bus → DARK-TECHNO.
7. **Assist null:** assist at strength 0 renders bit-identically to the pre-enrich master.
8. **Context defaults:** KICK-vs-VOCAL default diff table asserted; every bank preset loads
   + passes universal render assertions; old projects (no type param) default to AUTO.

Renders: `renders/OVERSEER/*.wav` (each preset over synthetic kick/vocal/mix signals).

## Controls

OVERSEER ships two editors from one bundle. Every parameter of both is listed below.

### Node controls

- **Instrument Type** — `Auto` (follows the classifier) or a pinned type that applies context-tuned
  defaults (KICK, BASS, VOCAL, PAD, PERC, BUS…).
- **Low Freq** — low-shelf corner frequency, 20 Hz–20 kHz.
- **Low Gain** — low-shelf gain, ±24 dB.
- **Bell 1 Freq** — first parametric bell center, 20 Hz–20 kHz.
- **Bell 1 Gain** — first bell gain, ±24 dB.
- **Bell 1 Q** — first bell width/resonance, 0.1–10.
- **Bell 2 Freq** — second parametric bell center, 20 Hz–20 kHz.
- **Bell 2 Gain** — second bell gain, ±24 dB.
- **Bell 2 Q** — second bell width/resonance, 0.1–10.
- **High Freq** — high-shelf corner frequency, 20 Hz–20 kHz.
- **High Gain** — high-shelf gain, ±24 dB.
- **Threshold** — compressor threshold, −60…0 dB (remotely overridable by the Master).
- **Ratio** — compressor ratio, 1–20:1 (overridable).
- **Knee** — compressor soft-knee width, 0–24 dB.
- **Attack** — compressor attack, 0.1–100 ms.
- **Release** — compressor release, 10–1000 ms.
- **Makeup** — post-compressor make-up gain, ±24 dB.
- **Drive** — tanh saturation amount, 0–24 dB (overridable).
- **Width** — M/S stereo width, 0 = mono … 1 = unity … 2 = wide (overridable).
- **Trim** — output trim, ±24 dB (overridable).
- **Mix** — dry/wet blend; 0 % nulls to the dry input, 0–100 %.

### Master controls

- **Assist** — how strongly theme-derived targets nudge the EQ tilt / MB-comp character / limiter
  drive; 0 % is display-only and a bit-exact identity, default 30 %, 0–100 %.
- **Suggest Only** — keeps the inferred theme purely advisory (no audio nudges).
- The four-band EQ (**Low Freq**, **Low Gain**, **Bell 1 Freq/Gain/Q**, **Bell 2 Freq/Gain/Q**,
  **High Freq**, **High Gain**) behaves as on the Node strip.
- **XO Low** — low/mid crossover frequency (LR4), 20 Hz–20 kHz.
- **XO High** — mid/high crossover frequency (LR4), 20 Hz–20 kHz.
- **Low Threshold** — low-band comp threshold, −60…0 dB.
- **Low Ratio** — low-band comp ratio, 1–20:1.
- **Low Makeup** — low-band make-up gain, ±24 dB.
- **Mid Threshold** — mid-band comp threshold, −60…0 dB.
- **Mid Ratio** — mid-band comp ratio, 1–20:1.
- **Mid Makeup** — mid-band make-up gain, ±24 dB.
- **High Threshold** — high-band comp threshold, −60…0 dB.
- **High Ratio** — high-band comp ratio, 1–20:1.
- **High Makeup** — high-band make-up gain, ±24 dB.
- **Knee** — shared multiband-comp soft-knee width, 0–24 dB.
- **Attack** — shared comp attack, 0.1–100 ms.
- **Release** — shared comp release, 10–1000 ms.
- **Ceiling** — brickwall limiter output ceiling, −12…0 dB.
- **Lim Release** — limiter gain-envelope release, 10–1000 ms.
- **Mix** — latency-matched dry/wet, 0–100 %.

## Recipes

1. **Warehouse channel + master (dark techno)** — put a **Node** on the kick with **Tight Techno
   Kick** and one on the bass with **Warehouse Bassline**; **LEARN** each so the types lock. On the
   master, load **Warehouse Master** and let the theme infer DARK-TECHNO, then raise **Assist** to
   ~40 % so the tilt and glue lean into the genre. Use a Node **Threshold**/**Drive** override from
   the Master grid to duck the bass under the kick without leaving the mastering view.
2. **Neurofunk bus control (dnb-breaks)** — Node **Growl Mid Bass** on the reese, **Warehouse Tops**
   on the hats; master **Neurofunk Master** with **XO Low** ~120 Hz and punchy **Low Ratio** so the
   sub stays tight while the mids breathe. Keep **Suggest Only** off so the theme assist actually
   shapes the limiter drive into the break.
3. **Vocal in a dark mix (vocal-rip)** — Node **Drowned Ghost Sit** on a ripped vocal: gentle
   **Knee**, **Width** wide, a **Bell 2** presence lift. On the master use **Ambient Bed** or
   **Gentle Master** with **Assist** low (~20 %) so the mastering stays out of the vocal's way, and
   watch the summary card confirm the vocal Node is classified VOCAL before you print.
