# Where each UESP Alftand/Blackreach screenshot was taken

**Status:** research, 2026-09-22 (research-018). Deliverable: the camera poses in
`tools/reference/uesp_shots_4x3.json` (25 shots at 1400x1050) and
`tools/reference/uesp_shots_blackreach_04.json` (1 shot at 1718x1080), both in the format fixed
by [`docs/design/reference-shots.md`](../design/reference-shots.md). The reference images
themselves stay in `local/reference/uesp/` (CC BY-SA, never committed).

Every pose was found from the converter database `C:/Modding/SkyrimConverted/skyrim_world.db`
(read-only), by looking at each reference myself and matching what it shows to placed objects.
Positions are Creation units, yaw is Skyrim's clockwise-from-north heading, pitch is positive
looking down; those are the engine's conventions, not mine, so a pose can be pasted into
`--start-position/--start-yaw` for a quick look.

## How to use this

```
python tools/research/uesp_space_map.py world:60 local/reference/maps/exterior.png \
    --center 78300,77500 --radius 3600 --terrain --filter DweFacadeTower --with-height
python tools/research/uesp_pose_check.py local/reference/uesp/SR-interior-Alftand_12.jpg \
    local/reference/fits/check12.jpg world:126562 21150 19000 2900 180 10 75 \
    --radius 4000 --filter DweFacade --filter DwePlat
python tools/compare_shots.py tools/reference/uesp_shots_4x3.json <render dir> <out dir>
```

* `uesp_space_map.py` draws a labelled top-down map of a space (with the LAND heightmap
  decoded from the raw `VHGT` blob, so the ravines are visible) - this is how the exteriors were
  read.
* `uesp_pose_check.py` projects the database's objects through a candidate pose onto the
  reference, so a pose can be judged and corrected by eye; this is how most interiors were
  placed. It also draws the horizon line, which is the quickest check on pitch.
* `uesp_pose_fit.py` fits a pose from landmark positions and their pixels (least squares, with
  `--search` for a multi-start search when the first guess is on the wrong side of the site).
* `uesp_probe.py` is the raw database query tool (`bases`, `biggest`, `box`, `bounds`, `doors`,
  `intocell`, `worldcells`, `landinfo`, ...). `uesp_tower_assign.py` enumerates tower
  assignments for the exterior shots.

## The spaces

| Space | Id | What it is |
|---|---|---|
| Tamriel (Alftand exterior) | worldspace 60, cells AlftandExterior01-03 (`0x8F63`, `0x8F82`, `0x8F83`) | the glacial ruins in the Pale, x 77800..81900, y 74000..81000, z -9300..-3400 |
| Alftand01 "Alftand Glacial Ruins" | interior 86723 (`0x152C3`) | 2329 refs, x -2921..8561, y 541..14696, z -1687..9149 |
| Alftand02 "Alftand Animonculory" | interior 355355 (`0x56C1B`) | 2234 refs, x 1383..11301, y -8785..4409, z -9241..2227 |
| AlftandZCell | interior 432216 (`0x69858`) | 789 refs, the small Dwemer rooms behind the two ruined-tower doors |
| AlftandWorld (the cathedral) | worldspace 432215 | cells at (-2..0, 0..1), x -4700..700, y 900..4300, z 291..8000 |
| Blackreach | worldspace 126562 | the Alftand end of it is cells `AlftandBlackreach01` (`0x237BD`) and `BlackreachElevatorAlftand` (`0x237B4`) around (21000, 18500, 2400) |

Verified from the database, not from file names: the four route doors are
`0x15D48` (Tamriel -> Alftand01), `0x92809` and `0x57056` (Alftand01 -> Alftand02),
`0x9256A` (Alftand02 -> AlftandWorld cell 0x69869) and `0x6998D` (AlftandWorld ->
Blackreach cell 0x2D4E0 at 21150,18531,2432). Two more doors lead from the exterior into
AlftandZCell: `0x5BDC3` at (75998,78971,-8106) and `0x5BDF1` at (73550,78431,-5609), both
under a `DweDoorFrameExBgtoExSm01`.

## The shots

Pose columns are `position (x, y, z) | yaw | pitch | hfov`, all in the units and conventions
above. "Fit" is the residual of the least-squares fit where one was run.

### Tamriel, Alftand exterior

