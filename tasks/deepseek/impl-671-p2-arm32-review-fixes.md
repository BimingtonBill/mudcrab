# Goal
Apply the confirmed pre-open review fixes to the arm-32 branch. Track: Phase 2 (upstream issue #117).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/arm-32-models` (checked out, current upstream main already merged in; do not switch). Use the worktree's own `target/`.
- The branch raises `max_model_spawns_per_frame` from 4 to 32 and adds `--io-threads` (crates/engine/src/config.rs, crates/engine/src/app.rs `io_pool_threads`, docs/roadmap/02-profiling.md section "Load speed defaults").
- PR text: `C:/Modding/Mudcrab/mudcrab/local/pr-drafts/105-pr-117b-load-faster.md` (edit it too).

# Fixes (verify each against the code first; if one is wrong, skip it and say why)
1. Bound `--io-threads`: accept 1..=64; a value outside that range (or 0 given explicitly? keep 0 = automatic) is rejected the same way other malformed values are handled in config.rs, or clamped with a warning: pick the style config.rs already uses for out-of-range numbers and say which. Reason: an unbounded count can make Bevy panic spawning threads, and each IO thread reserves 128 MiB of address space. Unit test.
2. config.rs comment on the arming default: the line "15 models is the largest batch a frame can be handed at once" contradicts the backlog explanation (after a jump about 2,000 loaded models queue up). Rewrite the comment so it explains the backlog and why 32.
3. docs/roadmap/02-profiling.md: "the jump and the loading-window frame times come from a separate measurement branch" -> name the sibling PR as `#<measure PR>` (a placeholder Upstream Dev fills at open).
4. PR text (105-pr-117b-load-faster.md):
   - "model appearing" is really "load request to model ready": rename everywhere.
   - "did not lengthen the worst frame" applies to the jump window only (the fly-through went 20.5 -> 22.2 ms): say so.
   - Use exact ranges instead of rounded ones (for example 15.2-18.4 ms against 14.7-20.4 ms).
   - CPU contention is a likely cause, not a finding: word it so.
   - Add to For reviewers: `--io-threads` with a missing value swallows the next flag, the same pattern as every flag on main, fixed by #94.
   - Mention the new `--io-threads` bound.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass.
- One commit `fix(engine): bound --io-threads and fix the arming comment`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>` (PR text is outside the repo; just edit the file). Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, each fix done or skipped with reason, test count. Under 200 words.
