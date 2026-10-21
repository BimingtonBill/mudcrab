# impl-912-p3-launcher-safety (track: Phase 3, upstream fix PR)

**Worker:** Claude subagent ("Claude impl #912"). **Worktree:** `C:\Modding\wkj-phase3-safety`, branch
`phase3/launcher-safety` (from origin/main f3fd0e4). **Target dir:** `CARGO_TARGET_DIR=D:/Mudcrab/phase3-target-safety`.

Background: `C:\Modding\Mudcrab\mudcrab\local\research\postmerge-2026-09-30-launcher.md` (a post-merge audit).
Fix two real bugs in `crates/launcher` only. Do not touch `crates/converter/**` (owned by another session; a
matching converter-side guard is being handled there).

## 1. Data loss (priority)
`accept_dropped_folder` (conversion/panel.rs ~459-492) makes any unrecognised dropped folder the Output; `pair()`
(conversion/mod.rs ~59) only checks it is non-empty. A successful Start then publishes by renaming the output to a
backup and deleting it, so dropping e.g. `C:\Games` or Documents and pressing Start deletes its contents.
Fix:
- A shared helper, e.g. `output_is_safe_target(path) -> Result<(), reason>`: OK when the folder does not exist, is
  empty, or holds a `conversion-manifest.json` (a previous conversion). Otherwise refuse, with a plain reason
  ("<path> is not empty and is not a Mudcrab conversion; choose an empty or new folder").
- Refuse the drop into the Output row with that notice (the row keeps its old value), AND refuse Start/Resume/Start
  over with the same notice if the output became unsafe (e.g. files appeared after the drop). Check at the moment of
  the press, not only at drop time.
- Also cover the default/detected Output path the same way (the check lives in one place).

## 2. Start/Resume while the engine runs
`click_controls` (panel.rs ~397-455) ignores `EngineProcess`, so Start/Resume run while the engine started by Play
holds `skyrim_world.db` open, and an hours-long run then fails at publish on Windows.
Fix: take `Option<Res<EngineProcess>>`; while present refuse Start, Resume and Start over with a notice
("Close the running game first: publishing replaces files the game has open."), and draw those buttons off in
`draw_controls` (panel.rs ~574) (hint text if there is a natural place). Check/Full check and Delete staging may
stay available (they don't touch the output... verify Delete staging only touches the staging folder).

## Optional
Remove the duplicate doc line at `engine_process.rs` ~14-16.

## Tests
- Unit tests for the safety helper: missing folder, empty folder, folder with manifest, non-empty folder without
  manifest (refused), a file path instead of a folder (refused).
- Panel/app tests: dropping a non-empty non-conversion folder leaves Output unchanged with a notice; Start with an
  unsafe output emits no Start effect; with an `EngineProcess` present, Start/Resume emit no effect and are drawn
  off. Use a real short child process for EngineProcess if needed (existing tests show how), or construct the
  guard so it can be tested with a simple marker.
- Keep every existing test passing; the headless whole-launcher test must still build all schedules.

## Checks (D: target dir)
`cargo fmt --all -- --check` may fail on origin/main in converter/texture.rs and engine/app.rs (upstream's own red
CI); ignore failures outside crates/launcher, do NOT fix them. Run:
```
cargo fmt -p launcher -- --check
cargo clippy -p launcher --all-targets -- -D warnings
cargo nextest run -p launcher
```

## Commit
Upstream style, e.g. `fix(launcher): never publish over a folder that is not a conversion; no Start while the game runs`
(one or two commits). Only crates/launcher and docs/specs/modding/launcher.md (update the drop/Start rules there).
No push. Handoff `C:\Modding\Mudcrab\mudcrab\docs\handoff\impl-912-p3-launcher-safety.md` (not committed).
Report: head, test list, check results, anything unsure.
