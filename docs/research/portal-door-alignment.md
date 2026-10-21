# Portal door alignment: the engine's door -> arrival map against the two doorways

Research for `lead-102-portal-door-alignment`, 2026-09-23. Every number here comes from
`C:/Modding/SkyrimConverted/skyrim_world.db` (converter schema 19) read by
`tools/research/portal_door_alignment.py`, or from arithmetic shown in the text. Nothing in
`crates/` was changed; the engine was not run.

The engine's functions were ported into that tool line for line, one measurement was re-derived by
hand from the raw rows (`local/agents/runs/research-110-portal-svens-recompute.1/report.md`), the
formula itself was verified against the Rust (`research-108-portal-map-verify.1`), everything that
consumes the map was inventoried (`research-109-portal-map-consumers.1`), and the design below was
attacked by a critic (`critic-111-portal-alignment-design.1`) whose findings are folded in. The
port and the hand arithmetic agree to 0.01 degree and 0.1 unit on every number in section 5.

**The unit of measurement is one direction, not one pair.** Every door with a `door_links` row is
measured as a source: 2,204 doors, 2,204 directions, 1,102 reciprocal pairs. Both directions of a
pair are included deliberately - the map is built from the *source* door's frame, so a pair can be
badly wrong one way round and nearly right the other (Sven's House is +12.27 degrees looking in and
+0.19 looking out), and which one a person sees depends on which side of the wall they stand on.

---

## 1. What the user saw, and what it is

Standing outside Sven's House, looking through the open door, the interior does not line up with the
doorframe: its floor, walls and beams stand at the wrong angle and the wrong place behind the
opening. The cause is not a rendering fault and not a perspective artefact. It is a **constant error
in the rigid transform the portal maps the viewer's pose through**: the transform is anchored to
**Skyrim's teleport target** (the destination link's `XTEL` point and heading - where the game puts
the player) instead of to **the destination doorway's own geometry**. The game never needed them to
be the same thing.

Two errors follow, and both are large:

| | at Sven's House, looking in (`0x0001CBB0` -> `0x0001CBAF`) | over the 1,468 directions whose two doorways are both known | over all 2,204 directions |
|---|---|---|---|
| the destination room is **turned** behind the door by | **+12.27 deg** | median 2.65 deg, p90 16.4 deg, **5.9% over 45 deg** | median 4.56 deg, **20.0% over 45 deg** |
| the destination doorway is **drawn away from** the source doorway by | **82.8 units** (80.8 of it towards the viewer) | median **75.5 units**; 84% between 32 and 128 | (see the tail in section 7) |

Both are visible from any pose, level and square or not, because the map is rigid: whatever the
viewer does, the destination is drawn through the wrong transform. That is exactly why the user
described it as wrong "even when the camera is level and square to the door".

An 82.8-unit displacement is large next to the thing being viewed: the farmhouse doorway is about
100 units wide and 176 units tall, and the room's own doorframe is drawn 80.8 units *nearer the
viewer* than the doorway they are standing in - in front of the plane of the door, filling the
opening from the wrong side. The 12.3-degree turn is large next to a room a few hundred units deep.
Together they are the screenshot.

**The same error reaches three more places, and a fix that changes only the map leaves two of them
broken:**

- the **portal quad** (the window) is laid out in the same `door_frame` the map is built from
  (`portal.rs:1807`, `:1844-1855`), so it is oblique to the real doorway by the source term's error
  - 11.6 degrees at Sven's House, which puts ~13 units of the quad's edges outside the opening;
- the **crossing plane** is the same frame (`player.rs:1087`), so the walk fires up to 8 units
  early on an oblique plane (`player.rs:1112`);
- the **clip plane** that starts the drawn destination is built from the anchor
  (`portal.rs:1818-1823`, `:537-547`), so changing the anchor without changing the clip moves the
  first ~70 visible units of the room (section 9.3).

**Why the existing check could not see it.** `docs/research/portal-frames.md` section 1 compares the
last frame before a crossing with a steady frame after it and finds a (0, 0) offset. That is true
and it will stay true: the portal camera and the crossing are placed by *the same map*
(`transition.rs:287-316`, `:357-379`), so frames either side of the swap agree by construction
whatever the map is. The check is blind to the map being wrong. Every captured pose in that check
was also level and square to the door and stood in the doorway plane.

---

## 2. The map the engine builds

`M_engine`, in runtime axes (Y up, `[x, y, z, w]`), verified line by line by research-108:

```text
frame    = door_frame(door_rotation, outward)                   # transition.rs:237
M_rot    = arrival_rot * Y180 * frame^-1                        # transition.rs:287  door_to_arrival_rotation
pose'    = arrival_pos + M_rot * (pose - door_position)         # transition.rs:303  portal_pose
rot'     = M_rot * pose_rot
crossing = the same map, then the render-origin rebase          # transition.rs:357  crossing_pose
```

| input | where it comes from |
|---|---|
| `door_position`, `door_rotation` | the source door's own `references` row: its Creation position and its runtime rotation (`streaming.rs:1192-1204`; interiors absolute, exteriors rebased by the cell root, whose only contribution is the render origin - no hidden parent transform, no second basis change) |
| `outward` | `crates/engine/src/doors.rs:87` `outward_from_return_link`, Creation axes, read off the link that leads **back into** the source door: the direction from the door to that link's arrival point when it is 16..512 units away (the "point rule", 1,848 of 2,204 doors), otherwise that link's arrival heading (356 doors) |
| `frame` | `door_frame` (`transition.rs:237`): a pure-yaw frame whose `-Z` is `outward`, built as `Quat::from_rotation_y(-atan2(east, north))`. Falls back to the reference rotation when `outward` is `None` (no door in this install is in that state) |
| `arrival_pos`, `arrival_rot` | the source door's own `door_links` row: the `XTEL` arrival position (Creation units) and `arrival_camera_rotation` = `Quat::from_rotation_y(-z)` (`transition.rs:216`) - **only the `z` of the `XTEL` rotation is read** |

