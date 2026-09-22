# research-020 - refine the reference camera poses by rendering and comparing

**Task:** research-018 estimated a pose for each of the 26 UESP reference screenshots without
being able to render. Render them all with the engine's `--shots` mode and iterate until each
render matches its reference's composition.

**Done:** all 26 poses rendered, compared and refined; the two shots files hold the new poses.
Final renders: `local/poses/final/`; side-by-side pairs: `local/poses/cmp-final/` (the contact
sheet `_sheet.jpg` has all 25 of the 4:3 file). Nothing under `local/` or `target/` is committed.

## What changed

| | |
|---|---|
| `tools/reference/uesp_shots_4x3.json` | 25 poses updated in place: position, yaw, pitch, `note` and `confidence`; every field name and every shot kept |
| `tools/reference/uesp_shots_blackreach_04.json` | its single pose updated the same way |
| `tools/compare_shots.py` | contact sheet no longer capped at `--sheet-height` (it silently dropped all but the first 10 pairs) |
| `tests/test_compare_shots.py` | new test: 25 synthetic pairs, the last cell must be drawn |
| `tools/research/shot_grid.py` | new: one labelled grid of any images, for judging a sweep |
| `tools/research/aim.py` | new: shots-file yaw and pitch from a camera position to a landmark |
| `docs/research/reference-shot-poses.md` | new section: the rendered result, the per-shot table, what is open, the rendering differences |

**Counts after refinement:** 1 `matched` (`SR-place-Alftand_Ruined_Tower`), 12 `close`,
13 `area only`. research-018's grades were 9 medium and 17 low; the render check demoted most of
the mediums, so the new grades should be read as *what the render shows*, not as a claim that
the shot is solved.

## Verified

* The engine's conventions on screen: `pitch` positive looks **down** (the `_06` top-down pose
  renders the roofs from above), and `yaw` is clockwise from north. research-018's sign
  convention was right; `SR-interior-Alftand_03`'s problem was the pose, not the sign.
* `python -m unittest discover -s tests`: 158 tests, OK (8 of them `tests/test_reference_shots.py`
  checking the shots files against the design contract, and 5 `tests/test_compare_shots.py`).
* Every pose in the two shots files was rendered at exactly the values written in the file, and
  the pairs in `local/poses/cmp-final/` are those renders.
* The interiors 04-10 of Alftand02 are now over the machinery the references show (the
  `DwePipe1wayHub01`/`X2way`/`Pipe2way01` cluster at 6618,-4449 and the `DwePlatBgMid02` decks),
  found by sweeping 8-16 wide views per space rather than by reasoning.

## Not verified, and where the risk is

* **The Alftand exterior towers** (9 shots, 7 weak). The identities of the seven towers within
  8000 units are still not settled; the poses are the ones whose renders put *towers* in the
  right parts of the frame. `local/scratch/assign_shots.json` holds research-018's 120 candidate
  assignments - rendering those is the cheap way to settle it (about two minutes of GPU).
* **`SR-interior-Alftand_02`** is in the AlftandZCell room that research-018 chose; that room has
  one dresser, a bench, a counter and a bust, while the reference shows two stone tables. The
  same furniture exists in Alftand01's crew rooms, which were not swept.
* **Blackreach `_02`, `_03`, `_04`** are in the right quarter of the cavern with the right
  landmark (the sun, the mushroom field) but not on the reference's line. The sun's visible orb
  is about 1400 below `BlackreachSun01`'s origin, estimated from renders rather than from the
  model bounds, so those poses can now be computed instead of guessed.
* **`SR-interior-Alftand_13` and `_14`** (AlftandWorld) are the weakest: the reference cameras
  are above the positions tried, among the tower roofs, and the renders came back dark.
* `hfov` 75 was kept everywhere. On the shots where a landmark's size can be measured against the
  reference (the ruined tower) the evidence at 75 is consistent; nothing forced a change, so the
  whole file still trades distance against field of view.

## For the lead

1. Look at `local/poses/cmp-final/_sheet.jpg` (all 25 pairs on one image) and the single pair
   `SR-place-Alftand_Ruined_Tower.jpg` - that one is the calibration shot: if the tower's size
   and position look right there, `hfov` 75 is right and the method is sound.
2. The rendering differences to turn into engine tasks are listed in
   `docs/research/reference-shot-poses.md`, "Rendering differences seen in the 26 renders". The
   two that block whole spaces: two Blackreach models fail conversion validation and never spawn
   (`WispAmbush.glb`, `FrostSpiderAmbush01.glb`), and there is no emissive/glow, without which
   every Blackreach reference is unreadable.
3. Next step for the poses: render research-018's 120 tower assignments for the exterior shots,
   then a projection overlay (`uesp_pose_check.py`) for the Alftand02 pipe frames.
