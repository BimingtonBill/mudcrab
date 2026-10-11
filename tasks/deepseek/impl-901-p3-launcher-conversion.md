# impl-901-p3-launcher-conversion (track: Phase 3, upstream PR "L3")

**Worker:** Claude subagent ("Claude impl #901"). **Worktree:** `C:\Modding\wkj-phase3`, branch
`phase3/launcher-conversion` (cut from `phase1/converter-window` a062046). **Target dir:**
`CARGO_TARGET_DIR=D:/Mudcrab/phase3-target` for every cargo command.

## Goal: one app

The workspace has two GUIs that both run the converter:

- `crates/launcher` (upstream's Phase 3 launcher, Bevy 0.19 UI with `bsn!` scenes): detects Skyrim,
  auto-starts a conversion on a background thread, shows a basic bar (`ConversionProgressEvent`,
  `ConversionChannel`, `ConversionStatus` in `main.rs`/`handlers.rs`), has a mod-manager panel stub
  with a drag-and-drop zone, and a Play button that starts the engine when conversion is complete.
- `crates/converter-gui` (built by another session this week): a much better conversion front end.
  `state.rs` is a Bevy-free state machine (`apply(state, input) -> (state, effect)`, table tests per
  transition, `controls()` decides which buttons are enabled); `runner.rs` runs the pipeline on a
  background thread with `Cancellation` (Stop; second Stop quits) and runs `converter::check_output`
  on its own thread; `lib.rs` has the status resource (bar never goes backwards, `ProgressEstimate`
  for rate and time left), the logic plugin and systems; `ui.rs` is a 900x560 window with Data/Output
  path rows (detected, or drop a folder), bar, stage/percent/rate/time-left lines, a notice pane,
  buttons Start / Stop / Resume / Delete staging / Check / Full check. Its `game_detection.rs` is a
  copy of the launcher's plus `data_dir_from_drop` and a `pub(crate) is_skyrim_data_dir`.
  Spec: `docs/specs/converters/converter-gui.md`.

**Decision (made by the user):** move converter-gui's logic and UI into `crates/launcher` and
delete the `converter-gui` crate and binary. The launcher becomes the one app.

## What to do

1. Read both crates fully first, and `docs/specs/converters/converter-gui.md`,
   `docs/specs/modding/launcher.md`, `docs/roadmap/03-luau-and-ui.md` §3.4.
2. Move `state.rs`, `runner.rs` and the status/logic parts of `lib.rs` into the launcher, e.g. a
   `crates/launcher/src/conversion/` module (`mod.rs`, `state.rs`, `runner.rs`, `status.rs`,
   `panel.rs` for the UI). Your layout call; keep it easy to read. Use `git mv` where a file moves
   whole so history follows.
3. Replace the launcher's old conversion path (`ConversionProgressEvent`, `ConversionChannel`,
   the old `ConversionStatus`, `run_background_converter`, `to_launcher_event`,
   `update_conversion_progress`, `sync_status_text`) with the moved state machine and runner. No two
   ways of running a conversion may remain.
4. One `game_detection.rs`: the launcher's, extended with `data_dir_from_drop` and
   `is_skyrim_data_dir` visibility and all tests from both copies (deduplicated).
5. The conversion panel lives in the launcher window (900x600 today; resize only if needed, keep it
   non-resizable or make it resizable, your call, say which). Keep the header, the mod-manager panel
   stub and the Play button. Layout suggestion: header; conversion panel (paths, bar, lines, buttons,
   notices); mod-manager stub below or beside it; Play at the bottom. If it doesn't fit, show the
   conversion panel while the conversion is not complete and the mod manager when it is (use the
   existing `LauncherState` states: `ConvertingAssets` exists and is unused). Make the notice pane
   not overflow with a long check result (cap lines, keep the newest, or scroll).
6. Behaviour:
   - Start-up: detect Skyrim (Data row) and the output (`GamePathConfig.converted_assets_path`,
     default `modern_assets`). If the output is already a complete conversion (the launcher's
     existing manifest + DB + integration-report check in `handlers.rs`), go straight to Finished /
     Play enabled. Keep that check (move it into the conversion module).
   - **No auto-start.** The user presses Start (or Resume when a staging folder from an unfinished
     run is known). A 60+ GB, 20+ minute job starts only on purpose.
   - Play is enabled only when the conversion is complete; it starts the engine as today (`--assets`).
   - Dropped items: a **folder** goes to the conversion paths (Skyrim Data or install root -> Data row,
     anything else -> Output row), only while no conversion or check is running; **files**
     (.zip/.7z/.esp/.esm/.esl) go to the existing mod drag-and-drop handler (still a stub that logs).
7. Delete `crates/converter-gui` (and its workspace member line in the root `Cargo.toml`); move its
   spec into the launcher spec: update `docs/specs/modding/launcher.md` with a "Conversion screen"
   section (states, buttons, what each does, drop rules), and delete or reduce
   `docs/specs/converters/converter-gui.md` accordingly (fix any links, e.g. `docs/specs/README.md`).
   The launcher's `Cargo.toml` gains nothing new except what converter-gui used (it already has
   bevy, crossbeam-channel, tokio, converter, winreg).
8. **Do not change `crates/converter/**`** (owned by another session). If the launcher needs a
   converter API change, stop and describe it in your report instead.

## Tests

- Keep every state-machine table test and the headless schedule/no-conflict test (the Bevy B0001
  `Query<&mut Text>` conflict crashed converter-gui at start once; `ParamSet` fixed it; a headless
  test that builds the schedule guards it). Extend that headless test to the **whole launcher app's
  systems** (the launcher's own systems plus the panel), so any query conflict between them fails a
  test instead of crashing at start.
- A headless launcher test for the panel: e.g. with a complete fake output, start-up lands in
  Finished and Play is enabled; with no output, Start is enabled and Play is not; a dropped Data
  folder fills the Data row.
- Game detection tests from both copies.

## Checks (what CI runs), all with the D: target dir

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p launcher -p converter
```

Then build and open it once to check it really starts (no panic in the first seconds) and take a
screenshot: `cargo build --profile quick -p launcher`, run the exe for ~10 s with a timeout, kill it.
If you can capture a screenshot of the window, save it to
`C:\Modding\Mudcrab\mudcrab\local\research\launcher-merged-first-run.png`.

**Build gate:** a disk-heavy conversion is running on this machine. Do not run any cargo command
until the lead tells you "builds free". Read, design, make and self-review all edits first; if you
have not been told by then, end your turn with an "edits done, not built" report (files changed,
design choices) and the lead will resume you.

## Commits

Upstream style, no internal references, task numbers or machine paths. Several commits are fine,
e.g. `refactor(launcher): move the conversion state machine and runner into the launcher`,
`feat(launcher): conversion screen with stop, resume and check`, `chore: remove the converter-gui
crate`. Commit only in `crates/launcher/`, `crates/converter-gui/`, root `Cargo.toml`/`Cargo.lock`
and `docs/specs/`. Then write `docs/handoff/impl-901-p3-launcher-conversion.md` in
`C:\Modding\Mudcrab\mudcrab` (NOT committed; the lead handles it): design, what moved where,
behaviour changes vs both old apps, tests, check results, open questions.