Consequences that matter for a fix:

- `M_rot` is a yaw about the vertical, so the map is rigid, keeps heights, and turns a look without
  tilting it. It is that property - not the choice of anchor - that lets the portal map the *eye*
  while the crossing maps the *feet* and adds `EYE_HEIGHT` afterwards (`portal.rs:1741`,
  `transition.rs:594-608`). Any replacement map must stay a pure yaw, or the two drift apart by
  `EYE_HEIGHT * (M*Y - Y)` - up to 240 units when the reference is tilted and `outward` is `None`
  (research-108, qualification A).
- The **destination door's own position and rotation never enter the map**. The destination is known
  to the engine only as `(arrival_pos, arrival_heading)` plus its cell. `DoorDestination.destination_ref_id`
  is written (`streaming.rs:1299`) and read nowhere in the crate (research-109 section 6). A
  doorway-anchored map therefore needs data the engine does not currently carry - though less of it
  than it first appears (section 9.5).
- `yaw(M_rot) = arrival_heading + 180 - frame_yaw`, so the map's angle error is exactly
  `(arrival heading - destination doorway facing) - (source frame - source doorway facing)`. The two
  terms are measured separately below.

---

## 3. The map the two doorways want

`M_geom`: the source doorway's own frame mapped onto the destination doorway's own frame, half a
revolution about the vertical so that walking into one door means walking out of the other:

```text
M_geom = T(destination doorway centre) * R(destination doorway facing) * Y180
         * R(source doorway facing)^-1 * T(-source doorway centre)
```

Same shape as `M_engine`, and `door_to_arrival_rotation`, `portal_pose` and `crossing_pose` already
compute it - with two details that a first reading gets wrong:

- **Call `door_to_arrival_rotation(F_source, F_destination)` and nothing else.** The function
  already contains the `Y180` (`transition.rs:288`), so adding one to `F_destination` turns the
  destination a second half-revolution. Feeding the two doorway frames straight in gives
  `F_dest * Y180 * F_src^-1`, which is the map above.
- **The pivot has to move too.** `portal_pose`/`crossing_pose` pivot on the argument they are given
  as `door_position` (`portal.rs:1810`, `transition.rs:589`), which today is the reference's origin.
  A doorway anchor has to pass the **source doorway's centre** as that pivot as well as the
  destination's centre as the arrival - changing only the arrival leaves the map off by
  `R * (centre - origin)`, which for the farmhouse door is 88 units of height, i.e. the player's
  feet land half a doorway in the air.

**The doorway's own centre** is the point the engine already measures for the quad: the model bounds
box centre placed by the reference, `position + scale * (rotation * bounds_centre)`, with
`statics.bounds_*` the *converted* model's bounds in runtime space (Y up) (`converter/src/integration.rs:102`,
`streaming.rs:1205`, `portal.rs:643-699`). 1,863 of the 2,204 doors have usable bounds (the missing
341 are mostly the invisible markers, whose model has none and whose portal falls back to
`DEFAULT_PORTAL_SIZE` on the reference origin, `portal.rs:590-594`); 259 have a reference scale other
than 1, which multiplies the box centre (`portal.rs:651-653`). It is not the reference's origin: for
the farmhouse door the box centre sits 88 units up and 13.5 units off along the model's own axis.
The quad uses the *silhouette* of that box in the frame rather than the box centre, which is the same
point when the frame and the rotation agree (research-108 section 3).

**The doorway's own facing** is the hard half, and this is where `impl-023`'s model-axis problem
lives. Three candidate answers, all Creation headings:

1. **the model's axis convention**: `reference yaw + C(model)`, where `C(model)` is the circular mean
   of `(link-derived facing - reference yaw)` over every placement of that model in the install.
   This is a fact about the model (its local front axis) measured from many placements, so a single
   odd link cannot move it.
2. **the link's arrival heading** - what the engine already uses as the *destination* facing.
3. **the link's arrival point direction** - what the engine uses as the *source* facing when the
   point is 16..512 units from the door.

Which one to trust is a question of the data, so it was measured (section 4). The tool's rule, used
throughout this report, is: **the model convention when the model's placements agree with each
other, the arrival heading when they do not.** "Agree" = at least 4 placements and at least 80% of
them within 15 degrees of the mean; 41 of the 104 door models pass, covering 1,468 of the 2,204
directions. The models that fail are the invisible `AutoLoadMarker01` (315 placements, whose
"facing" is whatever the trigger happens to point at), ship trapdoors, ladder doors, Dwemer facade
levers, and a few real door models whose placements sit in two orientations (`RiftenRWDoorLoad01`,
`WHmetalarchdoor`, `SHouseDoor02`).

The convention is not a fudge: research-110 rebuilt it from ten printed placements and got within
1.8 degrees of the full-install value, and where it is trustworthy the placements agree with it to a
median of 2.8 degrees. But it is evidence of *self-consistency*, not of correctness - it is fitted
to the very link-derived facings whose point-rule behaviour section 4.1 shows to be noisy - and where
it is used, it should be used with a per-door residual bound (section 9.4).

**Where the two doors share a base model, none of this matters.** The convention cancels out of the
map exactly:

