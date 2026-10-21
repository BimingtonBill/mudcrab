# research-069 - fit the Riverwood reference poses against the UESP screenshots

**Task:** the twelve poses in `tools/reference/riverwood_shots.json` were computed from the data
by research-055 and then rendered; four of them cannot match their reference at any yaw because
every exterior camera was put at ground + 120 and UESP's "place" screenshots are free-camera
shots from above the rooftops. Judge each pose against its reference and re-fit where the
evidence allows, as research-020 did for Alftand. No rendering was available (the lead has the
GPU and a user is on it), so the poses are proposals with their arithmetic shown.

## Done

| | |
|---|---|
| `docs/research/riverwood-reference-poses.md` | new: the ten references and what they actually show, a judgement table for the twelve shots, the refined poses with the arithmetic, what is open, the files |
| `tools/reference/riverwood_shots.json` | 6 poses re-fitted (position, yaw, pitch: `RW-01`, `RW-03`, `RW-04`, `RW-08`, `RW-09`, `RW-10`, `RW-12`); `RW-02` given the pose that renders it; `RW-05`'s `reference` set to null; every `note` rewritten in the Alftand register with the reasoning and an explicit confidence |
| `scratch-069/` | uncommitted scratch: the `uesp_pose_check.py` overlays the exteriors were fitted on, and crops of the aerial. Delete freely; every overlay is one command from the doc |

Nothing else was touched. No code, nothing under `local/`, no commit.

**Counts:** 0 `matched`, 3 `close`, 7 `area only`, 2 with no usable reference (from the poses as
they stand). No shot is `matched` because a render is the only confirmation and this pass had
none.

## The three findings that are not pose work

1. **`SR-place-Riverwood_Fishing.jpg` is not the mill.** It is the fishing spot at `Bridge01`
   (24,283,-39,517) — a fish-drying rack on the bank, the river under the bridge's arch, at
   dusk. `RW-05`'s pose (the mill's wheel, 1,760 away across the village) was paired with it, so
   a correct pose looked wrong. The `reference` on `RW-05` is now `null`; the reference belongs
   to `RW-06`'s subject, and that pose's brief is the terrain ring, so the doc gives a separate
   fishing-spot pose for the lead to add as a 13th shot if wanted.
2. **`SR-place-Faendal's_House.jpg` is a truncated download, and 8 bytes are all that is
   missing.** PIL: `image file is truncated (8 bytes not processed)`. It is the only one of the
   sixteen Riverwood references that will not decode (the other fifteen were decoded through
   `tools/research/shot_grid.py`), and it stops `tools/compare_shots.py` mid-run. Re-fetching it
   from UESP should fix it and unblock `RW-07`.
3. **Three references are 16:9 (3840x2160) and seven are 4:3.** One shots file cannot hold both
   aspects; `riverwood_shots.json` keeps 1400x1050, which is right for seven of the ten.
   `hfov` fixes the horizontal framing, so the poses are unaffected, but a 4:3 render of
   `RW-01`, `RW-02` and `RW-05` shows ~28% more vertically than their references. The test
   `tests/test_reference_shots.py` only lists the two Alftand files, so nothing fails; if you
   add the Riverwood file to that list, its aspect check will reject those three.

## Verified

* `tools/reference/riverwood_shots.json` parses and the first four pairs build cleanly with
  `tools/compare_shots.py` (it then stops on the truncated Faendal reference, above). All nine
  contract fields are present in every shot; exactly one space is set per shot; yaw 0..360,
  pitch -89..89, hfov 60..95.
* `RW-04` is the strongest fit: the overlay lands `FarmInn01`'s porch foot, the door
  `00013424`, both `BarrelFood01` references, `WalkwayStairs8`, the `FenceWoven02` run and the
  `StonewallEnd`/`Ivy` references on their own pixels in `SR-place-Sleeping_Giant_Inn.jpg`.
* `RW-03`'s overlay lands the door, the barrels, the steps, the fence and the ivy on theirs in
  `SR-place-Riverwood_Trader.jpg`.
* The door facings used (`RW-03` 294.6°, `RW-04` 311.8°) come from each door's own `XTEL`
  arrival, the method research-024 established.
* Every landmark position quoted in the two files came out of the converter database through
  `tools/research/riverwood_demo.py` / `uesp_probe.py`; every angle through `aim.py`. Nothing
  was invented and nothing was taken from a render (there was none).

## Not verified, and where the risk is

* **`RW-01`'s azimuth is good to about 5°.** The derivation is solid on two anchors (only one
  bridge within 9,000 units, so the water at the frame's left edge is `Bridge01`; the village's
  4,000-unit footprint at 45° of a 75° frame), but the overlay of the resulting pose leaves the
  inn's label on a walkway rather than a house. The doc gives the correction rule (bridge left of
  the frame edge → rotate right) and one alternative pose.
* **`RW-08`'s side is arguable.** The lake's position, the apparent sizes and the stone angles
  all put the camera on the **north-east** side (the pose in the file); the carvings — a
  four-legged animal on the reference's left stone — would put it on the south-west. One render
  each settles it.
* **`RW-02`'s near arch** sits at x 0.44 in the pose that renders its reference and x 0.15 in
  the reference; the doc says to rotate right in 5° steps until the arch measures 0.15.
* **The interiors** (`RW-09` to `RW-12`) were fitted by landmark bearings only, and two of them
  (`RW-10`, `RW-11`) render white until the glossiness/specular work lands, which makes them
  hard to grade on screen. `RW-12`'s reference is a whole-room view from outside the house and
  cannot be reproduced from inside a room 900 units across.
* **`hfov` 75 is unverified for all ten references.** Every one is a free camera and a free
  camera keeps the player's FOV, so 75 is carried over from the Alftand fits and trades directly
  against every stand-off distance above.

## For the lead

1. **Render the file and compare**: `tools/compare_shots.py tools/reference/riverwood_shots.json
   local/reference/rw5 local/reference/cmp5`. Expect `RW-04`, `RW-03` and `RW-02` to look like
   their references immediately; `RW-01` and `RW-08` are the two that need the second pass.
2. **The order that pays**: `RW-04` (confirms the method), then `RW-01` (the bridge's x and the
   village's width are the two measurements to steer by), then `RW-08` (render both sides), then
   the interiors once the exposure work lands.
3. **Re-download `SR-place-Faendal's_House.jpg`** — 8 bytes short — and `RW-07` becomes
   fittable; its surviving strip says the reference camera is above Faendal's roof.
4. If a matchable bridge shot is wanted, the fishing reference is the one to use and the doc has
   the pose; `RW-06` should keep its terrain-ring job.
