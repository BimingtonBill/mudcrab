# research-027 - animated doors and seamless crossing: design

**Deliverable:** `docs/design/animated-doors-and-seamless-crossing.md` (for `research-027`, the
overnight wave-1 brief). Nothing under `crates/` was touched; no game data was copied anywhere.

## What is verified

* **Doors carry real `Open`/`Close` animations.** Read the bytes of
  `meshes/dungeons/dwemer/door/DwemerSmallDoorLoad01.nif`,
  `meshes/dungeons/dwemer/door/DwemerLargeDoorLoad01.nif` and
  `meshes/architecture/farmhouse/FarmhouseAnimDoor01.nif` out of the converter's extracted tree
  (`C:/Modding/SkyrimConverted/vfs/meshes/`): a `NiControllerManager`, one `NiControllerSequence`
  per animation, `Cycle Type = Clamp`, weight 1.0, a `NiMultiTargetTransformController`, a
  `NiTransformInterpolator` + `NiTransformData` per animated node, and a `NiDefaultAVObjectPalette`
  mapping node names to block indices. The controller sequence's `Text Keys` name the sound events
  (`Sound: DRSDwemerSmall01Open`).
* **The `NiTransformData` layout is fitted and validated**: `Num Rotation Keys`, rotation type
  `XYZ_ROTATION_KEY`, then three nif.xml `KeyGroup`s (X, Y, Z: `Num Keys`, `Key Type`, keys), then
  a zero-key translation group. It consumes **645 of 645** `NiTransformData` blocks of the
  door-ish NIFs in the install exactly. Values are radians about the node's local axis; times are
  seconds. Calibration: `FarmhouseAnimDoor01`'s `Door01` swings -95.000 degrees over 1.0 s, and two
  dwemer nodes' rest transforms in the exported GLB equal their curve value at t = 0.
* **The vendored parser cannot read them yet.** `NiTransformController`,
  `NiTransformInterpolator` and `NiTransformData` are `// TODO` stubs and have no match arm, so
  they parse as `NifBlock::Unhandled` (`vendor/project-wormhole-nif/src/nif_block.rs:1205`,
  `:1209`, `:1213`, `:527`). Safe to extend: block sizes come from the header, so nothing is
  misread today.
* **The converter exports no animation at all** - `dwemersmalldoorload01.glb` has
  `"animations": []` - but it does preserve node names and rest TRS, which is what a clip needs.
* **The demo route's three real doors are all `DweDoorLarge01Load`** and their `Open` sequence
  moves their leaves by **5.4 and 8.7 degrees**; the fourth route door is an invisible
  `AutoLoadDoor01` marker with no model and no animation. **This is the finding the lead needs.**
  Implementing "the real animation" faithfully will look nearly static on the current route.
* **The portal's map is already the right crossing map.** `door_to_arrival_rotation` /
  `portal_pose` (`crates/engine/src/portal.rs:380`, `:390`) is rigid, keys off the `XTEL` arrival
  frame rather than the destination door, and the portal camera is placed by it - so applying it to
  the player's eye pose makes the doorway image and the real destination agree in the swap frame by
  construction.
* **Two concrete seamlessness bugs**: the leaf is un-hidden within 1 unit of the door plane
  (`portal.rs:142` + `portal.rs:875` + `portal.rs:753`), and the whole-screen ambient, clear colour
  and fog change with the active space (`app.rs:1395`), so the swap frame pops.
* **Bevy 0.19 facts the design depends on** (read in the registry, cited with `#L` in the design
  because `check_citations.py` resolves only repo-relative paths): `bevy_gltf` inserts an
  `AnimationPlayer` but **no `AnimationGraph`**, so the engine must build the graph and insert
  `AnimationGraphHandle` beside it; `RepeatAnimation` defaults to `Never`, and `ActiveAnimation::update`
  returns before the wrap-around, so a finished clip holds its last key - which is how a door that
  is open stays open.

## What is still unknown

1. **The translation/scale groups of `NiTransformData` are not fitted.** 637 of the 645 door blocks
   carry data after the rotation curves (the 8 I checked are all-zero); the payloads contain
   plausible times and look like further `(count, type)` groups, but I did not pin their field
   order. Task 1 of the plan must fit them before it can claim to export a door faithfully - a door
   that slides would otherwise be exported as a door that does not move. The fitter to extend is in
   `tools/research/door_animation_nif.py` (`fit_layout`).
2. Whether a 1-2 frame hole is visible at the swap: `update_portal` refuses to place its camera
   within `MIN_PORTAL_DOOR_DISTANCE` (1 unit) of the plane (`portal.rs:142`). The state-driven leaf
   hiding in the plan covers the leaf; the hole itself needs the runtime check in task 4.
3. Whether the engine can reach a clip's final pose reliably on the first frame (open question 3 in
   the design).
4. `--shots` cannot currently render an open door (no player, no way to pin an animation time).

## Blocked, not worked around

`Skyrim - Meshes0.bsa` stores its payloads as **LZ4 frames** and the Python `lz4` package is not
installed, so `tools/research/door_animation_nif.py` cannot read a NIF straight out of a BSA
(`python tools/research/door_animation_nif.py dump <bsa> <entry>` reports this and stops). I did not
write a substitute decoder. The work went ahead through the converter's already-extracted tree at
`C:/Modding/SkyrimConverted/vfs/meshes/`, which holds the same NIFs - so nothing in this note is
weaker for it, but the `dump`/`list`/`doors` commands need `python -m pip install lz4` to run.

## Files

* `docs/design/animated-doors-and-seamless-crossing.md` - the design (data path, state machine,
  crossing math, edge cases, four-task plan with disjoint owned files, schema/reconversion call).
* `tools/research/door_animation_nif.py` - BSA reader, NIF header + animation-block decoder, layout
  fitter, census, GLB inspector. **The brief asked for the design file and a handoff note only**; I
  committed these two tools as well because the plan's acceptance and verification steps name them
  as the instruments. The lead can drop them.
* `tools/research/route_door_models.py` - the route's door models from `skyrim_world.db`, read-only.

## Next step

The lead reviews the design and picks: (a) the demo route's doors (section 1.4 - a faithful dwemer
door barely moves), and (b) whether task 4 (the crossing map, which needs no converter work and
stops the teleport with today's static doors) should land first, ahead of the animation pipeline.
