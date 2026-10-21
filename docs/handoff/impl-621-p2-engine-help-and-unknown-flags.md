# engine `--help` and unknown options

Phase 2 contributor experience: the engine now prints a `--help` that lists
every option it accepts, and it refuses an option it does not know instead of
ignoring it. Before this, `--grid-z 3` parsed as nothing at all and the run
started with the default start position, so a typo looked like it worked.

## What changed

- `crates/engine/src/config.rs`: a `HELP_TEXT` const (every option, its
  argument, one line each, grouped into assets and start position, streaming,
  benchmark and profiling, diagnostics, help) next to the parser, and a
  hand-written `match` that still parses one arm per option. `from_args` and
  `from_env` now return `Result<ConfigAction, ConfigError>`: `ConfigAction::Help`
  for `-h`/`--help`, `ConfigAction::Run(Box<EngineConfig>)` otherwise, and a
  `ConfigError` naming the argument it did not match. An argument that looks
  like an option and is within two edits of a known one gets a "did you mean"
  (Levenshtein, no new dependency); a bare positional argument is refused too,
  since the parser accepts none.
- `crates/engine/src/main.rs`: prints `HELP_TEXT` and returns 0 for `--help`,
  prints `error: {error}` to stderr and exits 2 for a command line it cannot
  use. Any other outcome is unchanged: `engine::run(config)`.
- `crates/engine/tests/cli.rs`: runs the built binary, so the exit codes and
  the message on stderr are checked on the real path, not through a helper.

The suggestion list is the option tokens of `HELP_TEXT`, which the tests hold
equal to the parser's match arms: the two cannot drift without a test failing.

## Audit: flags the scripts and the docs pass

Every engine flag the four `scripts/phase2-*.ps1` pass is one the parser
already knew: `--assets`, `--worldspace`, `--grid-x`, `--grid-y`,
`--stream-radius`, `--auto-fly-speed`, `--max-commit-ms`, `--benchmark-*`,
`--accept-*`, `--acceptance-screenshot`, `--profile-*`, `--synthetic-instances`
and the five `*-fixture` flags. The other `--flags` in those scripts belong to
cargo (`--workspace`, `--all-targets`, `--all`, `--bin`, `--bins`, `--check`,
`--release`), git (`--quiet`, `--ignore-submodules`, `--short`) and
`world-inspect` (`--radius`, `--output`); `the_phase2_scripts_only_pass_options_the_parser_accepts`
reads the scripts back at compile time and fails if a new flag appears that is
neither an engine option nor one of those twelve. No script needed a change.

The docs name no engine flag the parser does not know either. The engine
command lines in `docs/` are `--assets`, `--worldspace`, `--grid-x`,
`--grid-y`, `--stream-radius`, `--benchmark-only`, `--synthetic-instances`,
`--benchmark-frames`, `--allow-incomplete-assets`,
`--diagnostic-asset-fallbacks` and `--streaming-fixture`; the other flags in
`docs/` (converter, dummy-content, `world-inspect`, cargo and git) are not the
engine's. Nothing in the repository passes `--version`, which the parser has
never accepted and now refuses.

## The help text as printed

`--help` and `-h` print this to stdout and exit 0, before any asset is opened:

