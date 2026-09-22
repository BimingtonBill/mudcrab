# research-018 - camera poses for the 26 UESP references

**Task:** find where each of the 26 Skyrim screenshots in `local/reference/uesp/*.jpg` was taken
and write the poses as shots files, plus a comparison tool.

**Done:** all 26 shots have a pose in
`tools/reference/uesp_shots_4x3.json` (25 at 1400x1050) and
`tools/reference/uesp_shots_blackreach_04.json` (the 1718x1080 one), in the exact format of
`docs/design/reference-shots.md`. The reasoning, landmark table and confidence grades are in
`docs/research/reference-shot-poses.md`.

**Verified**

* The spaces and the object positions every pose rests on come from
  `C:/Modding/SkyrimConverted/skyrim_world.db` (read-only), not from file names. The route doors
  (`0x15D48`, `0x92809`, `0x9256A`, `0x6998D`), the two extra doors into AlftandZCell
  (`0x5BDC3`, `0x5BDF1`) and the Blackreach exit structure are all confirmed from `door_links`.
* `python -m unittest discover -s tests` passes (157 tests, 8 of them new: `tests/test_compare_shots.py`,
  `tests/test_reference_shots.py`). The shots files are checked against the design contract
  (fields, exactly one space set, angle ranges, aspect, every reference image covered) and the
  comparison tool is tested on synthetic images in a temp directory.
* One high-confidence landmark match: `BlackreachSun01` (the only one in the world, at
  2112,9088,6336) for `SR-place-Blackreach_02` and `_04`. For `SR-place-Alftand_02` three of
  four fitted landmarks were also checked by eye against the picture
  (`local/reference/fits/check02w.jpg`).
* No engine, GPU or render run was needed or done.

**Not verified, and where the risk is**

* The Alftand exterior shots (`SR-place-Alftand*`, 9 of the 26) are the weak part. Seven similar
  towers stand within 8000 units and the frames do not disambiguate them; every assignment of
  the visible towers to those objects was scored (`uesp_tower_assign.py` + `uesp_pose_fit.py`),
  and the best residuals were 95-190 px, i.e. 7-14% of the frame. The poses are the best-scoring
  ones, marked medium where a projection was checked by eye and low otherwise.
* The Animonculory frames (04-10) are in the right machinery but at a guessed distance and
  angle; the camera will probably need moving a bit in the first render.
* `hfov` is 75 everywhere (the usual figure for Skyrim's world view at 4:3). It is the first
  thing to change if the renders look wrong, and it trades off with distance.
* Nothing is graded `high`; the brief's bar (several landmarks fitted) was not met anywhere.

**Next step for the lead:** render `tools/reference/uesp_shots_4x3.json` once impl-017 is in,
run `python tools/compare_shots.py tools/reference/uesp_shots_4x3.json <render dir> <out dir>`,
and look at the exterior pair for `SR-place-Alftand_02` first - it is the one shot where a
checkable assignment exists, so it tells you whether the pose method or my landmark reading is
the limiting factor. `local/scratch/assign_shots.json` holds the 120 alternative tower
assignments if that one is wrong.
