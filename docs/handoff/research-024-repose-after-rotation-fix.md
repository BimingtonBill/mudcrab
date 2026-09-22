# research-024 - re-fit the reference camera poses after the object-rotation fix

**Task:** commit `933b95f` made placed objects rotate clockwise, as Creation does. Every pose
research-020 had fitted against a face of a tower, a bridge or a wall was therefore fitted
against the wrong face. Render all 26 references again in the corrected world and re-fit the
poses.

**Done:** all 26 rendered and compared in the corrected world; the nine Alftand-exterior poses
re-fitted and written into the two shots files; the other 17 (interiors, AlftandWorld,
Blackreach) re-rendered, reviewed and left at research-020's numbers, which still show each
reference's space and subject. Final renders: `local/poses2/final-4x3/` and
`local/poses2/final-br04/`; side-by-side pairs: `local/poses2/cmp-final/` (`_sheet.jpg` has all
25 of the 4:3 file). Nothing under `local/` or `target/` is committed.

## What changed

| | |
|---|---|
| `tools/reference/uesp_shots_4x3.json` | 9 poses re-fitted (position, yaw, pitch, `note`): `SR-place-Alftand`, `_02`, `_03`, `_04`, `_05`, `_06`, `_07`, `_08`; `SR-place-Alftand_Ruined_Tower` left exactly as the lead set it. Every other shot and every field kept. |
| `tools/research/uesp_door_facing.py` | new: prints, for a door or a box of doors, the arrival point of the link that leads *to* it, which is the side its front faces |
| `tools/research/uesp_ref_list.py` | new: every placed object of a box, cell or worldspace on one line, with its own rotation and scale (`uesp_probe.py box` only gives per-base averages) |
| `tools/research/uesp_make_shots.py` | new: build a shots file of candidate poses from a one-line-per-pose spec, cloning a template shot's space and aspect |
| `docs/research/reference-shot-poses.md` | new section: the door-facing table, why the exterior references are from the north rim, the nine re-fitted poses, what is open, four more rendering differences |

**Counts after the re-fit:** 1 `matched` (`SR-place-Alftand_Ruined_Tower`), 15 `close`, 10
`area only` (research-020: 1 / 12 / 13). `_03`, `_04` and `_05` were promoted from `area only`.

## Verified

* All 25 + 1 shots rendered from the values now in the two files, no timeouts
  (`local/poses2/final-4x3/shots.log`).
* The door-facing of the three exterior doors that had not been checked:
  `0005BDF1` and `0005BDC3` face east, the entrance auto-load marker `00015D48` faces
  north-west, the lift lever `000641E8` faces north. Read from `door_links`, and consistent with
  the renders that were kept.
* `python -m unittest discover -s tests`: 158 tests, OK.
* The exterior references are shot from the **north** rim of the ravine looking south, not the
  west rim: from the north the frame holds the shacks at the left, the two ribbed-roof towers
  (T5 76240,77035 and T6 77992,78336, both `DweFacadeTower1x4Snow` + `Roof02Snow` at scale 1.25)
  to their right and the ruined tower at the right edge, which is the reference's order; from
  the west rim research-020's poses put a ribbed roof where the references have the big tower.

## Not verified, and where the risk is

* **`SR-place-Alftand` (the wide view).** The world and the order of the objects are right; the
  stand-off is not - the reference's camera is roughly twice as close to its left-hand tower.
  Its pose is a view of part of the site, and which part is a guess.
* **`_04` and `_05`** are inside the ravine, where the floor is not flat: 77000,77600,-5800 is
  air and 77800,76800,-6500 is ice (that is why research-020's `_04` came back black). Both are
  now above the ice and show the reference's bridge and tower, but the exact stand-off was found
  by rendering, not fitted.
* **`_07`** cannot be reproduced: the ShackKit huts are 192 units high inside and 260 wide, and
  the engine has no snow layer, so the reference's wide snow-covered floor is out of reach. The
  entrance scaffold platform is the other candidate for that shot.
* **The 17 interior/Blackreach poses** keep research-020's numbers. Their rooms and subjects
  still come out (see `local/poses2/cmp-r0/`), but none was re-fitted, so a rotation-sensitive
  fitting (Alftand_02's furniture, Alftand_13/_14 in AlftandWorld) is still only as good as
  research-020's eye in the mirrored world.
* **`hfov` 75** is unchanged and still unverified except on the ruined tower.

## For the lead

1. `local/poses2/cmp-final/_sheet.jpg` has all 25 pairs of the 4:3 file; the single pairs worth a
   look are `SR-place-Alftand_02.jpg` (the best of the re-fitted exteriors) and
   `SR-place-Alftand_04.jpg`.
2. The four new rendering differences are in `docs/research/reference-shot-poses.md` under
   "research-024"; the two that would change the most pixels are the flat blue sky (every
   exterior reference is an overcast day) and the gold-brass Dwemer roof material.
3. Next step for the poses: the interiors want the same treatment the exteriors got - render,
   compare, move - using `uesp_make_shots.py` for the candidate rounds, and `Alftand_13`/`_14`
   in AlftandWorld want a camera among the tower roofs rather than on the floor.
