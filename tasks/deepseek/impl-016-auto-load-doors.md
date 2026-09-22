# impl-016 - Auto-load doors cross on contact

Owned files: crates/engine/src/doors.rs,crates/engine/src/streaming.rs,crates/engine/src/world/database.rs,crates/engine/src/player.rs,crates/engine/src/transition.rs,crates/engine/src/portal.rs,crates/engine/src/demo_tour.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md` and the design
`docs/design/lights-and-auto-doors.md` (section "Auto-load doors") first. You own the listed files
because the contract change touches struct literals in all of them; change only what this task needs.

## Goal

In Skyrim, an `AutoLoadDoor` (an invisible load marker, e.g. the Alftand entrance `00015D48`, base
`AutoLoadDoor01`, model `AutoLoadMarker01.nif`) teleports the player when they walk into it. In the
engine every load door needs E, so the player has to find an invisible door and press a key at it.
Make auto-load doors cross on contact; keep E for all other load doors.

## What to build

1. **Contract** (`doors.rs`): `LoadDoor` gains `pub auto_load: bool` with a doc comment. Update every
   construction site (tests included) across the owned files.
2. **Streaming / database**: set `auto_load` for doors whose base object marks them as auto-load - the
   base's `statics.editor_id` starting `AutoLoadDoor` or its `model_path` containing `AutoLoadMarker`
   (case-insensitive). Check the real database read-only
   (`C:/Modding/SkyrimConverted/skyrim_world.db`, a script under `tools/research/` with `mode=ro`) for
   how many load doors match, and whether other editor-id patterns are auto-load too (look at the base
   `DOOR` records' flags if you can: Skyrim `DOOR` has an "automatic door" flag - UESP
   "Skyrim Mod:Mod File Format/DOOR"; if that flag is available in the data, prefer it and say so).
3. **Player** (`player.rs`): while walking or flying, when the player's feet enter an auto-load door's
   trigger volume while moving toward the door, write `ActivateDoor` once. The volume: the door's bounds
   if it has them (the same bounds the portal quad uses), else about 160 wide x 240 tall x 60 deep
   around the door's origin. Do not re-trigger until the player has left the volume (so arriving next
   to a return auto-load door does not bounce them back). Hide the `E Open` prompt for auto-load doors.
4. **Demo tour** (`demo_tour.rs`): keep crossing every route door with `ActivateDoor` as today (the
   tour must stay deterministic), but log whether each route door is auto-load.

## Tests (fail before your change)

- `LoadDoor` for an `AutoLoadDoor01` base gets `auto_load = true`; a `DweDoorLarge01Load` base gets false.
- The trigger: a player position entering the volume while moving toward the door fires exactly one
  `ActivateDoor`; standing still inside does not fire again; moving away does not fire; a non-auto door
  never fires on contact.
- Arriving inside a return door's volume does not trigger until the player has left it once.

## Scope

The owned files only. No crosstalk partner. Report per the implementation template, including the
count of auto-load doors in the real data and which rule you used.
