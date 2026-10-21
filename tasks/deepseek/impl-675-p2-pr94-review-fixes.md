# Goal
Fix the confirmed pre-open review findings on open PR #94 (`--help`, and the engine refuses unknown or malformed options). Track: Phase 2.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-engine-help`, branch `phase2/engine-help-v2` (checked out at the PR's current upstream head 9a538bbd; do not switch). Use the worktree's own `target/`. This PR is already open upstream (#94); you make LOCAL commits on top of its head. Never push. Never run engine.exe.
- Reviews to act on (read them): `C:/Users/Brannock/.claude-deepseek/agents/runs/review-668-prestage-open-94.1/report.md` and `C:/Users/Brannock/.claude-deepseek/agents/runs/review-668-mimo.1/report.md`. The PR text the reviewers read is `C:/Modding/Mudcrab/mudcrab/local/review/pr-open-94/pr-text.md` (do not edit it; list the PR-text corrections you recommend in your report instead).
- Verify every finding against the code before fixing; skip any that is wrong and say why.

# Fixes
1. `option_tokens` (config.rs ~line 702) panics when the scanned text ends with a bare `--` (`&after[length.max(1)..]` on an empty slice). Fix and add a test.
2. The script scan test reads `../../scripts` from `CARGO_MANIFEST_DIR` and panics when it is absent: skip cleanly when the folder is missing.
3. The PR claims every flag AND value the scripts pass still parses, but values come from a hand-written list. Make the test extract the values that follow engine flags in the scripts (best effort), or narrow the test name and doc to what it checks. Also check the scripts for `--max-commit-ms 0`, `--benchmark-duration 0` and negative `--stream-radius`: report any hits.
4. The scan is top-level only, and `word--word` text can create phantom tokens: make the scan recursive and only take tokens that start at a word boundary (after whitespace, a quote or line start).
5. A bare `--` now exits 2 as an unknown option where main ignored it: accept a bare `--` as a no-op (end-of-options is not needed; just ignore it) with a test.
6. `--max-commit-ms 0` used to be ignored (default kept) and now exits 2: keep the new strictness but make sure the error message is clear; report it as a behaviour change for the PR text.
7. Nit: `--typo --help` exits 2 because options are parsed left to right; make `--help` (and `-h`) win anywhere on the command line by checking for it before parsing, with a test.
8. Nit: help says it lists every option "with its default" but boolean flags show none: report the PR-text correction only.

# Scope
The files the findings name, and their tests. No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass (the test `app::tests::streaming_fixture_loads_its_interior_by_id...` is load-sensitive on main; if only it fails, rerun it alone and report).
- Commits in upstream style (`fix(engine): ...`), trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`, files staged by name. No push, no amend.

# Report
Commit ids; each finding: fixed / skipped (why); the PR-text corrections you recommend, as exact old -> new sentences; test count. Under 400 words.
