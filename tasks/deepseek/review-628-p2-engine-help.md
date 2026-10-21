# Goal
Review branch phase2/engine-help (4 commits, head a57dff0, cut from an older upstream main 6f583cd) for correctness, and list what a re-cut onto current upstream main must change. Question: is its design sound, and what breaks when it is rebuilt on current main?

# Context
- Checkout: this folder, detached at a57dff0. See the change with `git log --oneline 6f583cd..HEAD` and `git diff 6f583cd..HEAD`. Current upstream main is `origin/main` (7740c8e); compare with `git diff 6f583cd origin/main -- crates/engine/src/config.rs` and `git show origin/main:crates/engine/src/config.rs`.
- Intent: the engine gets `--help` (HELP_TEXT), refuses unknown options and bad values with a clear `ConfigError` instead of ignoring them, in `crates/engine/src/config.rs` (and callers).
- Known: current main added options this branch has never seen (at least `--max-unloads-per-frame`, `--max-upload-mib-per-frame`, `--max-model-spawns-per-frame`, and possibly others). A re-cut must add every option on main to the parser and HELP_TEXT, or it would refuse options main accepts.
- Main points to check: every option main's parser accepts, and whether the branch's parser and HELP_TEXT would cover it; value validation (ranges, missing values, a value that starts with `--`); that nothing the scripts in `scripts/` or `tools/` pass would now be refused (search them for engine flags); error and exit behaviour; tests.

# Scope
- Read only. Run only `git log`, `git diff`, `git show`, and reading/searching tools. Do not build or run tests. Do not edit anything.

# Done when
- Every option on current main is listed with covered / missing in the branch.
- Every flag used by the repo's scripts is checked against the branch's parser.
- The verdict is one of accept, fix first, reject, for the re-cut.

# Report
- Findings as `path:line - problem - why it matters`, most serious first; say for each whether you checked it in code or inferred it.
- The option table (option, on main, in branch parser, in HELP_TEXT).
- Under 500 words.
