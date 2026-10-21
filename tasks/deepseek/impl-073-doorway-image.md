# impl-073 - the door drawn over the doorway image, and the destination's own sun

## Goal

Land two fixes to the doorway the portal draws through an open load door, inside
`crates/engine/src/portal.rs` and `crates/engine/src/door_animation.rs` only:

* **Part A (the user's own note, worth more than Part B):** *"the door should animate and render
  over/on top of the portal"*. A leaf that swings past the doorway's plane, away from the player, is
  currently covered by the doorway quad and never seen. Draw the door a second time, at its mapped
  pose, in the destination image - see the design below.
* **Part B:** the doorway image is lit by no directional light at all, because Bevy filters
  directional lights per view by render layers and the engine's sun carries no `RenderLayers`. Give
  the portal camera a sun of its own, on the portal camera's layer, carrying the destination
  space's own sun tint.

Both parts land in one patch, with unit tests, and both are checked by the lead on the GPU: nobody
else runs the engine.

## Context

Read first, in this order:

* the module docs at the top of `crates/engine/src/portal.rs` (the layout of the portal, the render
  layers, the doorway image, the doors section);
* `docs/handoff/impl-066-portal-matches-the-world.md` (what was measured, and the two facts this
  task builds on: the doorway target is float now, and the door animates);
* `drawn_door_visibility` (`portal.rs:1034`) - the door visibility rule this task must not change.

Other leads are running on this checkout: one owns `crates/converter/**`, one owns the lighting
files. Do not touch anything outside your owned files.

### Part A - what is wrong, exactly

The doorway image is a **quad**: an opaque mesh of the main camera's view, standing in the doorway's
plane (`PORTAL_QUAD_OFFSET`, `portal.rs:207`), sampling the portal camera's render target by the main
camera's screen-space UV. The door's own leaf is an opaque mesh of the same view. Both are drawn in
one pass with one depth buffer, so **the nearer wins per pixel**, and there is nothing layer-shaped
in between to change that.

The quad stands at the *centre of the door model's bounding box* along the door's front axis
(`doorway_in_frame`, `portal.rs:530`); for Riverwood's `FarmhouseLDoor01` that is 13.5 units to the
`DoorBlack` side of the leaf's own plane. A leaf that swings **past that plane, away from the
player** is therefore behind the quad and covered by it: the player sees the room through the
doorway and no door at all. It is a thin foreshortened wedge at the opening's edge, and it is the one
case impl-066 deliberately left open.

### Part A - the design: the doorway's mirror

Draw the door a second time, **into the destination image**: a private instance of the door's own
model, on the portal camera's render layer, at the *mapped* pose of the door. Nothing else about the
portal changes.

Why this is right, and why it needs no shader and no new pass:

* `transition::portal_pose` (`transition.rs:303`) is the rigid map the portal camera itself is placed
  by: `M(p) = arrival + M_rot * (p - door_position)`, with `M_rot =
  door_to_arrival_rotation(door_frame(door_rotation, outward), arrival_rotation)`
  (`transition.rs:287`) and `arrival_frame` (`transition.rs:262`) giving the arrival position and
  rotation. `M(door_position) == arrival_position` by construction.
* The portal camera renders the destination from exactly `M(main camera pose)`, and the quad samples
  that image by the main camera's own screen-space UV, so **a point at `M(P)` is drawn on the pixel
  where `P` is** - `portal_projection` (`portal.rs:479`) changes neither clip-space `w` nor the
  projected `x`/`y` of anything beyond the doorway, which is the property the existing tests rely on.
* So: place the mirror's root at `M(door root pose)` and give every one of its descendants the *same
  local transform* as the corresponding source node, and every mirror node's world transform is
  exactly `M(source node world transform)`: the mirror is the door, seen through the doorway, in the
  right place, with the door's own swing on it.
  This has been audited independently (a separate worker's report): the quad's UV in
  `crates/engine/src/shaders/portal.wgsl` is exactly the drawing camera's screen position (the same
  formula as Bevy's own `frag_coord_to_uv`), taken from the **main** camera's view uniform, and is
  therefore independent of where the quad stands, which way it faces or how big it is. The one thing
  that would break the mirror's screen position is the portal camera's projection differing from the
  main camera's in x/y - it does not (the oblique clip plane writes only `clip.z`, and the copied
  `fov`/`aspect` are Bevy's own).
* Being part of the destination image, the mirror is **depth-tested against the destination's own
  geometry**, like everything else in that image, and it can only ever appear inside the opening
  (the quad is only visible where no nearer source geometry covers it).

Name it `PortalDoorMirror` or similar. **Do not call it a "twin"**: this codebase already uses
"twin" for impl-052's non-load look-alike model whose clips a load door borrows.

### Part B - what is wrong, exactly

`portal.rs:876-882` says the engine's one `DirectionalLight` "lights every view of a frame", so a
doorway into an interior seen from an exterior is drawn with the exterior's sun; that is the stated
reason the destination's sun could not be per camera. **Bevy does not work that way**, and the
doorway image is in a different, worse state:

* `bevy_pbr-0.19.0/src/render/light.rs:1642-1654` builds each view's directional lights from
  `directional_lights.iter().filter(|l| l.render_layers.intersects(view_layers))`, where `view_layers`
  is the **camera's** `RenderLayers` (`light.rs:1613`, `maybe_layers.unwrap_or_default()`), and
  `ExtractedDirectionalLight::render_layers` is the light entity's own `RenderLayers` component
  (`light.rs:124`, `:815`; layer 0 by default).
* The engine's sun is spawned at `app.rs:2117` with no `RenderLayers` at all, so it is layer 0; the
  portal camera is `RenderLayers::layer(DESTINATION_LAYER)`, i.e. layer 2.
* Therefore **the portal camera is lit by no directional light whatsoever**, whatever space is behind
  the doorway: a daylit Riverwood street seen from inside a house is drawn with ambient and fog only.

## Scope

### Part A - what to build

1. **Spawn.** When `PortalState::open_door` (`portal.rs:318`) becomes a door whose `DoorState` is
   `Opening` or `Open { animated: true }` (`doors::DoorState`), spawn one entity holding:
   * the door's own model, from the **same scene handle it already has** - the door entity carries
     `world::components::MeshHandle(String)` and the scene handle `streaming.rs:1259-1274` spawns for
     it. Reuse that handle; do not load the asset a second time. **Caution:** `streaming.rs` watches
     the component it spawns scenes through (with `PendingAssetProfile`) to measure and validate a
     reference's model, so make sure a mirror entity is not picked up as a reference's model root -
     a root measured as instance bounds would corrupt the doorway measurement. The safest shape is a
     plain scene-root component on a fresh entity, plus the layers below.
   * root `Transform`: translation `arrival_position`, rotation `M_rot * door_root_rotation`, scale the
     door root's own scale. Get it from `arrival_frame(&door.destination, origin.0)`,
     `door_frame(door_transform.rotation(), door.outward)` and `door_to_arrival_rotation` - the same
     three calls `update_portal` already makes. Do not re-derive the map anywhere.
   * `RenderLayers::layer(DESTINATION_LAYER)` (`portal.rs:137`), and the same on **every descendant**:
     `RenderLayers` is read per mesh entity and is *not* inherited, and a glTF scene spawns its meshes
     over several frames as assets load, so re-apply it to the whole subtree every frame while it
     spawns - the pattern `isolate_cells` (`portal.rs:1060`) already uses for cell roots.
   * `DoorLeaf { door }` (`doors::DoorLeaf`) on **every** node of the mirror, frame included. Two
     reasons, and they are why to use that component rather than a new marker:
     `door_animation::update_door_leaves` (`door_animation.rs:1324`) then draws or hides the mirror by
     the door's own state, exactly as it does the real leaf; and the walk probe skips anything under a
     `DoorLeaf` (`doors::mesh_is_out_of_the_way`, used by `player::player_walk`), so the mirror can
     never stop the player - it stands exactly where the player lands after a crossing. (`player.rs`
     is not yours; this is how you get the skip without touching it.)
2. **Mirror the pose every frame.** Copy the source door's *local* `Transform`s onto the mirror's
   nodes, paired one for one by the subtree structure (the two are the same scene asset: pair by a
   deterministic traversal from each root and check the counts match; while a node is still spawning,
   skip the frame rather than draw a half-mirror). The copy must see the animation's output, so it
   runs after the animation has advanced and before the transforms propagate: `PostUpdate`, after
   `bevy::animation::AnimationSystems` and before
   `bevy::transform::TransformSystem::TransformPropagate` (verify the exact paths in bevy 0.19).
   Copying local transforms means no second `AnimationPlayer`, no second graph, and no assumption
   about how the door is animated.
3. **Despawn** on the frame `open_door` stops being that door, or the door entity is gone, or its
   state leaves `Opening`/`Open`.
4. **Commands against possibly-dead entities.** This codebase has already been bitten by
   `commands.entity(e).insert(...)` on an entity a *crossing* despawns in the same frame (Bevy panics:
   "Entity despawned"; see the note at `door_animation.rs:1319-1323`, and the lead is hunting exactly
   that crash right now). Use `try_insert` / `try_remove` for anything queued against the mirror or a
   door, and never queue a command against an entity you have not just read as alive.
5. **Log it once per mirror**, in this codebase's style, e.g.
   `engine::portal door=0001CBB0 mirror placed at <pos>, N nodes, M meshes`. The lead has the GPU and a
   run's log is how this change is checked.

### Part A - tests it must have

* **The map holds**: for a door pose and a destination, every mirror node's world transform equals the
  map applied to the corresponding source node's world transform (small tolerance), including a door
  whose link-derived `outward` disagrees with its model rotation.
* **The screen position holds** (the decisive one): with the main camera at a pose in front of the
  door, a source node's clip-space position under the main camera's projection equals its mirror's
  clip-space position under the portal camera's projection, using the real
  `portal_pose`/`portal_projection` rather than a hand-written copy of them.
* The mirror is **not** spawned for an auto-load marker, for a door with no animation, or for a door
  the portal is not rendering through; and it is despawned when the portal retargets.
* Every mirror node is on `DESTINATION_LAYER` and carries the door's `DoorLeaf`.

### Part B - what to build

Give the portal camera a sun of its own, on the portal camera's layer only:

* a `DirectionalLight` entity on `RenderLayers::layer(DESTINATION_LAYER)`, with the same `Transform`
  **copied from the engine's own sun** (query `Query<&GlobalTransform, With<DirectionalLight>>` and
  take the one that is not yours; do not duplicate `app.rs`'s sun rotation constant here);
* its `color` and `illuminance` come from the **destination's own space**: `update_destination_atmosphere`
  (`portal.rs:883`) already computes `crate::app::space_atmosphere(catalog, destination)` for the
  destination and applies its ambient, fog and clear colour - set the sun from the same value
  (`atmosphere.sun`), where the existing per-destination change test already is;
* shadows: copy the engine sun's `shadow_maps_enabled` and `CascadeShadowConfig` so the doorway keeps
  its shadows, if that costs nothing surprising (`MAX_DIRECTIONAL_LIGHTS` is 10 on desktop and
  `bevy_pbr` budgets cascades per *view*, so a second sun does not starve the main camera's). If you
  hit a budget or shadow-map problem, leave shadows off on the destination sun and say so in your
  report - do not restructure `app.rs`;
* **do not** change the engine's sun, its layers, or `app.rs` at all. The main camera and the water
  reflection camera keep seeing the engine sun; the portal camera sees yours;
* correct the doc comment at `portal.rs:876-882`, which currently states the opposite of the truth.

Part B tests: the portal camera's layers intersect the destination sun's and the main camera's layers
do not; the main sun's layers do not intersect the portal camera's (that is the whole mechanism -
assert it); `update_destination_atmosphere` writes the destination's sun tint and illuminance into the
destination sun and leaves the engine sun alone; the destination sun's rotation equals the engine
sun's.

### Out of scope - deliberately

* The quad's plane, `PORTAL_QUAD_OFFSET`, the crossing, `drawn_door_visibility`'s rules, the door state
  machine, the door's own animation, the portal target's size and format: all unchanged.
* Anything in `app.rs`, `player.rs`, `streaming.rs`, `lights.rs`, `world/lighting.rs`, `demo_tour.rs`,
  `transition.rs`, `doors.rs`. If one of them needs a change, say exactly what and where in your report
  instead of making it.
* The mirror's *lighting* is the destination's - it is drawn in the destination's image, by the
  destination's view. That is expected; do not add a light for it.

Owned files: crates/engine/src/portal.rs, crates/engine/src/door_animation.rs

## Done when

Acceptance: cargo test -p engine

Before you report, run these yourself and quote their results in your report: `cargo fmt --all -- --check`
and `cargo clippy --workspace --all-targets -- -D warnings` (the harness runs only the acceptance
command above).

The tests listed above must exist and fail without your change, and your own reading of the diff must
be that nothing outside your owned files changed. The engine itself is **not** run here (the lead has
the GPU and the user is on that machine), so say in your report which of your claims only a run can
settle and what the lead should look at.

## Report

Short and in the project's voice: what you changed and why; the tests you added and what each pins;
the exact commands you ran with their results; anything you could not check; and every place where you
had to choose between two readings of this brief, with the one you took and why. If Part A turns out
to be impossible as designed, stop and report that with the evidence - do not improvise a weaker
mechanism (a depth bias, a moved quad, or hiding geometry) instead.
