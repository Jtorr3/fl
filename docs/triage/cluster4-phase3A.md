# Cluster 4: phase-3 A (flyby, cleave, pluck, shapeshift, chamber)

## FLYBY
VERDICT: NICHE
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Sync mode only matches the loop *rate* to tempo — the traversal phase free-runs from instantiation and never reads the transport position (plugins/flyby/src/dsp.rs:437-447, lib.rs:499 passes tempo only) — a "tempo-locked" orbit lands differently on every playback/bounce and drifts against loop points, so synced renders are non-reproducible.
- [MINOR] Sync divisions hard-code 4 beats/bar (dsp.rs:143-152) — "1 Bar" is wrong in any non-4/4 time signature.
- [MINOR] Output is hard-clamped to ±0.999 including the dry path (dsp.rs:599) — inter-plugin float headroom (>0 dBFS, routine in FL) gets digitally clipped even at mix = 0, silently breaking the advertised dry null and adding grit on hot buses (same pattern suite-wide, but FLYBY/CHAMBER clamp the *mixed* signal).
NECESSITY: A doppler fly-by is genuinely not covered by FL stock or the rest of the suite, and fits atmospheric transitions, but it's an occasional-transition toy the user could approximate with delay/pan/filter automation; not a weekly reach.

## CLEAVE
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] The capture snapshot is only latched when the internal position crosses the 2-bar wrap (dsp.rs:446-452 -> on_wrap dsp.rs:506-525), while a host seek re-syncs *without* latching (dsp.rs:374-383) — in any FL loop shorter than 2 bars (or not aligned to even-bar boundaries) `on_wrap` never fires, `pb_len`/`slice_count` stay 0, `trigger_step` early-returns (dsp.rs:684-686), and at mix = 1 the plugin outputs permanent silence; a 1-bar pattern loop, the most common FL workflow, never makes sound.
- [BROKEN] The documented stopped-transport free-run (dsp.rs:24-26, docs/CLEAVE.md:37) is defeated by the seek detector: with the host stopped, `bar_pos` freezes while `pattern_pos` advances, so every ~0.05 bars the position snaps back and re-primes (dsp.rs:376-383) — standalone playback machine-guns the same 1/40th of the pattern instead of free-running; the existing test (tests.rs:459-484) only checks the mix = 0 dry null so it can't see this.
- [MINOR] Transient mode re-slices the whole snapshot inside one `process_sample` call at each pattern wrap — up to ~16 s of audio through a 1024-pt STFT (~hundreds of FFTs) on the audio thread (dsp.rs:522 -> slice_transients dsp.rs:551-584) — a periodic CPU spike that risks an audible dropout every 2 bars at low buffer sizes.
- [MINOR] Pitched-up grains exhaust their slice before the gate ends and the read head clamps at the slice end, holding the last sample as a DC plateau for the rest of the grain (dsp.rs:482-486 with dur from dsp.rs:713-719) — audible thump/level offset on +12 st steps (the factory "Jungle Scatter" pattern sets ±12 st, dsp.rs:927-929).
NECESSITY: A transport-locked re-chopper with per-step reverse/pitch/roll/probability is exactly this user's breakcore lane and is not covered by Slicex (offline), Gross Beat (no slice sequencing), or anything else in the suite — worth fixing, not cutting.

