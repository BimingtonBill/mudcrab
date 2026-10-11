# Goal
Track: Phase 1. Design a small converter-only window so contributors can convert the game without
the command line: pick the Skyrim Data folder and the output folder, Start / Stop / Resume, a
progress bar with the current stage, % done, rate and time left, a readable summary or error at the
end. It is NOT the Phase 3 launcher (no mod manager, no setup wizard, no engine launch). Read-only
research: produce a design the lead can hand to a coder.

# Context
- Read `tasks/deepseek/context.md` first.
- Code (upstream main, read-only) at `C:/Modding/wkj-phase1`. Its checked-out branch right now is
  `phase1/vfs-hard-links`; the progress work is on branch `phase1/converter-progress` and is NOT in
  that checkout, so read its description here: `ProgressEvent` gains `bytes_completed`,
  `bytes_total`, `stage_fraction`, `overall` (whole-run fraction from per-stage weights) and a
  notice kind; `AssetPipeline::run_async_with_cancel(config, tx, Cancellation)` supports a stop
  button; a failed or cancelled run keeps its staging folder and reports it in
  `PipelineFailure { error, staging, cancelled }`; `--resume-staging <path>` resumes.
- The existing launcher is `crates/launcher` (Bevy 0.19 UI with `bsn!` scenes: `ui.rs`,
  `handlers.rs` runs the converter in the background and maps stages to hard-coded percentages,
  `game_detection.rs` finds the Skyrim install via the registry). Upstream's roadmap puts the
  launcher in Phase 3 (`docs/roadmap/03-luau-and-ui.md` §3.4) and the progress channel in Phase 1
  (`docs/roadmap/01-asset-pipeline.md`).

# Questions
1. Where should the window live so upstream accepts it as Phase 1: a new binary in the converter
   crate (e.g. `converter-gui`, behind a cargo feature so the CLI build stays lean), a new small
   crate, or a mode of the launcher? Weigh build time and dependencies (Bevy is already a workspace
   dependency; a native folder picker such as `rfd` would be a new dependency: say whether it is
   needed or whether typed paths + game detection suffice), and overlap with Phase 3.
2. What can be reused from `crates/launcher` (game detection, the background-runner pattern in
   `handlers.rs`, UI scenes) without depending on the launcher crate? Could the shared parts move
   to the converter crate so both use them?
3. The screen: a wireframe (ASCII) of the one window, its states (idle, running, stopping,
   finished, failed with resume, cancelled with resume) and the transitions; what each button does
   in each state.
4. The data flow: how `ProgressEvent`s reach the UI (channel, Bevy resource, system), how Stop maps
   to `Cancellation`, how Resume finds the kept staging folder, and how the time-left estimate is
   shared with the CLI renderer (same functions, not a copy).
5. Tests the coder should write (headless Bevy app tests of the state machine, no window needed)
   and how to check it by hand.
6. A step plan: 2-3 coder tasks, each with owned files and acceptance commands.

# Scope and rules
Read-only. Write only `local/research/phase1-converter-window-design.md` (under 1200 words). No
cargo builds (a conversion is running on this machine), no engine runs. Do not commit.
Then a summary under 300 words as your final message.
