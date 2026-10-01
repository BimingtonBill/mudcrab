# Goal
Track: portal. Do the cleanup and noise-reduction pass on branch `portal-merge-main` that the
research-282 audit lists, except its items 7 and 9 (main's gated `WorldPlayerPlugin` wiring and the
unused `SkyPlugin` half of `sky.rs` stay: deleting them only adds conflicts to the next merge of main).

# Context
- Audit (read it first, it has file:line for everything):
  `C:/Modding/wkj-portal-merge/local/agents/runs/research-282-portal-cleanup-audit.1/report.md`.
- Work in your own worktree on D: (C: is nearly full):
  `git -C C:/Modding/wkj-portal-merge worktree add D:/Modding-overflow/wt-284 portal-merge-main --detach`,
  and build with `CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/impl-284`. Commit there with prefix
  `[claude]`, one commit per group below, each green. Never edit `C:/Modding/wkj-portal-merge` itself.

# Scope (items by the audit's numbers)
1. Logs: 2 (demote the WALK speeds report to `debug!`), 3 (demote the LAND weld WARN to `debug!`;
   keep the "past the weld bound" one at `warn!`), 17a (`no ground under the player` to `info!`),
   17b (LogPlugin filter that drops the `gilrs` target).
2. Stale comments/tests: 4 and 18 (the gate's real reason is that main's capsule player knows nothing
   of doors; rename the test accordingly).
3. Item 1: `converted_model_path` lowercases the whole path like main; drop the collider port's local
   lowercasing; fix the comments; add the mixed-case output assertion from item 19.
4. Small dead code: 5, 6, 8 (for 8: delete the `FogCamera` inserts and import).
5. Item 10 (one `is_safe_relative_asset_path`). Item 11 only if it stays small (one `pub fn`
   resolver the bin can call); skip 12.
6. Docs: 14, 15, 16 as short banners/status lines.
Owned files: the files those items name in `crates/engine/src/` and the three docs. Nothing else.
Do not touch `player.rs` doorway code or the streaming pacing/arming logic beyond the lines named.

Acceptance (run in your worktree): `cargo fmt --all -- --check`,
`cargo clippy -p engine --all-targets -- -D warnings`, `cargo nextest run -p engine`.

# Done when
Acceptance passes on the last commit; every existing test still passes (list any you changed and why).

# Report
Commit list (hash, subject), per-item done/skipped with reason, tests changed, line delta. Then remove
`D:/Modding-overflow/wt-targets/impl-284` (keep the worktree for the lead to cherry-pick).
