# Goal
Review open PR #70 (launcher safety fixes) for correctness bugs: is it safe to merge, and what must change first?

# Context
- Checkout: the current folder, detached at the PR head 5c33dfb. Base: `origin/main` (7740c8e).
- The PR stacks on PR #69 (converter output-folder guard). #69's commits show in this diff: crates/converter/src/config.rs,
  lib.rs, pipeline.rs. Review those too, but the PR's own change is the launcher part:
  crates/launcher/src/conversion/mod.rs, conversion/panel.rs, engine_process.rs, handlers.rs, docs/specs/modding/launcher.md.
- What it claims:
  1. Data loss fix: publishing a conversion replaces the whole Output folder (old folder renamed aside, then deleted).
     Before, dropping any folder (e.g. C:\Games) on the launcher window made it the Output and Start would delete it.
     Now a folder is accepted as Output only if it does not exist, is empty, or holds `conversion-manifest.json`
     (the converter's `check_output_dir`). Checked on the drop (a refused drop keeps the old Output) and again on every
     Start, Start over and Resume press.
  2. While the engine started by Play runs (`EngineProcess` resource exists), Start, Start over and Resume are refused
     with a notice and drawn greyed out. Delete staging, Stop, Check, Full check stay available.
- Already checked: fmt, clippy -D warnings, nextest for launcher + converter pass (4 new launcher tests).
  So look elsewhere: logic holes, races, paths the checks miss.

# Scope
- Read only. You may run: `git diff origin/main HEAD`, `git diff origin/main HEAD -- <path>`, `git log`, `git show <sha>`.
- Do not fix anything, do not build: report.

# Done when
- Each claim above is checked against the code or marked "not checked".
- Specific questions answered:
  a. Is there any path to Start/Resume/publish that skips the output check (default/detected Output path at startup,
     the Output row changed another way, a conversion resumed from a leftover staging folder, the converter pipeline
     itself)?
  b. TOCTOU: the check runs at press time; the converter checks again before publish (config.rs/pipeline.rs). Is the
     publish-time check really there and does it cover resume?
  c. Can `EngineProcess` exist while the game has already exited (stale resource), wrongly blocking Start forever?
     Or be absent while an engine still runs (e.g. a second Play)?
  d. Symlinks/junctions, a manifest that is a directory, an unreadable folder, a path that is a file: handled right?
  e. Does the docs/specs change match the code?
- Verdict is one of accept, fix first, reject.

# Report
- Findings as `path:line - problem - why it matters - suggested fix`, most serious first. Mark each as certain or likely.
- What you ran and what it printed (short); what you only read.
