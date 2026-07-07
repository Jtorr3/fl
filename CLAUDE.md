# Qeynos Audio Suite — autonomous build in progress

On EVERY session start, in order:
1. Read `STATUS.md` — it holds CURRENT item, STEP, ATTEMPTS, and the LOG.
2. Read `PRD.md` §1 (loop contract) and the SPECS.md section for STATUS.md's CURRENT item.
3. Run `git status` + `git log -5`; reconcile per PRD §1.6 recovery protocol
   (dirty tree → wip commit, never stash/reset; trust test results over STATUS.md).
4. Resume at STATUS.md's STEP.

Rules that override everything:
- Never ask the user anything; decide, record the decision as a PRD/DEFERRED.md edit
  in the same commit, continue (PRD §0). Human-only actions go in CHECKPOINTS.md.
- Decisions live only in repo files, never in conversation memory.
- One item in flight at a time; checklist order in PRD §7 is canonical; plugins are
  referred to by NAME, never number.
- Failure valves are attempt-counted, not wall-clock (PRD §1.5).
- The build machine's Documents folder is OneDrive-redirected; resolve Documents via
  `[Environment]::GetFolderPath('MyDocuments')`. Cargo/build paths live outside
  OneDrive (repo at C:\dev\qeynos-vst-suite, CARGO_TARGET_DIR=C:\qvs-target).
- Token policy (PRD §8): Opus for all routine work; a single Fable subagent only via
  the 3-strikes escalation valve or the four named hard problems; ultracode/workflows
  ONLY at the §7 hard checkpoints and Phase 0 GO/NO-GO — never for building plugins.
  Log every escalation and ultracode use in STATUS.md.
