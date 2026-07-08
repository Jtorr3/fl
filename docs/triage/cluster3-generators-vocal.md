# Cluster 3: generators + vocal (snap, seance, voxkey, voxfit, ascend)

## SNAP
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] Key-track pitch is destroyed at the next block boundary: `configure()` unconditionally overwrites `f_end`/`f_start` from the Tune knob every block (plugins/snap/src/dsp.rs:288-289), clobbering the `note_on` key_hz override (plugins/snap/src/dsp.rs:373-376) set from lib.rs:378-385 — a key-tracked note plays transposed for <= one buffer (~3-10 ms) then audibly steps back to the knob pitch; render tests configure once and never pass key_hz (plugins/snap/src/render_tests.rs:21-28) so this is invisible to them.
- [MINOR] Clap "humanize" reseeds the jitter RNG with the same fixed constant at every note-on (plugins/snap/src/dsp.rs:338), so every hit gets the identical tap-jitter pattern — humanize reshapes the clap but never varies hit-to-hit, reading as a dead control under repeated 1/4-note claps.
NECESSITY: The only snare/clap source in the suite, explicitly user-requested as IMPACT's sibling, with genuinely careful retrigger/declick/width engineering; complements the loved first wave. Keep — just fix key-track.

## SEANCE
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] The "BPM-synced" chopper is only rate-synced, never phase-locked to the playhead: the gate phase free-runs from plugin load (plugins/seance/src/dsp.rs:287-299; only `transport().tempo` is read, plugins/seance/src/lib.rs:424), so 1/8-1/32 chops land at an arbitrary, non-repeatable offset from the beat grid — off-grid vocal chops in techno/dnb feel broken; the tests only verify gate *period*, not alignment (plugins/seance/src/tests.rs:66-98).
- [BROKEN] (suite-core engine, inherited by VOXKEY/VOXFIT) `ShiftEngine` never wraps its f32 synthesis-phase accumulators (`self.sum_phase[k] += tmp`, suite-core/src/shift.rs:265-266): high bins accumulate ~1.5e5 rad/s, so after ~10 min of continuous processing the float ulp exceeds several radians — top-octave phases quantize to garbage and the wet path grows progressively noisy/metallic over a long FL session until reset; every test runs seconds, so nothing catches it.
- [MINOR] SIZE / the DROWN macro re-writes FDN delay lengths live with no crossfade (plugins/seance/src/dsp.rs:377-397 -> `VarDelay::set_len` read-pointer jump, suite-core/src/fdn.rs:82-84), so automating DROWN (a headline macro) during a tail produces crackle/zipper — MURMUR dual-FDN crossfade exists but wasn't used here.
- [MINOR] Wash bypass (`amount < 1e-4`) returns early *without writing the wow buffer* (plugins/seance/src/dsp.rs:455-460), so sweeping WASH/GHOST up from exactly 0 jumps the wet onto a ~22 ms-delayed read of stale/zeroed buffer — a one-shot dropout/click at engage.
NECESSITY: This is the signature Cynthoni/Sewerslvt ghost-vocal tool and the keystone that owns the shared shift engine; nothing in FL stock or the suite duplicates the shimmer+drown+chop combination. Keep — fix the chopper phase-lock first.

## VOXKEY
VERDICT: USEFUL
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] Inherits the suite-core `ShiftEngine` unwrapped phase-accumulator degradation (suite-core/src/shift.rs:265-266, see SEANCE) — the retuned wet path deteriorates over long sessions.
- [MINOR] Confidence gate compares the *raw per-hop* MPM confidence with no median/hysteresis (`self.conf = r.confidence`, plugins/voxkey/src/dsp.rs:389; gate at dsp.rs:567), so on breathy vocals hovering at the gate the correction toggles on/off at ~21 ms hop rate — at Retune 0 (hard snap) this alternates ratio 1.0 <-> full correction, an audible gurgle the synthetic-glide test can't produce.
- [MINOR] The cepstral lifter is fixed at N/16 = 128 samples (suite-core/src/shift.rs:115), which only cleanly separates envelope from harmonics for f0 below ~300-375 Hz (documented as ~187 Hz safe, shift.rs:21-23) — on high female vocals (his typical dnb/breakcore rips) "formant preservation" partially tracks harmonics, giving comb/phasey retune artifacts on high notes.
NECESSITY: Real-time scale/MIDI retune with independent formant offset beats FL's Pitcher (dated, no formant offset) and complements NewTone (offline-only); central to the acapella workflow. Keep.

