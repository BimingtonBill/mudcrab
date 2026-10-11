# Goal
Track: Phase 1 (for upstream). A small converter-only window, `converter-gui`, so a contributor can
convert their Skyrim install without the command line: choose the Skyrim `Data` folder and the output
folder, Start / Stop / Resume, a progress bar with the stage, percent, rate and time left, a readable
summary or error at the end. It is NOT the Phase 3 launcher: no mod manager, no setup wizard, no
engine launch.

# Context
- Your checkout is cut from `phase1/converter-window`: upstream main + hard links + the stage clock +
  the live status line + a public `ProgressEstimate`, plus an empty `crates/converter-gui` crate
  already in the workspace (`Cargo.toml` with converter, bevy, crossbeam-channel, tokio, winreg on
  Windows; `src/main.rs` is an empty `fn main`). Upstream commit style (`feat(converter-gui): ...`),
  no `[deepseek]` prefix, no project-internal references.
- The design to implement (read it first; open with the Read tool, it is outside your checkout):
  `C:/Modding/Mudcrab/mudcrab/local/research/phase1-converter-window-design.md` (sections 3-5:
  the window, the state machine and its transitions, the data flow, the tests).
- The API you drive: `converter::AssetPipeline::run_async_with_cancel(config, tx, Cancellation)`,
  `converter::{PipelineConfig, PipelineFailure { error, staging, cancelled }, Cancellation,
  ProgressEvent, ProgressEstimate}` (`crates/converter/src/{pipeline,progress,lib}.rs`); resume is
  `PipelineConfig::resume_staging = Some(staging)`. The formatters the CLI uses are public in
  `progress.rs`; use them and `ProgressEstimate`, never a second copy of the maths.
- Patterns to copy (not depend on): `crates/launcher/src/handlers.rs` (background thread, tokio
  runtime, forwarding events into a crossbeam channel drained by a Bevy system with `try_recv`),
  `crates/launcher/src/ui.rs` (Bevy 0.19 UI with `bsn!` scenes), `crates/launcher/src/game_detection.rs`
  (copy it into `crates/converter-gui/src/game_detection.rs` with its tests; do not edit the launcher).
  Folder input: drag-and-drop (`FileDragAndDrop::DroppedFile`) plus detection; no new dependency.

# Scope
1. `src/state.rs`: the state machine with no Bevy types (`Idle`, `Running`, `Stopping`,
   `Finished(report)`, `Stopped { staging, cancelled }`), `apply(state, input) -> (state, effect)`,
   exactly the transitions of the design's table; unit tests for every transition.
2. `src/runner.rs`: runs a conversion on a background thread and reports `Progress`, `Finished`,
   `Failed` messages; Stop cancels through `Cancellation`; a second Stop exits the process.
3. `src/ui.rs` + `src/lib.rs` (`ConverterWindowPlugin` over a logic plugin) + `src/main.rs`: one
   900x560 window: two path rows (Data detected at start, both replaceable by dropping a folder),
   the bar (from `ProgressEstimate::overall`, never backwards), stage/percent/rate/time-left lines,
   a small notice pane, Start / Stop / Resume / Delete staging buttons enabled per state.
4. Headless tests with `MinimalPlugins`: a scripted message stream never lowers the bar; a failed run
   with a kept staging folder enables Resume; Resume produces a config with `resume_staging` set
   (assert on the effect; never start a real conversion in a test).
5. A short section in `docs/specs/converters/pipeline.md` (or a new `docs/specs/converters/converter-gui.md`)
   saying what the window is for and how to run it: `cargo run -p converter-gui`.

Owned files: `crates/converter-gui/**`, `docs/specs/converters/converter-gui.md` (new),
`docs/handoff/impl-826-p1-converter-window.md`. If you need another file (the workspace
`Cargo.toml` is already done), say so in the report instead of editing it.

# Done when
Own target dir: `D:/Mudcrab/shared-check-target/impl-826`.
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir D:/Mudcrab/shared-check-target/impl-826 -- -D warnings`
  `cargo nextest run -p converter-gui -p converter --target-dir D:/Mudcrab/shared-check-target/impl-826`
Also: `cargo build -p converter-gui --target-dir D:/Mudcrab/shared-check-target/impl-826` and
describe (you cannot see a window) what a manual run would show.

# Report
Per scope point: the change and the tests. A suggested commit message. Anything left for a manual check.