| Reference | What it shows | Landmarks used | Pose | Confidence |
|---|---|---|---|---|
| `SR-place-Alftand` (1440x1080) | high view over the whole ruin: door tower upper left, two shacks, a big pointed-roof tower right of centre, a small domed tower at the right edge | the two ShackKit clusters (79460,78620,-4650) and (80620,77450,-4530); the arch tower (80001,75827); the west tower (73422,78436,-5641) | 82500, 72500, -800 | 324 | 25 | 75 | low |
| `SR-place-Alftand_02` | from the ravine: the big arch tower right, a pointed-roof tower centre-left behind ice, a rope bridge, one shack at the left | arch tower (73422,78436,-5641); tower A (77992,78336,-6202); the ruined tower (75982,78972,-8106); shack (80620,77450) | 71872, 80768, -4295 | 122 | 27.3 | 75 | medium, fit rms 95 px, 3 of 4 landmarks checked by eye |
| `SR-place-Alftand_03` | the same ravine from the other side: the arch tower dominating the right, two shacks on the shelf at the left, a rope bridge running down to the right | as _02 | 71500, 80500, -3900 | 124 | 22 | 75 | low (no assignment fitted below 100 px) |
| `SR-place-Alftand_04` | from under a wooden footbridge, looking up at its underside and the rope | the entrance door platform: door `0x15D48` (78049,76985,-5859), `StockadeScaffoldBridge04` (77969,77096,-5822), the `MineScaffoldRopeLong03` cluster (77568..77697, 77318..77452) | 78100, 77250, -6600 | 320 | -25 | 75 | medium |
| `SR-place-Alftand_05` | from the ravine floor: a tower with a snowy roof above, a bridge crossing the top of the frame, a wooden scaffold at the right | tower A (77992,78336,-6202) and the bridge over the entrance (77969,77096,-5822) | 78100, 77000, -6900 | 10 | -18 | 75 | low |
| `SR-place-Alftand_06` | straight down on the whole site: two ribbed round roofs, two smaller domes, two shacks | the two scale-1.25 towers (77992,78336) and (80001,75827); the two shacks | 78076, 76141, -1292 | 48 | 88.5 | 75 | medium, fit rms 190 px |
| `SR-place-Alftand_07` | inside a wooden shack looking down at the snow-covered floor (bedroll, shovel, barrel) | the shack at (79460,78620,-4650) | 79500, 78600, -4150 | 300 | 70 | 75 | low |
| `SR-place-Alftand_08` | inside a shack looking out through the open frame | the same shack | 79450, 78580, -4180 | 250 | 0 | 75 | low |
| `SR-place-Alftand_Ruined_Tower` (2880x2160) | a small Dwemer tower standing alone in snow, one door under a big stone arch frame | the tower at (73422,78436,-5641) with its door frame at (73550,78431,-5609) | 73550, 76600, -5050 | 0 | 2 | 75 | medium |

### Interiors

