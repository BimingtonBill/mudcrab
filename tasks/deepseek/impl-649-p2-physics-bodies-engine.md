# Goal
Engine half of upstream issue #104 phase (a): placed movable objects (clutter) become dynamic Rapier bodies with their authored collision. They rest on terrain and statics and collide with the player, the debug tankards and each other. The player cannot push them yet (see Context). One reviewable PR, engine crate only.

# Context
- Track: Phase 2. Upstream commit style (`feat(engine): ...`). Branch `phase2/physics-bodies` in worktree `D:/Mudcrab/worktrees/p2-physics`, cut from Phase 1's `phase1/physics-bodies` (it carries the converter half and the shared `CollisionAsset.bodies` type plus the relaxed collision-version check in streaming.rs). CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-physics on every cargo command.
- Data contract: `C:/Modding/Mudcrab/mudcrab/local/research/p104-physics-body-contract.md`, read it first. The type is in `crates/shared/src/collision.rs`.
- Design research (read it; its file:line refs are for upstream main, so re-find them on this branch): `C:/Users/Brannock/.claude-deepseek/agents/runs/research-648-p2-physics-bodies-engine.1/report.md`. Decisions on top of it:
  1. **Who gets a dynamic body:** a placed reference whose body `kind` is "dynamic" AND whose base record type is one of MISC, WEAP, ARMO, BOOK, AMMO, ALCH, INGR, SLGM, KEYM, SCRL. CONT, MSTT, ACTI, FLOR, DOOR, FURN, STAT and TREE never become dynamic in phase (a) (Skyrim's barrels and chests are fixed). A reference gets exactly one of: a dynamic body, today's fixed collider (unchanged rules, `fixed_collision_record_eligible`), or nothing (today's behaviour for movable types without a dynamic body).
  2. **Where:** the body goes on the reference entity under its cell root, exactly where today's fixed collider is attached, in the readiness scan after the model is ready. It despawns with its cell (including the paced unloads). Its resting pose is not persisted (that's #90). No hand-off across cell boundaries.
  3. **Origin rebase:** add the dynamic clutter bodies to the rebase's body-teleport list next to `static_colliders` (`set_translation(.., false)`, velocity kept). Never add them to the rebase `participants` (which shift Transform): with the cell root also moving, that would shift them twice. Write a test that a moving clutter body keeps its world pose and velocity across a rebase.
  4. **Mass:** `ColliderMassProperties::Mass(0.0)` on every collider of the body, plus `AdditionalMassProperties::MassProperties(MassProperties::with_inertia_matrix(com, mass, inertia))` on the body. If the tensor isn't positive definite or the mass isn't finite and > 0, fall back to collider density and count it.
  5. **Shapes:** convex bodies use their shapes as today's collider builder does. A body with `convex: false` gets one convex hull of all its shape vertices (count it as `dynamic_clutter_convexified`). If no hull can be built, leave the reference without a dynamic body (count `dynamic_clutter_skipped`) and never fall back to a fixed collider for a movable type.
  6. **Damping, friction, restitution:** from the body (`Damping`, `Friction`, `Restitution`, Rapier's default Average combine). Leave the static colliders' values alone.
  7. **Velocity limits:** Rapier caps linear speed at `normalized_max_linear_velocity` (default 400) × `length_unit` (1 here), i.e. 400 units/s, which clips free fall under gravity 900 after 0.44 s. Raise that global cap, with a comment, wherever the physics plugin configures gravity. Then add a FixedUpdate system after the Rapier step that clamps each dynamic clutter body to its authored max linear and angular velocity (count clamps).
  8. **Collision groups:** add a clutter group; clutter collides with world, player, tankards and clutter; add it to the filters of the existing three groups. Update the group test.
  9. **Sleep:** let Rapier's default sleeping work; the deactivator type is not mapped in phase (a) (say so in a comment).
  10. **Player:** `apply_impulse_to_dynamic_bodies` stays false (the Rapier 0.35 panic comment). The player collides with clutter and can stand on it; pushing is a later phase.
  11. **Stable ID:** a `PhysicsObjectId(FormId)` component (or reuse the existing FormId component if it's already on that entity; then add nothing and say so).
  12. **Only in interactive physics runs** (`EngineConfig::interactive_world_physics()` or wherever today's static colliders are gated): benchmarks, shots and acceptance runs must not change.
  13. Read the collision extras under both keys `openSkyrimCollision` and `mudcrabCollision`, if Phase 1's branch doesn't already.
- Metrics on StreamingMetrics: `dynamic_clutter_bodies`, `dynamic_clutter_convexified`, `dynamic_clutter_skipped`, `dynamic_clutter_clamped`.

# Scope
Change only `crates/engine/src/physics.rs`, `crates/engine/src/streaming.rs`, `crates/engine/src/metrics.rs` (only if metrics need it) and their tests. Do not touch crates/converter, crates/shared or crates/dummy-content (Phase 1's). If a test needs a fixture NIF that dummy-content doesn't produce, build the CollisionAsset in the test directly and report what fixture Phase 1 should add. No new dependencies.

# Done when
- Tests cover: a dynamic body falls and settles on a fixed floor in the headless physics fixture; it collides with a tankard and with the player capsule (does not pass through); only "dynamic" + a movable type gets a body (a fixed body on MISC stays without a body, a dynamic body on STAT stays fixed, CONT never dynamic); a version 1 asset gives no dynamic bodies; the `mudcrabCollision` key is read; a non-convex dynamic body becomes a hull and is counted; mass properties match the authored tensor; the clamp caps an authored max velocity; a rebase keeps a moving body's pose and velocity; the body despawns with its cell; benchmark/shots configs spawn no dynamic bodies.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` pass.
- ONE commit, `feat(engine): placed clutter becomes dynamic Rapier bodies from authored physics data`, files added by name. No push, no amend.

# Report
Commit id; changed lines per file; test count before/after; each numbered decision you had to change and why; the fixture Phase 1 should add, if any. Under 350 words.
