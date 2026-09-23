# research-074 - where a doorway image could fail to line up with the world

## Goal

A user note says that the interior seen through an open load door does not line up with the doorway
it is seen through. Your job is a **read-and-reason audit of the code path that draws that image**,
ending in a list of every term that could offset, scale or otherwise displace the doorway image
relative to the world around it - each marked exact, approximate, or unproven - plus what the
project's own tests already pin.

The doorway image is a quad standing in the doorway plane, drawn by the main camera, sampling a
render target that a second camera ("the portal camera") rendered the destination space into. If
everything is right, a fragment of the quad shows the destination pixel that the same sight line
would hit if the player were in the destination space, so the image lines up with the world exactly.
Find out where that could break.

## Context

`C:\Modding\OpenSkyrimProject` is a Skyrim engine rewrite in Rust on Bevy 0.19. The relevant code:

* `crates/engine/src/portal.rs` - the whole portal: `update_portal`, `select_portal_door`,
  `portal_pose` callers, `doorway_in_frame`, `measured_doorway_box`, `portal_projection`,
  `doorway_clip_plane`, `resize_portal_target`, `setup_portal_camera`, `setup_portal_quad`,
  `show_load_door_leaves`, `isolate_cells`, and the module docs at the top (read those first - they
  state the design intent, and where the code disagrees with them that is a finding).
* `crates/engine/src/shaders/portal.wgsl` - the quad's fragment shader, which samples the render
  target. How does it get its UV, and is that the same screen position the main camera is drawing?
* `crates/engine/src/transition.rs` - `door_frame`, `arrival_frame`, `door_to_arrival_rotation`,
  `portal_pose`, `distance_in_front_of_door`: the rigid map from the door's frame to the arrival
  frame, used by both the portal camera and the player's crossing.
* `crates/engine/src/app.rs` - the main camera's setup (its projection, its `RenderLayers`, its
  transform) and the portal camera's counterpart.
* `docs/handoff/impl-066-portal-matches-the-world.md` and
  `docs/design/animated-doors-and-seamless-crossing.md` (sections 4.1-4.2) - what the previous tasks
  established and what they assumed.

Bevy's sources are readable at
`C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (e.g. `bevy_render-0.19.0`,
`bevy_core_pipeline-0.19.0`, `bevy_pbr-0.19.0`) - useful for what `PerspectiveProjection`,
`Camera`, `RenderLayers` and `Material`/shader plumbing actually do.

## Scope

* **Read only, and no shell.** You cannot run commands, write files, or look at rendered pixels; you
  have file reading and searching. Everything must come from source, docs and tests in this checkout.
  (If you can open a .png and see it, `local/demo/rwbt/00-door.png` and `local/demo/rwbt/00-arrived.png`
  are a doorway and the room behind it; if not, ignore this line.)
* Do not propose a fix or a change: this is an audit whose product is the list.
* Not your subject: the crash that happens on some crossings, the door's own animation, the lighting
  of the doorway (another task owns that), and anything under `crates/converter/**`.

## Done when

You can state, each with `file:line`:

* every step from "the main camera's pose" to "the pixel the quad shows", and for each step whether
  it is exact or an approximation - in particular whether the portal camera's **projection** matches
  the main camera's in field of view, aspect and screen-space mapping (the doorway quad's UVs go
  through `portal_projection`'s modified projection, so any difference that changes projected x/y is
  a defect), whether the portal camera's **pose** is exactly the map applied to the main camera's
  pose, and whether the target's **size or format** can move or scale the image;
* every place where the doorway quad's **geometry** is placed by a measurement rather than by the
  world: `doorway_in_frame`'s box centre, `PORTAL_QUAD_OFFSET`, the fallback `DEFAULT_PORTAL_SIZE`,
  and the door's own `outward` from link data - and what a mistake in each would look like on screen
  (a shift, a scale, a visible sliver, a doorway offset from the wall's opening);
* what the existing tests in `portal.rs` already pin about alignment (`the_window_stands_in_the_plane_the_crossing_fires_on`,
  `the_doorway_quad_is_laid_out_in_the_frame_the_doors_front_comes_from`, the `portal_projection`
  tests around `geometry_between_the_portal_camera_and_the_doorway_is_clipped`, and the clip-plane
  tests), and what an alignment defect would have to look like to slip past them;
* your verdict: is the code path exact, or is there a term that would put a systematic offset or
  scale on the doorway image? If you find nothing, say so plainly - a clean audit is a useful result.

## Report

The list, then the verdict, then the gaps in the tests. Cite `file:line` for every claim about the
code; where you are reasoning rather than reading, say so. Do not pad it: a short report that names
the one term that could be wrong is worth more than a long one that covers everything.
