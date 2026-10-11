# Goal
Independently map how to port upstream main's terrain normal maps onto this branch (the portal branch after its
main merge), so the result can be compared with another worker's recipe. Track: Portal (port prep, read only).

# Context
- The branch under study is commit `3570acb8` (the portal branch after its main merge). Your working folder is a
  different, older commit: do NOT read the working files. Read every file of the branch with
  `git show 3570acb8:<path>` and find things with `git grep <pattern> 3570acb8 -- <path>`. Upstream main is
  `origin/main` (a896b90): `git show origin/main:<path>`. Below, "HEAD" means 3570acb8.
- The portal branch merged main but deliberately kept its own engine terrain/streaming code, so main's terrain
  normal-map feature is NOT active here yet. Files: crates/engine/src/render.rs (TerrainExtension, TerrainSettings,
  from_quadrant), crates/engine/src/shaders/terrain.wgsl, crates/engine/src/streaming.rs (spawn_cell, surface
  readiness / validation), crates/engine/src/world/database.rs (landscape_normal).
- Questions to answer with file:line evidence on BOTH sides (HEAD and origin/main):
  1. Which parts of main's terrain work are already on HEAD (e.g. the vertex weight field, `for_quadrant`,
     `vertex_weights_only`, `grid_weight` in the shader)? Which normal-map parts are missing?
  2. The exact missing pieces: shader uniform fields and their order (layout must match the Rust struct),
     texture bindings numbers, the normal decode (green channel sign) and TBN, the Rust `TerrainSettings` fields,
     `set_normal_layers`, `drop_normal_maps`, the `from_quadrant` return type, where normals are loaded and with what
     sampler / colour space (sRGB must be off for normal maps).
  3. Streaming: how main waits for normal textures and what it does when one fails to load (fallback, counters);
     where HEAD's equivalent functions are and what would have to change.
  4. Portal-only behaviour on HEAD that a port must not overwrite (look at `from_quadrant` slot assembly,
     base-texture borrowing, `TerrainExtension::fixture`). Name each with file:line.
  5. Tests on main that cover normal maps, and HEAD tests that call the functions whose signatures would change.

# Scope
- Read only. You may run: `git show origin/main:<path>`, `git diff origin/main 3570acb8 -- <path>`, `git log`, `git grep`.
- Do not edit or build anything.

# Done when
- Every claim has file:line (HEAD) and/or origin/main path:line. Uncertain points marked "unsure".
- Keep it short: one list per question.

# Report
- Answers 1-5 as lists, then "Risks" (max 5 bullets): things a coder would most likely get wrong.
