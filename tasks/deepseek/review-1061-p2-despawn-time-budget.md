# Goal
Review impl-1059, which time-budgets the crossing despawn (Mudcrab engine, Rust/Bevy 0.19, track Phase 2). Decide accept or fix first.

# Context
- Worktree: D:/Mudcrab/worktrees/p2-impl-1059, branch `phase2/despawn-time-budget`, cut from portal 667483f2. Review `git diff 667483f2..HEAD` plus any uncommitted changes (`git status`, `git diff`).
- The brief, which is the contract: C:/Modding/Mudcrab/mudcrab/tasks/impl-1059-p2-despawn-time-budget.md. Measured problem: local/research/portal-crossing-owner-reprofile-2026-10-08.md §3, §4, §7.1.
- Files: crates/engine/src/streaming.rs (`despawn_cells` around :1471, `retire_backlog_bound` around :1441-1506, `despawn_subtree`, the RenderLayers stamp around :2533, `PortalPrewarm`), config.rs (`max_cell_unloads_per_frame` :218 and the new `max_unload_ms_per_frame` with its CLI flag), profiling.rs.

# Scope
Read only; change nothing. You may run git read commands and the gated checks:
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets` (with BUILD_GATE_OWNER=p2, and `df -h /d` first; stop if D: has less than 15 GB free).

# Done when
You have checked, citing file:line:
1. **Budget mechanics.** The elapsed clock is read after each `despawn_subtree` and at least one cell is unloaded per frame, so the backlog always drains. `0` keeps the old unbudgeted path exactly. Despawn stays exclusive and direct, so the measured cost is the real cost.
2. **The overflow escape.** It trips only when the backlog is above the bound AND did not shrink since the previous frame. A crossing burst that drains every frame never trips it. Sustained growth trips it within a frame or two. `retire_backlog_overflows` counts only escape trips, and a separate counter counts crossing-drain frames. Check edge cases: the first frame (no previous backlog), a backlog that stays flat at exactly the bound, and a budget that is too small to drain one cell.
3. **Determinism.** The brief claims "replay-stable (cell_order_key unchanged)". With a wall-time budget, the cells unloaded on each frame depend on machine speed. Say plainly whether the claim holds (the order is stable, the frame assignment is not), and whether anything relies on frame assignment (tests, timelines, multiplayer per-cell events).
4. **Visibility during the drain (the brief's required check).** Is a retired exterior cell, while it waits to be despawned, ever drawn after the crossing: on the interior's render layers, through a portal or window, or as LOD? Trace the RenderLayers stamp and anything that re-adds visibility. Is there an existing walked-run probe or screenshot proving it? If not, say exactly what probe is needed.
5. **PortalPrewarm.** A slower drain must not delay `PortalPrewarm::settled`, and there must be a test for that.
6. **Tests.** The four tests in the brief exist and would fail if the change were wrong (no tests that only confirm). The existing tests around streaming.rs:7288 and :7368 still pass.
7. **The default value** of `max_unload_ms_per_frame`, and where it comes from. The brief says it is picked from a pricing window that may not have run yet. If the default is a guess, it must be labelled as one and have a follow-up.

# Foundations check (answer explicitly)
What shortcut does this take, and what would break for a mod or another worldspace? Is a wall-time budget the right layer, or would a slower machine get a different world state that matters? Does any hard-coded number remain in the logic?

# Report
Status / Summary / Left / Next. Then: the verdict (accept / fix first / reject), the findings as `path:line - issue - fix` ordered by severity, the checks you ran with their Summary lines, and what you could not verify.
