# Goal
Round-2 fixes for three of our open upstream draft PRs (Mudcrab, Rust/Bevy 0.19), answering CodeRabbit findings. Verify each finding against the current code first. Fix it only if it is still valid. Skip it, with evidence, if the code already handles it or the finding is wrong.

# The three findings
1. **PR #121**, branch `phase2/arm-32-models`, worktree D:/Mudcrab/worktrees/p2-pacing (head e3cbc049).
   - CodeRabbit, review on cc861b7c, `crates/engine/src/app.rs` around line 435 (comment 4190147902): "Report when an existing IO pool cannot use the requested count."
   - The ask: in `engine::run`, check an existing pool via `IoTaskPool::try_get()` before reporting success. If its thread count differs from the requested `--io-threads`, return an actionable error; if the counts match, keep success.
   - Head e3cbc049 ("log the IO pool size as threads") may already address part of this, so check.
   - Read the comment in full: `gh api repos/Mudcrab-Team/mudcrab/pulls/comments/4190147902 --jq .body`.
2. **PR #124**, branch `phase2/console-coc`, worktree D:/Mudcrab/worktrees/p2-coc (head d21a554e).
   - CodeRabbit outside-diff comment on `crates/engine/src/cell_commands.rs:82-113`: "Prefer the active-worldspace exterior for `coc`."
   - The problem: the editor-id lookup uses an unordered `LIMIT 1`. When several CELL records share an editor id, it can pick an interior or another worldspace's cell, and `exterior_target` then rejects a valid destination.
   - The fix: prefer a matching exterior in the active worldspace, and keep the existing validation for names with no such exterior.
   - Read the review body: `gh api repos/Mudcrab-Team/mudcrab/pulls/124/reviews --jq '[.[]|select(.user.login|test("coderabbit"))]|last|.body'`.
3. **PR #125**, branch `phase2/physics-bodies-v2`, worktree D:/Mudcrab/worktrees/p2-physics (head 40950e8b).
   - CodeRabbit unresolved thread on `crates/engine/src/streaming.rs:1616` (comment 4190172074): "Sample more capsule directions before building the hull."
   - `shape_hull_points` adds only six points per capsule.
   - Read: `gh api repos/Mudcrab-Team/mudcrab/pulls/comments/4190172074 --jq .body`.

# Per finding
- Fix it with a focused test: one that fails without the fix where that is practical.
- One commit per PR, in upstream conventional style (`fix(engine): ...`), ending with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Commit path-only (`git add` named files). No amend, rebase or push. Never use `git stash`.

# Checks, per worktree, after its commit
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo nextest run -p engine --all-targets`

Use each worktree's existing build folder:
- #121: `--target-dir D:/Mudcrab/builds/p2-pacing`
- #124: `--target-dir D:/Mudcrab/builds/p2-console`
- #125: `--target-dir D:/Mudcrab/builds/p2-physics`

Read the folder's OWNER.txt first to confirm it is the right one.

The test `streaming_fixture_loads_its_interior_by_id_and_keeps_it_while_the_camera_crosses_exteriors` fails when the PC is busy. If it fails, rerun it alone and report both results.

# Rules
- Run `df -h /d` before each build. Below 15 GB free, stop and report.
- Run no background hold, wait or sleep commands. Builds run in the foreground.
- Do not run the engine binary.
- Do not touch any other worktree.

# Report (under 350 words)
- Per PR: the finding, fixed or skipped (with evidence), the commit sha, the file:line of the change, the test name, and the check results.
