# Goal
Track: Phase 2 (for upstream, contributor experience). The engine gets a `--help` that lists every
option it accepts, and it refuses an unknown option with a clear error instead of silently ignoring
it (today a typo'd flag looks like it worked).

# Context
- Your checkout is cut from upstream `main`. Upstream commit style (`feat(engine): ...`), no
  `[deepseek]` prefix, no project-internal references in code or commits.
- `crates/engine/src/config.rs`, `EngineConfig::from_args` (~84-206): a hand-written `match` over
  arguments; the catch-all `_ => {}` (~202) drops anything unknown. Keep the hand-written parser (no
  new dependency such as clap).
- `--help` / `-h`: print a usage text (every option, its argument, a one-line meaning, grouped
  sensibly: assets and start position, streaming, benchmark and profiling, diagnostics) and exit 0.
  Keep the text in one place next to the parser, and add a test that every option string the parser
  matches appears in the help text, so the two cannot drift (e.g. a const list of option names used
  by both, or a test that scans the source match arms; pick the simpler honest one).
- Unknown option: return an error naming it and suggesting `--help` (and the nearest known option if
  a simple edit-distance check finds one within 2). Positional arguments, if the parser accepts any
  today, keep working. Check how `from_args`' result is used (`main.rs`/`app.rs`) so the error
  reaches the user as a readable message and a non-zero exit.
- Before changing behaviour, grep `scripts/` (phase2-acceptance.ps1, phase2-close.ps1, any other
  script) and the docs for every engine flag they pass; list any that the parser does not know. If
  one exists, stop and report it rather than breaking the script.
- The dummy-content fixture: a bare run needs `--worldspace 1`; say so in the help text's example.

# Scope
Owned files: `crates/engine/src/config.rs`, `crates/engine/src/main.rs` (only if the error/exit path
needs it), `docs/handoff/impl-621-p2-engine-help-and-unknown-flags.md`. Nothing else.

# Done when
- `--help` prints the usage and exits 0; an unknown flag exits non-zero with the message; tests cover
  both, the drift check, and that every flag the scripts pass is known.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets -- -D warnings`
  `cargo nextest run -p engine`
- One or two commits, upstream style.

# Report
The help text as printed, the error text for `--grid-z 3`, the script-flag audit result, test names.
