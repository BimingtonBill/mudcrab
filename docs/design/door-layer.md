# The door layer (portal track)

Status: design, 2026-09-26. The user's direction from the demo of 2026-09-26 17:26
(`local/captures/2026-09-26_17-26-03`, notes 01, 03, 04): "a door replacer that doesn't
necessarily render in exterior or interior spaces rather than trying to match doors on both
spaces"; "doors should render on their own separate layer".

## The problem

A load door is two references, one in each space (Riverwood's `0001CBB0` outside, its twin inside
Sven's House). Today each space draws its own copy of the door, and the portal machinery keeps the
two consistent: one canonical leaf per pair drawn as a mirror (`PortalDoorMirror`,
`LeafShownByMirror`, `RoomLeafPairs`, `ROOM_LEAF_LAYER`), the far door driven from the near one
(`drive_far_doors`), leaves hidden or shown per state. Every rule in that set is a place for the two
copies to disagree, and the demo keeps finding them:

- the Dragonsreach entrance drew a second, closed pair of leaves behind the open pair (note 03;
  research-250: one reference, one model; the extra pair is the mirror);
- inside Dragonsreach the doorway was black, with the frame drawn and no leaf and no portal
  (note 02; research-250: a leaf hidden for a mirror that is not there);
- the destination's door and frame are drawn *inside* the portal texture, so at a low portal scale
  they are pixelated (note 04), and a leaf that does not fill the other side's arch shows sky around
  it (note 01, Whiterun's main gate from inside the city).

## The design

**A near load-door pair is drawn once, by one door entity that belongs to neither space.**

1. **The door layer.** For each load-door pair near the player (the pairs the portal already
   tracks), one *door-layer door* is spawned from the model of the door on the player's side: frame
   and leaves, animated by that door's own clips and `DoorState`. It is drawn by the main camera
   only, at full resolution, on its own render layer (`DOOR_LAYER`). The portal camera never draws
   it.
2. **Both native copies are hidden** while the pair is on the door layer: the player's-side door
   reference and its twin in the destination (so the portal image shows only the room or street
   beyond the doorway, never a door). A pair that is not on the door layer (far away, not a load
   door, a non-obvious transition such as a trapdoor) draws its native copies exactly as today.
3. **The doorway is filled in one place.** The portal quad sits in the doorway behind the door-layer
   door; depth decides what shows (the depth composite already exists). A closed leaf covers the
   quad; an open leaf reveals it. No state can leave the doorway with neither.
4. **Crossing keeps the same door.** At the swap, the door-layer door is not replaced: its transform
   is re-expressed in the new active space through the doorway map (the `DoorMap` that already
   aligns the two doorways), and it keeps its model, pose and animation. There is nothing to pop.
   Its model is the side the player came from; it becomes the new side's model only at a moment the
   switch cannot be seen: once the door is closed and the player is more than a few metres away, or
   the doorway is out of view.
5. **Scale and fit.** Where the two sides differ (Whiterun's gate is 1.0 outside and 1.1 inside),
   the door-layer door uses the player's side, and the doorway map carries the scale, so the leaf
   fits the arch the player is looking at.

## What it replaces

The mirror (`PortalDoorMirror`, `place_door_mirror`, `RoomLeafPairs`, `LeafShownByMirror`,
`ROOM_LEAF_LAYER`, the canonical-leaf choice) and the far-door driving (`drive_far_doors`) exist only
to keep two copies in agreement; with one copy they go. The doorway alignment (`DoorMap`, the floor
anchor probe), the swing scaling (`swing_factors`, double doors), the leaf-width doorway window
(`LeafOpening`) and the depth composite stay.

## The eye adjusts on crossing (note 05)

The user, on the portal-scale pop: "should imitate the real human eye adjusting to the new space".
Two parts:

1. **Before the crossing**, the portal's resolution rises from the portal-scale setting to full as
   the player approaches the doorway, so the image they step into is already close to full detail.
2. **After the crossing**, the new space comes into focus like an eye adjusting: a short soft-focus
   that clears over about half a second, stronger the lower the portal scale was, alongside the auto
   exposure's own adaptation (which already imitates the eye's brightness adjustment). It must never
   read as a transition effect: at portal scale 1 it is nil.

## Checks

- The Riverwood short tour and the full Riverwood tour pass.
- Captures at the user's poses show one door and no black doorway: Dragonsreach outside (03) and
  inside (02), Whiterun's gate from both sides (the 10/11 poses of the 11:44 captures and note 01),
  Sven's House, the Trader.
- At portal scale 0.1, the door in front of the player is sharp (note 04).
- Frames either side of a crossing are the same view (the tour's walk frames).
