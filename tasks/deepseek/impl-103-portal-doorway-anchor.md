# impl-103 - Anchor the portal on the two doorways instead of the teleport target

Owned files: crates/engine/src/transition.rs,crates/engine/src/portal.rs,crates/engine/src/doors.rs,crates/engine/src/world/database.rs,crates/engine/src/streaming.rs,crates/engine/src/player.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Read first: `docs/research/portal-door-alignment.md` (the measurement this brief comes from and the
design in its section 9), `docs/design/animated-doors-and-seamless-crossing.md`, and the module docs
of `transition.rs` and `portal.rs`.

## Goal

Make the interior seen through an open load door line up with the doorway: the **destination
doorway** must be drawn where the **source doorway** is, instead of the destination being anchored
to the link's `XTEL` teleport target. Today the room behind the door is drawn a median 75 units
(ninety-second percentile 138) from where the source doorway is, and 92% of the time *nearer the
viewer*; the room is turned behind the door by a median 2.65 degrees, over 45 degrees for 5.9% of
the directions whose two doorways are known. At Sven's House (`0x0001CBB0` -> `0x0001CBAF`, the
user's report) that is 82.8 units and +12.27 degrees.

The fix is a per-door choice of anchor, from the data, applied to the drawing, the crossing and the
streaming paths alike. Where the data does not support it, today's behaviour must be preserved
exactly.

## Context: the map today

```text
frame   = door_frame(door_rotation, outward)          # transition.rs:237, yaw-only, -Z is `outward`
M_rot   = arrival_rot * Y180 * frame^-1               # transition.rs:287
pose'   = arrival_pos + M_rot * (pose - door_position) # transition.rs:303
```

`door_position` is the source door's `GlobalTransform.translation()` (the reference origin);
`arrival_pos`/`arrival_rot` come from the source door's own `door_links` row, the `XTEL` arrival
point and `arrival_camera_rotation` of its heading (`transition.rs:216`). The portal camera
(`portal.rs:1805-1827`), the crossing (`transition.rs:357-379`, used at `:582-609`) and the doorway
mirror (`portal.rs:1122-1143`, `:1189-1223`) are all placed by it, which is why the frames either
side of a swap agree by construction.

Three more things are built from the *same source frame* and carry its error:

- the portal quad (the window) is laid out in `door_frame` (`portal.rs:1807`, `:1844-1855`), so it is
  oblique to the real doorway by the source term's error (11.6 degrees at Sven's House);
- the crossing fires on the plane through the doorway box centre with normal `frame * -Z`
  (`player.rs:1087`, `:1100-1112`), up to 8 units before it;
- the clip/near plane (`portal.rs:1818-1823`, `:537-547`) is documented as "the image of the source
  door's plane under the mapping", which it must remain.

## Context: the map this must become

Same shape, different anchor:

```text
M_geom = T(destination doorway centre) * R(destination doorway facing) * Y180
         * R(source doorway facing)^-1 * T(-source doorway centre)
```

Four details that are easy to get wrong (all checked against the source):

1. `door_to_arrival_rotation` **already contains the `Y180`** (`transition.rs:288`). Call it as
   `door_to_arrival_rotation(source_doorway_frame, destination_doorway_frame)` with no extra half
   turn.
2. The **pivot** must become the source doorway's centre as well. `portal_pose`/`crossing_pose`
   pivot on the point they are given (`transition.rs:589`, `portal.rs:1810`, `:1130`); changing only
   the arrival leaves the map off by `R * (centre - origin)` - 88 units of height for a farmhouse
   door, i.e. the feet land half a doorway in the air.
3. The **doorway centre** is the model bounds box centre placed by the reference:
   `position + scale * (rotation * bounds_centre)`, with `statics.bounds_*` the *converted* model's
   bounds already in runtime (Y up) space (`converter/src/integration.rs:102`, `streaming.rs:1205`,
   `portal.rs:643-699`). Not the reference origin. 1,863 of the 2,204 doors in the installed data have usable
   bounds.
4. The map must stay a **pure yaw**: the portal maps the eye while the crossing maps the feet and
   adds `EYE_HEIGHT` (`portal.rs:1741`, `transition.rs:594-608`), and they agree only while it is.
   Do not let a tilted reference frame into it.

## What to build

1. **The tiers** (report section 9.2), decided per door from the data:
   - **tier 1 - same model**: the source and destination doors have the same base model, both have
     bounds, and the arrival gate below passes. Full doorway anchor. **No per-model convention is
     needed**: `R(yawB + C + 180) * R(yawA + C)^-1 = R(yawB + 180 - yawA)`, so the two references'
     own rotations and their doorway centres are the whole map. 1,356 of 2,204 directions in the
     installed data.
   - **tier 2 - both conventions known**: different models, both models' placements agree on an axis
     convention, both have bounds, arrival gate passes. Full anchor. 185 directions.
   (Note: 259 of the 2,204 doors have a reference scale other than 1, so the doorway centre is
   `position + scale * (rotation * bounds_centre)`, not the unscaled box centre.)
   - **tier 3 - centre only**: bounds on both sides and the arrival gate passes, but a facing cannot
     be established. Anchor on the doorway *centres* and keep today's facings (the source frame from
     `outward`, the destination facing from the arrival heading). 55 directions.
   - **tier 4 - unchanged**: no bounds, or the gate fails: today's map, byte for byte. 608
     directions (358 with no doorway bounds box, 234 that fail the height gate, 16 the plan cap),
     320 of them invisible or cave-mouth `AutoLoadMarker01` triggers where today's behaviour is
     correct.