| Reference | What it shows | Landmarks used | Pose | Confidence |
|---|---|---|---|---|
| `SR-interior-Alftand_02` | a small round Dwemer room: two stone tables, a bench, cracked tiles, wooden posts | the `DweFacadeHallSm1wayRamp01/Alcove01` room pieces at (512..1280, 8500..8920, -6000..-6450) in AlftandZCell | cell 432216, 700, 8600, -6100 | 90 | 5 | 75 | low |
| `SR-interior-Alftand_03` | a stair corridor with brass pipes overhead and a golden Dwemer door above | door `0x92809` (4057,8792,-516) into Alftand02, the `DwePipe1way01` cluster (3081..3881, 8648..8952, -772/-836), `DwePipeGearAssemblyCCW01` (3272,8775,-76) | cell 86723, 3300, 8792, -900 | 90 | -25 | 75 | medium |
| `SR-interior-Alftand_04` | looking up a huge vertical cavern at pipe work | the ice shaft at (6351,-4729) stacked z -1000..-9400; pipe columns (6605,-4070) and (7678,-2637); `DweFacadeLiftUp01` (6479,-185,-7176) | cell 355355, 6479, -185, -6300 | 200 | -45 | 75 | low |
| `SR-interior-Alftand_05` | looking up at the ribbed underside of a round platform | as _04 | cell 355355, 6479, -185, -6800 | 180 | -55 | 75 | low |
| `SR-interior-Alftand_06` | looking up at a pipe column with two bell caps | `DwePipeCap01` x33 at (6605,-4070), z -6824..-512 | cell 355355, 6605, -4070, -6200 | 0 | -60 | 75 | low |
| `SR-interior-Alftand_07` | looking up past a pipe column at the ice wall | as _06 | cell 355355, 6605, -4070, -5400 | 90 | -55 | 75 | low |
| `SR-interior-Alftand_08` | looking up at pipes crossing high in the cavern | `DwePipe1wayX2way` x38 at (6978,-4304,-5003) | cell 355355, 7000, -3500, -5000 | 90 | -50 | 75 | low |
| `SR-interior-Alftand_09` | looking up at a second pipe column and its caps | `DwePipeCap02` x30 at (7678,-2637), z -7304..-64 | cell 355355, 7678, -2637, -5800 | 180 | -55 | 75 | low |
| `SR-interior-Alftand_10` | looking down at a round rimmed platform with a body on it | `DweFacadeLiftUp01` (6479,-185,-7176), the lift into the Animonculory | cell 355355, 6479, -185, -6600 | 90 | 45 | 75 | low |
| `SR-interior-Alftand_11` | on a stair below a golden Dwemer door, frost on the floor, cobwebs | door `0x92809` (4057,8792,-516), `FXSteamCrack` (3613/3223,8929/8948), `FXcobwebWall01` (3855,8574) | cell 86723, 3760, 8792, -1100 | 90 | -20 | 75 | medium |
| `SR-interior-Alftand_12` | standing inside a Dwemer arch at the top of a stair, looking out into a teal cavern | the Blackreach exit structure: `DweFacadeTowerArch01` + `DweFacadeHallSmExBg01` + the large door `0x4E504` at (21150,18531,2432), `DwePlatBgMid01` (20981,18714,2431) | world 126562, 21150, 19000, 2900 | 180 | 10 | 75 | medium |
| `SR-interior-Alftand_13` | looking down into a circular plaza ringed by domed towers, pipes overhead | AlftandWorld's towers (-3864,1543), (-3658,1161), (-4132,842) and the round platform cluster at (-2397..-3045, 1713..2534, 1255) | world 432215, -3200, 2200, 3400 | 0 | 55 | 75 | medium |
| `SR-interior-Alftand_14` | a stair up to a glowing orb on a stand between two domed towers | `FXGlowFillRndLitMid` (4x around -1091,2681,2167), `DweFloorLantern` (-1561,2550) | world 432215, -1000, 2400, 1500 | 342 | -20 | 75 | low |

### Blackreach

| Reference | What it shows | Landmarks used | Pose | Confidence |
|---|---|---|---|---|
| `SR-place-Blackreach` (1440x1080) | giant mushrooms and a waterfall column, a tower with a lit doorway beyond | `BlackreachEpicMushroom01` at (10929,8943,1533), scale 2.37, 5446 units tall | world 126562, 10000, 8000, 1400 | 20 | 5 | 75 | low |
| `SR-place-Blackreach_02` (1440x1080) | a huge mushroom filling the right half of the frame, the glowing orb on a tower in the distance | `BlackreachSun01` (2112,9088,6336) - the only one in the world; the `BlackreachGiantMushAnim05` cluster (1462,10885,1179) | world 126562, 4200, 10000, 1500 | 268 | -10 | 75 | medium |
| `SR-place-Blackreach_03` (1440x1080) | a domed tower with a lit doorway in the left foreground, a walkway, the cavern with water columns behind | the `BlackreachTowers`/`BlackreachCity` side of the cavern and the `BlackreachLargeDripAnim` columns | world 126562, 8000, 6000, 1500 | 300 | 0 | 75 | low |
| `SR-place-Blackreach_04` (1718x1080) | the golden lattice orb hanging under a Dwemer structure, water columns, a plaza with gratings below | `BlackreachSun01` (2112,9088,6336) | world 126562, 3900, 11400, 2100 | 218 | -25 | 75 | medium |

## What could not be pinned down, and what to do about it

**The exterior towers are ambiguous, and this is the biggest weakness.** The Alftand exterior
has seven tower-like Dwemer pieces within 8000 units - `DweFacadeTower1x4Snow` + `Roof02Snow`
at (77992,78336) and (76240,77035), both scale 1.25; the lift tower at (80001,75827) with
`DweFacadeTowerArch1x4Snow`; `DweFacadeTower02Snow` + `ArchAPSnow` at (75982,78972);
`DweFacadeTower1x3Snow` + `Arch01Snow` + `Roof01SnowHeavy` at (73422,78436); and
`DweFacadeTower01Snow` pairs at (76101,79918) and (76057,79974) - and the references show them
from angles where the roofs look alike. `uesp_tower_assign.py` + `uesp_pose_fit.py` were used to
score every assignment of the visible towers to those objects for `_02`, `_03` and `_06`; the
best residuals were 95 px (`_02`), 116 px (`_03`) and 190 px (`_06`), i.e. 7%..14% of the frame,
which is too loose to call high confidence. The poses written down are the best-scoring ones,
and the ones marked medium had their projections checked against the picture by eye
(`local/reference/fits/*.jpg`).

