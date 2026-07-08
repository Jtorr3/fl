# Cluster 5: phase-3 B (carve, nerve, halt, bandaid, patina, xray, chorale)

## CARVE
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] The advertised MOD target "MIX" is inert: the NERVE-modulated value is written into `Settings.mix` (plugins/carve/src/lib.rs:399) but the audio path mixes with the unmodulated base smoother (`self.params.mix.smoothed.next()`, lib.rs:433) which is what `process_sample` actually uses (plugins/carve/src/dsp.rs:420-427) — routing a NERVE LFO to Mix audibly does nothing while amount/maxdepth/sens routes work.
- [MINOR] SC-listen taps the raw sidechain with no latency alignment (plugins/carve/src/dsp.rs:410-414) while the plugin reports 2048 samples PDC — the auditioned sidechain arrives ~43 ms early against everything else, so A/B-ing Listen against the carved output flams.
- [MINOR] Attack/Release envelopes run at hop rate (coef from `hop_time`, plugins/carve/src/dsp.rs:186-194), so the whole 1-10 ms half of the Attack knob's range is indistinguishable (one 10.7 ms frame is the floor).
NECESSITY: A Trackspacer-style spectral ducker is exactly this user's workflow (rumble carved around the kick, instrumentals carved around ripped vocals) and FL has no stock equivalent — Fruity Limiter sidechain is broadband only. Keep.

## NERVE
VERDICT: CUT-CANDIDATE
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] Cross-DLL instance-id collision kills routing in real FL sessions: `new_instance_id` is `pid<<32 | per-DLL-static-counter` (suite-core/src/bus.rs:546-558), and every plugin type is its own DLL with its own counter, so in un-bridged FL (one process) the first instance of *every* Qeynos plugin gets the *same* id. `resolve_instance` returns the first slot matching the id (bus.rs:490-504), so a MOD route pointed at NERVE can silently bind to e.g. GRIT's spectrum slot — whose mod signals were zeroed at claim (bus.rs:273-275) and never written — and the modulation is silently dead depending on plugin load order; `set_label` (bus.rs:353-362) can likewise rename the wrong plugin's slot from NERVE's GUI.
- [MINOR] Routes are persisted against session-scoped ids that are regenerated every load (plugins/nerve/src/lib.rs:298-302, documented docs/NERVE.md:117-122) — every saved project reopens with all NERVE modulation silently dead until each listener's MOD source is re-picked by hand.
- [MINOR] Seqlock readers spin unboundedly (suite-core/src/bus.rs:384-388, 468-472): a process killed between the odd and even seq bumps (e.g. FL kills a bridged instance mid-publish) leaves seq odd forever and livelocks the audio thread of every reader plugin in every process.
- [MINOR] The MOD source picker lists every live bus slot of any kind (suite-core/src/ui.rs:854-869), so spectrum-only publishers (GRIT, CARVE, ...) appear as selectable modulation "sources" that always output zero.
NECESSITY: FL natively does this better for this user: Fruity Peak Controller / Envelope Controller / the link-dialog LFO modulate any VST parameter and *persist with the project*, while NERVE's routes die on reload by design and mis-resolve across DLLs. The DSP core is fine, but as suite plumbing it's a worse, more fragile version of a stock FL strength.

## HALT
VERDICT: NICHE
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] Two of the three advertised MOD targets are inert: modulated `s.mix`/`s.out_db` are written into `Settings` (plugins/halt/src/lib.rs:509-511) but `HaltCore` never reads `Settings.mix`/`out_db` (only Settings fields used are stutter/tape/quantize) and the output stage uses the unmodulated base smoothers (lib.rs:572-573, 582-585) — NERVE-modulating HALT's Mix or Out does nothing.
- [BROKEN] Stutter with a positive Pitch Step clicks every repeat: the read head runs at rate > 1 and wraps inside the loop slice with no crossfade (plugins/halt/src/dsp.rs:291-301); only the period retrigger is faded (dsp.rs:640-651), contradicting the "every loop-wrap is a 5 ms equal-power crossfade" claim (lib.rs:8) — un-faded discontinuities mid-period whenever pitch step > 0 (tests only cover pitch 0).
- [MINOR] Idle bypass skips the Out trim entirely (plugins/halt/src/lib.rs:575-586): with OUT != 0 dB, engaging/disengaging any mode steps the level by the full trim in one sample — a click on every trigger.
- [MINOR] Tape-stop at full stop holds the last sample as a frozen DC value at full amplitude (rate reaches 0 with amp 1, plugins/halt/src/dsp.rs:302-312) instead of decaying to silence — pins meters, pumps downstream compressors, and thumps on release.
- [MINOR] MIDI is folded to a block-rate held bitmap (plugins/halt/src/lib.rs:536-556), so a trigger note shorter than one audio buffer (on+off in the same block) never fires; and the documented trigger notes "C1..D#1" (docs/HALT.md:47) display as C3..D#3 in FL's own note convention, so a user placing FL's C1 gets nothing.
- [MINOR] Reverse held longer than ~16 s wraps the read head into freshly-written audio (32 s buffer, gap grows 2 samples/sample, plugins/halt/src/dsp.rs:520-524) — output garbles into the live input.
NECESSITY: Overlaps heavily with Gross Beat (tape stop, half-speed, reverse, stutter via time envelopes), which this user almost certainly has. HALT's real additions are momentary MIDI-note triggering, retrigger quantize, and the pitch-stepped/decaying stutter — and the pitch-step path is the buggy one. Worth keeping only as a performance convenience.

