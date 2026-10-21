# Goal
Pre-send review of branch phase2/reference-shots-v2 (engine `--shots <file> [--shots-out <dir>]`: render exact camera poses to PNG once streaming settles) before it opens as a DRAFT PR on realfakenerd/mudcrab. Verdict: accept, fix first, or reject.

# Context
- The full diff against current main is saved at `local/review-diffs/reference-shots-v2.diff` (git is not needed; read that file). Surrounding code: the working tree is branch `portal`, NOT main, so for engine files prefer what the diff shows; use the tree only for unchanged helpers and say so.
- The PR text is `local/pr-drafts/77-pr-reference-shots-v2.md`.
- Stated design: one worldspace per run (other-worldspace or interior shots skipped and logged); the settle rule waits for arming, retiring cells, the renderer path and a 2 s warm-up; `--shots` refuses to combine with benchmark, acceptance screenshot, auto-fly or fixture; a malformed pose file is refused before a window opens (15 tests). It parses options in main's current style; an open PR #94 replaces the engine's option parser, so the author notes two arms to add after #94 merges.
- Checks already run by the author: fmt, workspace clippy -D warnings, nextest 717 passed.

# Scope
- Read only. Do not build or run anything (a timing booking holds the machine).
- Look for: panics on user input (pose file, paths), a settle rule that could hang forever or fire early, file writes outside `--shots-out`, behaviour changes for runs without `--shots`, claims in the PR text the diff does not support, and anything a maintainer would push back on (size, style vs surrounding code).

# Done when
- Each claim in the PR text is checked against the diff or marked not checked; a verdict.

# Report
- Verdict, then findings `path:line - problem - why it matters` (at most 6, most serious first), then what is good. Under 400 words.