```text
R(yawB + C + 180) * R(yawA + C)^-1 = R(yawB + 180 - yawA)
```

1,718 of the 2,204 directions (78%) join two doors of the same model, and 1,356 of those also pass
the arrival gate and have a doorway box - so **62% of the world's door directions can be aligned
exactly with no per-model convention at all**, from the two references' rotations and their doorway
centres, which the engine already loads for any reference whose cell is resident. This is the
cheapest and safest tier of the recommendation (section 9.2).

---

## 4. The distribution

2,204 directions (every door with a link, as the source of its own crossing). All 2,204 doors have a
return link, so every one has an `outward`.

### 4.1 The map's yaw error (how far the room is turned behind the door)

`yaw error = (destination doorway facing - arrival heading) - (source frame - source doorway facing)`,
in Creation headings: a positive value means the engine draws the destination turned anticlockwise
of where the two doorways put it.

| bucket | both doorways known (1,468) | one or neither known (736) | every direction (2,204) |
|---|---|---|---|
| <= 1 deg | 450 (30.7%) | 59 (8.0%) | 509 (23.1%) |
| 1..5 | 558 (38.0%) | 88 (12.0%) | 646 (29.3%) |
| 5..15 | 305 (20.8%) | 123 (16.7%) | 428 (19.4%) |
| 15..45 | 68 (4.6%) | 113 (15.4%) | 181 (8.2%) |
| > 45 | 87 (5.9%) | 353 (48.0%) | 440 (20.0%) |
| median | 2.65 | 37.60 | 4.56 |
| p90 | 16.44 | 173.57 | 155.96 |

The two terms on their own, over every door with a return link (2,204 doors, so these are symmetric):

