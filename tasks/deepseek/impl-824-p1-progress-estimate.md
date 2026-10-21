# Goal
Track: Phase 1 (for upstream). Step A of the converter window: the rate and time-left estimate that
the command-line status line uses becomes a public, reusable `ProgressEstimate` in the converter
crate, so a GUI shows exactly the same numbers as the CLI (one implementation, not a copy).

# Context
- Your checkout is cut from `phase1/converter-progress` (upstream main + hard links + the stage
  clock + the live status line). Upstream commit style (`refactor(converter): ...` or
  `feat(converter): ...`), no `[deepseek]` prefix, no project-internal references.
- `crates/converter/src/progress.rs` holds `ProgressEvent`, `STAGE_WEIGHTS`, `overall_fraction`,
  `ProgressRenderer` (string-producing, caller owns the clock) and a private `SampleWindow`
  estimator plus a high-water `overall`. Design notes (read-only, open with the Read tool):
  `C:/Modding/Mudcrab/mudcrab/local/research/phase1-converter-window-design.md` §4 and §6 task A.

# Scope
1. A public `ProgressEstimate` in `progress.rs`: `observe(&ProgressEvent, Duration)`,
   `overall() -> f32` (monotonic), `items_per_second()`, `bytes_per_second()`,
   `time_left(elapsed) -> Option<Duration>`; `ProgressRenderer` holds one and keeps producing the
   same strings (the existing renderer tests and `tests/fixture_progress_output.rs` must pass
   unchanged).
2. Re-export `Cancellation`, `PipelineFailure` and `ProgressEstimate` from `crates/converter/src/lib.rs`
   if they are not already public at the crate root.
3. Tests: the estimate's numbers with a fake clock (as the renderer tests do), monotonic overall,
   no time-left before enough data.

Owned files: `crates/converter/src/progress.rs`, `crates/converter/src/lib.rs`,
`crates/converter/tests/` (new files only), `docs/handoff/impl-824-p1-progress-estimate.md`.

# Done when
Own target dir: `D:/Mudcrab/shared-check-target/impl-824`.
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir D:/Mudcrab/shared-check-target/impl-824 -- -D warnings`
  `cargo nextest run -p converter -p launcher --target-dir D:/Mudcrab/shared-check-target/impl-824`

# Report
The change, the tests, a suggested commit message.