## PLUCK
VERDICT: CUT-CANDIDATE
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] `Vel->Bright` is a dead knob: `fire_strum` retunes the strings with velocity-brightened damping (dsp.rs:756-761) but `configure` runs every block (lib.rs:591) and unconditionally retunes with `damp_coeff(s.damp, 0.0, 0.0)` (dsp.rs:738-741), erasing the brightening within one block (~10 ms) of a multi-second decay; offline tests call `configure` once per render so they can't catch it.
- [MINOR] The onset detector uses a fixed absolute threshold (`fast > 0.02`, dsp.rs:464) with no sensitivity parameter — material below roughly −34 dBFS never strums at all, and there is no user control to fix it.
- [MINOR] A strum only fires after the full 500-sample burst is captured (dsp.rs:812-822), so every pluck lands ~10 ms (plus strum stagger) behind the exciting transient with no latency reporting possible — rhythmically sloppy on percussive input.
- [MINOR] The spec'd in-loop "all-pass fine-tune" is inert — `c_ap` is hard-coded 0.0 (dsp.rs:282), reducing it to a compensated one-sample delay; the fractional delay does all tuning and the dispersion feature in SPECS.md:146-147 doesn't exist.
- [MINOR] The 2048-tap stereo body FIR runs even at Body = 0 % (dsp.rs:855 always calls `body.process`) — ~200M MAC/s of wasted CPU per instance when the feature is off.
NECESSITY: CHORALE (12-24 resonator bank, same KS core, sympathetic weighting, wet solo) supersedes PLUCK for turning audio into tuned dark resonance; PLUCK's only unique trick is the staggered strum, and this user's plucks come from synths. Redundant within the suite.

## SHAPESHIFT
VERDICT: CUT-CANDIDATE
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] The XY pad's corner labels are vertically mirrored against the DSP: labels place A/B at the top and C/D at the bottom of the pad (lib.rs:682-694), but the blend weights put A=(0,0)/B=(1,0) at the *bottom* and C/D at the top (dsp.rs:175-184), which is also what the manual promises (docs/SHAPESHIFT.md:25-26) — dragging the dot to "A · Tube" actually dials in corner C's shaper (default Cheby-3), so the plugin's primary control lies to the user.
- [MINOR] The ORBIT PHASE knob is only read inside the `!primed` gate (dsp.rs:397-407) — turning it during playback does nothing until the next reset.
- [MINOR] Cheby-3 is the raw polynomial `4x^3-3x`, which at sub-unity levels ~= -3x — a polarity-inverted 3x gain (dsp.rs:100-103) — so morphing toward the (default) Cheby corner partially cancels the other corners and the dry at partial mix, sounding like a volume-hole/phase glitch rather than distortion.
- [MINOR] Orbit sync is rate-only 4/4 free-run like FLYBY (dsp.rs:131-158, 387-392) — no transport lock, non-reproducible bounces.
NECESSITY: Fourth-plus distortion in the suite (GRIT, TRACER, PATINA, WIRE already cover saturation/character) on top of FL's own Distructor/WaveShaper; the XY orbit is the only novelty and the user's first-wave GRIT already owns this space.

## CHAMBER
VERDICT: NICHE
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Room-size changes call `Fdn8::set_delays` bare, once per block, whenever W/D/H moves >1 mm (dsp.rs:603-623), but the API explicitly requires the call site to crossfade ("click-masked by a crossfade at the call site", suite-core/src/fdn.rs:230-232, as MURMUR does) — dragging the room sliders during playback re-points live ringing delay lines and crackles.
- [MINOR] The pre-delay is an integer `set_delay` re-set every block from room diagonal + knob (dsp.rs:634-639) — dragging Pre-Delay (or room size) during audio steps the late-field input with no interpolation -> clicks.
- [MINOR] The wet always contains the direct arrival (the k=(0,0,0) image is included in the ER loop, dsp.rs:536-560) with no way to exclude it except ER/Late = 100 % late — used as a send reverb or at partial mix, a 1.5-50 ms delayed copy of the dry is re-added, giving slapback/comb coloration the user can't switch off.
- [MINOR] Wet DC blocker pole 0.995 (dsp.rs:365) puts the corner near ~38 Hz at 48 kHz — the wet path sheds exactly the 30-50 Hz region a rumble-reverb workflow needs (UNDERTOW/MURMUR are the better rumble tools, but it limits CHAMBER there).
NECESSITY: The suite already has MURMUR on the same Fdn8 core, and FL ships Fruity Convolver/Reeverb; CHAMBER's unique value (physical ER positioning) suits sound-design realism more than dark techno/breakcore aesthetics — occasional vocal-rip room placement at best.

CLUSTER-SUMMARY: 3 broken, 2 minor, 2 cut-candidates.
