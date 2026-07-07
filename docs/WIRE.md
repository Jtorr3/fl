# WIRE — codec degradation (Codec clone)

A *Codec*-style lo-fi destroyer built around a real **Opus** round-trip. Audio is
resampled to 48 kHz, bandwidth-limited and **crunched** (bit-depth + sample-rate
reduction), **encoded** with a pure-Rust Opus encoder at a chosen bitrate and mode,
put through simulated **packet loss** (dropped frames, click-free concealment),
**decoded**, and run through a re-encoding **regen** feedback loop for a tape-style
generation-loss effect — then width, dry/wet mix, and output trim.

Unlike a bitcrusher, WIRE's grit comes from an actual perceptual codec: at low
bitrates you hear Opus' pre-echo, band-splitting and transient smear; drop the
bandwidth and it collapses to a telephone; add packet loss and it stutters and mutes
like a bad VoIP call; wind up the regen and it eats itself alive.

## Codec plan (PRD §5)

**Plan A landed:** the pure-Rust [`opus-rs`](https://crates.io/crates/opus-rs) crate
(v0.1.x — PRD names it `opus_rs`; the published crate is hyphenated). No C, no CMake.
A link-test showed the crate's internal SILK-resampler paths at 12 k/24 k are buggy
(decode decorrelates), while its **48 kHz** path is reliable and its fidelity rises
monotonically with bitrate in both modes. So WIRE **always runs Opus at 48 kHz
internal** and realises the *Bandwidth* control as a **pre-codec low-pass** — exactly
the "approximate with bandwidth limiting and note it" fallback PRD §5 sanctions. This
dodges every buggy resampler path while keeping Bandwidth an audible, on-brand control.

One 20 ms frame encode+decode costs ~0.3 % of the real-time budget (benched), so the
codec runs **in the audio thread** (wrapped in `permit_alloc` for the suite's
`assert_process_allocs` guard). Stereo uses **two independent mono codec instances**,
the reliable path across the whole 6–128 kbps range — and independent L/R quantisation
adds a genuine codec-width artifact.

## Signal flow

```
 in (host) ─ SRC → 48 k ─┐
                         ▼
          x + regen_fb ─ Bandwidth LP ─ Crunch (bit + SR reduce) ─ Opus encode (Bitrate, Mode, FEC)
                         │                                              │
                         │                                    packet-loss drop? ── zero-fill + crossfade PLC
                         │                                              ▼
                         │                                         Opus decode
                         │                                              │
                         └───── regen: delay ─ soft-limit ─ DC-block ◄──┤ (× Regen Amount)
                                                                        ▼
                                              SRC → host ─ Width (M/S) ─ Mix (dry PDC-aligned) ─ Out
```

- **20 ms frames @ 48 k.** Reported latency = frame buffering + codec delay + SRC,
  measured with an impulse (reported == measured, done-bar §4). The dry path is
  delay-compensated by the reported amount so the mix stays phase-aligned.
- **Resampling** host ⇄ 48 k is streaming **linear interpolation** (a deliberate,
  documented quality choice: cheap, ~1-sample group delay, and its mild aliasing is
  on-brand for a degradation effect). At a 48 kHz host rate it is an exact pass-through.
- **Regen** is a per-sample feedback around the whole framed codec: decoded output is
  delayed, soft-limited (`tanh`) and DC-blocked before re-entering the encoder input,
  so repeated re-encoding compounds generation loss without runaway (PRD §3 feedback
  conventions).

## Parameters

| Param | Range | Default | Notes |
|---|---|---|---|
| **Bitrate** | 6–128 kbps | 32 kbps | Opus target bitrate (CBR). Lower ⇒ grittier, more artifacts. |
| **Mode** | Voice / Music | Music | Encoder profile: Voice = SILK/hybrid (VoIP), Music = CELT-leaning (Audio). |
| **Bandwidth** | Narrow…Full | Full | Pre-codec low-pass (Narrow ≈ 3.5 k, Medium 5 k, Wide 7.5 k, Super 12 k, Full 20 k Hz). |
| **FEC** | off / on | off | In-band forward error correction hint; also adapts the encoder to the loss setting. |
| **Packet Loss** | 0–100 % | 0 % | Probability a 20 ms frame is dropped ⇒ click-free zero-fill concealment. |
| **Crunch** | 0–100 % | 0 % | Pre-codec bit-depth (16→5 bit) + sample-rate (÷1→÷24) reduction macro. |
| **Regen Delay** | 0–500 ms | 120 ms | Feedback delay of the re-encoding generation-loss loop. |
| **Regen Amount** | 0–95 % | 0 % | Feedback gain of the regen loop (soft-limited + DC-blocked in-loop). |
| **Width** | 0–200 % | 100 % | M/S side-gain on the decoded stereo. |
| **Mix** | 0–100 % | 100 % | Dry/wet blend; dry is latency-compensated (PDC). |
| **Out** | −24…+24 dB | 0 dB | Output trim; the output is hard-guarded to ≤ 0 dBFS. |

## Presets

| Preset | Character |
|---|---|
| **Discord Ghost** | Low-bitrate wideband voice with intermittent dropouts — a VoIP ghost. |
| **Dying Stream** | Very low bitrate, heavy packet loss, FEC fighting it — a buffering, falling-apart stream. |
| **Hold Music** | Narrowband, low bitrate, collapsed width — muffled telephone hold-music. |
| **Generation Loss** | Re-encoding feedback compounds the artifacts each pass — tape-style generation loss. |
| **Subtle Digital** | Parallel, high-bitrate digital sheen for glue on a full mix. |
| **Bitcrushed Void** | Bitcrushed, starved and feeding back into the void — everything at once. |

## Done-bar (PRD §4)

- **6 kbps output correlates with the input LESS than 128 kbps output does** (both above
  a 0.3 floor) — the codec genuinely trades fidelity for bitrate.
- **Measured latency == reported latency** (impulse peak-lag, ±1 block; exact at 48 kHz).
- Plus the universal assertions (finite, ≤ 0 dBFS, non-silent, mix=0 nulls against the
  latency-aligned dry within −80 dB) and stability under extreme settings (6 kbps + 100 %
  crunch + 40 % loss + 95 % regen stays finite and bounded), packet-loss audibility, and
  clean operation at 44.1 / 48 / 96 kHz.

## Known limitations

- **Bandwidth is a pre-codec low-pass, not a true Opus internal-rate switch** (see the
  codec-plan note above; `opus-rs`' 12 k/24 k SILK paths are buggy). Recorded in
  `DEFERRED.md` with resume steps.
- **FEC** is wired to the encoder, but `opus-rs`' `decode()` has no true FEC/PLC recovery
  path (it rejects empty input), so WIRE synthesises its own concealment; FEC's audible
  benefit under loss is therefore limited. Also in `DEFERRED.md`.

Offline audition renders (each preset over pink noise and a chirp) are written to
`renders/WIRE/*.wav` by the crate tests.
