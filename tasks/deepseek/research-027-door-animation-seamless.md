# research-027 - Animated doors and seamless cell-to-cell walking: design

Read `tasks/deepseek/context.md` first. This is a read-only design task.

## Goal

The user wants two things:
- load doors that **open with their real animation**, like Skyrim's doors swinging or sliding;
- walking through an open door from one cell into the next **without noticing**: no teleport
  jump, no loading pause, no change in where you look.

The engine already has part of this:
- `crates/engine/src/portal.rs`: a portal camera renders the destination cell through the nearest
  load doorway;
- `crates/engine/src/transition.rs`: crossing a door teleports the camera to the `XTEL` arrival
  point and heading;
- `crates/engine/src/player.rs`: walking, E to open, and auto-load doors that cross on contact;
- impl-022 made load doors draw closed unless the portal is using them.

Design the rest.

## Questions to settle, with evidence

1. **Door animations in the data.** Find how Skyrim door meshes carry their open/close
   animation: `NiControllerManager`, `NiControllerSequence` names such as "Open" and "Close",
   `NiTransformController` and interpolators, or `BSBehaviorGraphExtraData` / Havok behaviour.
   Check real files: the Dwemer doors on the demo route are
   - `Dungeons\Dwemer\Door\DwemerSmallDoorLoad01.nif`,
   - `DweDoorLarge01Load` (find its model),
   - the Alftand01 to Alftand02 door `00092809`,
   - the AlftandWorld doors.

   The BSAs are in the read-only game install (`C:/Games/Steam/steamapps/common/Skyrim
   Special Edition/Data`). The converter can read BSAs: see `crates/converter/src/archive/`. Read
   its code and tests. If you need a NIF dump, describe the exact block layout from the converter's
   NIF parser (`crates/converter/src/nif/` or similar) and the NIF spec (niftools nif.xml
   knowledge), and say what you could and could not confirm.
2. **What the converter does today** with controllers and animations (`file:line`). Does the
   glTF output carry any animation? What would it take to export an "Open" sequence as a glTF
   animation clip, including the keyframe types, the node targeting and the time base?
3. **Bevy 0.19 playback.** How to play a glTF animation clip on a spawned scene: `AnimationPlayer`,
   `AnimationGraph` and `AnimationClip` handles from `Gltf`. Read the Bevy 0.19 source in the
   registry (`C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_animation-0.19*`
   and `bevy_gltf-0.19*`) and cite `file:line`. Also: how a door that is open stays open.
4. **Seamless crossing.** Design the crossing so the view never jumps:
   - The portal already maps the main camera's pose through `door_to_arrival_rotation` /
     `portal_pose`: a camera at the door lands on the arrival point.
   - When the player's eye passes through the doorway plane, apply that same mapping to the
     player's current pose (position and view direction). Do not snap to the arrival point.
   - Cover the timing: the destination must be resident and rendered, and the portal image and
     the real destination must match in the frame of the swap (exposure, lights, fog, ambient per
     space).
   - Cover collision: the door leaf must be open, and the player must be able to walk through.
   - Cover what happens behind the player: the return door should show the cell they came from
     as a portal.
   - Cover auto-load doors, which have no leaf, and the fact that Skyrim's `XTEL` arrival is
     offset from the destination door.
   - impl-023 (running now) changes the door's front to come from the return link's `XTEL`
     instead of the model axis; assume it lands.
5. **Door state.** Closed → opening (animation, and the portal activates once it is open
   enough) → open → closing, triggered by E or by approach. What does Skyrim do (activate to open,
   auto-close)? Keep it simple.

## Deliverable

`docs/design/animated-doors-and-seamless-crossing.md`, the design the lead will review:
- the data path (NIF → converter → glTF → engine);
- the door state machine;
- the crossing algorithm with the math;
- edge cases;
- an implementation plan split into 3 or 4 coding tasks with **disjoint owned-file lists**, each
  with its acceptance checks and runtime verification (`--demo-tour`, `--shots`; see
  `docs/design/reference-shots.md`);
- whether a converter schema bump and a reconversion are needed.

Commit only that file, plus a handoff note in `docs/handoff/`, with `[deepseek]` subjects. Do not
edit `crates/`.

## Report

A summary of the design, the plan table, and the open questions, with evidence for every claim
about data or code.