What would settle it in one pass: render the two or three best poses per exterior shot
(`local/scratch/assign_shots.json` holds the assignment candidates) and keep whichever puts the
tower in the right place. Roof shape is the quickest discriminator on screen - `Roof01` is the
four-sided tiled pyramid used by the west tower (73422,78436) and by the two towers at
(76101,79918)/(76057,79974), while `Roof02` is the round ribbed roof used by (77992,78336),
(76240,77035), the lift tower (80001,75827) and (75982,78972).

**The interior shots are in the right cavern but not at the right metre.** Only Alftand02 has
the pipe machinery the frames 04-10 show, and the pipe columns (`DwePipeCap01` at 6605,-4070 and
`DwePipeCap02` at 7678,-2637) and the lift (`DweFacadeLiftUp01` at 6479,-185,-7176) are the only
candidates, so those shots are placed on the machinery; the exact stand-off, height and angle are
guesses. Expect the render to need the camera pulled back or turned a little.

**Field of view.** Every shot uses `hfov` 75, the value usually quoted for Skyrim's world view
at 4:3. The brief warns it is not certain (65..90); if the renders look too wide or too narrow
against the references, `hfov` is the first knob, and it trades off directly against the
stand-off distance.

**Confidence grades.** `high` is unused: no shot had several landmarks fitted to the few-percent
level the brief asks for. `medium` covers the shots where the landmark is unique and its
identity is not in doubt (the Blackreach sun, the Alftand entrance platform, the doors). `low`
covers the exteriors whose towers could not be told apart and the interiors placed on the right
machinery but from the wrong distance.

## What the renders should be checked for

* **Alftand exterior**: seven towers exist; a render should show *towers*, in the right places,
  with roofs that are actually roofs - impl-012 dropped `EditorMarker` shapes and impl-009
  added the portal/load door handling, so watch that the load-door *frames*
  (`DweDoorFrameExBgtoExSm01`, `DweFacadeTowerArch*`) render and are not transparent.
* **Snow**: every exterior reference is a snow scene with snow drifts (`SnowDriftM01/M02`,
  `IcePile*`) and blowing-snow FX. The engine has no weather or particle pass yet, so it will
  look bare; that is expected, not a defect.
* **Bridges and scaffolds**: `StockadeScaffoldBridge03/04` (847 and 1231 units long),
  `StockadeScaffoldTop/Base` and the `MineScaffoldRopeLong03` ropes carry several frames
  (`_02`, `_04`, `_05`). Check that they are visible from below - the `ClampToEdge`/blend bugs
  of research-011 and impl-012 were all about this class of mesh.
* **Interiors**: the Animonculory frames (04-10) are a test of vertical scale - pipes run from
  z -9000 to 0 in one space, so if the render's fog or far plane clips, the frames will be
  empty. Alftand01/02 also have `BlackPlane01` (111 and 74 refs) above the camera in places.
* **Lighting**: the interiors are lit by the camera lantern plus ambient (Phase 2 has no LIGH
  lights for most of Alftand), so the green/blue casts of the references will not match yet.
  Blackreach in particular reads as emissive mushrooms and water columns, which the engine does
  not have; expect the shapes to line up and the colours not to.
* **Objects that should be in frame**: the entrance door platform and ropes (`_04`), the two
  wooden shacks (`_01`, `_03`, `_07`, `_08`), the ruined tower's arch and door
  (`SR-place-Alftand_Ruined_Tower`), the Blackreach sun and the giant mushrooms (`_02`, `_04`),
  and in Alftand02 the lift platform and the two pipe columns.

## Files

| File | What |
|---|---|
| `tools/reference/uesp_shots_4x3.json` | 25 poses, 1400x1050, the contract format |
| `tools/reference/uesp_shots_blackreach_04.json` | the 1718x1080 shot |
| `tools/compare_shots.py` | reference and render side by side, plus a contact sheet |
| `tools/research/uesp_probe.py` | database queries used to read the spaces |
| `tools/research/uesp_space_map.py` | labelled top-down maps, terrain included |
| `tools/research/uesp_pose_check.py` | project the world's objects through a candidate pose onto a reference |
| `tools/research/uesp_pose_fit.py` | least-squares and multi-start pose fitting |
| `tools/research/uesp_tower_assign.py` | tower-assignment candidates for the exterior shots |
| `tools/research/image_grid.py`, `image_sizes.py` | pixel grids over a reference, and image sizes |
| `local/reference/maps/`, `local/reference/fits/` | the maps and pose overlays (not committed) |
