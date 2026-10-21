# Goal
Map every piece of per-frame work the engine does to stream the world, and every budget, queue and limit that paces it, on upstream main. This is the factual base for a plan to rework the engine's pacing (how streaming work is spread across frames so the game never stutters).

# Context
- Rust, Bevy 0.19, bevy_rapier3d 0.36. Engine crate: `crates/engine/src/` (streaming.rs, app.rs, config.rs, metrics.rs, profiling.rs, render_timing.rs, render.rs, lights.rs, physics.rs, world/database.rs, world/cache.rs). Read the checkout you are given as upstream main.
- Known pacing pieces (confirm and find the rest): a database worker thread answering cell requests through a bounded queue (`try_request`); a cell commit time budget (`--max-commit-ms`); paced cell unloads (`--max-unloads-per-frame`, a `Retiring` state); paced model spawns (`--max-model-spawns-per-frame`, `PendingModel`); a per-frame GPU upload budget (`--max-upload-mib-per-frame`); an asset readiness scan; a light budget; render-origin rebasing.
- Known problems to explain from the code: the remaining worst frames during a cell crossing are cell commits (the main world system around 11-12 ms); when flying fast the model-arming backlog reaches thousands and models arrive seconds late; screenshot and settle gates can fire while uploads are still queued, because nothing counts that backlog; budgets are fixed counts, not frame-time based; queues are served oldest-first, not nearest-first.

# Questions
1. A table of every stage from "the camera moves" to "the object is drawn": stage, system name and file:line, schedule and thread (main world, render world, worker thread, async task pool), what bounds it per frame (count, bytes, milliseconds, nothing), its default and CLI flag, and the metric or profiler span that measures it.
2. Which stages have no bound at all, and which are bounded only by a count whose cost per item varies widely (cite why: e.g. a cell with 2,000 references vs 20).
3. Ordering and priority: for every queue, what order it is served in, and whether distance to the camera, visibility or "needed to stand on" (terrain and colliders under the player) affect it.
4. Feedback: does any budget read the actual frame time or the measured cost of the work it lets through? Which gates (readiness, settle, acceptance screenshot, `--shots`, benchmark start) decide "the world is loaded", and which backlogs each one counts or misses.
5. Main-thread work that could move to worker threads or async tasks (mesh building, terrain, collider building, decoding), with what blocks moving it (ECS access, Bevy asset API).
6. The configuration surface: every pacing flag and default, and which runs (interactive, benchmark, acceptance, shots, fixtures) change them.

# Scope
Read only. Change nothing. Read files and run only git read commands.

# Done when
Every question has an answer with file:line citations. Anything you could not determine is listed with what would settle it (a measurement, a profile).

# Report
The stage table first, then one short section per question, then the five changes you think would most reduce the worst frame, each tied to a cited cause. Under 1,800 words.
