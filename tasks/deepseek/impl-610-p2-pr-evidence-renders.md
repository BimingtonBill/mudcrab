# Goal
Track: Phase 2. Produce clean before/after evidence (upstream main vs a PR bundle, both on the
upstream-schema data) for four upstream PR bundles, as labelled image pairs and sheets, so the PRs
can show what they change. You write no project source code.

# Context
- Git repository: `C:/Modding/Mudcrab/mudcrab` (all branches live there). Your working checkout is
  the worktree `C:/Modding/wkj-evidence` (detached at upstream main 6f583cd). Assets (read-only):
  `C:/Modding/Mudcrab/SkyrimConverted-upstream`.
- The engine renders exact camera poses with `--shots <file> --shots-out <dir>`, but that flag exists
  only on branch `phase2/reference-shots` (commit a96d50e). So every build you make is
  `<base> + git merge --no-edit phase2/reference-shots` on a local throwaway branch named
  `evidence/<name>` (create with `git -C C:/Modding/wkj-evidence switch -c evidence/<name> <base>`).
  If the merge conflicts, stop that pair and report the conflicting files (do not resolve them).
- Shots files: `C:/Modding/Mudcrab/mudcrab/tools/reference/riverwood_shots.json`, `world_shots_wide.json`,
  `world_shots_4x3.json` (shots contract: `docs/design/reference-shots.md`). You may write your own small
  shots JSON (same format) into `C:/Modding/Mudcrab/worktrees/p2-tools/evidence/` for a pose the files lack.
- Comparison tool: `python C:/Modding/Mudcrab/worktrees/p2-tools/flip_shots.py <before_dir> <after_dir> --out <dir>` makes
  a FLIP table, error maps and a before|after|error sheet (`flip-sheet.jpg`). Pillow is installed for any
  extra side-by-side sheet (write the script to a file; never `python -c`).
- The pairs (base = the "before" build, branch = the "after" build; branch names are local branches):
  1. **water**: main (6f583cd) vs `phase2/water`. Riverwood shots RW-01, RW-05, RW-08 (river, mill, stones),
     plus one low-angle pose over the river that shows the reflection (make it: camera ~150 units above
     the water surface, pitch about -10 degrees, looking across the river).
  2. **terrain-textures**: main vs `phase2/terrain-samplers-weights` only (the normal-maps half needs
     reconverted data: skip it and say so). Two or three ground close-ups where layer edges and
     texture stretching show (make poses: ~200 units above the ground, pitch -35 degrees, near Riverwood
     roads/grass edges).
  3. **sun-shadows**: main vs `phase2/sun-shadows`. Two wide daylight poses from `world_shots_wide.json`
     where long shadows should reach across the view.
  4. **sky**: main vs `phase2/sky`. Three poses with open horizon (from riverwood/world shots).
- Build: in the worktree, `cargo build --profile quick --bin engine` with
  `CARGO_TARGET_DIR=C:/Modding/wkj-evidence-target` and `CARGO_BUILD_JOBS=8` (one target for all builds;
  they are sequential). After each build copy `target/quick/engine.exe` to
  `C:/Modding/Mudcrab/worktrees/p2-tools/evidence/bin/<name>.exe` and check its size/time changed (touch a changed file
  and rebuild if cargo reuses a stale exe). Run shots with the copied exe:
  `<exe> --assets C:/Modding/Mudcrab/SkyrimConverted-upstream --shots <file> --shots-out <dir> --run-label "evidence <name>"`.
- Before EVERY cargo build and every engine run, read the last 15 lines of
  `C:/Modding/Mudcrab/mudcrab/local/team/2026-09-27.md` (it may not exist yet: then there is no booking).
  If a line says BOOKING with no later done/released line, wait (re-read every 60 s, at most 30 min).
  Other sessions run timing benchmarks that your builds and renders would spoil.

# Scope
Write only under `C:/Modding/Mudcrab/worktrees/p2-tools/evidence/` (images, sheets, logs, your shots JSON, notes), the
worktree `C:/Modding/wkj-evidence` (throwaway `evidence/*` branches and merges only, no source edits),
and the target dir. Do not push. Do not touch any other branch. Do not delete anything outside
`C:/Modding/Mudcrab/worktrees/p2-tools/evidence/`.

# Done when
- For each pair: `C:/Modding/Mudcrab/worktrees/p2-tools/evidence/<name>/before/`, `after/`, a FLIP `flip-sheet.jpg`, and a
  `LABEL.txt` stating: before commit, after commit (both `git rev-parse --short HEAD` of the evidence
  branch), that `phase2/reference-shots` was merged in only for `--shots` (a local-only switch), assets
  folder, shots file and pose names, date.
- A top-level `C:/Modding/Mudcrab/worktrees/p2-tools/evidence/README.md` listing each pair, its sheet, and one line on what
  visibly changed (look at the images; say plainly if nothing visible changed).

# Report
Per pair: paths, commits, what changed visibly, FLIP means, any conflict or failure. Total wall time.
