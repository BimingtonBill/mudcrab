# impl-903-p3-engine-exit-message (Phase 3, upstream PR "L4")

**Worker:** Claude impl #903. **Branch:** `phase3/engine-exit-message` in `C:\Modding\wkj-phase3-engine`,
commit `bd26dfc` on main `6f583cd`. Only `crates/launcher/` changed.

## What changed

- New `crates/launcher/src/engine_process.rs`:
  - `EngineProcess` resource (the `Child` plus the log path);
  - `EngineProcess::spawn(command, log)`: stdout and stderr go to one log file,
    `std::env::temp_dir()/openskyrim-engine-stderr.log`, truncated at each launch. stdout uses
    `File::try_clone`. If the clone fails, stdout is inherited; if the file can't be created,
    both streams are inherited and the message says "without an error message".
  - `poll()` uses `try_wait()` and never blocks. On a non-zero exit it reads the last 8 KB of
    the log (`read_log_tail`), dropping a partial first line, and runs every line through
    `clean_line` (ANSI codes stripped, trailing whitespace trimmed, cut at 300 characters).
  - `engine_exit_message(code, lines, log)`:
    - exit code 0: "OpenSkyrim engine closed.";
    - other codes: "The engine stopped (exit code N):", the last 12 lines, then
      "Full log: <path>" (only when there are lines to show);
    - negative codes also appear in hex (Windows NTSTATUS crash codes);
    - no code at all: "no exit code; it was killed".
    - The engine's text is shown as-is; nothing matches on its wording.
- `handlers.rs`:
  - `launch_engine` stores the `EngineProcess`, and returns at once if one already exists.
  - New `watch_engine_process` system (registered in `main.rs`) removes the resource on exit and
    shows the message.
  - `handle_play_button_click` ignores Play while the engine runs and says so.
  - `show_status` writes the status text directly, because `sync_status_text` skips its update
    once a conversion has run (the `ConversionChannel` resource is never removed). On main, that
    already hid the old "engine started" message after a conversion.
- Relaunching after the engine exits works: `NextState::set` re-runs `OnEnter` for the same state
  (checked in the bevy_state 0.19.1 source).

## Design changes asked for by the lead during review

The first version piped stderr through a reader thread. That was replaced by the log file, because
a pipe ties the engine's life to the launcher: after the launcher closes, a failed stderr write
makes `eprintln!` panic. stdout was then sent to the same file for the same reason.

## Checks (CARGO_TARGET_DIR=D:/Modding-overflow/phase3-target-l1)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p launcher --all-targets -- -D warnings`: clean.
- `cargo nextest run -p launcher`: 15/15 pass. The 12 new tests cover:
  - the message for exit 0, exit 2 with log, a long tail, trailing blank lines, empty output, a
    kill and a hex crash code;
  - `clean_line`;
  - the log-tail reader, on a 2,000-line file and on a short file;
  - a real child (`cmd /C` on Windows, `sh -c` elsewhere) writing to stdout and stderr, which
    checks their order in the file and that the file is truncated;
  - the fallback when the log folder is missing.

## Not done / open

- Not run against a real engine, since that needs the engine branches with the new messages. A
  manual check: start the launcher with stale data, press Play, and read the status text.
- A second launcher instance would truncate the log file while the first instance's engine is
  still writing to it (edge case, not handled).
- Long engine output makes the status text tall, up to 12 lines plus the log line. The UI has no
  scrolling.
