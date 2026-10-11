# impl-904-p3-try-launcher-kit (track: Phase 3, local only, never upstream)

**Worker:** Claude subagent ("Claude impl #904"). **Worktree:** `C:\Modding\wkj-phase3-try`, branch
`phase3/try-launcher` (from `phase3/launcher-conversion` e1ffb36). **Target dir:**
`CARGO_TARGET_DIR=D:/Mudcrab/phase3-target` (warm from L3's builds).

Goal: a folder the user can open and try the launcher end to end by themselves, on small fake data.

## 1. Merge L4 into this branch

`git merge bd26dfc` (branch `phase3/engine-exit-message`: the engine's stdout+stderr go to a temp log,
the launcher shows the tail when the engine exits early, and no second engine starts). It was written
against main's old launcher; this branch rewrote `handlers.rs` (conversion moved to
`src/conversion/`, messages go through `ConversionStatus::push_notice`). Resolve so that:
`launch_engine` uses `EngineProcess::spawn` and stores the resource; `watch_engine_process` runs each
frame and puts its message in the notice pane (`push_notice`, drop L4's `show_status`/`StatusText`
workaround: this branch has no StatusText); Play is refused with a notice while an engine it started
runs, and `draw_play`/`play_available` treat a running engine as "Play off" with a hint line saying
so. Keep L4's `engine_process.rs` and its tests. Register the system in `LauncherPlugin`, and make sure
the whole-launcher headless test still covers it. Read `docs/handoff/impl-903-p3-engine-exit-message.md`
and `impl-902-p3-launcher-integrate.md` in `C:\Modding\Mudcrab\mudcrab\docs\handoff\` first.

Checks: `cargo fmt --all -- --check`, `cargo clippy -p launcher --all-targets -- -D warnings`,
`cargo nextest run -p launcher`. Commit the merge (upstream style message is fine, local branch).

## 2. Build

`cargo build --profile quick -p launcher --bin launcher` and `cargo build --profile quick -p engine --bin engine`
(both from this worktree, so the converter inside the launcher and the engine share one schema).
The `quick` profile is defined in `C:\Modding\.cargo\config.toml` (release without LTO).

## 3. Assemble the kit at `D:\OpenSkyrim-Launcher-Test\`

- `launcher.exe` and `engine.exe` side by side (the launcher starts `engine.exe` from its own folder).
- `game_data\Data\`: the fake Skyrim install. Generate it fresh:
  `cargo run --profile quick -p dummy-content -- gen D:\OpenSkyrim-Launcher-Test\game_data\Data`
  (add `--with-interior` if that flag exists and works). The launcher finds `game_data\Data` in its
  working folder before looking at Steam, so the user's real Skyrim is never touched.
- `Start launcher.cmd`: `cd /d "%~dp0"` then starts `launcher.exe` (working folder = the kit, so it
  finds `game_data` and writes `modern_assets` inside the kit).
- `Make leftover conversion.cmd`: creates `modern_assets.staging-1-1\vfs\` in the kit (and deletes
  `modern_assets` if present, after a `choice` prompt), so the user can see the "unfinished conversion
  found: Resume / Delete staging" screen.
- `Reset.cmd`: removes `modern_assets`, `modern_assets.staging-*` and `modern_assets.backup-*` from the
  kit only (paths relative to `%~dp0`, with a `choice` prompt).
- `README.txt`: plain words, short. What this is (a test build of the new launcher, fake data, nothing
  touches the real Skyrim, everything stays in this folder, ~X MB). Steps to try: Start -> watch it
  finish (under a second on the fake data) -> Check / Full check -> Play (what to expect: see step 4) ->
  close the engine -> try Make leftover conversion -> Resume / Delete staging -> drop a folder on the
  window to change Data/Output. What is not in this build yet (no Browse button; the console window
  with logs is normal). How to delete it all (delete the folder; also `%TEMP%\openskyrim-engine-stderr.log`).

## 4. Try Play once on the fake data

Run the kit's launcher flow headlessly as far as possible: convert the fixture with the kit's own
launcher is not scriptable, so instead run `converter` from this worktree on the kit's game_data into
a scratch output, OR start the kit's `engine.exe --assets <converted fixture>` directly for ~15 s with a
timeout. Find out what the engine does on the fixture with only `--assets` (the launcher passes nothing
else): does it open a window and show the fixture world, or exit with a message? Write the answer in
README's Play step ("a window opens showing a small test world; close it to return" or "the engine
stops and the launcher shows why: ... ; that is expected on fake data"). Do NOT change the engine.
Delete any scratch output afterwards; leave the kit clean (no `modern_assets`).

## Rules

No push, no GitHub. Don't edit `crates/converter/**` or `crates/engine/**`. No broad filesystem scans.
If anything in steps 1-2 fails in a way you can't fix quickly, stop and report. Final report: short,
with the kit's file list and size, check results, and what Play does on the fake data. Handoff note:
`C:\Modding\Mudcrab\mudcrab\docs\handoff\impl-904-p3-try-launcher-kit.md` (not committed).
