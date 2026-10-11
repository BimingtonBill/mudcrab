# Goal
Independently check how upstream main's paced cell unloads and paced model spawns can be ported onto the portal
branch while keeping three required properties, so the result can be compared with another worker's recipe.
Track: Portal (port prep, read only).

# Context
- The branch under study is commit `0d426186` (the portal branch after its main merge). Your working folder is a
  different, older commit: do NOT read the working files. Read the branch with `git show 0d426186:<path>` and
  `git grep <pattern> 0d426186 -- <path>`. Upstream main: `git show origin/main:<path>` (a896b90). "HEAD" = 0d426186.
- Main paces streaming: cells leaving the streaming window are marked `Retiring` and despawned a few per frame
  (`despawn_cells`, budget `max_cell_unloads_per_frame`), and model spawns are armed a few per frame
  (`PendingModel`, `arm_pending_models`, budget `max_model_spawns_per_frame`). Files: crates/engine/src/streaming.rs,
  crates/engine/src/config.rs. The portal branch kept its own streamer (cell tiers `CellDetail`, door crossings that
  change worldspace or enter interiors, pre-streamed door destinations `PrestreamCells`, `ActiveCell`), so main's
  pacing is NOT on HEAD yet.
- Required properties (from the engine owner) any port must keep:
  (1) a retiring cell is never re-requested while its despawn is being spread out: it is revived, or its despawn
      finishes first;
  (2) a pending model spawn whose cell unloads is dropped (nothing left queued, nothing spawned into a dead cell);
  (3) the pacing fields of `StreamingMetrics` are really written (not left at default).
- A design question to judge: should unloads caused by a space change (leaving an interior, leaving a worldspace
  through a door) stay immediate, with only same-worldspace drift paced? Look at what HEAD does on a door crossing
  today and what could go wrong if such cells were paced (a left-behind interior still drawn, roots lingering for
  systems in portal.rs or lights.rs that treat "root present" as "cell resident").

# Scope
- Read only. You may run: `git show <rev>:<path>`, `git diff origin/main 0d426186 -- <path>`, `git log`, `git grep`.
- Do not edit or build anything.

# Done when
- For each property (1)-(3): how main guarantees it (origin/main path:line, and the main test that proves it), and
  what HEAD's streamer needs so a port keeps it (HEAD file:line of the places that would change).
- The design question answered with evidence.
- Every claim has file:line; uncertain points marked "unsure". Keep it short.

# Report
- (1), (2), (3), Design question, then "Risks" (max 5 bullets).
