# Goal
Review impl-1133 (commit 3001bbb1 on branch phase2/paced-lod-chunk-unload, base portal e1b6d05f) before it merges into portal: paced, hidden LOD-chunk retire through the cells' unload budget; option B `--lod-keep-hidden-in-interiors`; `release_layer_source` made a per-entity observer. Track: Phase 2 (A16). Read only.

# Context
- Worktree: C:/Modding/Mudcrab/mudcrab/local/impl/impl-1133-p2-paced-lod-chunk-unload. Diff: `git diff e1b6d05f 3001bbb1 -- crates` (config.rs, portal.rs, streaming.rs, streaming/lod.rs; ~1100 lines).
- Brief: tasks/deepseek/impl-1133-p2-paced-lod-chunk-unload.md. Coder's report: local/agents/runs/impl-1133-p2-paced-lod-chunk-unload.1/report.md (claims and unresolved points).
- Why: local/research/engine-foundations/analysis-1130-a14-crossing-trace-2026-10-09.md and analysis-1134-ui-components-on-world-entities-2026-10-09.md.

# Review questions (one verdict per question, file:line evidence)
1. Correctness of the retire state machine for chunks: hide on leave (RetireHidden), revive on return (same root, right status, visibility restored exactly), no double despawn, no leak (a chunk can never stay Retiring/Held forever without a path out), fail_chunk and track_lod_readiness interplay, worldspace changes.
2. One budget: cells and chunks share the unload budget and clock; cells first; can chunks starve cells or vice versa; does the count budget (`--max-unload-ms-per-frame 0`) still work.
3. Option B: Held chunks hidden and out of the queue; return shows them the same frame; a return to another worldspace retires them. The coder's single B run had `swap_wait_timeouts = 1`: read the code and say whether held chunks' terrain patches can trip the drawn bridge's in-view wait (swap_bridge.rs, streaming.rs invariant) and what the fix would be.
4. portal.rs: the per-entity observer is attached exactly where a node becomes a layer source, every such path; the global registration is gone; queue_handled is right; behaviour unchanged for source nodes; the test pins both halves.
5. Tests: do they test the claims (hide on retire, paced despawn under budget, revive, B keep-and-show), or only the happy path?
6. Foundations: what shortcut does this take, and what would break for a mod or another worldspace?
7. Anything that should be fixed before merge vs follow-ups.

# Report
Status / Summary / Left / Next, then a verdict per question (OK / fix first / follow-up) with evidence, and the list of must-fix items.
