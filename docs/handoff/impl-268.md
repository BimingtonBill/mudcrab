# impl-268 - Streaming audit leftovers (portal)

Written by a Claude subagent for Portal Dev. Track: portal. Findings closed: ES-20260928-01,
ES-20260926-03, ES-20260926-02 (`local/agents/memory/findings.jsonl`).

## 1. `has_failed` and the interior retries (ES-20260928-01)

`StreamingWorld` now remembers the interior the plan last streamed for (`active_interior`, set at
the top of `plan_cells`, before the camera check). `has_failed` reads a `Failed { Full }` cell as
failed only when `retry_failed_interior` owes it nothing, so the interior the camera stands in is
waited for through its `INTERIOR_FAILED_RETRIES` retries and is failed for good once the last one
fails. `retry_failed_interior` takes `Option<u32>` (the active interior) instead of `&ActiveCell`,
so the plan and `has_failed` ask the same question the same way. Exteriors and interiors the camera
is not in are unchanged: failed at the first failure. The contract comment on `has_failed` says so.

Callers (`shots::settle_counts`, `transition::destination_is_loaded`) are untouched. Note for
whoever owns transition.rs: its doc comment on `destination_is_loaded` still says a failed cell is
one "which streaming never retries"; with this change that is exactly true of what `has_failed`
reports, but the wording could mention the interior retries.

Tests: `a_retried_interior_is_not_failed_until_its_retries_run_out`,
`an_exterior_full_failure_is_failed_at_once`.

## 2. One owner for the grid-to-render offset (ES-20260926-03)

`streaming::grid_translation(grid, origin)` is the one place the formula is written. Read other
ways: `cell_translation` and `reposition_cell_roots` (cell roots), `update_render_origin` (the
camera's rebase shift), `render_position` (Creation to render: subtract the origin's translation),
and two new inverses, `absolute_render_position` (render back to the absolute Bevy frame) and
`camera_grid` (the grid a render point stands in; `plan_cells` uses it). Routed through them:
`plan_cells`, `demo_tour::camera_space_key` (was `grid_of` on a hand-written offset),
`pose_capture::creation_position` and `app::drive_streaming_fixture`. No behaviour change: every
rewrite is bit-identical except the sign of a zero `y` (`-0.0 + 0.0`).

Copies left, in files this task did not own:
- `portal.rs` `ActiveSpace::camera_grid` (~932-937): a copy of `streaming::camera_grid`.
- `portal_bench.rs` `camera_cell` (~772-775): `grid_of` on the offset; `streaming::camera_grid`.
- `transition.rs` `creation_from_render` (~1004-1008): the exterior arm is
  `absolute_render_position`.
- `world/components.rs` (~29-40): a grid-relative position helper with its own (y-up, Creation-axis)
  offset; not the render mapping, so left alone.

Test: `the_grid_offset_and_its_inverse_agree_at_a_non_zero_origin`.

## 3. The Full commit and the frame's time budget (ES-20260926-02)

Deferring a full commit is safe: a held answer goes to the front of the next frame's queue with
everything behind it in order, so it is the next frame's first commit. The decision is now one
function, `commit_waits_for_next_frame`: a per-tier count (full: `max_cell_commits_per_frame`;
terrain: `MAX_TERRAIN_CELL_COMMITS_PER_FRAME`) and, for **both** tiers, the time budget - no commit
starts once `max_commit_micros_per_frame` is used up, **except the first commit of a frame**, which
the time budget never holds. "First" counts commits *started* (`commits_started`), including ones
that failed validation or found an empty ring grid, so a run of cheap failures still counts against
the budget as before.

Behaviour change beyond the brief, on purpose: the terrain arm also gets the first-commit exemption.
Before, a terrain answer at the head of the queue was held whenever draining the worker's answers
alone had used the budget (or the budget was 0), and could be held every frame. With the default
16.67 ms budget this never happened in practice.

The budget still bounds when commits *start*, not when they finish: the last commit to start under
budget can overrun it, and that frame is still counted as a violation (`commit_budget_exceeded`).
The doc comment on `commit_waits_for_next_frame` states that exactly.

Tests: `a_full_commit_waits_when_the_frame_budget_is_spent_before_it_starts`,
`the_first_commit_of_a_frame_is_never_held_by_the_time_budget`.