2. **The arrival gate** (what "the game's arrival lands at the doorway" means):
   `plan distance(arrival point, destination doorway centre) <= 256 units` and
   `|the anchored landing's height error| <= 64 units`, the height error being
   `(arrival_inside_z - destination door z) - (arrival_outside_z - source door z)
    + (destination box-centre height) - (source box-centre height)`
   (report section 4.3). Use the constants named and documented in one place, not literals.
3. **A per-model axis convention**, from the database layer: for every model, the circular mean
   over its placements of `(link-derived facing - reference yaw)`, and a model counts as "known"
   when it has at least 4 placements and at least 80% of them are within 15 degrees of that mean.
   One aggregate query over `references` x `statics` x `door_links` at load is enough (2,204 rows);
   do not change the database schema and do not touch the converter.
4. **The destination door's own data**: today `DoorDestination` carries the ref id and the arrival
   frame, and `destination_ref_id` is written (`streaming.rs:1299`) and read nowhere. Carry the
   destination doorway's centre, facing and tier. For tier 1 the destination reference is a spawned
   entity whenever the destination cell is resident - which the portal and the crossing already
   require - and every reference carries its `FormId` (`streaming.rs:1217`), so an index by `FormId`
   is enough; a database lookup by id is equally acceptable. Say which you chose in the report.
5. **The drawing**: `update_portal` must use the new anchor for the portal camera **and** build the
   clip plane from the destination doorway's plane (leaving it on the old inputs starts the visible
   room ~70 units in - a missing band of floor and wall around the aperture). `place_door_mirror`
   uses the same anchor. With the clip moved, check what a *closed* destination door leaf does in
   the aperture (`portal.rs:1553-1617` hides only auto-load and source-door references) and say what
   you found.
6. **The crossing**: the `Mapped` path uses the new anchor; the player must land in the destination
   doorway, on the destination floor. **Decide and document the `Snap` path**
   (`transition.rs:583-586`, used only by `--demo-tour`, `demo_tour.rs:552`): if it keeps landing on
   the `XTEL` point it disagrees with what the portal just drew at anchored doors - state plainly
   what you chose and why, and update `transition.rs:1052`/`:1131` if you move it.
7. **The cell/streaming paths**: a link's destination cell is `door_links.destination_cell_id`, the
   resolved cell of the destination *reference*, not the arrival point's grid cell. The residency
   gate (`transition.rs:412-420`), the pre-stream grid (`:192-196`, `:383-406`) and the exterior
   origin rebase (`:475-481`) must agree with the anchor, or an exterior destination streams the
   wrong cell. `destination_cell_id` is nullable: no cell means no anchor (tier 4).
8. **Docs**: the module docs of `transition.rs`, `portal.rs` and `player.rs:480-488` all describe
   the `XTEL` anchor as the design; bring them to the new one, including the `AS DRAWN`/tier rules.

## Tests (write them to fail before your change)

- **The user's own case, from the real rows.** Fixture: Sven's House, `0x0001CBB0` (position
  20669.508, -46394.473, -122.135; rotation (0,0,2.6378073692321777) rad; its link arrival
  (-510.168, -198.877, -16.0), heading -0.03340071067214012 rad; bounds centre (0.0002, 88.0,
  -13.5); scale 1) and `0x0001CBAF` (position -511.494, -254.942, -16.0; rotation (0,0,pi); its link
  arrival (20644.045, -46318.414, -121.862), heading -0.48340344429016113 rad; same bounds). Assert
  that the map takes the source doorway's centre onto the destination doorway's centre within 1
  unit, that looking into the source door maps onto the destination doorway's facing within 1 degree,
  and that heights are preserved. Today's behaviour is 82.8 units and 12.27 degrees of error, so the
  test fails before your change.
- **Off-axis.** From a pose 200 units in front of the door and 200 units to the side, the drawn
  destination doorway centre must still land on the source doorway centre. A test that only looks
  along the doorway normal will not catch a 12-degree turn.
- **Tier 1 without any convention**: a fixture pair sharing a model, with no convention available,
  must still be anchored exactly.
- **Tier 4 is exactly today's behaviour**: for a door with no bounds (or a gate failure), assert the
  mapped pose equals `arrival + (M_engine rotation) * (pose - door_position)` bit for bit.
- **The gate**: a pair whose arrival point stands 600 units inside the room keeps the arrival anchor.
- **The clip plane**: the near plane is still the image of the source doorway plane under whichever
  map is in force (`portal.rs:2137`, `:2291`, `:2840` are the existing ones to keep passing, with
  `portal.rs:3906` - the mirror's leaf landing on the same pixels - as the regression test for the
  drawing half).
- Keep `the_four_real_route_links_still_round_trip` (`transition.rs:871`) and the `Snap` tests
  passing where the anchor does not apply; where a test pins the old anchor at a door that now
  takes the anchor, rewrite it deliberately and say so in your report.

## Scope

Only the owned files. Do not change the converter, the database schema, or `crates/shared/`. Do not
run the demo against the real assets unless `docs/research/portal-door-alignment.md` section 8's
poses are what you are checking - the engine build is enough for the unit tests. Report per the
implementation template: what changed, the test names, what you could not do, and anything in the
design above that turned out to be wrong.
