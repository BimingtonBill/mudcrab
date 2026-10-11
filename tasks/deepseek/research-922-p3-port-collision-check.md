# Goal
Independently check how upstream main's world collision (rapier terrain and static colliders) can be ported onto the
portal branch without breaking its doors and its pre-streamed door destinations, so the result can be compared with
another worker's recipe. Track: Portal (port prep, read only).

# Context
- The branch under study is commit `0d426186` (the portal branch after its main merge). Your working folder is a
  different, older commit: do NOT read the working files. Read the branch with `git show 0d426186:<path>` and
  `git grep <pattern> 0d426186 -- <path>`. Upstream main: `git show origin/main:<path>` (a896b90). "HEAD" = 0d426186.
- Main builds rapier colliders for terrain and static objects while streaming (crates/engine/src/streaming.rs) and has
  a physics player (`WorldPlayerPlugin`, crates/engine/src/physics.rs). HEAD keeps its own camera-as-player
  (crates/engine/src/player.rs: walking by mesh ray casts, E to open doors, auto-load doors that cross on contact,
  door leaves solid until open), door crossings (crates/engine/src/transition.rs), and a portal (crates/engine/src/portal.rs)
  that pre-streams the cell behind a door and renders it on its own render layer (`DESTINATION_LAYER`) with a second
  camera, while the player stands in the active space. HEAD gates main's physics player off (app.rs, `WORLD_PHYSICS_PLAYER`).
- The other worker recommends: keep HEAD's player and give it a rapier ray-cast probe; add terrain and static colliders
  only for full-detail cells; disable colliders of cells that are not in the active space; shift rapier bodies on the
  render-origin rebase. Check these independently; do not assume them.
- Questions, with file:line evidence on both sides:
  1. Door leaves and auto-load doors: how does HEAD decide a door leaf is solid or passable, and how do auto-load doors
     fire on contact? Would rapier colliders on DOOR references (or their absence) break either? Does main ever build
     colliders for DOOR records?
  2. Pre-streamed destination cells: where are they placed in world space relative to the active space, and what marks
     their role (portal.rs)? If their colliders were enabled, could they block the player or a ray? What is the single
     source of truth for "this cell is in the active space", and in which schedule/order is it written relative to the
     crossing (transition.rs) and the player's movement?
  3. Render-origin rebase: what does HEAD's rebase move (streaming.rs, `update_render_origin`), and what does main do
     for rapier bodies on a rebase (fixed bodies do not follow transform teleports)? Which main test pins it?
  4. Ring cells (`CellDetail::Terrain`): should they get colliders? What happens underfoot during a Terrain->Full upgrade?

# Scope
- Read only. You may run: `git show <rev>:<path>`, `git diff origin/main 0d426186 -- <path>`, `git log`, `git grep`.
- Do not edit or build anything.

# Done when
- Questions 1-4 answered with file:line (HEAD) and/or origin/main path:line; uncertain points marked "unsure". Short.

# Report
- Answers 1-4 as lists, then "Risks" (max 5 bullets).
