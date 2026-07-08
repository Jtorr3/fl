# Cluster 2: lese-clone wave (drift, wire, ouroboros, swarm, smudge, murmur, undertow)

## DRIFT
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Output hard-clamps every sample at +/-0.999 including the dry path (plugins/drift/src/dsp.rs:407-408) — a dry signal peaking above -0.01 dBFS is hard-clipped even at mix=0, and the up-to-+36 dB resonant bells routinely slam this clip with no soft knee (the mix=0 null test only uses 0.5-amplitude input).
- [MINOR] BPM Sync matches rate only — the glide phase free-runs and never reads transport position (plugins/drift/src/dsp.rs:290-300, plugins/drift/src/lib.rs:477; no `pos_beats` anywhere in the crate) — a "synced" sweep lands in a different place on every play/render, so bounces are not reproducible.
NECESSITY: An endless Shepard filter riser is a genuine hypnotic-techno device FL cannot fake (a normal automated filter sweep has to end), it is zero-latency and cleanly implemented, and it complements rather than duplicates ASCEND (which generates tension, DRIFT processes existing loops). This user would actually reach for it on rolling basslines and percussion loops.

## WIRE
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Packet-loss concealment fades out into a drop but never fades back in — the dropped packet is simply never decoded, so the next good frame resumes from stale decoder overlap state at arbitrary amplitude (plugins/wire/src/dsp.rs:578-604) — audible clicks at dropout re-entry on sustained material, contradicting the "click-free concealment" claim (no test measures re-entry discontinuity).
- [MINOR] The FEC toggle has no decoder-side path — lost packets are never run through any `decode_fec`/PLC recovery, so FEC only makes the encoder reserve redundancy bits (plugins/wire/src/dsp.rs:556-559 vs 578-604) — the knob's audible effect is a slight quality *drop*, not loss recovery (docs/WIRE.md:109 admits this; the param table still sells it as error correction).
- [MINOR] `reset()` recreates both Opus encoder/decoder objects with heap allocation on the audio thread outside any `permit_alloc` (plugins/wire/src/dsp.rs:505-520) — potential dropout at every FL transport stop/start; the alloc guard only instruments `process()` so tests never see it.
- [MINOR] Reported latency at non-48 k host rates is a rounded rescale of the 48 k figure that ignores the actual push/pull resampler group delays (plugins/wire/src/dsp.rs:715-718) — ~1-2 samples of dry/wet misalignment at 44.1 k (FL's default rate) causing mild HF combing at partial mix; the latency test only runs at 48 k (plugins/wire/src/tests.rs:96-128).
NECESSITY: A real Opus round-trip (bitrate mush, packet dropouts, generation-loss regen) is exactly the "ripped Discord/phone vocal" texture the Sewerslvt-adjacent workflow wants, and nothing in FL or the rest of the suite does codec artifacts (PATINA is analog lo-fi, the crunch stage alone is redundant with Fruity bitcrushing but the codec is not). Keep.

## OUROBOROS
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] The Freeze-Mix blend branches instantaneously on the freeze bool — `if s.freeze { fm*mixed + (1-fm)*dry } else { mixed }` (plugins/ouroboros/src/dsp.rs:926) — toggling FREEZE with FREEZE MIX < 1 steps the output by (1-fm)*(wet-dry) in one sample -> click; input gate and feedback are smoothed (dsp.rs:868-872) but this branch is not, and the freeze tests only measure settled steady-state (plugins/ouroboros/src/tests.rs:208-275).
- [MINOR] GUI factory-preset apply never sets `freeze_mix` (plugins/ouroboros/src/lib.rs:376-416) while the offline preset loader does read it (plugins/ouroboros/src/presets.rs:177) — loading a factory preset leaves whatever freeze_mix the user last had, and the dirty-dot state can disagree with the offline render tests.
NECESSITY: Effects *inside* a 110 % feedback loop (pitch-up regeneration, closing filters, freq-shift clangor) is something FL's mixer topology genuinely cannot build, and it is the dub-delay/self-oscillation engine for both dark techno and breakcore. One of the strongest of this wave; keep.