```text
OpenSkyrim engine

Usage: engine [options]

Streams converted cells from a world database. With no options it reads
"modern_assets" and starts in worldspace 0x3c at cell (0, 0).

Assets and start position:
  --assets <dir>                        converted asset directory to stream (default: modern_assets)
  --worldspace <id>                     worldspace form id, decimal or 0x-hex (default: 0x3c)
  --grid-x <cell>                       starting cell x (default: 0)
  --grid-y <cell>                       starting cell y (default: 0)
  --allow-incomplete-assets             run despite a failed manifest or integration-report gate
  --headless                            run without opening a window

Streaming:
  --stream-radius <cells>               cells streamed around the camera (default: 2)
  --max-commit-ms <ms>                  cell commit time allowed per frame (default: 16.67)
  --auto-fly-speed <units/s>            fly the camera forward at this speed; 0 holds it still

Benchmark and profiling:
  --benchmark-only                      run the synthetic benchmark; opens no world database
  --benchmark-frames <count>            stop after this many measured frames
  --benchmark-duration <seconds>        stop after this many measured seconds
  --benchmark-warmup-frames <count>     frames discarded before measuring (default: 60)
  --benchmark-output <file>             benchmark report path (default: benchmark-report.json)
  --synthetic-instances <count>         instances in the synthetic benchmark (default: 250000)
  --accept-min-fps <fps>                fail the run below this average frame rate (default: 60)
  --accept-p95-ms <ms>                  fail the run above this p95 frame time (default: 16.67)
  --accept-max-memory-growth-gib <gib>  fail the run above this memory growth (default: 0.5)
  --acceptance-screenshot <file>        write a screenshot when the run ends
  --profile-output <dir>                profile bundle directory (default: no bundle)
  --profile-scenario <name>             scenario name recorded in the profile (default: adhoc)
  --profile-run-id <id>                 run id recorded in the profile (default: run-1)
  --profile-commit <hash>               commit recorded in the profile (default: unknown)
  --profile-dirty-worktree              record uncommitted worktree changes in the profile
  --profile-hardware <text>             hardware recorded in the profile (default: unspecified)

Diagnostics:
  --diagnostic-asset-fallbacks          log every fallback asset substitution
  --material-fixture                    run the material fixture (no world database)
  --terrain-water-fixture               run the terrain-water fixture (no world database)
  --transform-bounds-fixture            run the transform-bounds fixture (no world database)
  --renderer-fixture                    run the renderer fixture (no world database)
  --streaming-fixture                   stream the fixture world instead of the full asset set

Help:
  -h, --help                            print this message and exit

Example (the dummy-content fixture's worldspace is 1):
  engine --assets modern_assets --worldspace 1

```

## The error text

```text
> engine --grid-z 3
error: unknown option '--grid-z'; did you mean '--grid-x'? Run with --help to list every option.
```

Exit code 2. `--grid-z` is one edit from both `--grid-x` and `--grid-y`, and
the first one the help text lists wins. An option with no close match still
names itself ("`unknown option '--frobnicate'. Run with --help to list every
option.`"), and a bare word is reported as "`unexpected argument 'riverwood'.
Run with --help to list every option.`".

## Tests

In `config.rs`: `help_is_requested_by_either_spelling`,
`help_wins_over_the_rest_of_the_command_line`,
`an_unknown_option_is_refused_and_the_nearest_option_is_named`,
`an_unknown_option_with_no_close_match_is_still_named`,
`a_positional_argument_is_refused`, `the_arm_scan_still_finds_the_parser_arms`,
`help_lists_every_option_the_parser_accepts`,
`help_names_no_option_the_parser_rejects`,
`the_phase2_scripts_only_pass_options_the_parser_accepts`, plus the existing
`parses_runtime_options` (moved to the `ConfigAction` return).
In `tests/cli.rs`: `help_prints_the_usage_and_exits_zero` (stdout is exactly
`HELP_TEXT`, exit 0) and
`an_unknown_option_exits_nonzero_and_names_the_option`.

The drift check is not vacuous: it failed on the first run of this work,
reporting `--help never names -h`, which is the missing short form it was
written to catch.

## What was run

- `cargo fmt --all -- --check` - clean.
- `cargo clippy -p engine --all-targets -- -D warnings` - clean.
- `cargo nextest run -p engine` - 63 tests passed, 2 of them the new CLI ones.
- The help text above was extracted from `HELP_TEXT` when this note was
  written, and `help_prints_the_usage_and_exits_zero` asserts the real binary's
  stdout equals that const byte for byte.

## Notes

- The engine accepts no positional arguments today, so a stray word is now an
  error rather than silently ignored; nothing in the repository passed one.
- A value that fails to parse (for example `--worldspace abc`) is still
  ignored, as before: this change is about arguments the parser does not
  recognise at all.
- The dummy-content fixture is worldspace 1 (its `WRLD_FORM_ID`), which is why
  the help text's example is `engine --assets modern_assets --worldspace 1`.
