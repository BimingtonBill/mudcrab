# research-020 - Refine the reference camera poses by rendering and comparing

Read `tasks/deepseek/context.md`, `docs/design/reference-shots.md` and research-018's results
first: `docs/research/reference-shot-poses.md`, `docs/handoff/research-018-reference-shot-poses.md`
and `tools/reference/*.json`. The design's shots-file format is the contract.

## Goal

research-018 estimated a camera pose for each of the 26 UESP reference screenshots
(`local/reference/uesp/`) without being able to render. The lead then rendered them all with
the engine's `--shots` mode. Most renders do not show what the reference shows yet: several
face straight into a wall, floor or ceiling, and some look up where the reference looks down.
Make each render match its reference's **composition**: the same place, the same things in the
same parts of the frame. You can now render, so iterate: move the camera, render, look, repeat.

You can see images: `Read` a `.png` or `.jpg` and you get the picture. Judge every pose by
looking at the reference and the render together.

## Tools you can run

- Render a shots file:
  ```
  target/release/engine.exe --assets C:/Modding/SkyrimConverted --shots <file.json> --shots-out <dir>
  ```
  It opens a window, renders each shot once streaming has settled, writes `<dir>/<name>.png`
  and `<dir>/shots.log`, and exits by itself, in about 1 to 3 s per shot. Make small shots files
  under `local/poses/` holding only the shots you are working on. For one shot, try several
  candidate poses at once with different `name`s, such as `SR-place-Alftand_02-a` and `-b`.
  Render into `local/poses/<round>/`.
- `python tools/compare_shots.py <shots.json> <render dir> <out dir>` puts each reference and its
  render side by side. **Known bug:** its `_sheet.jpg` shows only the first 10 pairs although it
  says 25. Fix it, add a test, and commit the fix.
- The research-018 scripts under `tools/research/`: maps, projection, pose fitting, database
  probes. The database is read-only at `C:/Modding/SkyrimConverted/skyrim_world.db`.

## Conventions (check them, do not assume)

- `yaw` is clockwise from north (Creation `+Y`), and `pitch` **positive looks down**. The lead
  suspects some research-018 poses have the pitch sign flipped. Examples: `SR-interior-Alftand_03`
  (pitch -25, the render looks up at ceiling pipes, the reference looks along a floor) and
  several Animonculory shots at pitch -45..-60. Confirm the convention with one render of a
  known scene before changing anything.
- The `position` is the camera's eye in Creation units. A player's eye is about 120 above the
  floor, but many references use a free camera.
- `hfov` 75 is a guess. It trades against distance. Once a pose's position is right, compare
  object sizes to settle hfov, and use the same hfov for all shots if the evidence agrees.

## What good looks like

For each reference, in priority order:
1. The right space and area. If a reference is in a different cell than research-018 thought,
   move it: the `SR-interior-Alftand_02` room and the `_07`/`_08` shacks are known doubts.
2. The main landmarks in the same parts of the frame, within about 10% of the frame width.
3. Similar framing (hfov, pitch).

Rendering differences are **not** your job. The renderer is still incomplete: snow cover,
colours, fog, glow and lighting will differ, and some objects may be missing. Match the geometry
and the composition. Do list the rendering differences you notice (such as missing objects, or
objects that look wrong) in the report, because they are the next engine tasks.

## Rules

- Only you run the engine while this task runs, and it must exit before you start another run.
  If a run hangs past 5 minutes, report it instead of retrying forever.
- Update `tools/reference/uesp_shots_4x3.json` and `tools/reference/uesp_shots_blackreach_04.json`
  in place. Keep every shot, update `note` with the new confidence and what matched, and keep
  the field names.
- Commit your own paths only: `tools/reference/`, `tools/compare_shots.py`, `tests/`,
  `tools/research/`, and `docs/research/reference-shot-poses.md`. Use subjects starting
  `[deepseek]`, and leave a handoff note in `docs/handoff/`. Renders and scratch go in `local/`
  and are never committed.
- Run `python -m unittest discover -s tests` before your final commit.
- Stop when every shot is matched as well as you can get it, or after about 3 hours, whichever
  comes first. A few "could not find it" results are fine; explain them.

## Report

A table: name, final pose, confidence (`matched`, `close` or `area only`), and what matched.
Then the final render folder, and the list of rendering differences you saw, each with the
shots that show it.
