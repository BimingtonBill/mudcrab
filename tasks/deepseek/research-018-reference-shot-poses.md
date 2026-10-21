# research-018 - Where was each reference screenshot taken? Draft camera poses

Read `tasks/deepseek/context.md` and the design `docs/design/reference-shots.md` first. The design's
shots-file format is the contract: keep its field names, units and sign conventions exactly.

## Goal

For each of the 26 Skyrim screenshots in `local/reference/uesp/*.jpg` (Alftand and Blackreach,
from UESP), find where in the game world the camera stood and which way it looked. Write the
poses as shots files the engine can render (the `--shots` mode is being built at the same time by
impl-017), plus a comparison tool for when renders exist.

You can see images: `Read` a `.jpg` or `.png` and you get the picture. Look at every reference
yourself; do not guess from file names.

## Context

- **Database** (read-only, open it with `sqlite3` URI `file:C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db?mode=ro`):
  - `"references"` holds every placed object: `id, cell_id, worldspace_id, base_form_id,
    is_exterior, pos_x, pos_y, pos_z, rot_x, rot_y, rot_z, scale`.
  - `statics` holds the base objects: `id, editor_id, model_path`, and more. Check the columns
    with `pragma table_info`.
  - `door_links` holds the load doors.
  - `cells` holds the cells: interior names, grids, worldspaces.
  - Positions are Creation units, Z up.
- **Spaces:**
  - Tamriel is worldspace 60. The Alftand exterior (the glacial ruins in the Pale) is around
    (77000..80000, 76000..79000); the exterior end of the entrance is door `0x15D48`'s
    counterpart.
  - Interior 86723 is Alftand01, "Alftand Glacial Ruins".
  - Interior 355355 is Alftand02, "Alftand Animonculory".
  - Worldspace 432215 is AlftandWorld, the Alftand Cathedral area.
  - Worldspace 0x1EE62 = 126562 is Blackreach.
  - Use the database to confirm all of this, and to find which space each reference shows.
- **Headings:** yaw is clockwise from north, as in the design. A reference's yaw can be estimated
  from which landmark sits where in the frame.
- **FOV:** Skyrim's default world FOV is usually given as 75 degrees horizontal at 4:3 (the
  screenshots are 4:3, 1400x1050 or 1440x1080; `SR-place-Blackreach_04.jpg` is 1718x1080).
  Treat hfov as unknown within about 65..90 and say what you assumed.
- Many interior references are free-camera views that look down or up into large caverns, so
  pitch matters.
- `docs/research/`, `tasks/deepseek/t02-*` and the design docs under `docs/design/` describe the
  Alftand route. The four route doors are 0x15D48, 0x92809, 0x9256A and 0x6998D.

## Method (suggested; improve it if you find better)

1. **Identify the space and the landmarks in each reference.** Examples: Dwemer towers with
   pointed roofs, the rope bridge, the wooden shacks, the big pipes, the stairs, the
   Animonculory's central column, Blackreach's giant mushrooms and the Silent City tower.
2. **Map the space.** Write a script under `tools/research/` that draws a top-down map with
   Pillow. Show the references of that space, with their model names (shortened), positions and
   heights, and a grid in Creation units. Save the maps as `local/reference/maps/*.png`, then
   look at the maps and the reference together.
3. **Estimate a pose.** Work out the camera position, height, yaw, pitch and hfov that put those
   landmarks where the screenshot shows them. A small projection script helps: project the known
   landmark positions through a candidate pose into a W x H image, and compare with the pixel
   positions you read off the reference. You can fit the pose with scipy's `least_squares`
   (numpy and scipy are installed; OpenCV is not). Landmark origins are not exactly the visible
   feature you click on, so accept a few percent of error.
4. **Grade your confidence** for each shot: `high` if several landmarks were fitted, `medium` if
   you placed it by eye on the map, `low` if you only know the space. Keep the low ones: a render
   from roughly the right place still helps.

## Deliverables

- `tools/reference/uesp_shots_4x3.json` holds every 4:3 reference at 1400x1050.
  `tools/reference/uesp_shots_blackreach_04.json` holds the 1718x1080 one. Both follow the
  design's format, with `reference` set, and `note` giving the space, landmarks used, confidence
  and fit error.
- `tools/compare_shots.py <shots.json> <render dir> <out dir>`: for each shot whose render
  `<render dir>/<name>.png` exists, writes `<out dir>/<name>.jpg`, the reference left and the
  render right, each scaled to the same height, with a caption bar (name, pose, confidence).
  It also writes `<out dir>/_sheet.jpg`, all pairs on one contact sheet. Write a test for it
  under `tests/` (synthetic images in a temp dir) that runs with `python -m unittest`.
- The mapping and fitting scripts under `tools/research/`, runnable again.
- `docs/research/reference-shot-poses.md` has a table per reference: space, landmarks, pose,
  confidence. Add anything the renders should be checked for, such as objects visible in the
  reference.

## Rules

- Never copy Bethesda data or the reference images into tracked files. The images stay in
  `local/reference/`; maps and scratch go in `local/`.
- Do not run the engine or any GPU program. The lead renders.
- Commit your own paths only (`tools/reference/`, `tools/compare_shots.py`, `tools/research/`,
  `tests/`, `docs/research/reference-shot-poses.md`), with subjects starting `[deepseek]`, and a
  handoff note in `docs/handoff/`.
- Run `python -m unittest discover -s tests` before your final commit.

## Report

For every reference, one line: name, space, pose, confidence. Then the three references you are
least sure of, and why. Then what the lead should look at first in the renders.