| term | median | p90 | > 45 deg |
|---|---|---|---|
| destination: `XTEL` arrival heading vs the destination doorway's own facing | 1.56 deg | 122.6 | 15.9% |
| source: the engine's frame vs the source doorway's own facing | 3.00 deg | 84.5 | 13.1% |
| (the point rule against the doorway's own facing, over the 1,848 doors it applies to) | 2.56 deg | 30.5 | 8.7% |
| (the arrival heading against the doorway's own facing, same doors) | 1.28 deg | 79.1 | 12.0% |
| (`XTEL` arrival heading against the link-derived facing, all 2,204) | 1.75 deg | 15.0 | 5.1% |

The third and fourth lines are worth reading together: **the point rule and the heading each beat the
other somewhere.** The heading is better in the median (1.28 vs 2.56), the point is better in the
tail (8.7% vs 12.0% over 45 degrees). So there is no free win in simply switching one for the other:
the model convention - exact where it is known - is the answer, and the rule above keeps today's
behaviour where it is not.

### 4.2 The offset (where the destination doorway is drawn)

Over the 1,468 directions whose two doorways are both known, as the offset between where the source
doorway's centre lands under the engine's map and where the destination doorway's centre actually is
- exactly the displacement an eye sees at the opening:

| 3D offset | directions |
|---|---|
| <= 8 units | 2 (0.14%) |
| 8..32 | 23 (1.6%) |
| 32..128 | 1,231 (83.9%) |
| 128..512 | 207 (14.1%) |
| > 512 | 5 (0.3%) |
| **median** | **75.5 units** |

Split by direction, `depth` along the line of sight (`-` = drawn nearer the viewer than the source
doorway, i.e. the room pulled towards the camera):

| depth | directions |
|---|---|
| -256..-64 | 840 (57.2%) |
| -64..-16 | 507 (34.5%) |
| -16..16 | 24 (1.6%) |
| > 16 | 91 (6.2%) |
| sideways part | median 4.2, p90 18.9, max 169 |

So at almost every door in the game the room behind the door is drawn *too near* by tens of units,
and the direction of the error is the same everywhere. This is the systematic defect; the yaw error
is the one that varies door to door. The extreme buckets are at the edge of the measurement's
resolution (section 10) - the median and the p90 are not.

Why: the anchor is the `XTEL` arrival point, and Skyrim puts the arriving player **inside the room**,
not in the doorway. Over every direction the arrival point stands **median 70 units past the
destination doorway** (depth > 32 units for 94%), and median 131 units from the doorway's centre in
3D - the doorway centre being ~88 units up (the box centre), so most of that 131 is height and is
not an error.

### 4.3 The vertical

The map is a pure yaw, so it keeps every height; the anchored landing's height error is the
difference between the two sides' floor levels as each is measured against its own doorway, plus the
difference of the two models' bounds-centre heights above their references:

| the anchored landing's height error | directions |
|---|---|
| <= 8 units | 1,092 (49.6%) |
| 8..32 | 434 (19.7%) |
| 32..128 | 324 (14.7%) |
| 128..512 | 300 (13.6%) |
| > 512 | 54 (2.5%) |
| median | 8.3 units (p90 208.4) |

(Of it, the floors' own offsets against their doorways contribute median 5.1 units, and the
models' box-centre heights contribute the rest - which cancels exactly when the two doors share a
model, as they do at Sven's House, where the anchored landing would be 0.3 units off the floor.)

So the vertical is usually right by either anchor and is *not* what the user saw; it is the gate
that keeps ladders, trapdoors and other-level doors out of a doorway anchor (section 9.2).

### 4.4 Pitch and roll

The map difference is a pure yaw at every one of the 2,204 directions (|pitch| and |roll| both 0.000
degrees), because both maps are yaw-only by construction. The `XTEL` rotations carry a roll (`rot_y`)
above 2 degrees for 14 doors (max 22.8), which the engine drops; that is a rotation about the view
axis, not a yaw, and cannot be what the user saw. It is however silently discarded everywhere
(`transition.rs:214-217`).

---

## 5. Sven's House

Pair `0x0001CBB0` (exterior door, `Tamriel`, cell `0x00000D74`) / `0x0001CBAF` (interior door, cell
`0x0001CB84` `RiverwoodSvensHouse`), both the model
`Architecture\Farmhouse\FarmhouseLDoor01.nif`, whose convention over 159 placements is
**178.718 degrees** (median residual 2.8 degrees).

Raw rows (all from the database):

| | exterior `0x0001CBB0` | interior `0x0001CBAF` |
|---|---|---|
| reference position (Creation) | (20669.508, -46394.473, -122.135) | (-511.494, -254.942, -16.000) |
| reference yaw | 151.135 deg | 180.000 deg |
| doorway's own facing (yaw + convention) | **-30.147 deg** | **-1.282 deg** |
| engine's frame (link-derived outward) | **-18.510 deg** (point rule: the return link's arrival is 80.2 units away at bearing -18.51) | **+1.354 deg** (point rule: 56.1 units away) |
| bounds centre (model frame) | (0.0, 88.0, -13.5) | (0.0, 88.0, -13.5) |
| doorway centre (Creation) | (20676.02, -46406.30, -34.13) | (-511.49, -268.44, +72.00) |
| its link's arrival heading | -1.914 deg (inside, 56.1 units from the interior door) | -27.697 deg (outside, 80.2 units from the exterior door) |

**Looking in (`0x0001CBB0` -> `0x0001CBAF`), the user's case:**

```text
destination term = -1.282 - (-1.914) =   +0.63 deg   the interior doorway agrees with the XTEL heading
source term      = -30.147 - (-18.510) = -11.64 deg  the exterior door's frame is 11.6 deg off its own doorway,
                                                     because the engine took the facing from the arrival point,
                                                     80.2 units out and 11.6 deg off the door's axis, instead of
                                                     from the door's own facing or the arrival heading, -27.70
map yaw error    = +0.63 - (-11.64)   = +12.27 deg
as drawn         = the interior doorway stands 82.8 units from the exterior doorway: depth -80.8
                   (nearer the viewer), sideways 17.9, vertical 0.0
```

**Looking out (`0x0001CBAF` -> `0x0001CBB0`):** the two terms nearly cancel - destination -2.45,
source -2.64 - so the map yaw error is **+0.19 deg**; the exterior doorway is drawn 106.9 units from
the interior doorway (depth -105.5). The asymmetry is real and worth knowing: the error is a
property of the *source* door's frame, so the same doorway can be badly wrong one way round and
nearly right the other. It is also why the source-frame rule deserves as much attention as the
destination one.

**Does that explain the screenshot?** Yes, on both counts, and by a wide margin:

- 82.8 units of displacement is 47% of the doorway's 176-unit height and 83% of its width. The
  interior's own doorframe and the room behind it are drawn *in front of* the plane the viewer is
  looking through, so the room does not sit behind the doorframe at all.
- 12.3 degrees of turn is the "wrong angle" itself, and it is the larger of the two for a level,
  square-on view, because the room's vertical edges (its doorjamb, wall corners, beams) are what the
  displacement leaves visible.
- and the quad, the crossing plane and the clip plane inherit the same frame error (section 1), so
  the aperture itself is oblique to the doorway by 11.6 degrees at this door.

The hand check (research-110) reproduced all four numbers - +12.269 deg, 82.77 units, 80.81 depth,
106.87 units - from the raw rows alone, and flags one presentation detail: the tool prints the
`AS DRAWN` depth with the opposite sign to a component along the door's own facing; this report's
tables use "negative = drawn nearer the viewer".

---

## 6. The worst directions

The engine's map turned furthest from the two doorways' geometry (top 12 of 20; the rest are the
same shapes). "source -> destination" names the direction the user would be looking in.

| yaw error | direction | source term | destination term | offset |
|---|---|---|---|---|
| 180.00 | `000C0E0F` WindhelmPalaceoftheKings -> `000C0E13` WindhelmPalaceUpstairs02 | -173.2 | +6.8 | 107 u |
| 180.00 | `0201534F` Tamriel -> `01002FDD` ccPlaceholder02Interior01 | +81.3 | -98.7 | 0 u |
| 180.00 | `01002FCC` ccEEJSSE003_Interior01 -> `01002FC9` RiftenRatway01 | +81.3 | -98.7 | 40 u |
| 180.00 | `00089D5F` RiftenBeggarsRow -> `00089D60` RiftenWorld | -2.0 | +178.0 | 122 u |
| 180.00 | `01002FE4` ccPlaceholder06Interior01 -> `020153C0` Tamriel | -98.7 | +81.3 | 213 u |
| 180.00 | `000BF22B` SolitudeCastleDour -> `000BF22C` SolitudeJail01 | -178.1 | +1.9 | 104 u |
| 180.00 | `0009CCE0` RiftenRomlynDrethsHouse -> `0009CE34` RiftenWorld | -2.0 | +178.0 | 120 u |
| 180.00 | `00056660` FortSnowhawk01 -> `00057FBC` Tamriel | +0.1 | -179.9 | 196 u |
| 180.00 | `00089D60` RiftenWorld -> `00089D5F` RiftenBeggarsRow | +178.0 | -2.0 | 97 u |
| 180.00 | `0200748E` DLC1VampireCastleCourtyard -> `0200F5EC` DLC1VampireCastleGuildhall | +178.4 | -1.6 | 196 u |
| 180.00 | `0006DCB0` ThalmorEmbassy03 -> `0007DED3` ThalmorEmbassy02 | -22.3 | +157.7 | 101 u |
| 179.99 | `02012F73` DLC1DarkfallPassageWorld -> `02012F74` DLC01FalmerValley | +0.2 | -179.8 | 197 u |

The offsets are the same 0-200-unit range as everywhere else; the angles are not. The list also
shows the asymmetry: the Riften Beggars' Row pair appears in both directions (`00089D5F` ->
`00089D60` at 180.00 degrees, and back at 180.00 with the terms swapped), because each direction
carries its own source door's frame. Section 7 says what those 180-degree rows are made of.

---

## 7. What the 180-degree tail is made of

Three different things, and they need different answers:

1. **Doors that are not doorways at all.** `AutoLoadMarker01.nif` (315 doors) is an invisible trigger
   with no opening; its "facing" is whatever the trigger faces, so a yaw error measured against it
   means nothing. Cave mouths are the same case: `0002893F` Tamriel -> `00031898`
   BleakFallsBarrow02 is an auto-load marker whose arrival point is 701 units away, and the measured
   "-166 degrees" is the marker's own axis, not the cave's. The right behaviour there *is* today's
   arrival-anchored map, which is why the recommended rule keeps it. 320 of the 608 directions that
   would keep today's map under section 9's rule are these.
2. **An individual door whose own placement disagrees with its model's convention** - the model as a
   whole passes the "80% within 15 degrees" rule, but this placement does not (the other 20%). The
   convention is a population fact and predicts nothing for that door. The worst such cases
   (`00089D5F`/`00089D60` in Riften, both real wall doorways 122 units apart with matching arrival
   points, measured at 180 degrees) are this: the pairwise geometry is fine and the convention is
   the thing that is wrong. A per-door residual bound is what keeps the anchor away from them
   (section 9.4).
3. **Genuine data disagreement**, where the door is a real wall doorway with a trustworthy
   convention and the `XTEL` heading truly points elsewhere. The clear case is `00056660`
   FortSnowhawk01 -> `00057FBC` Tamriel: `ImpWoodDoorDoubleLoad01` has a tight convention (18
   placements, median residual 1.7 degrees), the interior door is at yaw 90 with its arrival 184
   units east of it, the exterior door at yaw 225 with its arrival 128 units out along its own
   facing - and **both** arrival headings are 180 degrees from their own door's facing: the player
   emerges from each door facing back into it. Under a doorway anchor this case is *improved*, not
   broken: the map carries "looking into the doorway" onto "looking into the room", so the player
   comes out facing the room instead of the door. The cost is that the landing differs from the
   game's own by 138-196 units, inside the gate in section 9.2.

So of the 5.9% of known-doorway directions over 45 degrees (87 of 1,468): most are group 2 (the
convention cannot decide that door) and a minority are group 3 (real data). The group-1 rows are
only in the "one or neither doorway known" column and are not defects at all. This split is the
largest uncertainty in the report, and it is what the gate in section 9.2 is built around.

---

## 8. Check sites across the world

Standing poses computed from the door's engine frame at 200 units out, eye 120 units over the door's
own `z`, looking back at the door (`pitch 0`). `position` is Creation `(east, north, up)`; `yaw` is a
Creation heading. `as drawn` is the offset between the destination doorway and the source doorway
under the engine's map (negative depth = the destination is drawn nearer the viewer), for the
direction named in the fourth column.

| # | stand at (space) | through it, into | yaw error | as drawn | pose |
|---|---|---|---|---|---|
| 1 | `0001CBB0` Tamriel, cell `00000D74` (Sven's House, the user's case) | `0001CBAF` `RiverwoodSvensHouse` | **+12.27** | 82.8 u (depth -80.8) | pos (20606.0, -46204.8, -2.1), yaw 161.5 |
| 2 | `00013424` Tamriel, cell `00000D74` (Sleeping Giant Inn) | `00013419` `RiverwoodSleepingGiantInn` | +0.16 | 89.2 u (depth -88.9) | pos (23171.1, -44397.2, 111.7), yaw 131.7 |
| 3 | `00013420` Tamriel, cell `00000D74` (Alvor and Sigrid's House) | `000133FB` `RiverwoodAlvorsHouse` | +3.25 | 76.1 u (depth -75.9) | pos (21503.5, -44880.8, 38.7), yaw -43.7 |
| 4 | `00036F00` WhiterunWorld, cell `0001A270` (Uthgerd's House) | `000918E6` `WhiterunUthgerdTheUnbrokensHouse` | **-14.96** | 109.2 u (depth -74.5) | pos (21287.1, -4534.2, -2705.8), yaw 96.4 |
| 5 | `00037F24` SolitudeWorld, cell `00037EE6` (Angeline's Aromatics) | `00016A68` `SolitudeAngelinesAromatics` | +6.09 | 131.4 u (depth -46.2) | pos (-63375.2, 106268.2, -8164.0), yaw -8.9 |
| 6 | `00037B0C` BleakFallsBarrow02, cell `000371DD` (inner Nordic door, the best case) | `00038DFB` `BleakFallsBarrow01` | +1.05 | **15.2 u (depth -3.0)** | pos (7171.7, 1160.0, -520.0), yaw -178.9 |
| 7 | `0006998D` AlftandWorld, cell `00069869` (the demo route) | `0004E504` `Blackreach` | -0.00 | 47.7 u (depth -47.6) | pos (-4255.8, 855.7, 1424.8), yaw -125.0 |
| 8 | `00092809` Alftand01, cell `000152C3` (the demo route) | `0005704B` `Alftand02` | +5.19 | 47.6 u (depth -47.1) | pos (3858.7, 8817.7, -396.0), yaw 97.4 |
| 9 | `00056660` FortSnowhawk01, cell `00015276` (the 180-degree case) | `00057FBC` Tamriel | **-180.00** | 188.6 u (depth +181.1) | pos (1800.0, 64.0, -200.0), yaw -90.0 |
| 10 | `0002893F` Tamriel (Bleak Falls cave mouth, an auto-load marker) | `00031898` `BleakFallsBarrow02` | -166.04 | 708.1 u | pos (-12581.1, -47477.3, 4088.0), yaw 48.2 |

Sites 1-5 are the common case (a real house or shop door, a real error of 10-140 units and 0-15
degrees); 6 and 7-8 are the well-authored and demo-route cases where the map is nearly right, so a
fix must not disturb them; 9 is a genuine data disagreement (and a case the anchor improves); 10 is
a door with no doorway, where today's map is the right answer. Sites 2 and 3 also have the reverse
direction measured (+5.23 and -1.42 degrees), and site 4's reverse is -5.66.

Every one of these views needs the door **open** to show anything: the portal only exists while a
door is open, and `--shots` cannot open one (`tasks/deepseek/impl-150-portal-shots-open-door.md` is
the task that gives it that). Until that lands, the sites can still be walked by hand with
`play-riverwood-demo.cmd` or the engine's `--demo`/`--start-position` switches, standing at the poses
above. **Look from off-axis too**: a pose 200 units out and 200 units to the side is what makes the
turn visible, and 12 degrees on a level, square-on view is subtle.

The Alftand route (`00015D48` Tamriel -> `000152CF` Alftand01, `00092809`, `0009256A`, `0006998D`)
is why the demo looked right: its doors carry 0-5 degrees of yaw error and 47-57 units of offset,
better than the median. A tour that only walks this route cannot see the defect, which is one more
reason the check sites above are wider. **A check must also look from off-axis**: every existing
captured pose is square to the door, and 12 degrees of yaw error is nearly invisible when the view
is centred on the doorway and level.

---

## 9. The design question

### 9.1 The three options, with what the data says each costs

**A. Draw and cross with `M_geom`.** The doorway lines up exactly, by construction, and the walking
crossing stays seamless in the strong sense it has today (one map for the camera and the crossing,
and it is rigid). Cost: the player lands where the *doorway* is, not where the game puts them -
median 70 units closer to the door than the `XTEL` point (p90 641, max 57,500 for a mis-linked
pair). Where the two sides' floor levels do not sit alike against their doorways the landing height
changes too (section 4.3), and the destination doorway's placement is not in the engine's data at
all today.

**B. Draw with `M_geom`, cross with `M_engine`, hide the difference.** The player keeps landing
exactly where Skyrim puts them. Cost: a jump of the map difference at the swap - median ~100 units
and 2.7 degrees, at Sven's 82.8 units and 12.3 degrees, and whatever the pair's error is at the
tail. `demo_tour.rs`'s walk-through frames are where it would be seen, and the only way to hide it
is a fade or a blend while the player is looking straight through the doorway - the one moment the
feature exists for. Note that today's walking crossing already fires up to 8 units *before* the
doorway plane (`player.rs:1104-1112`), so the landing is already a few units off the `XTEL` point;
a second anchor makes that gap visible rather than harmless.

**C. Keep `M_engine`, correct its inputs.** This fixes only the *facing* half, and only where the
input rule is wrong. The measurable part is the source term, where the point rule is worse than the
arrival heading in the median but better in the tail (section 4.1), so switching rules is not a
clean win. Correcting the inputs with the model convention fixes that term almost exactly where it
is known - and it must be done anyway (the quad, the crossing plane and the clip all use the same
frame) - but **it cannot touch the offset**: the room is still drawn from the arrival point, 70
units inside, so the doorframe still does not line up. This option does not fix what the user
reported.

### 9.2 Recommendation

**A per-door choice of anchor, from the data, applied to the drawing, the crossing and the
streaming paths alike.** In order of preference:

| tier | when | what it does | directions |
|---|---|---|---|
| **1. same model** | the two doors share a base model, both have bounds, and the arrival gate below passes | full doorway anchor; no convention is needed at all, because it cancels (section 3) | 1,356 (61.5%) |
| **2. both conventions known** | different models, both models' placements agree, both have bounds, arrival gate passes | full doorway anchor | 185 (8.4%) |
| **3. centre only** | bounds on both sides and the arrival gate passes, but a facing cannot be established | doorway *centre* anchor with today's facings: the offset (section 4.2) is fixed, the facing error stays | 55 (2.5%) |
| **4. arrival anchor** | anything else - no bounds, or the arrival gate fails | unchanged, today's map | 608 (27.6%) |

**70% of the world's door directions get exact alignment** (tiers 1-2), 2.5% more get the
systematic offset fixed, and 27.6% keep today's behaviour. The arrival gate, which is what tier 4's
"arrival gate fails" means:

```text
the game's arrival point stands within 256 units of the destination doorway's centre in plan
and the anchored landing would stand within 64 units of the destination floor
```

Why this shape and not plain A: the gates are what make it safe. The plan cap bounds how far the
player is moved from the game's own landing; the height cap keeps the 234 directions whose anchored
landing would leave the destination floor - ladders, trapdoors, other-level doors - on the arrival
anchor, where the vertical is the whole point; the missing-bounds and convention gates keep the 358
doors with no doorway box and the doors whose facing cannot be established on the behaviour they
have today. Why not B: it converts a constant, invisible map error into a visible jump in the
doorway, the one place the feature is watched. Why not C alone: it leaves the offset.

Threshold sensitivity, so the two numbers can be chosen rather than guessed (all 2,204 directions):

| plan cap | height cap | anchored | player moved: median / p90 / max | tier 4 keeps > 45 deg |
|---|---|---|---|---|
| 64 | 64 | 28.4% | 52 / 61 / 64 | 26.9% |
| 128 | 64 | 59.8% | 65 / 94 / 127 | 41.4% |
| **256** | **64** | **63.6%** | **66 / 105 / 245** | **44.8%** |
| 256 | 128 | 66.0% | 66 / 107 / 245 | 47.6% |
| 512 | 128 | 66.2% | 66 / 110 / 433 | 47.8% |

The vertical cap is close to free (64 -> 128 buys 2.3 points of coverage for 3 points more height
error); the plan cap is not (64 -> 128 buys 31 points of coverage and doubles the p90 landing
displacement). 256/64 is the knee, and ~1.5 doorway widths is the reason: past that the game is
pointing somewhere other than the door. Note that the same-model tier is unaffected by the plan cap
in practice - those doors are the ones Skyrim authored consistently - which is why tier 1 covers
62% of directions at any cap.

### 9.3 What the crossing has to do with it, and the four things the anchor moves

Drawing and crossing must use **one** map, and it must keep its yaw-only property (section 2), or
the portal's eye mapping and the crossing's feet mapping drift apart.

- Doors in tiers 1-3: the crossing is the same `crossing_pose` with the new anchor and the new
  pivot. The player lands in the destination doorway at the height their feet had above the source
  door's base - which section 4.3 shows is the destination floor whenever the height gate passes,
  and the gate is what guarantees it.
- Doors in tier 4: unchanged, including the `Snap` path (`transition.rs:583-586`,
  `demo_tour.rs:552`). **But `Snap` is a style, not a door property**: at an anchored door the
  portal would be drawn with `M_geom` while `ActivateDoor` still lands the camera on the `XTEL`
  point - option B's jump, in the path `--demo-tour` uses when it is not `--walk`. Either the snap
  path has to move to the same anchor (and `transition.rs:1052`/`:1131`, which assert the `XTEL`
  landing, be rewritten with it) or the tour has to be told the two paths differ. This is the first
  thing to settle before implementing.
- **The clip plane** (`portal.rs:1818-1823`, `:537-547`) is built from the anchor and is documented
  as "the image of the source door's plane under the mapping". Under a doorway anchor its inputs are
  no longer that image, and left alone the near/oblique clip starts the visible destination ~70
  units into the room - a band of missing floor and wall around the near edge of the aperture,
  trading the reported defect for another of the same size. It has to be built from the destination
  doorway's plane.
- **The destination door's own model** is invisible today largely by accident: the leaf and frame
  stand inside that clipped slab. With the clip moved to the destination doorway plane, a *closed*
  destination leaf sits exactly in the aperture. Nothing hides the destination door today
  (`portal.rs:1553-1617` hides auto-load doors and the source door the portal shows), so this has to
  be decided deliberately.
- **The doorway mirror** (`portal.rs:1122-1143`, `:1189-1223`, placed by `portal_pose`) moves with
  the map. It exists for animated doors with a scaled-up swing - exactly the Riverwood house doors
  of the demo - and under the anchor it moves from ~70 units inside the room onto the doorway plane,
  into the clip slab. Its `portal.rs:3906` test (the mirror's leaf landing on the same pixel as the
  door's own leaf) is the test that will notice.
- **The destination's cell** must be read from the link's resolved destination
  (`door_links.destination_cell_id`, which the converter fills from the destination *reference*)
  rather than from the arrival point's grid: three paths currently derive the cell from the arrival
  point (`transition.rs:192-196`, `:383-420`, `:475-481`) and must move together, or an exterior
  destination streams in the wrong grid cell. `destination_cell_id` is nullable and a link may name
  only a worldspace (`streaming.rs:1294-1307`), so the fallback has to be stated: no cell, no anchor.

### 9.4 What would still be wrong under it

- **608 directions (27.6%) keep the arrival anchor**, so they keep today's error: 45% of them are
  over 45 degrees. 320 of those are not doorway-alignment problems at all (invisible markers and
  cave mouths, where the arrival anchor is correct), 106 are trapdoors, ladders and levers, 157 are
  real door models (mostly those that fail the height gate or the plan cap), and the rest have no
  model. Of the 608, 358 are there because a doorway has no bounds box at all, 234 because the
  anchored landing would leave the destination floor, and 16 because the game's arrival is far from
  the doorway.
- **55 directions in tier 3** get the offset fixed but keep the facing error (they are mostly the
  two-orientation models). A per-door facing better than the convention (the converted model's own
  doorway plane, or a per-door table) is what they would need.
- **The 5.9% of anchored directions that are real data disagreements** are handled *better* by the
  anchor (section 7 group 3) but their landing still differs from the game's - up to the plan cap.
- **The convention gate should be tightened per door.** The model rule is a population rule, and a
  single placement can disagree with its own model (section 7 group 2, the 180-degree Riften pair).
  A door should be refused the anchor when its *own* link-derived facing is more than, say, 45
  degrees from its model's convention - the point-rule evidence, which is better in the tail than
  the heading, is the right reference where it applies. This is not applied in the numbers above and
  would move a small number of directions from tier 2 to tier 4.
- **Residual risk not measured here:** a door whose destination doorway is not streamed when the
  anchor is needed (the portal already gates on the destination cell being resident, so a same-model
  pair can be resolved from the spawned destination reference - but tier 2 needs the destination's
  model convention, which comes from the model, not from the entity).

### 9.5 Where the doorway data comes from

**Tier 1 needs nothing the engine does not already have.** For a same-model pair the facing cancels,
so the map is the two references' own rotations (already on the spawned entities and already read
for every door) and the two doorway centres (the model bounds, which the engine already carries on
every reference whose cell is resident: `streaming.rs:1205-1222`). The only missing piece is
resolving the *destination* door entity from `destination_ref_id` - it is spawned whenever the
destination cell is resident, which the portal and the crossing already require, and every reference
carries its `FormId` (`streaming.rs:1217`), so an index by `FormId` is enough. **No converter
change, no schema bump, no reconversion.**

**Tier 2 needs the per-model convention**, which is a world-wide statistic. Options, cheapest first:

1. one aggregate query over `references` × `statics` × `door_links` in the engine's database layer
   at load (2,204 rows: the same tables and joins the loader already runs), no schema change;
2. the same figure computed by the converter into a new table or a `statics` column - a schema bump
   and a reconversion, for a number the engine can compute in milliseconds;
3. a hand-kept table of 104 models.

Option 1 is the recommendation; the others are recorded because a reconversion is the expensive path
and this report found no need for one.

### 9.6 How to prove it, and what would falsify it

The property to assert is not "the frames either side of the crossing agree" (that holds today) but
**"the destination doorway is drawn where the source doorway is"**, and it must be measured from
poses that are *not* square to the door - a pose 200 units out and 200 units to the side is what
makes 12 degrees visible. The tool's `summary` and `modes` commands are the data-level form of that
property; `impl-104` turns it into a gate, and `impl-103`'s tests are its engine-level form.

This report is wrong if: the model convention is not the doorway's facing (falsified by a door whose
placements agree and whose convention is still wrong - section 7 group 3 is where to look); or the
frame the user saw is not this map (falsified by a fix that leaves Sven's House unchanged - and
note the quad, the crossing plane and the clip must move too, or it will); or the doorway is not
where the box centre says (falsified by a rendered check at the eight sites above).

---

## 10. Uncertainties

- **The convention is an estimate.** Where the model's placements agree it is a good one (median
  residual 2.8 degrees over 159 farmhouse placements); where they do not, the tool falls back to the
  arrival heading and the facing error is unmeasurable. The 5.9% and 20.0% `> 45` figures are upper
  bounds on what a data-driven fix can repair, not lower bounds on the defect.
- **The "as drawn" numbers use the doorway box centre** as the doorway's position. The engine's quad
  uses the silhouette of that box in the frame, which is the same point when the frame and the
  rotation agree (research-108 section 3); where they disagree by a quarter turn the quad measures a
  different silhouette. The box centre is placed with the reference's `XSCL` (`portal.rs:651-653`,
  259 doors have a scale other than 1) and falls back to the reference origin where a door has no
  valid bounds, as the engine's quad does. The median and p90 of the offset are robust to the
  silhouette question; the extreme buckets (`<= 8 units`, `> 512`) are not, and are quoted only to
  show the shape.
- **Where a facing is unknown, the report's fallback is the arrival heading** - the same evidence the
  engine uses today. That keeps the *measurement* honest (it cannot claim an improvement there) but
  it also means the "one or neither doorway known" column describes today's behaviour, not the
  doorways.
- **The vertical error's model.** Section 4.3 assumes the anchored map takes the source doorway's
  box centre to the destination's and carries the feet with it. The engine also re-grounds the
  player after a crossing within `STEP_HEIGHT`/`SNAP_DOWN` (40/60 units, `player.rs:85-88`), so
  errors below ~40 units are absorbed and the 64-unit gate is conservative by design.
- **Only doors with a `door_links` row are measured** (2,204 of them). A reference that looks like a
  door but has no link cannot be crossed, so it cannot be part of this defect.
- **The eye/feet and timing details** (research-108 qualifications A and C, and its note that the
  portal reads the previous frame's camera transform) are inherited from the engine as it is; they
  are not part of this measurement.

---

## 11. Re-running it

```text
python tools/research/portal_door_alignment.py selftest    # the port's checks against the engine's behaviour
python tools/research/portal_door_alignment.py probe       # counts, rules, the point-vs-heading comparison
python tools/research/portal_door_alignment.py summary     # every distribution in section 4
python tools/research/portal_door_alignment.py modes       # the section 9.2 tiers and what each keeps
python tools/research/portal_door_alignment.py modes sweep # the threshold table
python tools/research/portal_door_alignment.py pair 1CBB0  # both directions of a pair: raw rows and numbers
python tools/research/portal_door_alignment.py trace 1CBB0  # the same, step by step for hand-checking
python tools/research/portal_door_alignment.py worst 20
python tools/research/portal_door_alignment.py models Farmhouse      # a model's convention and its spreads
python tools/research/portal_door_alignment.py placements Farmhouse  # every placement, with the numbers it was fitted from
python tools/research/portal_door_alignment.py list Riverwood        # directions whose cell name matches
python tools/research/portal_door_alignment.py spots 1CBB0 36F00     # section 8's table
```

The tool reads `C:/Modding/SkyrimConverted/skyrim_world.db` read-only (mode=ro) and has no other
input; `selftest` must pass before any number in this report is trusted.
