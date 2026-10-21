# Goal
Review branch phase3/launcher-drop-and-console (head 5c8447e) for correctness bugs before it goes upstream: is it safe to merge?

# Context
- Checkout: the current folder, on the branch. Base: `origin/main` (a896b90).
- Commits: 08ab54c (launcher: no console on Windows release builds, log to %TEMP%\mudcrab-launcher.log, engine
  started with CREATE_NO_WINDOW); b553720 (merge of Phase 1's 2faea76: `converter::check_outside_data`, reviewed
  separately, only check how the launcher uses it); 5c8447e (launcher refuses an Output that overlaps the Data folder
  on the Output drop, on Start/Start over/Resume presses, and warns on a Data drop that holds the Output).
- Files: crates/launcher/src/main.rs, logging.rs (new), engine_process.rs, handlers.rs, conversion/mod.rs,
  conversion/panel.rs, docs/specs/modding/launcher.md.
- Already checked: fmt, clippy -D warnings, nextest workspace 712 passed. A release launcher build ran on Windows:
  no child console process, the log file was written; with the Output inside the Data folder, Start was refused with
  the notice and no staging folder was created. So look elsewhere.

# Scope
- Read only. You may run: `git diff origin/main HEAD`, `git diff origin/main HEAD -- <path>`, `git log`, `git show <sha>`.
- Do not fix or build anything: report.

# Done when
- Answered:
  a. Logging: can the LogPlugin custom layer fail to install, double-log, panic, or block (e.g. File writer
     without buffering or locking from several threads; tracing's MakeWriter for File)? Is anything important still
     only on stdout/stderr?
  b. windows_subsystem: anything that breaks without a console (a panic message, the converter's in-process
     stdout printing, std handles being invalid)? Is the cfg_attr right for debug vs release?
  c. CREATE_NO_WINDOW: side effects on the engine (its own window, its stdout/stderr redirection to the log file)?
  d. Overlap check: every path where the Output or the Data folder can change is covered (drop, Detect button,
     startup detection, Start/Start over/Resume)? Wrong error wording for an unreadable path? Relative paths
     (the default Output is relative "modern_assets") resolved against the right directory?
  e. Tests: do the new tests really fail if the feature is removed? Does logging.rs's test touch a shared
     file other tests or a running launcher use?
  f. Docs match the code?
- Verdict: accept, fix first, or reject.

# Report
- Findings as `path:line - problem - why it matters - suggested fix`, most serious first, each marked certain/likely.
- What you ran and what it printed (short).
