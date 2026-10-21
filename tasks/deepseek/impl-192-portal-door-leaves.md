# impl-192 - Load-door leaves are visible from behind, and double doors open as far as Skyrim's

Owned files: crates/engine/src/door_animation.rs,crates/engine/src/doors.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's captures are in `local/captures/2026-09-25_03-57-58/`, notes in its `notes.md`:
- **Leaves seen from behind show only edges.** `08.png` and `09.png` (Bleak Falls Barrow's big double door, "a bit broken", "this way too") and `13.png` ("inside of keep doors have no polygons or textures"): seen from the far side, an open load door's leaf shows only thin edges. Skyrim's load-door leaves are one-sided, because the game never shows their back: a load door teleports. Our seamless doors do show it, from inside and as the mirror leaf.
- **Double door stops short.** `04.png` (Honningbrew Meadery, "this type of door doesn't open the whole way"): the leaves stop at roughly 60-70 degrees.

## What to build

1. **Draw every load door's leaf meshes double-sided.**
   - Where: when the door scene is instanced, which is where `DoorLeaf` is marked (door_animation.rs ~2085-2101) or where the door model is spawned.
   - Give the leaf materials `cull_mode: None` and `double_sided: true`. Clone each material once per door model, not per frame, and leave other statics untouched.
   - Check that the back face's lighting looks sane (Bevy flips the normal for double-sided materials).
2. **Find out why the Meadery door stops short.** Is 60-70 degrees the model's own Open clip, or our playback? Our playback code is `swing_degrees`, `swing_scale`, `SWUNG_OPEN_DEGREES` and `MAX_SWING_SCALE` (door_animation.rs ~1041-1083).
   - Print the clip's own end angle for the Meadery door model (its reference is in the capture's `04.json` state, under the doors within 1000 units) and for Bleak Falls' door.
   - If our scaling caps it, fix the rule so a double door opens to the doorway-clear angle.
   - If the clip itself ends at 60-70 degrees, say so and leave it.

## Checks

- Unit tests for the material change (a leaf material comes out double-sided, a non-door material does not) and for any swing-rule change.
- `cargo build --profile quick --bin engine`. Doors are closed in a `--shots` run, so check with the short tour instead: `--demo riverwood --walk --demo-tour local/t192 --tour-doors 2` must end `tour PASSED after 2 crossings (short tour)`. Look at its door shots from both sides.
- Under `local/`, read only `local/t192*` and the capture folder named above.

## Report

What changed; the Meadery and Bleak Falls clip angles and what you did about them; test names; the tour's last line; an image path showing a leaf from behind.
