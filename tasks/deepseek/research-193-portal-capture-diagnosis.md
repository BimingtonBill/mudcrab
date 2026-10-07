# research-193 - Diagnose three doorway faults from the user's captures

Track: portal. Read-only: change nothing.

The captures are in `local/captures/2026-09-25_03-57-58/`. Each capture has a PNG and a JSON (the pose, the active space, and the door states within 1000 units). `notes.md` has the user's notes and every crossing.

## Faults

### 1. A closed door stands inside the Riverwood Trader's open doorway (`01.png`)

The Trader's door `0001341F` is open (the prompt says "E Close"), but inside the open doorway, close behind the frame, stands a **closed door** instead of the Trader's interior.

The Trader also has a real second load door, `00070E69`, 224 units above, linked to the same interior. The interior's doors are `000133E9` and `00070E68`.

Which door is drawn there?
- the far door, not hidden;
- the wrong far door hidden (the upper one); or
- the mirror leaf?

Trace how the far door is chosen when an interior has two doors to the same exterior:
- in portal.rs: `drawn_door_visibility`, `place_door_mirror` and the far-door hiding;
- in transition.rs: `OpenDestinationDoor`.

### 2. The doorway image is offset (`06.png`, `14.png`)

At Chillfurrow Farm (`06.png`, "this alignment is off") and Cracked Tusk Keep (`14.png`, "alignment issues"), the image through the doorway is offset. At Chillfurrow the room sits too high, with a flat grey band below.

The doorway map anchors both doorways. The code is in `crates/engine/src/portal.rs` (the doorway-anchor code from impl-103) and in `doors.rs` (`DoorwayFacings`, `DoorAnchor`).

Compute where each doorway's centre and facing end up on both sides. Use the world DB (`C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db`, opened read-only with `mode=ro`) and the door models' bounds. Say which number is wrong for these two doors but right for Sven's House.

### 3. Bleak Falls Barrow is lit differently through the doorway (`10.png`, `11.png`)

Going from Bleak Falls Barrow 01 into 02 ("lights difference"), the room seen through the doorway is lit differently from the same room once you are inside.

Compare what the portal camera gets with what the active space gets:
- the portal camera: `update_destination_atmosphere`, the destination's ambient, fog, the lights on layer 2, and the `lights.rs` budget per space;
- the active space: `atmosphere.rs` `update_atmosphere`.

Which inputs differ?

## Report

For each fault:
- the cause, with file:line and numbers;
- the exact change that would fix it: the function, and what it should compute instead.

Keep it concrete: the lead turns each fault into a coder brief for portal.rs.
