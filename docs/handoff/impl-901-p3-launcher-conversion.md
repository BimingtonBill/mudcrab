# impl-901-p3-launcher-conversion (Phase 3): converter-gui merged into the launcher

Worker: Claude impl #901. Worktree `C:\Modding\wkj-phase3`, branch `phase3/launcher-conversion`
(from `phase1/converter-window` a062046). Target dir `D:/Modding-overflow/phase3-target`.

## What moved where

| From | To |
| :--- | :--- |
| `crates/converter-gui/src/state.rs` | `crates/launcher/src/conversion/state.rs` (git mv; unused `ConversionState::staging()` removed) |
| `crates/converter-gui/src/runner.rs` | `crates/launcher/src/conversion/runner.rs` (git mv; paths only) |
| `crates/converter-gui/src/ui.rs` | `crates/launcher/src/conversion/panel.rs` (git mv; now a `Scene` inside the launcher window) |
| `crates/converter-gui/src/lib.rs` | `crates/launcher/src/conversion/mod.rs` (plugins, resources, effects, output check) + `status.rs` (`ConversionStatus`) |
| converter-gui `game_detection.rs` | merged into the launcher's (`data_dir_from_drop`, `pub(crate) is_skyrim_data_dir`, all tests) |
| launcher `ConversionProgressEvent`, `ConversionChannel`, old `ConversionStatus`, `run_background_converter`, `to_launcher_event`, `update_conversion_progress`, `sync_status_text`, `detect_skyrim_and_start_conversion` | deleted; the moved state machine and runner are the only conversion path |
| launcher's manifest + DB + integration-report check | `conversion::output_is_complete` |
| `docs/specs/converters/converter-gui.md` | deleted; content is `docs/specs/modding/launcher.md` §3 "Conversion screen" |

`crates/converter-gui`, its workspace member and its `Cargo.lock` entry are gone. The launcher's
`Cargo.toml` is unchanged. `crates/converter/**` untouched; no converter API change was needed.

## Design

- `main.rs`: `LauncherState`, `LauncherSet` (Input -> Logic -> Actuate -> Draw), `LauncherPlugin`
  (everything but the window; `main` = `DefaultPlugins` + `LauncherPlugin`, tests use it headless).
- `conversion/mod.rs`: `GamePathConfig` (converter-gui's `WindowPaths` merged into it: Data and
  Output rows, engine `--assets`), `CurrentConversion` (was `WindowState`), `OutputReady`,
  `ConversionLogicPlugin` (no UI), `ConversionPanelPlugin` (panel systems, start-up detection,
  `refresh_output_ready`).
- Play gate: `OutputReady` (the old launcher check) and no run Running/Stopping (publish renames
  over the output, which fails while the engine holds files). The check reads the whole manifest,
  so it runs only on the first frame, when the Output folder changes, and when a run ends.
- A complete output at start-up leaves the state machine `Idle` (no fake `Finished` report); Play
  is enabled and Start reads "Convert again". Same buttons as `Finished`.
- `LauncherState` follows the conversion: FirstRunSetup (no Data folder), ConvertingAssets (a run
  going, or one still to make), ModManager (output ready); LaunchingEngine is transient (Play ->
  `launch_engine` on enter -> back next frame).
- Window: 900x600, not resizable (unchanged). Header, conversion panel (grows; notice pane takes
  the leftover height), compact mod-manager stub, hint + Play row. Notice pane is a
  `bevy::ui_widgets::ScrollArea` (`Overflow::scroll_y`, column so text wraps): new notices scroll
  to the end, a check result to its first line.
- Drops: folders -> conversion rows (Data folder or install root -> Data, else Output) only while
  the path rows are on; `.zip/.7z/.esp/.esm/.esl` files -> mod stub (logs); other files -> notice.

## Behaviour changes

- Launcher: no auto-start (Start/Resume only); adds Stop (second Stop exits 130), Resume, Delete
  staging, Check, Full check, Detect, folder drops; bar never moves backwards; engine start and
  launch errors go to the notice pane (the old status line is gone); Play off during a run.
- converter-gui: now inside the launcher window; "not found" wording says "drop onto the launcher".
- Progress messages now go through `Input::Progress` (a no-op transition) so the variant is used in
  the binary crate; `PendingEffects::pending()` is `#[cfg(test)]`.

## Tests

All converter-gui state, logic, runner and panel tests kept (renamed resources). New:
whole-launcher headless schedule test (B0001 guard for every launcher + panel system), complete
output -> Play on / ModManager / Idle / no run / "Convert again", no output -> Start on / Play off,
dropped install root fills the Data row, `output_is_complete` cases, `OutputReady` follows the
Output folder, drop routing and lock, Start label, `is_mod_file`, `play_available`,
`launcher_state_for`.

## Checks

All with `CARGO_TARGET_DIR=D:/Modding-overflow/phase3-target`, 2026-09-28:

- `cargo fmt --all`: reflowed a few test lines, nothing else.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean on the first run.
- `cargo nextest run -p launcher -p converter`: 248 passed, 9 skipped.
- First run: `cargo build --profile test -p launcher --bin launcher` (reused the nextest build;
  the `quick` profile would have rebuilt every dependency before the 13:15 timing booking).
  `D:/Modding-overflow/phase3-target/debug/launcher.exe`, run 12 s from
  `D:/Modding-overflow/phase3-scratch/first-run` and killed by `timeout`: window created, no panic,
  no error, no `modern_assets` written (nothing auto-started). No screenshot (the lead takes it).

Commit: `a1e55cd refactor(launcher): run conversions from the launcher and remove converter-gui`
(one commit; the docs went into it too because they were already staged).

## Open questions

- Resume only knows staging folders from this session (the launcher-resume task covers earlier ones).
- Pressing Start while an engine launched from here still runs will fail at publish (Access denied);
  the launcher does not track the engine process.
- The start-up output check parses the full manifest on the first frame (about a second on a full
  install), as the old launcher did in `Startup`.
- A Check that finds problems does not turn Play off; `OutputReady` is only the cheap check.
- Mouse-wheel scrolling of the notice pane relies on Bevy's UI picking; confirm in the window.