## VOXFIT
VERDICT: USEFUL
BUG-STATUS: BROKEN
FINDINGS:
- [BROKEN] TILT/PROXIMITY/AIR (and the SIT macro's tone moves) respond on a seconds timescale: their OnePole smoothers get per-sample coefficients (`set_time(12 ms, sr)`, plugins/voxfit/src/dsp.rs:396-401) but are stepped only once per block in `configure` (dsp.rs:472-476; OnePole semantics suite-core/src/dsp.rs:31-38), making the effective time constant ~= 12 ms x block-size ~= 4-6 s at 512-sample buffers — knobs and preset loads audibly creep in; the offline tests bypass it because `process_stereo` calls `reset()` which force-jumps the EQ (dsp.rs:447-453), so no test can see it.
- [BROKEN] The PV `ShiftEngine` is always in-path with no bypass/crossfade at formant = 0 (plugins/voxfit/src/dsp.rs:491-495): even the default state runs the whole vocal through a phase-vocoder identity that only nulls to ~= -8 dB on transient material (suite-core/src/shift.rs:41-45), so a user reaching for just de-ess/tilt/air still gets transient smearing + phasiness + 2048-sample latency — it can never be a transparent channel tool, which is exactly how a "make it sit" plugin gets used against stock EQ.
- [MINOR] De-esser band is 5 kHz->Nyquist, not the spec'd 5-9 kHz split (DEESS_XOVER, plugins/voxfit/src/dsp.rs:49, 302-317 vs SPECS.md:313-314): sibilant reduction also ducks all 10 kHz+ air, compounding the dark tilt on aggressive SIT settings.
- [MINOR] Inherits the suite-core unwrapped phase-accumulator long-session degradation (suite-core/src/shift.rs:265-266).
NECESSITY: Pitch-independent formant shift + SIT macro is the one piece FL stock genuinely lacks and it targets his exact rip-conforming workflow — but as shipped the two BROKEN items make it feel worse than stock EQ for everything except the formant move. Keep and fix (add a formant=0 engine bypass + fix the smoothers).

## ASCEND
VERDICT: CUT-CANDIDATE
BUG-STATUS: MINOR
FINDINGS:
- [MINOR] TRIGGER/MIDI notes are silently ignored whenever the transport is playing (`if self.playing { return; }`, plugins/ascend/src/dsp.rs:373-375), so you cannot fire a riser at an arbitrary song position — the button visibly does nothing during playback, which reads as broken even though it's documented.
- [MINOR] With Auto-Cut off, the envelope resets 1->0 in a single sample at every boundary (plugins/ascend/src/dsp.rs:449-457) and the volume term jumps full->5 % floor (dsp.rs:533) — a hard level step/click only partly masked by the impact.
- [MINOR] The tonal stack uses a naive `2*ph-1` saw (plugins/ascend/src/dsp.rs:499-504) that aliases as the +24 st rise pushes it up; audible fizz at high Key/Octave with tone-heavy balance.
NECESSITY: While the transport runs it emits a -26 dB source floor and re-fires an impact+riser on *every* 8/16/32-bar boundary from bar 0, so in a real arrangement you must gate it with playlist automation anyway — at which point riser samples, Vital/VITALGEN patches, or SWARM/DRIFT + automation do the job with more control. Weakest fit in this cluster for his workflow; cut or redesign around a playable/MIDI-triggered one-shot.

CLUSTER-SUMMARY: 5 broken, 9 minor, 1 cut-candidates.
