# Goal
Review the measurement commit for the engine pacing rework (step 1, upstream issue #117) as an outside reviewer would: find bugs, numbers that would mislead, and anything that changes engine behaviour when it should only measure.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/pacing-measure`, commit 1a7f6eb7 on upstream main 461517bf. Diff: `git -C D:/Mudcrab/worktrees/p2-pacing show 1a7f6eb7`.
- Intent: new benchmark report fields (`time_to_world_ready_ms`, the `--benchmark-jump` cell jump and `time_to_world_ready_after_jump_ms`, `fly_lag` distances at which models finish loading while `--auto-fly-speed` flies, `render_pipelines` counts per render frame), a fixed `streaming/terrain_mesh` span and new spans for references, terrain validation and seam welding. Measurement only: nothing about what loads, in what order or how fast may change.

# Questions
1. Behaviour: does anything change what the engine does in a normal run or an existing benchmark (system order, new resources required by existing systems, the readiness scan's results, the camera)? Cite file:line.
2. Correctness of each number: does `time_to_world_ready_ms` measure what it says (start time, the predicate's inputs being current on the frame read, a world that is "ready" too early because a counter lags a frame)? Can the jump's ready time be credited before the planner sees the new position? Is the fly-lag distance taken in the right space (render origin rebases)? Are the pipeline counts (`created`, `became_ready`, `waiting`) right across frames, and recorded on the matching render frame?
3. Coverage gaps: which spikes would these numbers miss (warm-up frames, the first load, frames outside recording)?
4. Robustness: panics, unbounded memory, division by zero, a run that never becomes ready.
5. Tests: do they prove the predicate and the counts, or could they pass with the feature broken?

# Scope
Read only. Change nothing. Only git read commands.

# Done when
Every question is answered with file:line at 1a7f6eb7.

# Report
Findings first (severity: blocker / should fix / nit, file:line, input then wrong result, fix), then one line per question. Under 700 words.
