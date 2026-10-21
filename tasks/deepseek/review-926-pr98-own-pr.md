# Goal
Is upstream PR #98 (ours, by BimingtonBill) safe to merge into `main`? Review it as a stranger would review someone else's PR: the main question is **what used to work on main and now fails or behaves differently** for an existing user, script or caller. The author's own reviews checked that it does what it set out to do; do not repeat that.

# Context
- Checkout: this folder is the PR head, based on current `main` a896b903. Full diff against main: `review-diffs/pr98.diff`. The PR title and description: `review-diffs/pr98-body.md`. If `git` works, `git diff a896b903...HEAD` gives the same diff.
- Already done: CI `tests_pass` green; CodeRabbit reviewed and its findings were fixed; the author's DeepSeek and GPT Sol reviews before sending. Skip style nits.
- Mudcrab is a Rust/Bevy 0.19 engine that runs Skyrim data converted by `crates/converter`; `crates/launcher` starts the engine binary.

# Scope
- Read only. Read the diff, the touched files and their callers. Use `git` read commands and grep if they work. Do not build or edit.
- Do not fix anything: report.
- Focus:
- Does anything that worked on main change when `--shots` is NOT used? Default startup, normal play, existing `--benchmark-only`/tour/other flags, window creation, frame pacing, systems added unconditionally to the app (they must be gated so normal runs pay nothing).
- The shot pose file format: is it documented in docs/specs/engine/reference-shots.md exactly as parsed? Malformed input: clear error naming the file and line, no panic.
- PNG capture: correct frame captured (after the pose is applied and assets are loaded, not a frame early), correct colour space/format, output path handling (existing files, missing folders, non-ASCII paths), exit when done (no hang if an asset never loads: is there a timeout?).
- Anything platform-specific (Windows paths, GPU readback alignment: rows padded to 256 bytes in wgpu).
- Then: claims in the PR description that the code does not back up; tests that do not test what they claim.

# Done when
- Each claim in the PR description is marked checked (with file:line) or not checked.
- Every regression candidate is either confirmed with evidence or dismissed with the reason.
- The verdict is accept, fix first, or reject.

# Report
- Findings as `path:line - problem - why it matters`, tagged blocking / should-fix / nit, most serious first.
- "Used to work, now fails" list (empty if none), each with the exact old and new behaviour.
- What you ran and what it printed; what you only read.
