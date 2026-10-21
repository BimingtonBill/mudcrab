# Goal
Fix three review findings on open PR #94 (branch phase2/engine-help-v2, worktree D:/Mudcrab/worktrees/p2-engine-help, head 60915f36): the engine now refuses unknown options (exit 2), so every place in the repository that shows an engine command line must use real options, and the scripts-scan test must cover every script.

# Context
- The PR adds `--help` and refuses unknown options and bad values. The parser and HELP_TEXT are in crates/engine/src/config.rs.
- Finding 1: `.github/ISSUE_TEMPLATE/03-performance.yml` ~line 23 says `cargo run -p engine -- --scenario world`; `--scenario` is not an engine option. Replace it with a command that works, e.g. `cargo run --release -p engine` (check the template's surrounding text so the instruction still reads right).
- Finding 2: crates/engine/src/config.rs ~1360: the test that scans scripts reads 5 hard-coded files with include_str! and checks flag names only. Make it read EVERY file in the repo's `scripts/` folder at test time (std::fs::read_dir on Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts"), keep files ending .ps1, .sh or .py), keep the existing NON_ENGINE_FLAGS allowlist and the floor guard (adjust the floor if needed), and keep the separate test that parses the values the scripts give. Do NOT add any new engine option to make a script pass; if a script in scripts/ on this branch passes a flag that is not an engine option and is not in the allowlist, report it rather than hiding it.
- Finding 3: docs/specs/modding/launcher.md ~line 124: a sketch spawns `engine --config config.json`. Change it to what the launcher really passes: `--assets <path>` (see crates/launcher/src/handlers.rs).

# Scope
Change only: .github/ISSUE_TEMPLATE/03-performance.yml, crates/engine/src/config.rs (the scripts-scan test only), docs/specs/modding/launcher.md. Nothing else.

# Done when
- In the worktree (CARGO_TARGET_DIR is already set; do not change it): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` all pass.
- Commit as ONE new commit on top (never amend or rebase): `git add` the three files, message `fix(engine): every shown engine command line uses real options; the script scan reads all of scripts/`, ending with `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`.

# Report
Commit id; which script files the scan now reads and how many flags it checks; test counts; any script flag you had to report. Under 200 words.
