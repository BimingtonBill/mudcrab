# Portal brainstorm: shared context (research-207/208/209)

Track: portal. The user asked for "intelligent, creative and dynamic ways of developing portals".

## What the portal is today (2026-09-25)

OpenSkyrim re-implements Skyrim's world in Rust/Bevy 0.19, from converted game data (`C:\Modding\SkyrimConverted`, read-only).

**The portal feature.** Skyrim's load doors (a house door, a dungeon door) normally teleport the player behind a loading screen. Here they are seamless:
- The far side (the interior cell, or another worldspace) streams in near the door, on its own render layer (layer 2).
- A second camera, the portal camera, renders it through a door-to-destination rigid map (`DoorMap`, anchored on the two doorways' thresholds).
- The result is composited into the doorway with a flat quad on the doorway plane plus an oblique clip plane.
- Walking through swaps the two worlds: `ActiveCell` changes, and the old side becomes the portal view.

**Doors.**
- Doors play Skyrim's Open/Close clips, scaled to about 90 degrees.
- The far door is hidden, and a mirror copy of the near door is drawn in the destination image.
- E opens and closes a door. A door waits for its destination to load before opening. Doors auto-close when the player walks away.

**Performance work done today.**
- The doorway renders only its screen rectangle (`sub_camera_view`).
- It is skipped when off screen or behind a wall (ray casts).
- The portal camera is HDR, so it shares the main view's pipelines.
- The destination's sun shadows reach 3,000 units.
- The destination gets 16 of 64 point lights.
- Cell isolation is change-driven.

**Known problems.**
- The seam where the exterior model and the interior model meet at the doorway: two different doorframes, lit differently, joined at a plane (the user's captures in `local/captures/2026-09-25_10-27-59/`).
- A one-frame lighting lag on the swap frame (being fixed).
- Door leaves hinge on different sides inside and outside (a mirror copy instead of the far door's own leaf).
- Cave mouths, ladders and trapdoors are parked (they keep the old teleport landing).

**Code.**
- `crates/engine/src/portal.rs` (the portal camera, quad, clip plane, isolation, mirror);
- `transition.rs` (crossings, prestreaming, `DoorMap`);
- `doors.rs` (anchors);
- `door_animation.rs`;
- `lights.rs`, `atmosphere.rs`;
- `docs/roadmap/backlog-engine-and-portal.md` ("Portal plugin" section);
- `docs/design/portal-*.md`;
- `local/research/*.md` (today's research: capture faults, occlusion culling, first-open hitch, frame-cost audit).

**Skyrim data** (read-only) is at `C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data`. The world DB is `C:/Modding/SkyrimConverted/skyrim_world.db` (open read-only).

## Rules for brainstormers

- Read-only: change no tracked file, build nothing, run no engine.
- Write your output to the file your prompt names, under `local/research/`. Any scratch goes in the scratch folder your prompt names.
- Be creative, but ground every idea: say what in our code or Skyrim's data it builds on, and what it would take.
- Aim for ideas nobody has listed yet. Check the backlog so you don't repeat items already planned, and label any you build on.
- For each idea give:
  - a one-line pitch;
  - how it works;
  - what the player sees or gains;
  - feasibility (S/M/L effort, risk);
  - the first experiment that would prove or kill it.
- End with your top 5, ranked by (player-visible payoff × feasibility).