## SWARM
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Grid-sync spawns the entire cluster — `round(density*period)` up to 128 grains — on one single sample, while the 1/sqrt(density*size) loudness normaliser assumes grains spread uniformly in time (plugins/swarm/src/dsp.rs:517-529 vs 567-571) — synced bursts play ~sqrt(n) (~ +9-10 dB on the factory "Rhythmic Swarms" presets) hotter than the same settings unsynced and can slam the +/-0.999 hard clamp on hot input; render tests can't catch it because the clamp keeps peak <= 0 dBFS.
- [MINOR] The density/size normaliser is computed from the *instantaneous* un-smoothed params and applied to grains already sounding (plugins/swarm/src/dsp.rs:569-571) — automating DENSITY down (200->20) instantly multiplies the live cloud by ~sqrt(10) -> audible level jump/zipper.
- [MINOR] "Tempo-locked" grid sync is tempo-periodic only — the spawn countdown free-runs from reset and never reads transport position (plugins/swarm/src/dsp.rs:534-545; no `pos_beats` in the crate) — bursts land on the beat only if playback happens to start on one, and renders don't match playback (docs/SWARM.md:129 promises "tempo-locked granular bursts").
- [MINOR] Every grain mono-sums the stereo capture buffer before panning — `0.5*(read(cap_l)+read(cap_r))` (plugins/swarm/src/dsp.rs:555) — the wet path destroys the source's stereo image and rebuilds width only from random pan, despite SPECS/docs advertising a stereo capture buffer.
- [MINOR] Freeze-Mix branch is un-smoothed on the freeze bool (plugins/swarm/src/dsp.rs:598-602) — toggling freeze with FREEZE MIX < 1 clicks (same defect as OUROBOROS; freeze tests only check settled states).
NECESSITY: FL has no live-input granulator (Fruity Granulizer is a dated sampler), and a 128-voice cloud with freeze + octave-shimmer feedback is core vocabulary for the atmospheric dnb / vocal-rip ambience side of this user's work. Keep, but the sync mode needs the burst-normalisation fix before the "Rhythmic Swarms" bank is trustworthy.

## SMUDGE
VERDICT: NICHE
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] SCRAMBLE's fixed x2 decorrelation makeup gain (plugins/smudge/src/dsp.rs:69, applied at 509) becomes a pure volume boost when the permutation is near-identity: at RANGE = 0, `redraw_perm` returns the identity (dsp.rs:421-423) so AMOUNT is exactly a 0..+6 dB gain knob with zero scrambling, and small ranges similarly overshoot loudness — the +/-3 dB energy done-bar only tests RANGE = 1 (plugins/smudge/src/tests.rs:143-196).
NECESSITY: The scramble/spectral-delay/stretch ops are real glitch textures FL doesn't have, but the blur/smear half of the plugin heavily overlaps EMBER (a first-wave favorite that already owns the "spectral smear + freeze" lane), and chaos-macro spectral mangling is an occasional sound-design reach, not a per-track tool. Worth keeping only as the once-in-a-while weirdness box.

## MURMUR
VERDICT: CUT-CANDIDATE
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] The Freeze-Mix blend branches instantaneously on the freeze bool (plugins/murmur/src/dsp.rs:456-459) — engaging/releasing freeze with FREEZE MIX < 1 steps the output in one sample -> click; the input duck is smoothed (dsp.rs:407-408) but this branch is not, and the freeze tests only measure after the smoother settles (plugins/murmur/src/tests.rs:56-97).
NECESSITY: It is a competently built FDN reverb, but it's the suite's *third* reverb (CHAMBER models spaces, SEANCE has the shimmer wash) on top of FL's strong stock reverbs, and its single differentiator — a re-randomised room per onset — is subtle at the default 35 % mix and duplicated nowhere in this user's stated workflow; freeze washes are already covered by SWARM/EMBER/SEANCE. Hard to see him reaching for it.

## UNDERTOW
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] The rumble path's DC blocker uses a fixed R = 0.995 (plugins/undertow/src/dsp.rs:233, applied to the FDN sum at 486), a ~38 Hz first-order highpass at 48 k — inside the very sub band the plugin generates: ~-2 dB at a 50 Hz rumble fundamental, -4.4 dB at TUNE = C1 (32.7 Hz) and worse toward C0 (16 Hz), and the corner scales with sample rate (~76 Hz at 96 k, gutting the sub); no test measures rumble low-end response.
NECESSITY: This is *the* KAS:ST/hard-techno rumble-kick workflow (strip click -> saturate -> dark FDN -> sub LP -> key-locked tune peak -> self-ducked) collapsed into one insert, replacing a fragile FL send-reverb + sidechain + EQ chain; mono-below-150 Hz and exact rumble-muted null are the right calls. The most on-taste plugin in this cluster; fix the DC-blocker corner and it's first-wave quality.

CLUSTER-SUMMARY: 0 broken, 15 minor, 1 cut-candidates.
