# Goal
On Windows release builds, the Mudcrab launcher opens without a console window beside it, and its logs go to a file
instead, so nothing is lost. Track: Phase 3 (upstream-bound, held locally).

# Context
- Crate: `crates/launcher` (Bevy 0.19 app). Entry: `crates/launcher/src/main.rs` (adds `DefaultPlugins`, the
  conversion panel plugin, handlers). Today it has no `windows_subsystem`, so Windows opens a console with Bevy's
  logs next to the launcher window.
- The launcher starts the engine (`crates/launcher/src/engine_process.rs`, `EngineProcess::spawn`, called from
  `handlers.rs` `launch_engine`). The engine's stdout and stderr already go to a temp log file
  (`stderr_log_path()` = `%TEMP%\mudcrab-engine-<pid>.log`). The engine is a console-subsystem program: once the
  launcher has no console, Windows would open a NEW console window for the engine. Prevent that with
  `std::os::windows::process::CommandExt::creation_flags(0x0800_0000)` (CREATE_NO_WINDOW) on Windows only.
- Decisions already made (do these, don't redesign):
  1. `#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]` at the top of main.rs.
     Debug builds (`cargo run -p launcher`) keep their console.
  2. The launcher's log goes to a file in every build, not only release: `std::env::temp_dir()` joined with
     `mudcrab-launcher.log`, created fresh (truncated) at each start. Put the path in one pub fn
     (e.g. `launcher_log_path()`), next to `stderr_log_path()` in engine_process.rs or in a small new module.
  3. Do it through Bevy's `LogPlugin` (configure it on `DefaultPlugins` with `.set(LogPlugin { custom_layer, .. })`):
     a `bevy::log::tracing_subscriber::fmt::layer()` writing to that file, no ANSI colours. Keep the default console
     output too (it simply goes nowhere in a release build with no console). If the file cannot be created, log to
     the console only and keep starting; never panic.
  4. Replace the launcher's own `println!`/`eprintln!` (handlers.rs ~140 "Mod dropped into launcher",
     engine_process.rs ~166 "Could not poll the engine process") with `info!` / `warn!` so they reach the file.
  5. Add CREATE_NO_WINDOW for the engine command on Windows (cfg(windows)), in `launch_engine` or `EngineProcess::spawn`.
  6. Document it in `docs/specs/modding/launcher.md`: release builds on Windows have no console; the log is at
     `%TEMP%\mudcrab-launcher.log` (and the engine's at `%TEMP%\mudcrab-engine-<pid>.log`, already documented or add it).
- No new dependencies: `tracing_subscriber` is re-exported by bevy (`bevy::log::tracing_subscriber`). If it is not
  reachable that way, stop and report instead of adding a dependency.

# Scope
Owned files: crates/launcher/src/main.rs, crates/launcher/src/engine_process.rs, crates/launcher/src/handlers.rs, crates/launcher/src/logging.rs, docs/specs/modding/launcher.md
- Change only the owned files (logging.rs is optional, a new module if you want one). No Cargo.toml changes.
- No git commands. Do not touch crates/converter or any other crate.

# Done when
Acceptance: cargo nextest run -p launcher
- Also run: `cargo fmt -p launcher -- --check` and `cargo clippy -p launcher --all-targets -- -D warnings`, and
  `cargo build --release -p launcher` must compile (proves the cfg_attr and the Windows-only code build).
- A unit test checks that the log file is created at `launcher_log_path()` and that a line logged through the layer
  (or the writer you build) ends up in it. Keep the test independent of other tests (use its own temp path if the
  global logger cannot be installed twice in one test binary; test the writer/layer builder function directly).
- Existing tests still pass, including the headless whole-launcher test that builds all schedules.

# Report
- What you changed and the decisions that weren't obvious (how the layer is built, what happens if the file fails).
- The commands you ran and what they printed (short).
- Anything that still writes to stdout/stderr and so is lost without a console (e.g. converter library `eprintln!`s).
