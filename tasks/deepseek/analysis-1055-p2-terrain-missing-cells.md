# Goal
Find the root cause of exterior terrain (ground) cells that sometimes do not load or are not drawn, in Mudcrab's engine (Rust, Bevy 0.19) on main. The user has seen it for a while and thinks it started when streaming pacing landed (paced unloads/spawns: CellStatus::Retiring, PendingModel, the despawn and spawn budgets, the upload byte budget). Track: Phase 2. Read and analyse only; propose a fix, do not write one.

# Evidence so far
- Two `--shots` runs of the same poses (Riverwood, assets C:/Modding/Mudcrab/SkyrimConverted-p2main-v24) with a build that renders frames faster than main: about 6% of pixels differ between runs, and the differences are holes in the ground through which the water plane and the sky show. Both runs reported "settled" at the same frame count (601 and 160), which suggests the capture rule counts frames instead of waiting for terrain.
- Main's own build gave identical shots twice, but the user has seen missing ground in normal play too, so it is not only the shot timing.
- Known separate cause: the terrain edge-weld validator drops whole cells whose shared LAND edges differ by design (Markarth 15, Solitude 14 cells): local/agents/runs/research-552-p2-city-land-validation.1/report.md.

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-main (detached at origin/main f40ca1df). Read only.
- Read first: local/research/engine-foundations/analysis-1034-terrain-path-map.md (the terrain path), local/research/port-prep-pacing.md and local/agents/runs/research-921-p3-port-pacing-check.1/report.md (the pacing design: Retiring, PendingModel, budgets), local/research/pacing-prior-art.md.
- Code: crates/engine/src/streaming.rs (cell states, collect_cells, spawn_cell, despawn and retire paths, terrain validation and weld, terrain readiness), streaming/lod.rs (LOD tiles hidden by ready full-detail terrain: can a tile be hidden while its full-detail terrain is not drawn?), the upload throttle (max_upload_bytes_per_frame, RenderAssetBytesPerFrame), crates/engine/src/shots.rs (what "settled" waits for), app.rs, config.rs.

# Scope
Read only. Change nothing. Run no cargo and no engine.

# Done when
You have traced, with file:line, every path by which an exterior cell inside the stream window can end with no visible terrain while its LOD tile is hidden or absent: (a) the cell never requested or dropped by a budget; (b) Retiring/revive races (a cell retiring while re-requested; request_cell reviving a pinned Retiring cell); (c) terrain spawned but invisible (visibility gated on readiness that never completes: textures, normal maps, the upload budget, collider enable); (d) weld or validation failures dropping a cell; (e) LOD hiding a tile before its full-detail cell is drawn, or showing neither; (f) what --shots "settled" means and whether it waits for terrain at all. For each path: whether it can happen, how likely it is, and the log line or metric that would show it.

# Foundations check (answer explicitly)
The fix you propose must make terrain completeness explicit (no frame counts, no sleeps), keep pacing (we do not remove budgets to hide the bug), and work for any worldspace and for mods. What shortcut would a quick fix take here, and why is it wrong?

# Report
Status / Summary / Left / Next, then: the ranked list of causes with file:line evidence, the instrumentation to add to confirm the top ones (exact counters or log lines), the proposed root fix per cause, and what you could not verify. Write your findings also to local/research/engine-foundations/analysis-1055-terrain-missing-cells.md.
