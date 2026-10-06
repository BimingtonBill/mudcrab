# research-275 - Porting main's world collision and WorldPlayerPlugin onto the portal branch

Track: portal. Read-only; change nothing.

## Goal
A step-by-step porting recipe for upstream main's world collision (rapier terrain colliders, static
colliders from converted collision data) and its walking player (`WorldPlayerPlugin`,
`interactive_world_physics`, `MovementTuning`) onto the portal branch, which has its own walking
player (`crates/engine/src/player.rs`, `PlayerPlugin`: ray-cast ground following, E to open doors,
F to fly) and its own door crossings (`crates/engine/src/transition.rs`).

## Scope
Read only. Portal tree: `C:/Modding/wkj-portal-merge` (`crates/engine/src/physics.rs` is main's,
compiled but not wired; `player.rs`, `transition.rs`, `portal.rs`, `streaming.rs`, `app.rs`,
`config.rs`). Main's copies and the common ancestor are in `C:/Modding/MudcrabProject/local/merge/`:
`streaming_main.rs` (search `TerrainCollider`, `StaticColliderSource`, `StaticCollisionCache`,
`collider`, `ColliderDisabled`, `rapier`), `streaming_base_to_main.diff`, `main_app.rs` (search
`WorldPlayerPlugin`, `interactive_world_physics`, `RapierPhysicsPlugin`, `physics_fixture`),
`base_app.rs`. The converter now embeds collision in GLB extras (`mudcrabCollision`,
`crates/shared/src/collision.rs`).

## Web questions
none

## Done when
You can say: which main pieces to bring (file:line), where each hooks into portal's code (file:line);
how portal's tiered cells (`CellDetail::Terrain` ring cells must stay collider-free?), interiors
(`ActiveCell`), pre-streamed door destinations on `crate::portal::DESTINATION_LAYER` (colliders of a
space the player is not in must not block the player) and the render-origin rebase interact with
rapier bodies; whether `WorldPlayerPlugin` should replace portal's `PlayerPlugin` or the portal player
should gain collision, and what each costs (door E-to-open, auto-load doors on contact, the seamless
crossing that carries the player's pose, `--walk`, `--demo-tour` walking); and an order of steps that
keeps the build green and the door tours passing.

## Report
1. Recommendation (replace vs extend portal's player) with reasons.
2. Recipe: numbered steps with source and target file:line.
3. Interactions: tiers, interiors, destination space, rebase, crossings: each case and the rule.
4. Tests to port (names) and portal tests affected.
5. Risks and open questions.
Under 250 lines. Cite file:line for every claim.
