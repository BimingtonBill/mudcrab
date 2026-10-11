# Goal
First working slice of upstream issue #104 phase (a), engine half: placed movable clutter whose converted model carries an authored "dynamic" rigid body becomes a Rapier dynamic body with its authored mass, inertia, centre of mass, damping, friction and restitution. Everything else stays as today: fixed bodies stay fixed, and models from version 1 assets (no `bodies`) get no new physics. Track: Phase 2. This becomes a DRAFT PR; a few clutter objects that fall, settle and can be knocked over is enough.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-physics`. Create branch `phase2/physics-bodies-v2` from upstream main `461517bf` (`git switch -c phase2/physics-bodies-v2 461517bf`). Use the worktree's own `target/` (no CARGO_TARGET_DIR). Never run engine.exe.
- The data comes from the converter half, open as draft PR #118 (commits `60379b3d`, `448a51c2` in this repository): GLB scene extras `openSkyrimCollision` gain `version: 2` and a `bodies` array. Contract: `C:/Modding/Mudcrab/mudcrab/local/research/p104-physics-body-contract.md` (read it fully).
- This PR is based on main, not on #118. To compile, it carries #118's `crates/shared/src/collision.rs` change and #118's two small `crates/engine/src/streaming.rs` hunks (relaxed version gate and `bodies: Vec::new()` in two tests) BYTE-IDENTICAL, so both PRs merge cleanly in either order: take them with `git checkout 448a51c2 -- crates/shared/src/collision.rs` and by reapplying the exact hunks from `git diff a896b903 448a51c2 -- crates/engine/src/streaming.rs`. Do not change those lines further.
- Design (follow it unless the code proves it wrong; say where you deviate): `C:/Users/Brannock/.claude-deepseek/agents/runs/research-648-p2-physics-bodies-engine.1/report.md` (Q1-Q7, risks). Key points: body = the reference entity (child of the cell root, despawns with the cell); add it to the render-origin rebase's body-teleport list like the static colliders, never to `participants`; `ColliderMassProperties::Mass(0.0)` on each collider + the full tensor as `AdditionalMassProperties::MassProperties` on the body; `Damping`, `Friction`, `Restitution`; new collision group CLUTTER that collides with world, player, tankards and clutter; raise Rapier's global linear velocity cap and clamp per-body authored max linear/angular velocity in a FixedUpdate system; gate on `kind == dynamic` AND a movable base record type (MISC, WEAP, ARMO, BOOK, AMMO, ALCH, INGR, SLGM, KEYM, SCRL; leave CONT, ACTI, FLOR fixed for now and say so in a comment); non-convex dynamic bodies use a convex hull (counted); bodies arm with their model (the existing arming budget), and cap live dynamic bodies (like `MAX_LIVE_TANKARDS`) with a counter for the overflow.
- Earlier unfinished attempt to salvage from: `git diff 448a51c2 c5f34717` (1,100 lines in physics.rs and streaming.rs, never compiled). Reuse what is right; it is not authoritative.
- "Off the main thread where it is cheap": building the convex hull and the mass properties should happen once per model (cache by model path like the existing collision decision cache), not per frame; do not add threads.
- Known limit to keep: the character controller does not push dynamic bodies (`apply_impulse_to_dynamic_bodies: false`, physics.rs). The player collides with clutter; debug tankards (`spawn_debug_tankard`) can knock it over.

# Tests (headless, existing styles)
- physics.rs: a dynamic clutter body falls and settles on the fixture arena; mass properties match an authored tensor; the velocity clamp caps authored max velocities; clutter collides with tankards and the player; a render-origin rebase keeps pose and velocity.
- streaming.rs `model_app`-style: dynamic + movable type spawns a dynamic body; dynamic on a non-movable type stays fixed; a version 1 asset spawns nothing new; non-convex dynamic gets a convex hull and the counter; the body despawns with its cell.

# Scope
`crates/engine/src/physics.rs`, `crates/engine/src/streaming.rs`, `crates/shared/src/collision.rs` (byte-identical to 448a51c2 only), docs under `docs/` if a section describes collision (one short paragraph). No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass.
- Commits on `phase2/physics-bodies-v2`, upstream style, e.g. `feat(engine): dynamic clutter bodies from authored rigid-body data`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend.

# Report
Commit ids, what works, deviations from the design and why, the new metrics/counters, test count, anything left for a later slice. Under 400 words.
