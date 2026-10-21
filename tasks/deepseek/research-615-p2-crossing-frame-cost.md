# Goal
Track: Phase 2. Find out what the engine does on the frame where the camera crosses into a new
terrain cell while streaming (upstream `main`), where that frame's time goes, and propose two
concrete fixes: spreading the despawns over several frames, and rebasing the world origin without
unloading and reloading cells. Read-only; the output is a report another worker implements from.

# Context
- Checkout: `C:/Modding/wkj-phase2` is upstream `main` plus bundle merges; read the engine at
  upstream `main` with `git -C C:/Modding/wkj-phase2 show origin/main:<path>` (e.g.
  `origin/main:crates/engine/src/streaming.rs`), not the working tree. Engine code is in
  `crates/engine/src/` (streaming, world/, app.rs, the benchmark and frame-time code).
- Known so far (from earlier stress runs, not re-verified): a crossing despawns about 7 cells in one
  frame, and an origin rebase unloads and reloads cells. The stress scenario (radius 3, flying 5000
  units) had frame p95 ~12 ms on a quiet machine, with spikes on crossings.
- Timing data on disk (read-only): `C:/Modding/Mudcrab/worktrees/p2-tools/stress-ab/runs/` (per-run output of an
  interleaved A/B, including per-frame or per-system timing files if present) and
  `C:/Modding/Mudcrab/worktrees/p2-tools/acceptance-main-60s/` (acceptance scenario output with frame times).
  List the folders first; use what exists. If the per-frame data cannot tie spikes to crossings,
  say what instrumentation would (which spans, where).
- Bevy 0.19. Vendored sources, if you need an API's behaviour, are in
  `C:/Users/Brannock/.cargo/registry/src/*/bevy_*-0.19.*`.

# Scope
Read only. Write nothing except your report. Do not run cargo or the engine.

# Done when
- Every step of a crossing frame is listed in execution order with `file:line` at origin/main:
  what triggers it, what it despawns/spawns/loads, and whether it runs on the main thread.
- The cost of each step is given from data where the data allows, otherwise estimated from the
  code (entity counts, per-entity work), marked as measured or estimated.
- Two designs, each with the files and functions it changes, the invariant it must keep (no cell
  drawn twice, no hole in view, commit budget), the risk, and the test that would show it works:
  (1) spread despawns across frames under a budget; (2) rebase without unloads.
- A recommendation: which to do first, and the smallest first commit.

# Report
Sections: Crossing frame (ordered steps), Costs (table: step | cost | measured/estimated | source),
Design 1, Design 2, Recommendation, Open questions. Cite `file:line` for every code claim.
