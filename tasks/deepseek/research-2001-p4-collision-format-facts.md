# Goal
Collect the code facts a design note on "binary prebuilt collision" needs: how collision shapes
travel today from the converter through the GLB to Rapier colliders in the engine, what that
costs, and which profiling spans already cover it. Read-only: change no files.

# Context
- Mudcrab: Rust/Bevy Skyrim SE reimplementation. Checkout `C:\Modding\Mudcrab\mudcrab`, branch
  `portal` (our demo track). Upstream state is `origin/main` (read with `git show origin/main:<path>`,
  `git grep -n <term> origin/main -- <paths>`). Report facts for BOTH `portal` (working tree) and
  `origin/main` where they differ.
- Known starting points (verify, line numbers may have moved):
  - `crates/shared/src/collision.rs` (`COLLISION_ASSET_VERSION`, `CollisionAsset`, `CollisionBody`,
    `CollisionShape`, `BodyKind`).
  - Converter writer: `crates/converter/src/` (search `openSkyrimCollision`, `CollisionAsset`).
  - Engine reader and collider build: `crates/engine/src/streaming.rs` around the old lines
    4033-4146 (JSON parse, `Collider::convex_hull`, trimesh, per-model cache), terrain colliders
    (`terrain_collider_from_mesh`), `StaticColliderSource`, `fixed_collision_record_eligible`.
  - Background: `local/research/p104-physics-body-contract.md`,
    `local/research/engine-foundations/analysis-882-883-cell-load-cost.md` (section "Collision").
- Profiling: search `crates/engine/src/profiling.rs`, `render_timing.rs` and `streaming.rs` for
  span names / `info_span!` / timing CSV columns that cover collider building or model first
  placement. Also `scripts/phase2-*.ps1` for how profiling runs are started.

# Scope
Read-only. Allowed commands: `git show`, `git grep`, `git log`. No builds.

# Done when
Each question answered with path:line (say `portal` or `origin/main`):
1. **Shape kinds and storage today.** Every `CollisionShape` variant and its fields; where in the
   GLB it is stored (scene extras key, JSON), units/scale, and the version constant(s) and how a
   reader handles an unknown or old version.
2. **Converter side.** Which function builds each shape from NIF/Havok data (bhkConvexVerticesShape,
   MOPP/compressed mesh, box, sphere, capsule, list/transform shapes); does it already compute hulls,
   or only collect points? Where the GLB extras are written.
3. **Engine side.** The full path from loaded GLB to `Collider`: function names, which parry
   builders (`convex_hull`, `convex_decomposition`, `trimesh`, `compound`, primitives), which run
   on the main thread vs a task pool, the per-model cache (key, lifetime), and what happens when a
   hull build fails.
4. **Cost evidence.** Any measured numbers already in the repo or `local/` for collider building
   (cite the file); otherwise say none.
5. **Existing spans.** Exact span/column names that would show collider-build time today, and
   whether the build is inside a named span or hidden in a larger one (name the larger one).
6. **Other readers.** Anything else that reads the collision extras (tools, `world-inspect`,
   tests, fixtures) and would break on a format change.
7. **Model identity.** How a GLB path is derived from the NIF path and whether the converter records
   any per-model source hash / provenance (manifest, `assets` table, #185-style keys) that a
   derived collision cache could key on.

# Report
Markdown under about 180 lines, sections `## 1` ... `## 7`, citations inline, UNVERIFIED where
you could not confirm. End with a 5-line `## Summary`.
