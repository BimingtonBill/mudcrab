# Goal
Is upstream PR #94 (ours, by BimingtonBill) safe to merge into `main`? Review it as a stranger would review someone else's PR: the main question is **what used to work on main and now fails or behaves differently** for an existing user, script or caller. The author's own reviews checked that it does what it set out to do; do not repeat that.

# Context
- Checkout: this folder is the PR head, based on current `main` a896b903. Full diff against main: `review-diffs/pr94.diff`. The PR title and description: `review-diffs/pr94-body.md`. If `git` works, `git diff a896b903...HEAD` gives the same diff.
- Already done: CI `tests_pass` green; CodeRabbit reviewed and its findings were fixed; the author's DeepSeek and GPT Sol reviews before sending. Skip style nits.
- Mudcrab is a Rust/Bevy 0.19 engine that runs Skyrim data converted by `crates/converter`; `crates/launcher` starts the engine binary.

# Scope
- Read only. Read the diff, the touched files and their callers. Use `git` read commands and grep if they work. Do not build or edit.
- Do not fix anything: report.
- Focus:
- Which engine invocations worked on main and now fail? Check every existing caller of the engine binary in the repo: `scripts/` (phase2-*.ps1 and others), `crates/launcher` (how it builds the engine command line), docs/README command examples, tests, CI workflows. Any flag spelled differently, a flag value form (`--x=y` vs `--x y`), a flag the launcher passes that the new parser does not know, positional args, env-var driven options.
- Does refusing unknown options break forward/backward compatibility between launcher and engine versions (an older launcher passing a removed flag, or a newer one passing a flag this engine lacks)?
- Exit codes and where errors go (stderr vs a window on Windows `windows_subsystem`), so a launcher-started engine that refuses its options does not just vanish silently.
- `--help` output: matches the real option set; no option parsed but undocumented or vice versa.
- Then: claims in the PR description that the code does not back up; tests that do not test what they claim.

# Done when
- Each claim in the PR description is marked checked (with file:line) or not checked.
- Every regression candidate is either confirmed with evidence or dismissed with the reason.
- The verdict is accept, fix first, or reject.

# Report
- Findings as `path:line - problem - why it matters`, tagged blocking / should-fix / nit, most serious first.
- "Used to work, now fails" list (empty if none), each with the exact old and new behaviour.
- What you ran and what it printed; what you only read.
