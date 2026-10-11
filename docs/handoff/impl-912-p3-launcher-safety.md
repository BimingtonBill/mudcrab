# impl-912-p3-launcher-safety (Claude impl #912, Phase 3)

Worktree `C:\Modding\wkj-phase3-safety`, branch `phase3/launcher-safety` (from origin/main f3fd0e4). Not pushed.

## Commits
- ad8a929 `fix(launcher): never publish over a folder that is not a conversion; no Start while the game runs`
- 53d589d merge of e91a126 (`phase1/output-dir-guard`, converter `check_output_dir`), per the coordinator
- 5493ed7 `refactor(launcher): use the converter's output-folder check` (head)

## What changed (crates/launcher, docs/specs/modding/launcher.md)
- `conversion::output_is_safe_target(&Path) -> Result<(), String>` (mod.rs): the one launcher entry point;
  now a thin wrapper over `converter::check_output_dir` that words each `OutputDirError` for the pane
  ("<path> is not empty and is not a Mudcrab conversion; choose an empty or new folder." etc.).
- `accept_dropped_folder`: an unsafe folder is refused with that notice; the Output row keeps its value.
- `click_controls`: Start (incl. Start over) and Resume re-check the output at the press (covers the default
  `modern_assets` too), and are refused with `GAME_RUNNING_NOTICE` while `EngineProcess` exists.
- `ControlButton::available` takes `game_running`; `draw_controls` draws Start/Resume off while the engine runs.
  Delete staging (only removes the state's staging path), Stop, Check, Full check unchanged.
- Play's launch notice now says "Start and Resume are off until it closes." (handlers.rs).
- Removed the duplicate doc line in engine_process.rs.

## Tests (nextest -p launcher: 108 passed)
New: `conversion::tests::only_a_new_empty_or_converted_folder_is_a_safe_output`,
`panel::tests::a_dropped_folder_of_other_things_is_not_taken_as_the_output`,
`panel::tests::start_and_resume_refuse_an_output_that_holds_other_things`,
`panel::tests::start_and_resume_are_off_while_the_game_runs` (real exited child as EngineProcess).
Changed: `dropped_folders_fill_the_rows_and_files_do_not` (the mod file now lives outside the output folder).

## Checks
`cargo fmt -p launcher -- --check` OK; `cargo clippy -p launcher --all-targets -- -D warnings` OK;
`cargo nextest run -p launcher` 108/108.

## Noticed (converter, not fixed)
`OutputDirError::NotConverterOutput`'s Display in e91a126 (config.rs ~152) contains two runs of ~18 spaces
mid-sentence (a lost `\` line continuation in the format string). The launcher does not show that text.