## BANDAID
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Unconditional hard clamp at +/-0.999 (plugins/bandaid/src/dsp.rs:31, 290): a near-full-scale kick with +6..12 dB attack boost — the plugin's headline use — hard-clips digitally right on the transient, and any float-headroom signal above 0 dBFS clips even at neutral settings, violating the "neutral nulls exactly" claim (Out is applied pre-clamp so trimming down avoids it, but nothing warns you).
- [MINOR] L/R run fully independent detectors and gain smoothers (one core per channel, plugins/bandaid/src/lib.rs:340-343, 385-391; per-channel envelopes dsp.rs:259-279) — asymmetric stereo transients get different per-channel gains, wandering the image on wide drum buses (classic unlinked-transient-designer smear).
NECESSITY: FL's stock Transient Processor is single-band; a 3-band LR4 transient designer with exact neutral null fills a real gap for this user's drum-bus work (kick attack vs rumble sustain per band). Keep.

## PATINA
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Same +/-0.999 hard ceiling on the final output including the mix=0 dry path (plugins/patina/src/dsp.rs:46, 645, 659) — hot float-headroom signals are hard-clipped even fully bypassed-by-mix, breaking the documented dry-pass contract; +9 dB head-bump on loud low end reaches it easily.
- [MINOR] With wow/flutter/age up, the wet mean delay exceeds the fixed reported 30-sample latency by up to ~10 ms (one-sided modulation on top of the base, plugins/patina/src/dsp.rs:26, 349-350) — at partial mix the dry/wet comb/flange is acknowledged in a code comment but never surfaced in the GUI, so "Mix 50% + Age" feels mysteriously phasey on transients.
NECESSITY: Tape/vinyl degradation is core to the user's atmospheric-dnb/breakcore palette; it complements WIRE (digital codec loss) rather than duplicating it, the null/identity engineering is genuinely good, and FL's stock lo-fi options don't cover keyed noise + wow/flutter + azimuth in one unit. Keep.

## X-RAY
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Legend, hover, and click-solo key on `instance_id`, which collides across plugin DLLs (root cause suite-core/src/bus.rs:546-558): most sources display as "#1"/"#2" (the id&0xFFFF is just the per-DLL counter) and soloing/hovering one instance highlights or fails to dim every other instance sharing the id (plugins/xray/src/lib.rs:390-421).
- [MINOR] A source vanishes from the overlay 3 s after its host stops calling `process` (STALE_MS, suite-core/src/bus.rs:61, 415-417) — FL's smart-disable or a muted/frozen track silently drops curves the user expects to keep comparing (Freeze must be pressed first).
NECESSITY: A session-wide multi-source spectrum overlay is something FL stock genuinely cannot do (Wave Candy is one source per instance, no overlay); dropping a publishing X-RAY on any track makes it a probe. Directly serves kick/rumble/pad balance work. The suite-core tap and seqlock plumbing are sound (torn reads genuinely prevented, CPU negligible). Keep.

## CHORALE
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Same +/-0.999 hard ceiling including the dry at mix=0 (plugins/chorale/src/dsp.rs:628) — hot inputs clip even when fully dry, contradicting the "Mix = 0 nulls the dry input" claim for float-headroom signals.
- [MINOR] Retuning is a block-rate hard jump: `configure` re-solves every loop delay each block with no glide or crossfade (plugins/chorale/src/dsp.rs:541-553, 263-271), so changing Root/Scale/Spread or the held MIDI chord while the bank is ringing snaps sustaining tails to new delay taps — an audible zap/warble on every chord change, which is the instrument's primary playing gesture.
- [MINOR] High held notes octave-stack toward Nyquist and hit the 2-sample delay clamp (plugins/chorale/src/dsp.rs:270, 508-513), leaving the top resonators of a high chord audibly out of tune (the +/-10-cent tuning claim only holds for lower stacks).
NECESSITY: A continuously-excited sympathetic resonator bank fed by vocal rips/pads is a signature dark-texture machine for this user; nothing in FL stock or elsewhere in the suite does it (PLUCK is onset-strummed, not sympathetic). The sympathetic band-weighting via the spectrum tap is a real differentiator. Keep.

CLUSTER-SUMMARY: 2 broken, 5 minor, 1 cut-candidates.
