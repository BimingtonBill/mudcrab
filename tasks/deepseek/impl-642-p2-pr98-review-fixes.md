# Goal
Fix review findings on open PR #98 (branch phase2/reference-shots-v2, worktree D:/Mudcrab/worktrees/p2-shots, head 0b29629c): `engine --shots <file>` renders camera poses to PNG. The most important fix: a failed run must exit non-zero.

# Context
- Code: crates/engine/src/shots.rs (pose file parsing and validation, the run loop, shots.log), crates/engine/src/app.rs (run(), prepare_shots), crates/engine/src/main.rs, crates/engine/src/metrics.rs (acceptance writes AppExit::error()), docs/specs/engine/reference-shots.md (spec), scripts/phase2-acceptance.ps1 (~line 126 reads the engine's exit code; read only).

# Fixes
1. BLOCKING. app.rs ~229: `app.run();` discards the returned `AppExit`, then returns Ok(()), so a failed shots run, and a failed acceptance run (already on main), exits 0. When app.run() returns `AppExit::Error(code)`, make the process exit non-zero: return an error from run() (main.rs maps it), or exit with that code. Check that scripts/phase2-acceptance.ps1 still behaves: its exit-code check now sees failures. Add a test if feasible (for example, a small function mapping AppExit to a Result, unit-tested).
2. shots.rs name validation (~187/449): also refuse names that are invalid file names on Windows, before the window opens: any of `< > " | ? *`, a trailing dot or space, and reserved device names (CON, PRN, AUX, NUL, COM1-COM9, LPT1-LPT9; case-insensitive; with or without an extension, e.g. "con.png" or "Nul.x"). Add these to the malformed-file tests.
3. Window size (~578/671): read the actual window size when the shot is taken. When it differs from the file's width/height, record that in shots.log on that shot's line, not only in the engine log. Also refuse a width or height above 8192 in validation, with a test.
4. shots.log (~969): append each shot's line to shots.log as soon as the shot finishes (create the file at the first write), instead of writing the log once at the end, so a killed run keeps what it did. Keep the FAILED lines and the final outcome.
5. Spec docs/specs/engine/reference-shots.md: make it match: the names rule, the size cap, the exit codes, when the log is written, and the quiet-window frame count. The code and the doc currently disagree by one (10 vs 11 quiet frames): make them agree, saying which you kept.
Skip other nits.

# Scope
Change only: crates/engine/src/{shots.rs, app.rs, main.rs} and docs/specs/engine/reference-shots.md. Read-only: everything else.

# Done when
- In the worktree (CARGO_TARGET_DIR is already set; do not change it): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` all pass.
- Commit as new commits on top (never amend or rebase), upstream style, e.g. `fix(engine): a failed engine run exits non-zero` and `fix(engine): shots refuse Windows-invalid names and oversized frames; log each shot as it ends`, each ending with `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage only the files you changed.

# Report
Commit ids; the exact behaviour changes (especially exit codes, and what the acceptance script now sees); test counts; anything left. Under 300 words.
