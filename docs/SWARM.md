# SWARM — mass granulator (Glow clone)

A cloud of grains, sprayed out of a rolling capture of the input. Audio is continuously
written into a **10-second stereo circular buffer**. A **grain scheduler** — free-running
**poisson** (randomised inter-onset intervals) or **grid-sync** to the host tempo — spawns up
to **128 concurrent grains**. Each grain, randomised the instant it is born, reads an
interpolated window of the buffer with its own position, pitch, size, window, pan, and
direction. Sum the cloud, optionally send it through a **+12 st shimmer** feedback that
re-enters the buffer to bloom, and blend against the dry signal. **Freeze** locks the write
head so the buffer holds, turning any moment into an infinite, evolving texture.

```
 in ──┬───────────────────────────────────────────── dry ─────────────────┐
      │(write, unless frozen)                                            (1-mix)
      ▼                                                                     ▼
  [10 s circular capture buffer]  ◄── + shimmer(+12 st, tanh, DC) ◄──┐  out = dry·(1-mix)
      ▲   ▲   ▲  (interpolated reads)                                │      + wet·mix
   ┌──┴─┬─┴─┬─┴───────────────┐                                      │
   │ grain pool (≤128 voices) │── sum → pan/width → wet ─────────────┴──► shimmer send
   └──────────────────────────┘        (steal oldest when full)
```

## How a grain is made

On each scheduler event a grain is drawn and assigned to a free voice — or, if all 128 are
busy, it **steals the oldest** (SPECS: voice cap 128, steal oldest). Randomised at spawn:

- **Position** — a read point trailing the write head by a fixed 300 ms, **sprayed** ±`Spray`
  (0–500 ms) around that head.
- **Pitch** — **scattered** ±`Scatter` (0–24 st), free or **quantised** to whole semitones;
  playback resamples the buffer at `2^(st/12)`.
- **Size** — the grain length, `Size` 10–500 ms.
- **Window** — a **Tukey** window (flat centre, raised-cosine edges) so grain boundaries are
  click-free.
- **Pan** — an **equal-power** position drawn within the stereo `Width`.
- **Reverse** — with probability `Reverse`, the grain plays backward through its window.

Grain amplitude is **density-normalised** by `1/√overlap` (`overlap = density·size`), so the
wash stays near unity however many voices pile up.

## Read-head / position model

Grain reads are **interpolated and wrap circularly** over the whole 10 s buffer. A grain whose
read span momentarily crosses the write head (only at high pitch on a long grain) reads older
buffer content rather than garbage — a benign, on-brand granular artefact, never NaN/inf.

## Shimmer feedback

The summed cloud is fed to a **+12 st** (octave-up) pitch shifter, through an **in-loop `tanh`
soft-limiter and a one-pole DC blocker** (PRD §3), scaled by `Shimmer`, and written back into
the capture buffer. Re-granulating the octave-up signal builds the classic ascending
"shimmer" bloom. A makeup drive on the send offsets the grain cloud's round-trip loss so the
feedback reaches ~unity loop gain at `Shimmer = 100 %` (it blooms/sustains) and self-oscillates
into a **bounded** limit cycle past that — the `tanh` guarantees it can never explode. At 110 %
over 30 s the output stays ≤ 0 dBFS with no NaN.

## Freeze

Freeze **locks the write head**: the buffer stops recording and holds its current 10 s of
history, while the input is still monitored into the **dry** path per `Mix`. The grain cloud
keeps reading the frozen buffer, so the texture sustains indefinitely even if the input goes
silent (done-bar: RMS > −50 dBFS over 5 s of silent input). No factory preset ships with Freeze
on — a from-scratch render with it engaged (empty buffer) would be silent; "Frozen Cathedral"
reaches an evolving near-static wash with huge grains + shimmer instead.

## Latency

A granulator is a **time-smearing effect**, not a fixed-latency FIR stage, so — like OUROBOROS
— SWARM reports **zero latency** (`set_latency_samples(0)`) and asserts the **`mix = 0` null**
against the dry input (the suite's lag-0 `assert_single_coherent_peak` coherence check does not
apply: there is no lag-0 wet to align).

## Parameters

| Group | Param | Range | Notes |
|---|---|---|---|
| Cloud | Density | 1–500 gr/s | Grain spawn rate (skewed). Poisson, or clustered on the grid when synced. |
| Cloud | Size | 10–500 ms | Grain length. |
| Cloud | Spray | 0–500 ms | Position spray around the read head. |
| Cloud | Scatter | 0–24 st | Random pitch spread (±), per grain. |
| Cloud | Quantize | on/off | Snap scatter to whole semitones. |
| Cloud | Reverse | 0–100 % | Probability a grain plays backward. |
| Cloud | Shimmer | 0–110 % | +12 st feedback send into the buffer (bounded, self-oscillates past 100 %). |
| Cloud | Freeze | on/off | Lock the write head; input still monitored per Mix. |
| Scheduler | Sync | on/off | Grid-sync the scheduler to host tempo (else poisson). |
| Scheduler | Division | 1/16 … 1 Bar | Grid tick when synced: 1/16, 1/8, 1/8·, 1/4, 1/4·, 1/2, 1 Bar. |
| Output | Width | 0–100 % | Stereo pan spread of the grains. |
| Output | Mix | 0–100 % | Dry/wet. |
| Output | Out | −24…+24 dB | Output trim. |

## Presets

| Preset | Character |
|---|---|
| Texture Bed | Dense, slow, wide bed of medium grains — an ambient pad-maker. |
| Frozen Cathedral | Huge overlapping grains + shimmer ⇒ an evolving, near-frozen cathedral wash. |
| Shimmer Bloom | Octave-up shimmer feedback blooms into a rising, angelic cloud. |
| Rhythmic Dust | Sparse, tempo-synced clusters of tiny grains — rhythmic granular dust. |
| Reverse Swell | Mostly-reversed medium grains, wide — smeared backward swells. |
| Granular Chaos | Everything cranked: high density, wide pitch scatter, reverse, shimmer. |

## Done bar (mechanical, PRD §4 + build brief)

- Universal: no NaN/inf, peak ≤ 0 dBFS, non-silent, `mix = 0` nulls vs dry < −80 dB.
- **Onset count scales monotonically with density** across 3 settings (5 / 50 / 200 gr/s). On an
  **impulse-seeded frozen buffer**, each grain that reads across the single seeded impulse emits
  one sharp click; onsets are counted as relative-threshold crossings of the output envelope with
  a 3 ms refractory gap (relative threshold ⇒ robust to the density-normalisation gain). Counts
  come out strictly increasing.
- **Freeze with silent input sustains output** (5 s RMS > −50 dBFS).
- **110 % shimmer feedback stays bounded** (peak ≤ 0 dBFS, finite over 30 s).
- Voice cap never exceeded (≤ 128 active); every extreme-macro render stays finite and ≤ 0 dBFS.

## Try it in FL

Find more plugins → add **Qeynos SWARM** on a pad, vocal, drum loop, or bus. Load **Texture Bed**
for an instant ambient cloud; raise **Density** and **Size** for a thicker wash, **Spray** and
**Scatter** to smear it. Load **Shimmer Bloom** and let the +12 st feedback ascend; tick **Grid
Sync** and pick a **Division** with **Rhythmic Dust** for tempo-locked granular bursts. Hit
**Freeze** to lock the current 10 s of audio and play the buffer as an infinite, evolving pad
(the dry input still passes per **Mix**). **Reverse** flips grains backward; **Width** spreads the
cloud across the stereo field. Zero reported latency.
