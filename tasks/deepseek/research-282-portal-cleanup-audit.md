# research-282 - Cleanup and noise-reduction audit of the portal catch-up branch

Track: portal. Read-only; change nothing.

## Goal
List what the cleanup pass should remove or fix on branch `portal-merge-main` (worktree
`C:/Modding/wkj-portal-merge`), now that upstream main is merged into the portal and four of main's
engine features were ported onto the portal's own streamer. Where both sides built the same feature,
keep one implementation; find dead paths, unused flags, leftover debug code, stale TODOs, stale docs,
and noisy logs.

## Scope
Read only: `C:/Modding/wkj-portal-merge` (crates/engine, crates/shared, crates/launcher, docs/,
scripts/, tools/). Do not read `local/`. Reference for what main has and why:
`C:/Modding/MudcrabProject/local/merge/port-notes.md`.

Known facts to build on:
- Kept from portal, not main: `streaming.rs`, `render.rs`, `terrain.wgsl`, `atmosphere.rs`, the portal's
  `PlayerPlugin`, render layers (main's placed-object layer was not taken).
- Ported from main: paced unloads + model spawns, terrain normal maps, a sky dome (`sky_dome.rs`, using
  main's `sky.rs` which is otherwise unused: `SkyPlugin`, `apply_sky`, `apply_fog`), rapier colliders +
  the player's rapier probe. Main's `WorldPlayerPlugin`, `PlayerControlsPlugin`, `MovementTuning`,
  debug tankards, `PhysicsFixturePlugin` are in `physics.rs` but gated off for portal runs
  (`WORLD_PHYSICS_PLAYER = false` in `app.rs`).
- Merge leftovers to check: `StreamingMetrics` fields that are never written; `streaming_center` vs
  `camera_grid`; `request_cell` + `pinned`; the `TerrainCollider` marker comment in `streaming.rs` that
  says colliders are not built (they are now); main's `--physics-fixture`, `interactive_world_physics`,
  `movement_tuning` code paths in `app.rs`; two `--shots` implementations? (`shots.rs`); duplicated
  helpers in `config.rs`; docs that describe main's behaviour the portal does not have, or portal
  behaviour main's merge made stale (docs/specs, docs/design, docs/roadmap).
- Model paths: portal's `converted_model_path` keeps the database capitalisation, main lowercases
  (`streaming.rs`), and the collider port lowercases locally. Say whether portal should lowercase like
  main and what that would touch.
- Run-time noise seen in tours: `LAND edge welded onto the resident neighbor` WARNs (a few per run),
  `no ground under the player: holding position` at start, `No mapping found for UUID` (gilrs).

## Web questions
none

## Done when
Every item has: file:line, what it is, evidence it is dead/duplicate/stale (who calls it, or not),
the proposed action (delete, merge into X, demote log to debug, rewrite doc section), the risk, and
an estimated line count. Group by: code (engine), code (other crates), docs, logs, tests.

## Report
A table per group, most valuable first, then a short recommended order. Under 300 lines. Cite
file:line for every claim. Do not propose new features.
