# impl-177 - Shadow views take their light's render layers, so the doorway's world casts sun shadows

Owned files: crates/engine/src/shadow_layers.rs,crates/engine/src/lib.rs,crates/engine/src/render.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's demo note of 2026-09-25: "sun shadows should display on exterior if you're inside looking out." research-175 traced it, and the lead checked the key Bevy lines himself.

## The cause (Bevy 0.19, read from the vendored sources in the cargo registry)

- The portal camera renders the destination space on `RenderLayers::layer(DESTINATION_LAYER)` (= 2, `crates/engine/src/portal.rs` ~189, ~1125), lit by its own sun, `PortalDestinationSun`, which carries layer 2 as well (`portal.rs` ~1141-1153). The main camera renders `[0, 1]`; the engine sun is on no layers (= layer 0).
- Bevy builds each directional light's cascades per view, and picks each cascade's visible casters by the **light's** layers against the mesh's, so the portal sun's cascades do collect the destination's meshes.
- But `queue_shadows` (`bevy_pbr-0.19.0/src/render/light.rs`, query ~2512 and the filter ~2582-2586) filters again by the **shadow view entity's** `RenderLayers`: `view_light_entities: Query<(&LightEntity, &ExtractedView, Option<&RenderLayers>)>`, then `maybe_view_render_layers.unwrap_or_default()` (= layer 0) must intersect the mesh's layers. `prepare_lights` (~1866-1898) spawns the shadow view entities with `ShadowView`, `ExtractedView`, a frustum and `LightEntity`, **never** `RenderLayers`. So every shadow view acts as layer 0, and no layer-2 mesh is ever queued into the portal sun's shadow pass. The doorway's exterior is sunlit but has no shadows at all.

## What to build

An engine-side fix, no Bevy fork: in the **render app**, a system that gives every directional-light shadow view the render layers of the light it belongs to, after `prepare_lights` creates the views and before `queue_shadows` reads them.
- New file `crates/engine/src/shadow_layers.rs` with a small plugin (`ShadowViewLayersPlugin`), registered next to the other renderer plugins in `render.rs` (`VercidiumRendererPlugin::build`, the `app.add_plugins((...))` list), and `pub mod shadow_layers;` in `lib.rs`.
- The system runs in `Render`, ordered `.after(bevy::pbr::prepare_lights)` (in `RenderSystems::CreateViews` or any later set that is still before `RenderSystems::QueueMeshes`). Check that `prepare_lights` is public, or find the public set/label to order against, and say which you used. Its commands must be applied before `queue_shadows` runs.
- For each view entity with `LightEntity::Directional { light_entity, .. }`, read the light's `ExtractedDirectionalLight::render_layers` (render world, `light.rs` ~109-124) and insert that `RenderLayers` on the view entity when it differs from what the view already has. Point and spot lights: leave them alone (none of ours use layers), but say in the report whether the same bug applies to them.
- Keep the module doc short and factual: the Bevy lines, why the insert is needed, and that it can be deleted when Bevy's `prepare_lights` gives shadow views their light's layers.

## Checks

- A unit test that builds the render-world pieces it can (or tests the pure part: given a `LightEntity::Directional` and a light's layers, the layers the view gets). Be honest about what a unit test can't reach.
- `cargo build --profile quick --bin engine`, then a short door tour twice, once on the build **without** your change (the lead's `target/quick/engine.exe` in `C:\Modding\MudcrabProject`, if the harness gave you its copy, or `git stash` your change) and once with it:
  `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t177-before --tour-doors 2`
  and the same into `local/t177-after`. The tour's survey screenshots inside the first house (`01-*`, taken after crossing in, looking around) include the open door with the daylit exterior seen through it. Compare those images between the two runs and say whether the exterior through the doorway now shows sun shadows (trees and houses casting onto the ground), naming the image files. Both tours must end `tour PASSED after 2 crossings (short tour)`. Read only `local/t177-*` under `local/`.
- The main view's own shadows are unchanged: compare the exterior screenshots (`00-*`) between the runs.

## Report

Files changed and why; the ordering you used; test names; the before/after images and what differs; whether point/spot shadow views have the same bug; the tours' last lines.
