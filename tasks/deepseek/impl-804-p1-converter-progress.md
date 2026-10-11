# Goal
Track: Phase 1 (for upstream). Contributors get a good experience converting the game from the
command line: while the converter runs, the terminal shows ONE live status line (stage, % done by
weight, items/s or MB/s, elapsed, time left); per-asset lines only with `--verbose`; a clear
summary at the end; and on failure or Ctrl+C a plain-English message with the exact command to
resume. The progress events carry the data a GUI needs too (a converter window reuses them later).

# Context
- Your checkout is cut from upstream main + three commits: two converter hard-link commits and a
  stage-timing commit (`main.rs`: time-stamped progress lines, `StageClock`, `--report-json`
  records when each stage ran). Build on that stage-timing code; do not remove it.
  Upstream commit style (`feat(converter): ...`), no `[deepseek]` prefix, no project-internal
  references (task numbers, fork branch names, local paths) in code, comments, docs or messages.
- Today: `crates/converter/src/progress.rs` (`ProgressEvent {stage, completed, total,
  current_file, message}`, count-based `fraction()`); the CLI prints one line per event
  (`main.rs`, ~73,500 lines per run); `Extracting`'s total is the archive count, so 23 events cover
  24 GB; the manifest is written once at the end; there is no Ctrl+C handler (the pipeline already
  has a `cancelled` flag, find it in `pipeline.rs`); a failed publish leaves a staging folder that
  `--resume-staging <path>` can reuse (`staging_path` in `pipeline.rs`). Measured stage costs on a
  real Skyrim SE run are in `C:/Modding/Mudcrab/mudcrab/local/research/phase1-converter-speed-plan.md`
  section 0 (read-only for you: it is outside your checkout, open it with the Read tool): textures
  are ~85% of a fresh run's time.
- The launcher (`crates/launcher/src/handlers.rs`) maps stages to hard-coded percentages. Do not
  edit the launcher; keep `ProgressEvent` backwards compatible for it (add fields, do not remove
  or rename; `fraction()` keeps its meaning).

# Scope
1. `ProgressEvent` gains optional byte counts (`bytes_completed`, `bytes_total`) where they are
   cheap to know (archive file tables for extraction; source sizes the batches already stat), and a
   whole-run fraction (`overall`) computed from per-stage weights. Weights: a small table in
   `progress.rs` with a comment saying where the numbers come from (the measured stage shares); a
   reconversion that reuses most outputs moves through stages fast, and that is fine.
2. Extraction reports progress per file (or per N files / bytes), not only per archive.
3. The CLI: when stderr is a terminal (`std::io::IsTerminal`), redraw one status line at most ~4
   times a second (`\r` + clear-to-end), e.g.
   `Textures  61%  [overall 72%]  412 items/s  00:12:31 elapsed  ~00:04:50 left`; print a new line
   only when the stage changes. When not a terminal (piped to a file, CI), print at most one plain
   line every few seconds per stage plus every stage change, each with the existing time stamp.
   `--verbose` restores one line per asset. Time left: smoothed rate (EWMA) over the overall
   fraction; show nothing until there is enough data. NO new dependencies (no indicatif etc.; if you
   think one is needed, say so in the report).
4. End summary: converted / reused / failed counts, total time, per-stage times (the stage clock
   already has them), output size, and where the report is. On failure: the first few failures in
   plain English, and the exact `--resume-staging "<path>"` command when a staging folder was kept.
5. Ctrl+C: set the pipeline's cancel flag, let in-flight work drain, say so on screen, keep the
   staging folder and print the resume command; a second Ctrl+C exits immediately. Use only what
   `tokio` (already a dependency) offers (`tokio::signal::ctrl_c`).
6. Tests: the rate/ETA maths and the throttling as pure functions (unit tests with a fake clock);
   the overall fraction is monotonic across stages; the non-terminal output mode on a fixture
   conversion prints far fewer lines than events; Ctrl+C handling at least as a unit test of the
   cancel path if a real signal is impractical.
7. `docs/specs/converters/` and the converter usage text (`--help`): document the status line,
   `--verbose` and the resume command.

Owned files: `crates/converter/src/progress.rs`, `crates/converter/src/main.rs`,
`crates/converter/src/pipeline.rs` (progress emission, cancel wiring and the failure message only;
stay away from `staging_path`/`publish_directory` internals and the texture reuse path, which other
branches edit), `crates/converter/src/archive/mod.rs` (progress emission only),
`crates/converter/tests/` (new files only), `docs/specs/converters/`,
`docs/handoff/impl-804-p1-converter-progress.md`. If you need another file, say so in the report
instead of editing it.

# Done when
These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir D:/Mudcrab/shared-check-target -- -D warnings`
    (workspace, because the launcher consumes `ProgressEvent`)
  `cargo nextest run -p converter -p launcher --target-dir D:/Mudcrab/shared-check-target`
(`shared-check-target` is shared with other branches; if a test fails naming a schema or version
number, re-run it without `--target-dir` before believing it, and report both.)
Also paste into the report what the terminal shows (non-terminal mode) for the fixture conversion
used by the tests, start to end.

# Report
Per scope point: the change and the tests. The sample output. A suggested upstream commit message
(subject + body). Anything you chose not to do and why.
