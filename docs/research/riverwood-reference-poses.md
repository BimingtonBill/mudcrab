# Riverwood reference poses, fitted against the UESP screenshots

**Status:** research, 2026-09-23 (research-069). The twelve camera poses in
`tools/reference/riverwood_shots.json` were judged against the UESP screenshots in
`local/reference/riverwood/` — the folder holds sixteen, of which the set pairs ten — and
re-fitted where the evidence allowed. This is the Riverwood counterpart of
`docs/research/reference-shot-poses.md` (research-018/020/024 for Alftand).

**No render was made.** The lead has the GPU and a user on it, so every pose here is a proposal
with its arithmetic shown, exactly as research-055's were, and each one says what a render
should show if it is right. What *was* used, and is reproducible:

```
python tools/research/riverwood_demo.py ground|river|modelsnear|notes    # terrain and landmark facts
python tools/research/riverwood_demo.py buildings <x0> <y0> <x1> <y1>    # every house shell, size and rotation
python tools/research/aim.py <cam xyz> <target xyz>                      # yaw and pitch to a landmark
python tools/research/uesp_pose_check.py <reference> <out.png> world:60 <x y z yaw pitch> <hfov> \
        --radius <r> --filter <model substring>
```

`uesp_pose_check.py` draws the database's own objects through a candidate pose onto the
reference, which is how the exteriors here were fitted: a pose is right when the door, the
barrels, the stairs and the fences land on their photographs. Overlays looked at (uncommitted
scratch at the repository root, 25 MB, delete freely): `scratch-069/check-inn.png` (the inn),
`check-trader2.png` (the Trader), `check-street.png` (the walkway street), `check-stones.png`
and `check-stones-ne.png` (the stones from two sides), `aerial-candidates.jpg` (four aerial
candidates on one sheet), `aerial-leftedge.png` (the bridge at the reference's left edge) and
`stones-left.png` (the reference's left stone at 3x). `scratch-069/cmp-check/` holds the four
pairs `compare_shots.py` built for this file before it stopped on the truncated Faendal
reference.

## The references, and three things about them that the pose work has to know

| reference | size | what it is |
|---|---|---|
| `SR-place-Riverwood.jpg` | 3840x2160 (16:9) | the whole village from a free camera above the roofs, river and stone bridge at the left |
| `SR-place-Riverwood_02.jpg` | 3840x2160 (16:9) | the street under the Trader's covered walkway, shot from outside it |
| `SR-place-Riverwood_Fishing.jpg` | 3840x2160 (16:9) | **the fishing spot at Bridge01**, not the mill |
| `SR-place-Riverwood_Trader.jpg` | 1200x900 | the Riverwood Trader's front and its walkway, from the street |
| `SR-place-Sleeping_Giant_Inn.jpg` | 1024x768 | the Sleeping Giant Inn's front corner, from above its porch |
| `SR-place-Guardian_Stones.jpg` | 1200x900 | the three stones from above, lake and islands behind them |
| `SR-interior-Sleeping_Giant_Inn_Common_Room.jpg` | 1440x1080 | the common room, fire pit close at the right |
| `SR-interior-Riverwood_Trader.jpg` | 1440x1080 | the shop, fireplace left, counter right |
| `SR-interior-Alvor_and_Sigrid's_House.jpg` | 1200x900 | the room from the door end, hearth left, bed centre, table right |
| `SR-interior-Faendal's_House.jpg` | 1200x900 | the room, hearth left, bed centre, table right |

1. **`SR-place-Riverwood_Fishing.jpg` is not the mill.** It shows `Bridge01` (`00022469`,
   24,283,-39,517) with the river under its arch, a fish-drying rack on the near bank and a
   fishing creel; the camera is within a few hundred units of the bridge, at dusk. The pose in
   the shots file aims at the mill's water wheel 1,760 units away across the village, so the
   pairing — `reference` on `RW-05` — is wrong, not the pose. Fixed in the file (see RW-05
   below); the reference itself is the bridge's, and `RW-06` is the shot that already looks at
   the bridge.
2. **`SR-place-Faendal's_House.jpg` is truncated, and can be fixed by downloading it again.**
   Its header decodes (a complete 1400x1050 baseline JPEG) but the pixel data stops 8 bytes
   short: PIL raises `image file is truncated (8 bytes not processed)` on any full decode, and
   `tools/compare_shots.py` and `tools/research/shot_grid.py` both stop on it. What a reader
   sees is the top 17% of the frame plus a vertical smear — the stretched last rows, not a
   broken screenshot in the source. It is the **only** one of the sixteen Riverwood references
   that fails to decode (the other fifteen were decoded through `shot_grid.py`); re-fetching
   https://en.uesp.net/wiki/File:SR-place-Faendal's_House.jpg fixes it, and then this shot can
   be fitted like the rest. Until then `RW-07` is judged on its own subject.
3. **Three references are 16:9 and seven are 4:3.** The shots file keeps the Alftand register's
   1400x1050 (4:3), which is right for seven of the ten. `hfov` fixes the *horizontal* framing,
   so the poses below are unaffected; a 4:3 render of `RW-01`, `RW-02` and `RW-05` shows ~28%
   more vertically than the 16:9 reference does, and the comparison should be read on the
   horizontal frame.

## What each pose is, and how good it is

Grading is research-020's: **matched** (a reviewer would accept it as the same view),
**close** (right subject, right arrangement, some metres or degrees out), **area only** (the
right place, not the reference's view). "Now" is the pose in the file as it stands; the last
column is the pose proposed below.

| shot | reference | now | proposed | why |
|---|---|---|---|---|
| `RW-01-village-from-the-south` | `SR-place-Riverwood` | **area only** | elevated from the west-south-west | the pose is at ground + 120 on the Helgen road and renders the Trader's walkway; the reference is a free-camera aerial from above the rooftops whose frame holds the bridge, the whole village and the river at once |
| `RW-02-street-to-the-inn` | `SR-place-Riverwood_02` | **area only** | kept at the pose that renders it | the current pose stands inside the walkway, against a planter; `RW-01`'s *pose* renders this reference's view almost exactly, so the fix is the pairing plus a small nudge |
| `RW-03-trader-front` | `SR-place-Riverwood_Trader` | **area only** | 1,095 from the door, on the door's own axis | the current pose is under the walkway with the building's underside across the top; the reference is the whole house from the street |
| `RW-04-inn-front` | `SR-place-Sleeping_Giant_Inn` | **area only** | 1,330 from the door, 830 above its terrace | the current pose is inside the porch (a barrel fills the right third); the reference looks down on the roof from outside the front corner. The new pose is the strongest fit in the set: its overlay lands the door, both barrels, the stairs, the woven fences and the ivy on their photographs |
| `RW-05-mill-wheel-across-the-river` | *(none)* | **no reference** | kept as it is | renders the mill, its turning wheel, the log pile and the river; the UESP shot of that name is the bridge fishing spot, so the `reference` is now null |
| `RW-06-river-to-the-bridge` | *(none)* | **no reference** | kept as it is | aims exactly at `Bridge01`, 4,436 away; an engine-only view of the distant terrain ring (see below) |
| `RW-07-village-from-the-south-hill` | `SR-place-Faendal's_House` | **reference unusable** | kept as it is | shows the village from Faendal's gate with the wicket fence in the foreground — a good view, but not the elevated roof-level shot the reference's surviving strip is |
| `RW-08-guardian-stones` | `SR-place-Guardian_Stones` | **area only** | 960 from the platform, 420 above it | the current pose is inside a rock 600 units short of the stones; the reference is a close high view of the three stones |
| `RW-09-inn-common-room` | `SR-interior-Sleeping_Giant_Inn_Common_Room` | **area only** | same eye, yaw turned 60 degrees right | the room, the fire pit and the trestle tables are right, but the render has the fire pit on the left where the reference has it filling the lower right |
| `RW-10-trader-shop-floor` | `SR-interior-Riverwood_Trader` | **area only** | 140 closer, yaw 17 right | the right room and the counter, but the render centres the fireplace where the reference has it filling the left quarter, and the shop is blown white (the glossiness/roughness bug, not the pose) |
| `RW-11-alvors-house` | `SR-interior-Alvor_and_Sigrid's_House` | **close** | kept as it is | hearth left, bed centre, wardrobe right — the reference's own arrangement; the render is closer to the bed than the reference is and blown white |
| `RW-12-faendals-house` | `SR-interior-Faendal's_House` | **area only** | same eye, yaw 27 right | the render puts the hearth square in the middle; the reference has it at the left with the bed to its right |

Counts: 0 `matched`, 3 `close` (`RW-02`, `RW-03` once rendered, `RW-11`), 7 `area only`, 2 with
no usable subject (`RW-05`, `RW-06`). No shot is `matched` because a render is still the only
way to confirm a fit, and this pass had none.

---

## The poses, with the arithmetic

Everything below is Creation units, yaw clockwise from north, pitch positive looking down
(`docs/design/reference-shots.md`). Landmark positions are from the converter database read
through `tools/research/` — not from a description of the render.

### RW-01-village-from-the-south — `SR-place-Riverwood.jpg`

**Proposed: `17500, -47500, 1100`, yaw `75.2`, pitch `17.6`, hfov 75.**

The aerial cannot be made from the road. What the reference fixes, in its own pixels (3840x2160):
the stone bridge's arch at about (140, 570) — **x 0.036, y 0.264** of the frame — and the
village's thatched roofs spanning roughly x 0.30..0.90, the river and its boardwalks across the
foreground.

*Only one bridge exists within 9,000 units of the village centre* (`riverwood_demo.py modelsnear
22100 -46130 9000 x Bridge`): `Bridge01` `00022469` at (24,283,-39,517,-114), 6,964 away at
bearing 18.3°. So the wedge of water at the frame's left edge is Bridge01, and the camera must
be placed so that it is 34.8° left of the view axis (0.5 - 0.036 = 0.464 of a 75° frame).

Taking the camera 4,955 from the village centre and 10,546 from the bridge — a stand-off that
also makes the village's 4,000-unit footprint span 45° of the frame, which is what the reference
shows — the bridge is at bearing 40.4° and the village centre at 73.4° (`aim.py`). The yaw that
puts the bridge on 0.036 is therefore `40.4 + 34.8 = 75.2`, which puts the village centre on
x 0.48 and the inn (bearing 64.8) on x 0.36.

The height and pitch come from the bridge's vertical. At z 1,100 the bridge, 10,546 away and at
z -114, lies 6.6° below the horizontal and has to sit at y 0.264 of a 16:9 frame
(vfov 46.6°), i.e. 11.0° above the view axis: `pitch = 6.6 + 11.0 = 17.6`. The village centre
then falls at y 0.43, which is where the reference's roofs are. The camera is 1,200 above the
village ground (-130) — high enough to see the roof planes from above, which is the thing the
ground-level pose cannot do.

**If this is right the render shows**: the stone bridge with its arch in the left edge, slightly
above the middle; the river and the boardwalk-walkways crossing the bottom of the frame; the
village's thatched roofs from about a third of the width to the right edge, the inn's block and
the bigger houses about the middle; pines over the rooftops and the rock face above them.

**Alternatives if it is not.** The same anchor was fitted with the camera further north and
higher, (16,500,-45,000,1,000) yaw 89.6 pitch 17, which recruits the mill's *water wheel* to the
left edge as well; and the lead's `local/reference/rw_sweep.json` family (18,500..20,000,
-48,500..-49,500, 600..1,800) all look at the village from the south-west, where they cannot put
the bridge on the left edge at all — 20° is the most the bridge and the village can be separated
from any southern viewpoint. If the render comes out with the bridge too far right, the yaw is
the knob; if the village is too small, the camera comes along the bearing towards it.

### RW-02-street-to-the-inn — `SR-place-Riverwood_02.jpg`

**Proposed: `18700, -46700, -62`, yaw `75.5`, pitch `-0.6`, hfov 75 — the pose `RW-01` was
rendered at, which turns out to be this reference's view.**

The overlay of that pose on the reference lands `Farmhouse02Walkway` (21,767,-45,457), the
`Ivy01/02/03` references, the `WalkwayStairs/End/Bend` pieces and the `StonewallEnd` walls on
the beams, the arcade, the ropes and the stone wall the photograph shows, and the whole
composition — the covered walkway from the left, the arch through it, the ivy-hung stone wall
at the right, the street below — is the reference's.

The residual: the walkway's near arch sits at x 0.44 in the render and x 0.15 in the reference,
and the reference has more mountain and sky above the roof. That is a ~20° azimuth difference
and no rotation can close it without swinging the walkway end-on, so the likeliest reading is
that the reference is shot at a different point of the same walkway run (the
`Farmhouse02Walkway` + `WalkwayBend01/End01` chain runs from 21,200,-45,970 to 21,767,-45,457).
**Render this pose, then rotate right in 5° steps until the near arch is at 0.15 of the width**
— that is the one measurement the reference gives.

### RW-03-trader-front — `SR-place-Riverwood_Trader.jpg`

**Proposed: `21240, -45370, -25`, yaw `118.0`, pitch `-14.5`, hfov 75.**

The Trader is `Farmhouse02.NIF` at (22,176,-45,867,-119), rz 2.356 (135°), 1,092 x 1,000, and
its door `0001341F` (22,022,-45,713) faces **294.6°** — read from its own link, whose arrival
(21,949.96,-45,679.83) is 79 units north-west of the door. The camera therefore stands on that
bearing, at ground level: 1,095 from the building's centre at (21,240,-45,370), where the ground
is -155 (the arrival's ground, `riverwood_demo.py ground`), i.e. eye height ~-25, aimed
therefore at the house's mid-height (z 250) → `yaw 118.0, pitch -14.5` (`aim.py`).

**If this is right the render shows**: the Trader's two storeys and thatched roof filling the
frame from x 0.25 to 0.75, the covered walkway across the front at the bottom third with its
ropes and posts, the door behind the walkway at the centre, the hanging shop sign left of the
door, the woven fence and the stone revetment at the right, the smithy's thatched roof cut by
the left edge, and the mountains above.

### RW-04-inn-front — `SR-place-Sleeping_Giant_Inn.jpg`

**Proposed: `22366, -43676, 600`, yaw `131.8`, pitch `16.0`, hfov 75.**

The Sleeping Giant Inn is `Farmhouse\Inn01.nif` at (23,864,-44,503,-232), rz 2.304 (132.0°),
**1,944 x 1,224** — the largest building in the village. Its door `00013424` (23,320,-44,530)
faces **311.8°** (the same link method: the arrival 23,259.77,-44,476.16,-8.25 is 82 units
north-west of the door). The camera stands on that bearing 1,330 from the building's centre at
(22,366,-43,676), where the ground is -101, i.e. **830 above the inn's terrace**, and is aimed
at the porch 1,330 away and 350 below → `yaw 131.8, pitch 15.3` (16.0 written, aiming a little
above the door's foot so the roof comes into the frame as the reference has it).

This pose was checked with `uesp_pose_check.py`: the projections of `FarmInn01` (the porch
foot), `FarmhouseLDoor01` `00013424` (the door), the two `BarrelFood01` references (the barrels
either side of it), `WalkwayStairs8` (the steps at the right), the `FenceWoven02` run and the
`StonewallEnd*Ivy`/`Ivy01/02` references all land on their photographs. That is two independent
landmarks at their own pixels, plus five that fall correctly by eye.

**If this is right the render shows**: the inn's big thatched roof filling the upper half from
above (the camera is above the eaves), the front wall and porch across the middle with the door
at about x 0.42, the barrel to the right of the door, the walkway and its steps at the right
with the woven fence, the hanging sign at the left of the door, and the ground with ferns at the
bottom — the reference's frame almost element for element.

### RW-05-mill-wheel-across-the-river — reference now `null`

**Pose unchanged: `21000, -43800, 100`, yaw `255.4`, pitch `0.5`.**

The pose renders the mill (`Lumbermill01` `0004F743` at 19,277,-44,311, 822 x 898, on the far
bank with its tongue between two channels of the river) with the water wheel
(`000A1FF8` 19,650,-43,941), the log pile, the river and the pines behind — the lead's pick from
`local/reference/rw_sweep4/_sheet-mill.png`, and the only one of the four candidates that shows
the wheel, the mill and the log deck together. The `reference` is set to `null` because the UESP
image filed under that name is `SR-place-Riverwood_Fishing.jpg`, the fishing spot at Bridge01
(see the top of this document); the pose is not the thing that was wrong. If the wheel's motion
is the point of the shot, note the Idle clip is 16.67 s a revolution.

### RW-06-river-to-the-bridge — no reference

**Pose unchanged: `23600, -43900, 72`, yaw `8.9`, pitch `0.9`.**

`aim.py` confirms the yaw is dead on: `Bridge01` is 4,436 away at bearing 8.9° from this eye, so
the bridge sits on the frame centre and the river runs out of the bottom of the frame towards
it. **What it is for, and whether it earns its place:** it is the set's one view of the
**terrain ring** — the `--terrain-radius` far terrain that is not cell-streamed — across open
water with nothing in the foreground to hide it, and the only pose in the set that looks along
the river's own line for four and a half thousand units. It is worth keeping for that, and it is
not a UESP match; nothing in `local/reference/riverwood/` frames the empty middle of the river.

If the lead wants a *sixth* matchable exterior instead, the fishing reference belongs to this
subject: standing on the bank 500-700 units south-west of the bridge, looking north-east along
it, reproduces it (`SR-place-Riverwood_Fishing.jpg` wants the bridge's two arches at the left,
the river under them, the fish rack at the right against the bluff, and the sun low). Its
compass is the bridge's own: `Bridge01` is at (24,283,-39,517), and the bank below it is at
about (23,700,-40,200). It is **not** in the file: `RW-06`'s existing job is the ring, and one
shot should not be asked to do both.

### RW-07-village-from-the-south-hill — reference unusable

**Pose unchanged: `21663, -48130, 128`, yaw `8.4`, pitch `2.8`.**

`SR-place-Faendal's_House.jpg` is truncated (8 bytes of pixel data missing, see the top of this
document), so there is nothing to fit against; the
surviving strip is a roof seen from *above* close range, which is a free-camera shot over
Faendal's house (`Farmhouse05` `00019053` at 21,640,-48,385,-189, 904 x 936) and not a doorstep
view. The pose stands on Faendal's door threshold (`00012DFE` 21,663,-48,130, z 35) at ground +
93 and looks north; the render shows the wicket gate and fence in the foreground with the
village's houses and thatched roofs beyond, which is the view the shot's name claims. Keeping
it; if the reference's own subject is wanted, the camera wants to be above that house's roof
about (21,600,-47,100,600) looking north.

### RW-08-guardian-stones — `SR-place-Guardian_Stones.jpg`

**Proposed: `3200, -59000, 1848`, yaw `236.5`, pitch `26.0`, hfov 75.**

The three stones are exterior references in cell `00009B91` "GuardianStones" (grid 0,-15):
Warrior `000E7BDD` (2,454,-59,142, z 1431), Mage `000E7BD6` (2,480,-59,477, z 1428), Thief
`000E7BDC` (2,854,-59,492, z 1435) — an L, with the Warrior 335 north of the other two, and all
three the same size (`riverwood_demo.py notes WarriorStone|WizardStone|ThiefStone` returns the
same bounds for all three — and a box that cannot be a stone's true height, (-196,-370,-5464)..
(195,6059,410), so the database is no use for how tall they are; the reference is). The camera
sits 960 from the Mage at bearing 236.5° (`aim.py`), on the platform's north-east side, 420
above the stones' bases and above their tops, pitched 26° down.

Why north-east and not south-west — the reference shows the lake *behind* the stones, and the
water's edge is west and north of them (`riverwood_demo.py river 3C 0 7000 -62000 -55000 500
560`: wet x 0..650 at y -59,000, x 1,900..3,500 at y -57,000, x 3,500..7,000 at y -55,000). From
the north-east the water lies across the whole background with the far shore and islands in it,
and the high rock that fills the reference's left edge is the mountain to the south-east (z
2,383 at 5,000,-60,000 and 2,743 at 3,000,-61,000). The distances also match the reference's
apparent sizes (left : centre : right = 1 : 1.45 : 1.31; this pose gives 1 : 1.44 : 1.26), and
the three stones subtend 21° and 23°, against the reference's 21° and 28°.

**If this is right the render shows**: the Thief stone at the left about 600 away, the Mage
stone at the centre about 860, the Warrior at the right about 760, all three seen slightly from
above on the Nordic platform (the `NorPlat*` pieces) with its roots and the paved circle, the
lake with its islands filling the background from the left of centre to the right edge, and a
dark rock face and pines at the left.

**The one thing that argues the other way.** The carvings: the reference's left stone carries a
four-legged animal, the middle a robed figure and the right a bird-like creature, and if the
left stone is the Warrior rather than the Thief then the camera is on the south-west instead
(2,200,-59,900, yaw 52, pitch 29), which puts the Warrior on the left and the Mage in the
middle. The south-west reading cannot put the lake behind the stones, so the north-east pose is
the one written down, but **this shot is the one to settle by rendering both** — one render each
and the lake's position decides it.

### RW-09-inn-common-room — `SR-interior-Sleeping_Giant_Inn_Common_Room.jpg`

**Proposed: `-250, -360, 120`, yaw `301.0`, pitch `-5.0` (cell 78790).**

The eye is unchanged — it is the inn door's own arrival (-250.34,-360.40,0 plus the 120 eye
height, facing the arrival's -0.0084 rad ≈ north) — and only the yaw moves. The fire pit
`FarmIntLHFirePit03` is at **(-512, 0)**, 449 from the eye at bearing 324.0° (`aim.py`), with
the two trestle table runs at y -353 and y +357 behind it. Turning the camera 60° right of north
to yaw 301 puts the pit at 0.81 of the frame width, i.e. filling the lower right, and the
tables, the loft windows and the room's north wall across the left — which is the reference's
frame.

**If this is right the render shows**: the stone fire pit's raised rim close in the lower right,
the room's floor and rug to its left, the trestle tables and benches along the far side, the
timber posts, and the inn's band of small windows high on the wall above them.

### RW-10-trader-shop-floor — `SR-interior-Riverwood_Trader.jpg`

**Proposed: `-230, -380, 120`, yaw `31.0`, pitch `0.0` (cell 78793).**

The Trader's fire is `FireplaceWood01Burning` at **(-191, 57)** with the cooking pot and spit at
(-178,-123), and the counter is the `CounterSet` L at (-43,-60), (-43,-188) and (21,-188). From
the proposed eye the fire is at bearing 5.1° (440 away, 3.9° above the eye) and the counter's
corner at 44.2° (275 away, 12.6° below); yaw 31 puts the fire at x 0.15 and the counter at 0.67,
which is the reference's left-to-right arrangement, with the shelf `CommonShelf01` at (-208,22)
between them. The eye is 140 north-west of the door arrival (-190.75,-472.09), inside the shop.

**If this is right the render shows**: the shop's big stone fireplace with its fire and cauldron
filling the left quarter, the hanging meat and garlic over it, the counter running from the
centre to the right edge with the shopkeeper behind it, and the ceiling beams above. It will
still be over-exposed until the glossiness/specular work lands; that is the engine's, not the
pose's.

### RW-11-alvors-house — `SR-interior-Alvor_and_Sigrid's_House.jpg`

**Pose unchanged: `-513, -206, 360`, yaw `0.0`, pitch `-5.0` (cell 78792).**

This one is already right and the database says why. From the door arrival
(-512.82,-205.79,240) the room's fittings are: the hearth `FarmInt2Hearth02` at **(-768, 0)**,
255 to the west; the double bed `000134AB` at **(-511, 151)**, 357 due north; the wardrobe
`0005ADBA` at (-265,205), 468 to the north-east; the table run at (-772,-128) and the round
table at (-768,96) to the west of the hearth. Looking north therefore puts the hearth at the
left, the bed dead centre and the wardrobe at the right, which is the reference's arrangement —
and the render (`local/reference/rw4/RW-11-alvors-house.png`) shows exactly that, hearth, bed
and cupboard in those places. The reference is a wider view from further back; the room is only
900 units across, so the eye cannot retreat much past the door without leaving the house.

### RW-12-faendals-house — `SR-interior-Faendal's_House.jpg`

**Proposed: `-514, -197, 104`, yaw `27.0`, pitch `-6.0` (cell 78794).**

Faendal's fire `0005BF69` is at **(-509, 239)** on the north wall, with the mounted elk head
`0005BF61` directly over it at (-508,239), the cooking pot at (-481,77), the bed `000134AD` at
**(-234, 8)** and the cupboard `00038B5C` at (-297,213). From the door arrival
(-513.57,-196.98,-16) the fire is 446 away at bearing 0.7° and the bed 350 away at 53.8°, so the
current yaw of 0 puts the hearth squarely in the middle (which the render does); yaw 27 moves
the hearth to the left quarter with its elk head and brings the bed to the right of centre.
**What cannot be matched**: the reference is a whole-room view — hearth at 0.1, bed at 0.45,
table at 0.8 — and those three fittings are 53° apart from any eye inside a room 900 units
wide. The reference camera is outside the south wall (a free camera); reproducing it means
rendering through the house.

---

## What is open, and what would close it

1. **`RW-01`'s azimuth.** The bridge at the frame's left edge and the village's 45° of width are
   both satisfied by (17,500,-47,500,1,100) yaw 75.2, but the overlay of that pose leaves the
   inn's label on a walkway rather than a house, so it is right to about 5°. The one render that
   settles it: if the bridge is left of the frame's edge, rotate the camera *right*; if the
   village's east houses are cut off, rotate left.
2. **`RW-08`'s side.** North-east (this file) or south-west (the carving reading): one render
   each shows the lake behind the stones or in front of them.
3. **The two arch readings in `RW-02`.** The near arch's x is the measurement: 0.15 in the
   reference, 0.44 at the pose in the file.
4. **`hfov` 75 is unverified for all ten references.** Every reference here is a *free camera*
   shot, and a free camera in Skyrim keeps the player's FOV, so 75 is a guess carried over from
   the Alftand fits. It trades directly against every stand-off distance above: if the renders
   are consistently too wide, the camera is closer than stated, and vice versa.
5. **The interiors' exposure makes grading hard.** `RW-10` and `RW-11` render white (the
   glossiness exponent bug, `docs/research/specular-gloss-mapping.md`, and impl-064/impl-062);
   the poses may be better than they look until that lands.
6. **Nothing here was rendered.** Every "if this is right the render shows" is a claim the next
   `--shots` run either makes good or falsifies, which is the point of writing it down.

## Files

| file | what |
|---|---|
| `tools/reference/riverwood_shots.json` | the twelve poses, updated (positions, yaws, pitches, references, notes) |
| `docs/handoff/research-069-riverwood-pose-fit.md` | the handoff note |
| `scratch-069/` | the overlays and crops looked at, uncommitted (see above; delete freely) |
| `tools/research/aim.py`, `riverwood_demo.py`, `uesp_pose_check.py` | the tools used here, unchanged |
