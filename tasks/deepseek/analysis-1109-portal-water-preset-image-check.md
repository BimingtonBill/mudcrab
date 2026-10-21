# analysis-1109-portal-water-preset-image-check (track: Portal; queue B3). Runs the engine; writes only to the scratch folder.

## Goal
Prove with pixels that the water view-policy preset's shadowless reflection draws the same content as the normal
reflection on portal data (no missing cells, objects, lamps, sky, doors or LOD), so the preset can be priced honestly.
Code: worktree C:/Modding/Mudcrab/mudcrab/local/impl/impl-1094-portal-reflection-layer-coverage (b7bd1c42 = view-policy
merge + reflection-layer fix). Data C:/Modding/Mudcrab/SkyrimConverted-portal-rooms (read-only).

## Setup
- Build once: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --release --bin engine`, with
  BUILD_GATE_OWNER=portal and CARGO_TARGET_DIR=<that worktree>/target. `df -h /c` first; below 15 GB stop and report.
- Flags: read crates/engine/src/config.rs for the exact names (`--water-reflection-preset`, `--reflection-scale`,
  `--shadows`, `--reflection-off`) and their help text; cite them.
- Output only under D:/Mudcrab/scratch/portal-water-preset-image/ (OWNER.txt exists). If writing there is refused, use
  <worktree>/target/water-preset/ and say so.
- Shots (`--shots <json> --shots-out <dir>`, format in crates/engine/src/shots.rs, examples tools/reference/*.json): ~10
  exterior poses where water reflection is large in frame, in Riverwood (river by the mill, by Sven's door with the
  portal open, see tools/reference/baseline_portal_heavy.json and doorway_shots.json) and one or two other water views.
  Include at least one pose where the portal doorway is visible on screen.
- Hold guard: before EVERY engine run, run `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`; if any
  `holds:` line is listed, wait (re-check every 60 s with `python -c "import time; time.sleep(60)"`), never start an
  engine while a hold is set.
- Runs (each with a `--run-label`, 2 repeats each for noise): A = default (no preset) with `--shadows off`;
  B = water preset with `--shadows off`; C = default with shadows on; D = water preset with shadows on.
  A vs B isolates content (both shadowless); C vs D shows what the preset changes in the normal look.
- Guard against blank frames: a shots run can time out with cell_resident=false and write blank PNGs (seen in
  analysis-1092). Check each PNG is not blank/uniform and that the log shows the cell resident; redo bad ones.

## Analysis
Per pose: noise (repeat vs repeat), A-vs-B difference inside the water area (pixels with any channel delta > 8, plus
FLIP if python can import it, else mean abs delta). If the preset renders the reflection at a lower size, compare after
downscaling both to the preset's size and say so. For any pose with a real A-vs-B difference, save crops and say what is
missing or added (an object, the sky, a door, LOD, a lamp). C vs D: describe the visible difference (shadows only?).

## Report (under 500 words)
Table pose | noise | A-vs-B diff | verdict; overall verdict: content parity holds / fails (with cases). Paths of shots
JSON, images, diff maps, scripts.
