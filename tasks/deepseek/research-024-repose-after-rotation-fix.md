# research-024 - Re-fit the reference camera poses now that objects rotate correctly

Read `tasks/deepseek/context.md`, `docs/design/reference-shots.md`,
`docs/research/reference-shot-poses.md` and `docs/handoff/research-020-reference-poses.md` first.

## Goal

research-020 refined the camera poses of the 26 UESP reference screenshots by rendering and
comparing. But until commit `933b95f`, the engine rotated every placed object the wrong way
(counter-clockwise instead of clockwise). So many poses were fitted to wrongly rotated towers,
bridges and rooms. Example: `SR-place-Alftand_Ruined_Tower` was "matched" from the south, but its
door actually faces east. The lead has re-posed that one (camera east of the tower, yaw 270), and
it now lines up. Redo the others the same way: render, compare, move, render again, against the
correctly rotated world.

## Tools you can run

The same as research-020 (read its brief, `tasks/deepseek/research-020-refine-reference-poses.md`,
for the details):

- render: `target/release/engine.exe --assets C:/Modding/SkyrimConverted --shots <file> --shots-out <dir>`;
- compare: `python tools/compare_shots.py`;
- sweep and aim: `tools/research/shot_grid.py`, `aim.py`;
- probe the database: `uesp_probe.py`.

Build nothing: `target/release/engine.exe` in the lead's checkout already has the rotation fix.
You can see images.

A new clue that research-020 did not use: a load door's facing is known from its link data. The
`XTEL` of the link that leads *to* a door puts the arriving player in front of it, facing away from
it (`door_links` where `destination_ref_id` = the door). A door is seen from the front on the
side of that arrival point. Use this for the towers and doors in the Alftand exteriors.

## Rules

- Update `tools/reference/uesp_shots_4x3.json` and `uesp_shots_blackreach_04.json` in place, and
  keep every shot and field. Leave `SR-place-Alftand_Ruined_Tower` as the lead set it, unless you
  can clearly improve it.
- Renders and scratch go in `local/poses2/`.
- Other workers may run the engine at the same time; that is fine. Run one at a time yourself.
- Commit only `tools/reference/`, `tools/research/` and `docs/research/reference-shot-poses.md`,
  with `[deepseek]` subjects, and a handoff note in `docs/handoff/`. Run
  `python -m unittest discover -s tests` before the final commit.
- Stop after about 2.5 hours at most.

## Report

A table: name, final pose, confidence (`matched`, `close` or `area only`), what changed and why.
Then the final comparison folder, and any rendering differences you see that research-020 did
not list.
