# Goal
Phase 1 (converter) track. Make the converter's progress line standard and never silent: the same line format for a conversion and for `converter check`, a line that says what the run is doing, and a plain heartbeat line in log files and `--verbose` runs during quiet stretches. Why: on 2026-10-07 the user closed a conversion console because it looked idle during a long stage; a log file written during that stage showed no line for minutes, because nothing prints off a terminal until the next event arrives.

# Context
Base: `origin/main` (the worktree is made from it). Everything is in `crates/converter/src/`.
- `progress.rs`: `ProgressRenderer` turns `ProgressEvent`s into text. `update(event, elapsed)` prints on events, throttled (terminal 250 ms via `TERMINAL_REFRESH`, off a terminal `LOG_REFRESH` = 5 s, plus on every stage change). `tick(elapsed)` redraws between events, but only on a terminal: it returns `None` when `!self.terminal` or `self.verbose`. `line()` builds the status line: `Stage  NN%  [overall NN%]  N items/s  N MB/s  hh:mm:ss elapsed  ~hh:mm:ss left`. `format_clock`, `format_elapsed`, `ProgressEstimate` are shared helpers; the renderer is tested against a fake clock (see `mod tests`).
- `main.rs`: the printer task runs `renderer.update` on events and `renderer.tick` on a 250 ms timer (`ProgressRenderer::TERMINAL_REFRESH`), and writes with `write_status`. `run_check` (the `converter check` command) has its own ad-hoc line, `eprint!("\rChecking {done}/{total} files")`, shown only after 1 s and erased at the end. It calls `converter::check_output(output, mode, progress)` where `progress` is `Fn(done: usize, total: usize) + Sync` (called from several threads).
- `ProgressStage` and `ProgressEvent` are public and used by `crates/launcher` (it matches on `ProgressStage`). Do NOT add, remove or rename an enum variant or a public field, and do not change serde output.
- Stage weights and the LOD stage's 0.0 weight are out of scope.

Foundations check (the project's five points) for this brief: it fixes the cause (a log printed nothing between events), not the symptom (no slowing, hiding or capping of anything); it lives in the renderer, the one place every front end reads; cost is per refresh, not per asset; no new dependency, no terminal-size crate; nothing is hard-coded to Skyrim data; it never changes what the converter produces.

# What to build
1. **A line that says what it is doing.** Append the current activity to the status line: the file name (last path component) of `current_file` when present, else the event `message`. Keep the whole line to at most 110 characters by truncating this suffix (character-aware, not byte slicing; end a cut with `...`). No terminal-width detection and no new crate.
2. **No fake zero.** When the stage has reported no total and no bytes (`progress_fraction` has nothing to go on: `total == 0`, no `stage_fraction`, no bytes), print `--%` instead of `0%` and add the time spent in this stage (`hh:mm:ss in stage`), so a long countless stage visibly differs from a stalled one. Track the stage start in the renderer from the `elapsed` passed to `update`.
3. **Heartbeat when nothing happens.** `tick()` must also work off a terminal and in `--verbose`: when no line has been printed for `LOG_REFRESH`, print the plain log form `[h:mm:ss.s] <status line>\n` from the last event (same as `update` prints off a terminal), and reset the throttle. In `--verbose` the heartbeat is the same plain line. The terminal redraw behaviour stays as it is. `main.rs` already ticks every 250 ms, so only the renderer changes there; check that the printer task needs no edit for this.
4. **`converter check` uses the same line.** Replace the ad-hoc `Checking n/m files` line in `run_check` with the standard line. Add to `progress.rs` a small public type or function the check path can drive with `(done, total)` and the elapsed time and that produces exactly the standard format (percent, rate, elapsed, `~left`, the `what` = `Checking <done>/<total> files`) with a terminal redraw on a terminal and the heartbeat log line off one. Share the line-building code with `ProgressRenderer::line` rather than copying it (extract a helper both call). No new `ProgressStage` variant. The check's callback runs on several threads: keep the lock held only while formatting, and never block a worker. Keep the current rule that a fast check (under 1 s) prints nothing, and that the line is erased before the report prints (on a terminal), exactly as today.

# Scope
Owned files: crates/converter/src/progress.rs, crates/converter/src/main.rs
- Change only the owned files. A file changed outside the list fails the scope check.
- No new or upgraded dependencies. No git commands.
- Keep existing behaviour and existing tests that still describe real behaviour; update an existing test only where the line format deliberately changed, and say which in the report.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --bin converter --lib, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p converter --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo check -p launcher --all-targets, cargo fmt --all -- --check
- Run them yourself before you stop. `extraction_does_not_wait_for_a_front_end_that_stopped_reading` is a known flaky test: rerun once before calling it a failure.
- New tests with the fake clock, written to catch the change being wrong: (a) off a terminal with no events after the first, `tick` prints nothing before `LOG_REFRESH` and exactly one line at `LOG_REFRESH`, and none again until another `LOG_REFRESH`; (b) the same in `--verbose`; (c) a stage with no total prints `--%` and the in-stage time, a stage with a total prints a percentage; (d) a long file name or message is truncated to the 110-character cap on a char boundary (include a multi-byte name); (e) the `check` line has the same fields as the conversion line; (f) no line ever carries a stale `\r` in log mode.
- A short note at the top of `ProgressRenderer`'s doc comment says the line is the one standard line for a conversion and for check.

# Report
- What you changed and the decisions that were not obvious (the helper you extracted, the truncation rule).
- The commands you ran and what they printed (counts of tests passed).
- A sample of the new line in each mode (terminal, log, verbose, check), copied from a test's output.
- What you assumed, and the cheapest check that would show the change is wrong.
