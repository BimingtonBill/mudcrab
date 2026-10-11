# impl-903-p3-engine-exit-message (track: Phase 3, upstream PR "L4")

**Worker:** Claude subagent ("Claude impl #903"). **Worktree:** `C:\Modding\wkj-phase3-engine`,
branch `phase3/engine-exit-message` (from `main` 6f583cd). **Target dir:**
`CARGO_TARGET_DIR=D:/Mudcrab/phase3-target-l1` (shared with the resume branch, same base).

## The problem

The launcher's Play (`crates/launcher/src/handlers.rs`, `launch_engine`) spawns the engine with
`--assets <dir>` and forgets the child. When the engine refuses to start (stale or incomplete data,
a bad option), it prints why on stderr and exits, and the user sees nothing: the window just says
"engine started". Two pending engine branches make those messages good (a schema-mismatch message
with the exact reconvert command; bad options exit with code 2 and a message on stderr), so the
launcher only has to show them.

## The change (launcher-only; no other crate)

1. Spawn the engine with `stderr` piped (stdout left as is, or inherited). Keep the `Child` in a
   resource.
2. A system polls `try_wait()` each frame (cheap). While the engine runs, the status says so.
   A reader thread drains stderr into a bounded buffer (keep the last ~40 lines or ~8 KB) so a
   chatty engine never blocks on a full pipe.
3. When the engine exits:
   - exit code 0: "OpenSkyrim engine closed." and drop the buffer;
   - non-zero, or killed: "The engine stopped (exit code N):" followed by the last stderr lines
     (trimmed; the last ~12 lines is enough), in the launcher's status text.
   Show the text as-is: do not match on engine wording (it may change).
4. Play is disabled (or ignored with a message) while an engine started from the launcher is still
   running, so two engines don't start by accident.
5. Keep main's code style (read `main.rs`, `handlers.rs`, `ui.rs` first). No new dependencies.

## Tests

- Pure functions for the message: exit 0, exit 2 with stderr lines, a very long stderr (only the
  tail kept), an empty stderr, a kill with no code.
- The bounded buffer: keeps the tail, never grows past the bound.
- If practical, a test that spawns a real short child process (e.g. the test binary itself or
  `cmd /C exit 2` behind `cfg(windows)` plus `sh -c` otherwise) and checks the exit text. Skip this if
  it gets flaky.

## Checks (D: target dir)

```
cargo fmt --all -- --check
cargo clippy -p launcher --all-targets -- -D warnings
cargo nextest run -p launcher
```

## Commit

Upstream style, e.g. `feat(launcher): show why the engine stopped when it exits early`. Only
`crates/launcher/`. Then write `docs/handoff/impl-903-p3-engine-exit-message.md` in
`C:\Modding\Mudcrab\mudcrab` (not committed).
