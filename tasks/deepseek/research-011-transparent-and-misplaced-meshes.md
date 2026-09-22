# research-011 - Why some objects render transparent or misplaced

Read `tasks/deepseek/context.md` first. **Research only**: write documentation and analysis scripts
under `tools/research/`; do not change engine or converter code.

## Goal

The user reports objects that "seem to be transparent or misplaced that shouldn't be" in the
Alftand -> Blackreach demo. Identify every class of such object, its root cause, and a fix recipe.

## Evidence

Screenshots from a scripted tour of the route (the engine at commit `6de6829` plus a survey), in
`local/demo/survey1/`: `NN-survey-K.png` are four views at each stage (00 = Tamriel outside
Alftand, 01 = Alftand01 interior, 02 = Alftand02 interior, 03 = AlftandWorld cavern, 04 = Blackreach),
`NN-door.png` the view at each door, `05-walked.png` after walking in Blackreach. The run's log is
`local/demo/survey1.err` (camera positions in `local/demo/survey1/tour.txt`). `Read` the images.

Already visible in `00-survey-1.png`: large grey/white shapes (glacier ice, rock or snow drift?)
drawn semi-transparent over the scene, with the scaffolding visible through them.

**Known lead for "misplaced":** pre-streamed destination cells are spawned into the same render space
as the active cell, so when the player stands near a door the next room's geometry is drawn around
them at the wrong place. `impl-009-door-portals` (running now) is fixing that. Confirm or refute it
from the screenshots and code (`crates/engine/src/transition.rs`, `crates/engine/src/streaming.rs`
`wanted_cells` / `spawn_cell`), and look for **other** misplacement causes too: rotation or scale
conversion errors, NIF node transforms dropped, editor-only markers rendered, LOD meshes drawn beside
full meshes.

## Where to look

- Materials: the converter's `crates/converter/src/material.rs` (alpha flags from `NiAlphaProperty`,
  shader flags, what becomes glTF `alphaMode`/`alphaCutoff`/`doubleSided`), and the engine side
  (`crates/engine/src/streaming.rs` material validation, `crates/engine/src/render.rs`).
- Converted assets (read-only): `C:/Modding/SkyrimConverted/meshes/**/*.glb` - a GLB's JSON chunk shows
  each material's `alphaMode`, `alphaCutoff`, `doubleSided` and the `OPEN_SKYRIM_material` extension.
  `C:/Modding/SkyrimConverted/skyrim_world.db` (read-only, open with `mode=ro`) maps references to base
  objects and models; `tools/research/` scripts from earlier tasks show how.
- Identify the actual meshes in the screenshots: use the camera positions in `tour.txt` and the DB
  (references near those positions, their `model_path`) to name the models you see. Name them.
- The original NIFs are in the game's BSAs (`C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data`)
  and extracted copies under `C:/Modding/SkyrimConverted/vfs/meshes/` - compare the NIF's
  `NiAlphaProperty` flags / threshold with what the GLB says.

## Deliver

`docs/research/transparent-and-misplaced-meshes.md`: a table of defect classes (what it looks like,
example models with paths, how many models in the whole converted set are affected - count with a
script over the GLBs, root cause with `path:line`, verified or inferred), then a fix recipe per class
(converter change, engine change, or both, and the test that would catch it). Plus
`docs/handoff/research-011-transparent-and-misplaced-meshes.md`.
`python tools/check_citations.py docs/research/transparent-and-misplaced-meshes.md` must report nothing
broken. Commit: `git add <paths>` then
`git commit -m "[deepseek] research-011: transparent and misplaced meshes" -- <paths>`.

## Report

300-600 words: the defect classes ranked by how much of the screen they affect, root causes with the
decisive `path:line`, how many models each touches, and the fixes.
