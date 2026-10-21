# Goal
Review the #184 routes-and-frame-budget commits on local branch `phase2/perf-routes` (Mudcrab, Rust/Bevy 0.19), with special attention to whether the over-budget owner attribution can be trusted.

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-routes. Commits to review: `git diff 111ca6be..77fd3e10` (82233d10 feat(engine): follow a route file and report frame times against a budget; fae76d9b feat(scripts): add travel and check route scenarios; 77fd3e10 test threshold fix).
- New files: crates/engine/src/route.rs, crates/engine/src/frame_budget.rs, scripts/routes/travel.json, scripts/routes/check.json. Edited: app.rs, config.rs, metrics.rs, pacing.rs, render_timing.rs, lib.rs, scripts/phase2-baseline.ps1, docs/roadmap/02-profiling.md.
- Purpose: measure the goal "portal demo at a stable 160 fps (6.25 ms) while travelling"; pass bar 99.9% of frames <= 6.25 ms, none over 12.5 ms. Rules: no shortcuts (the route runner never waits for loading after the start, never slows or hides anything); the engine contains no route-specific knowledge; owners are never guessed.
- Smoke result (one repeat): travel route p99.9 15.8 ms, 2,109 over-budget frames, ALL attributed to render_thread (main_world 0, gpu 0, upload 0, unknown 0); check route 429 render_thread, 1 gpu.
- Suspicion to settle: an earlier standing-still CPU breakdown on main showed span render_world/main_world median 12.6 ms against render_world/render_thread 9.4 ms (local/perf-baseline/2026-10-06-main-final/baseline.md, "Slowest CPU spans"), so "main_world never owns a slow frame" looks wrong. Possible causes: the per-cycle filing of render frames under main cycles with pipelined rendering (render of extract k filed under cycle k+1); what "main schedule" time measures versus the existing render_world/main_world span; extract or commit subtraction; the swapchain acquire being counted as gpu; ties.

# Scope
Read only. Change nothing.

# Done when
Every place that computes a side's time or the owner is traced from the timing source to the report, with file:line; you say whether the 100% render_thread result is (a) plausible and why, or (b) a bug, with the exact faulty line and a fix; and you check the route runner against the rules above.

# Report
Verdict first (accept / fix needed), then findings as path:line - issue - fix, then what you verified, then what you could not verify.
