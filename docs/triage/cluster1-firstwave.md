# Cluster 1: first wave (grit, ember, impact, tracer, overseer)

## GRIT
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] The MOD section (NERVE listen routes) is completely dead: the modulated drive/depth/mix/out computed into `base` (plugins/grit/src/lib.rs:488-491) are unconditionally overwritten per-sample by the unmodulated host smoothers (plugins/grit/src/lib.rs:526-531) — routing a NERVE LFO to DRIVE/DEPTH/MIX/OUT audibly does nothing (IMPACT's identical retrofit works because it has no per-sample overwrite, proving the pattern deviation).
- [MINOR] Toggling AUTO-GAIN is unsmoothed — `ag` jumps between 1.0 and a ratio clamped to ±12 dB in a single sample (plugins/grit/src/dsp.rs:341-347) — an audible click/level step when switching it mid-playback.
- [MINOR] The unconditional ±0.999 output clamp (plugins/grit/src/dsp.rs:358) hard-clips even the pure dry path at mix=0, so inserting GRIT on an FL bus running normal >0 dBFS float headroom digitally clips signal that FL itself would pass cleanly.
NECESSITY: Sidechained kick-driven distortion is the core KAS:ST rumble-bass technique and has no FL-stock or in-suite equivalent (TRACER/SHAPESHIFT distort but nothing else is sidechain-driven); the DSP (compensated 4x OS, auto-gain, latency-true nulls) is genuinely solid.

## EMBER
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] GUI preset loading never applies `freeze_mix` — `apply_preset` sets every other key but omits it (plugins/ember/src/lib.rs:209-234), so the three Freeze-bank presets authored with `"freezemix"` 0.9/0.7/0.5 (plugins/ember/src/presets.rs:55-59) silently keep whatever Freeze Mix was already on the knob (offline tests use `settings_from_preset`, which does read it, so they can't catch this).
- [MINOR] Disengaging Freeze while Freeze Mix < 1 steps the output discontinuously — the crossfade branch is gated on the unsmoothed `cfg.freeze` bool (plugins/ember/src/dsp.rs:397-401), so output jumps from `fm·wet + (1−fm)·dry` to full wet in one sample (possible click); engaging is safe, releasing isn't.
NECESSITY: Spectral attack/decay-per-band smoothing with 60 s tails and freeze is exactly the atmospheric-dnb/ambient texture engine this user's Cynthoni side needs; nothing in FL stock or the rest of the suite (MURMUR/SMUDGE are different animals) does per-bin temporal fading.

## IMPACT
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] Key Track is functionally dead: `note_on` writes the MIDI-note frequency into `f_end` (plugins/impact/src/dsp.rs:257-261), but `configure` — called unconditionally at the top of every process block (plugins/impact/src/lib.rs:437) — overwrites `f_end` from the Pitch End param (plugins/impact/src/dsp.rs:234), so the key-tracked pitch survives only until the next buffer boundary (~3-10 ms) then audibly snaps back to the knob value; the documented "play the sub-bass melody from the keyboard" workflow (docs/IMPACT.md:116) cannot work, and no test covers reconfigure-during-note.
NECESSITY: A dedicated tuned-kick/808 synth is central to both of this user's genres; the deep preset bank, phase-continuous declicked retrigger, and embedded transients beat FL's stock Fruity Kick easily. Its MOD section, unlike GRIT/TRACER's, actually works.

## TRACER
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] The MOD section (trim/mix/out routes) is dead by the same pattern as GRIT: modulated values written into `base` (plugins/tracer/src/lib.rs:657-659) are overwritten per-sample by the unmodulated smoothers (plugins/tracer/src/lib.rs:696-698) — every route the MOD UI offers is silently a no-op.
- [MINOR] The instability guard threshold `wet.abs() > 16.0` (plugins/tracer/src/dsp.rs:437) can be tripped by legitimate settings — four bands hard-clipped near ±1 with band levels toward +12 dB (lin ≈4) sum past 16 — causing repeated filter-tree resets plus 256-sample fade-ins, i.e. rhythmic dropout/stutter at extreme-but-reachable settings (the extreme-params test uses 0 dB band levels so never hits it).
- [MINOR] The unconditional ±0.999 output clamp (plugins/tracer/src/dsp.rs:453) hard-clips the latency-matched dry path at mix=0 on >0 dBFS float bus headroom, same as GRIT.
NECESSITY: Pitch-locked LR4 multiband saturation (MPM tracker with median/hysteresis/slew, confidence freeze, MIDI override) is the suite's most technically distinctive distortion and ideal for sliding 808s/reese work; no FL-stock equivalent, and the tracker/crossover core checks out end-to-end.

## OVERSEER
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] The Node's saturation stage has no transparent setting: at the DRIVE minimum (0 dB → pre-gain 1.0) the signal still passes through full `tanh(x)/1` curvature (plugins/overseer/src/node.rs:356-357), adding measurable harmonics and ~2 dB peak compression on hot material on every Node instance — the strip can never be run clean at mix=1.
- [MINOR] Master ENRICH assist targets are computed and published only inside the editor tick (plugins/overseer/src/lib.rs:1048-1050); with the Master GUI closed the audio thread keeps applying stale targets (or none if never opened), so with ASSIST > 0 the processing silently depends on whether/when the editor was last open.
NECESSITY: The user's stated favorite, and it earns it — Node/Master bus with overrides, LEARN/theme ENRICH, correct latency reporting, BS.1770 metering validated against analytic references; the dynamics/limiter/EQ cores are the cleanest code in the cluster. Keep, and honor the memory-noted wish-list (instrument context is already half-built here).

CLUSTER-SUMMARY: 3 broken, 7 minor, 0 cut-candidates.
