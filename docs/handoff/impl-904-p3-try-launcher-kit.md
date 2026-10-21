# impl-904-p3-try-launcher-kit (Phase 3, local only) - handoff

Worker: Claude impl #904. Worktree `C:\Modding\wkj-phase3-try`, branch `phase3/try-launcher`.
Target dir `D:/Modding-overflow/phase3-target`.

**Status: done.** The branch head is `734982a Merge commit 'bd26dfc' into phase3/try-launcher`.
The kit is at `D:\OpenSkyrim-Launcher-Test\` (174 MB) and is clean: no `modern_assets` in it.

## 1. L4 merged (734982a)

- `engine_process.rs` was taken from L4 unchanged, with its tests.
- `handlers.rs` was resolved from this branch's version:
  - `play_available(ready, state, engine_running)`.
  - A new private `play_off_reason` feeds both the notice for a refused Play press and the Play
    row hint. While the engine runs, both say "Play is off while the OpenSkyrim engine runs
    (process N): close it to play again."
  - `launch_engine` returns early if an `EngineProcess` exists. Otherwise it calls
    `EngineProcess::spawn(.., &stderr_log_path())` and inserts the resource.
  - `watch_engine_process` puts the exit message into the notice pane with `push_notice`. The
    multi-line message goes in as one notice. L4's `show_status`/`StatusText` code was dropped.
- `main.rs`:
  - `mod engine_process`.
  - `LauncherPlugin` registers `watch_engine_process` in `LauncherSet::Logic`.
  - New whole-launcher test `a_running_engine_turns_play_off_until_it_exits`. A real child
    (`cmd` ping for about 1 s, then exit 3) stands in for the engine. The test checks that Play is
    drawn off with the hint, that a press only adds a notice, that the exit notice carries the
    code and the stderr line, and that Play comes back.
- Spec `docs/specs/modding/launcher.md`: the Play bullet (log file, running state, exit messages)
  and an `engine_process.rs` row.
- Checks:
  - `cargo fmt --all -- --check`: clean after `cargo fmt`, which only re-wrapped some asserts.
  - `cargo clippy -p launcher --all-targets -- -D warnings`: clean.
  - `cargo nextest run -p launcher`: 99/99 passed.

## 2. Builds

`cargo build --profile quick -p launcher --bin launcher -p engine --bin engine` took 6 min 29 s.
The outputs are `D:/Modding-overflow/phase3-target/quick/{launcher,engine}.exe`. The converter
bin was also built, for step 4.

## 3. The kit, `D:\OpenSkyrim-Launcher-Test\`

`launcher.exe` (88 MB), `engine.exe` (86 MB), `game_data\Data\` (12 files from
`dummy-content gen --with-interior`), `Start launcher.cmd`, `Make leftover conversion.cmd`,
`Reset.cmd` and `README.txt`. The .cmd files and the README use CRLF line endings.

- **Scripts:** both scripts were run with piped Y/N answers and behave as intended:
  - Make leftover conversion: with no output; with an output, answering N; with an output,
    answering Y.
  - Reset.
- **Start launcher.cmd:** the launcher finds `game_data\Data`, Output is `modern_assets`, and it
  opens with Start on and the "No complete conversion ... yet" notice. It writes nothing until
  Start is pressed. A PrintWindow capture of the launcher window only is in the session
  scratchpad as `kit-launcher.png`.
- **Resume on the fake leftover:** checked with the converter CLI
  (`--resume-staging` on an empty `staging-1-1\vfs`). It completes a full conversion.

## 4. Play on the fake data

The fixture was converted with the worktree's converter into a scratch folder (0.3 s, 16
converted, complete). The kit's `engine.exe --assets <that>` was then started directly.

- A window titled "OpenSkyrim" opens and stays open (runtime initialized, worldspace 0000003C,
  about 60 fps, 412 entities, no warnings apart from gilrs).
- Closing it (WM_CLOSE, as the X does) gives exit code 0, so the launcher shows "OpenSkyrim
  engine closed."
- **The window shows only a flat dark grey screen.** Captures of the engine window alone
  (PrintWindow, Vulkan and DX12) show plain grey, while the same capture of the launcher window
  shows its UI.
- Supporting clues:
  - `render/main_opaque_pass_3d/vertex_shader_invocations` stays at 0;
  - `--acceptance-screenshot` never fired in 30 s (its assets or renderer readiness never
    became true).
- The README says the grey screen is expected on the fake data. The engine was not changed.
  Whether the fixture should show something from the default camera is an engine and fixture
  question, not investigated.
- The scratch output and all logs were deleted.

Care point: a first full-screen grab (CopyFromScreen, not DPI-aware) captured other windows on
the user's desktop. It was deleted right away. Later captures used PrintWindow on the target
window only.

## Open

- The grey engine view on the fixture (above).
- Nothing was tried by hand in the kit: drag and drop, the mouse wheel, and a real Play press
  through the launcher (the one path that writes `%TEMP%\openskyrim-engine-stderr.log`).
