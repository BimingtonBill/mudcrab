# Goal
Track: Phase 2 (upstream, issue Mudcrab-Team/mudcrab#112, PR B "go through a load door"). Finish the parked
door crossing on branch `phase2/door-preload` so it is ready for review: clean clippy, the known open points
fixed, tests for each. No engine window runs; the lead takes the real-data evidence.

# Context
- Base: the commit named in "This run" (main merged in, incl. #89/#91 and `rust-toolchain.toml`). The WIP is
  commit "wip(engine): door crossing and fade (parked)": `crates/engine/src/door_crossing.rs`, `ActiveSpace`
  in `streaming.rs`, a `TeleportPlayer` block in `physics.rs`, registration in `app.rs`. Read its design in
  `C:/Modding/MudcrabProject/tasks/deepseek/impl-693-p2-door-crossing.md`.
- Own worktree on D:: `git -C C:/Modding/MudcrabProject worktree add D:/Mudcrab/worktrees-696 <base>
  --detach`, `CARGO_TARGET_DIR=D:/Mudcrab/builds/impl-696`. If rustc crashes with
  STATUS_STACK_BUFFER_OVERRUN, delete the target dir and use `-j 4`.
- Upstream style: commit subjects like `fix(engine): ...`/`feat(engine): ...`, no `[claude]`; each commit ends
  with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. New commits on top; never amend.
- `physics.rs` belongs to Phase 2 Dev's open work: change only the `TeleportPlayer` block.

# Do
1. `cargo clippy --workspace --all-targets --all-features -- -D warnings`: fix every warning in the branch's own
   lines (door_crossing.rs, doors.rs, the ActiveSpace/door parts of streaming.rs and database.rs, the teleport
   block, app.rs registration). If warnings remain in code main already had, list them; don't fix them.
2. Arrival height: XTEL arrival is the feet position. Make the teleport put the capsule's feet there (body
   origin vs feet offset from the capsule constants in physics.rs) and the camera at eye height. Test it.
3. Restore pose: if a render-origin rebase happens during the fade-out, the restore pose for a failed crossing
   must still be the same world place. Store it in Creation/world terms or account for the rebase. Test it.
4. Activation: a load door without `ExpectedModelBounds` must still be activatable (fall back to the bounds
   component the reference does carry, or a fixed small box around its origin; say which). Test it.
5. The INFO line per crossing: door form id, destination (cell id or worldspace + grid) and ms from E to
   fade-in start, so the lead can grep it.
6. Remove anything WIP-only (comments saying parked, debug prints).

# Scope
Owned: `crates/engine/src/door_crossing.rs`, `doors.rs`, `streaming.rs`, `world/database.rs`, `app.rs`, the
teleport block in `physics.rs`, `lib.rs`. Never `console.rs`, `crates/converter/**`, `crates/shared/**`.
Rules: never git push, commit named paths only, never kill engine.exe by name, no timing runs, no engine
windows, and NO background hold/wait/sleep commands: run builds and tests in the foreground with a long
timeout (up to 600000 ms); at most one background command for a job over 10 minutes.

Acceptance: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
(clean, or only pre-existing main warnings listed), `cargo nextest run --workspace --all-targets` (report
the known load-sensitive `streaming_fixture_loads_its_interior...` separately if it fails).

# Report
Commits, per item done/how, tests added, clippy and nextest results (exact counts), and the command line to
start the engine at Riverwood near Sven's house door (existing flags) for the lead's evidence run. Remove
the target dir when done; keep the worktree.
