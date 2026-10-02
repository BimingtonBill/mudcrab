# Goal
Track: Phase 2 (upstream #112 door-crossing draft). Fix the confirmed findings of the pre-open reviews (DeepSeek
#700, MiMo #701) on branch `phase2/door-preload`. One round; keep each fix small.

# Context
Base: the commit named in "This run". Own worktree on D: (`git -C C:/Modding/OpenSkyrimProject worktree add
D:/Modding-overflow/wt-702 <base> --detach`, `CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/impl-702`).
Upstream style (`fix(engine): ...`), trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`, new
commits only. Owned: `crates/engine/src/door_crossing.rs`, `doors.rs`, `world/database.rs`, `streaming.rs`
(only lines this branch added or the stale comment below). Never physics.rs (its teleport block is frozen: an
identical copy lives on another open branch), console.rs, crates/converter, crates/shared.
Rules: no git push; commit named paths; NEVER `git stash` (stashes are shared by every worktree: use `git diff
<base>` or a second worktree); no engine windows; no background hold/wait/sleep commands (foreground builds,
timeout up to 600000 ms); if rustc crashes with STATUS_STACK_BUFFER_OVERRUN delete the target and use -j 4.

# Fix (confirmed by the lead)
1. **The readiness gate is global, not the landing cell's (MiMo 2; real-data evidence).** The hold waits for
   `StreamingMetrics.pending_asset_instances/pending_surface_instances == 0`, counts over every streaming cell,
   so an exterior landing waits for the whole stream window and hits the 10 s timeout (Sven's house -> Riverwood:
   `WARN ... the destination was not ready in time` after 10.0 s, `D:/Modding-overflow/door-evidence-run3/run.log`).
   Scope it to the landing cell: nothing pending UNDER THE LANDING ROOT (pending model / pending asset / pending
   surface components on its descendants: find the marker components the streamer uses), plus the existing
   asset-load check under that root. Test: pending work in another cell does not hold the fade-in; pending work
   under the landing root does.
2. **Restore from an interior (both reviews).** `Restore.feet` adds the render-origin offset in every space, but
   interiors use absolute coordinates (origin unused while an interior is active), so a failed crossing that
   started inside an interior restores the player off by origin x 4096. Store/use raw feet for an interior
   start. Test: a failed crossing from an interior with a non-zero render origin lands where the player stood.
3. **One line per crossing names where the player landed (DeepSeek 3, MiMo 7).** On the restore path the INFO
   line names the restored place (and says `restored`), and a failed restore still emits the line.
4. **Settle frames off by one (DeepSeek 7):** the fade-in starts after exactly `SETTLE_FRAMES` loaded frames.
5. **Failed assets (DeepSeek 6):** keep `Failed` as done, but count failed assets under the landing root and
   WARN once with the count at fade-in.
6. **Timeout diagnosis:** the timeout WARN also says what was still pending (resident? pending components count
   under the root, first not-loaded asset path if any).
7. **NULL arrival columns (MiMo 8):** a `door_links` row with NULL position/rotation is no link (logged once at
   debug), never an error that fails the cell. Test.
8. **Comments (MiMo 6, 7):** the stale "no runtime path unloads one yet" comment in streaming.rs; the fallback
   box comment ("at the hinge" vs a centred ±80 box).

Acceptance: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo nextest run --workspace --all-targets`, `cargo test --workspace --doc` (exact results in the report).

# Report
Commits, per item what changed, tests added, exact acceptance results. Remove the target dir; keep the worktree.
