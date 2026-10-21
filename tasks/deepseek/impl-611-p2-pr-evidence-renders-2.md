# Goal
Track: Phase 2. Second round of before/after evidence for upstream PR bundles: five labelled image
pairs, plus one diagnosis. You write no project source, except the merge resolution in part B.

# Context
- You work in the worktree `C:/Modding/wkj-evidence` (repository `C:/Modding/Mudcrab/mudcrab`). The
  first round's output is in this worktree's `evidence/`: read `evidence/README.md` first. It explains
  the layout, the pitch sign (negative pitch looks UP), the shots files in `evidence/shots/`, and the exes
  in `evidence/bin/`. Write all new output under `evidence/` in this worktree (`evidence/<name>/before`,
  `after`, `flip/`, `LABEL.txt`), and add a section per pair to `evidence/README.md`.
- Every engine build = a base branch plus `git merge --no-edit phase2/reference-shots` (for `--shots`, a
  local-only switch) on a throwaway branch `evidence/<name>`. Build with
  `cargo build --profile quick --bin engine` (`CARGO_BUILD_JOBS=8`, this worktree's own `target/`), then
  copy `target/quick/engine.exe` to `evidence/bin/<name>.exe`. If cargo reuses a stale exe, touch a
  changed file and rebuild.
- Compare with `python C:/Modding/Mudcrab/worktrees/p2-tools/flip_shots.py <before> <after> --out <dir>`.
- Upstream-schema data: `C:/Modding/Mudcrab/SkyrimConverted-upstream` (schema 14; used by main and every bundle).
  The reconverted data `D:/Mudcrab/SkyrimConverted-upstream-s15` (schema 15) is ONLY for part D's
  "after".
- Before every build or engine run, read the last 15 lines of
  `C:/Modding/Mudcrab/mudcrab/local/team/2026-09-27.md`. If a BOOKING line has no later done/released/
  DONE line, wait (re-read every 60 s, at most 30 min).

# Parts
A. **Riften reflection diagnosis.** In round 1 (`evidence/sun-shadows/`, pose `SR-place-Riften_Fishery`)
   the "before" water shows vertical reflections of the dock posts and the "after" does not. The bundle
   `phase2/sun-shadows` = `phase2/sun-shadow-cascades` + `phase2/water-reflection-gating`. Build
   `phase2/sun-shadow-cascades` alone (+ reference-shots) and render the same pose (take it from
   `evidence/shots/sun-shadows.json`). Report: are the post reflections present with cascades alone?
   (Present means the gating turns the reflection off by mistake at that pose. Absent means the shadows
   hide them.) Put the three images side by side in `evidence/riften-reflection/sheet.jpg` (before |
   cascades-only | bundle), made with a Pillow script file.
B. **sky.** `phase2/sky` + reference-shots conflicts in `crates/engine/src/app.rs` and `crates/engine/src/lib.rs`.
   Resolve it by keeping both sides' additions (module declarations, CLI/config wiring, plugin/system
   registration: each side adds independent things). Build, and if it fails, fix only the merge. Render
   `evidence/shots/sky.json`: before = `evidence/bin/before.exe`, after = the sky build. Save the conflict
   resolution as `evidence/sky/resolution.diff` (`git diff HEAD~1 -- crates/engine/src/app.rs crates/engine/src/lib.rs`
   after committing the merge).
C. **terrain blending and normal maps.** Before = `evidence/bin/terrain-textures.exe` (samplers and weights
   only). After = `phase2/terrain-textures` (the full bundle, adds normal maps). Also render main
   (`before.exe`) at the same poses. Make 3 poses where relief and layer blending show: low sun is not
   controllable, so choose ground ~150-300 units away, pitch 20-35 degrees down (POSITIVE pitch looks
   down), at places with mixed ground layers (road edge into grass, rock into dirt near Riverwood or
   Whiterun farms). Sheet: main | samplers-only | full bundle.
D. **vertex-colours.** Before = `before.exe` on `C:/Modding/Mudcrab/SkyrimConverted-upstream`. After = branch
   `evidence/s15-converter` (already exists: main + phase2/vertex-colours + schema bump) + reference-shots,
   built, run on `D:/Mudcrab/SkyrimConverted-upstream-s15`. Poses: Riverwood houses and stone walls,
   plus a gravel road, from `C:/Modding/Mudcrab/mudcrab/tools/reference/riverwood_shots.json` (RW-01, RW-02,
   RW-03, RW-11). Say what changed visibly (vertex shading on walls; any foliage change).
E. **engine-loading Markarth crash.** On upstream main, streaming dense Markarth overflowed an IO thread's
   8 MiB stack ("has overflowed its stack"; see
   `C:/Modding/Mudcrab/mudcrab/local/agents/runs/impl-222-claude-io-stack-overflow.1/report.md`).
   Find Markarth in the world DB: read-only python script on
   `C:/Modding/Mudcrab/SkyrimConverted-upstream/skyrim_world.db`, `sqlite3.connect('file:...?mode=ro', uri=True)`;
   the `worldspaces` and `cells` tables, MarkarthWorld or its Tamriel grid. Run `before.exe` there for 60 s
   (`--assets C:/Modding/Mudcrab/SkyrimConverted-upstream --worldspace <id> --grid-x X --grid-y Y
   --benchmark-duration 60 --run-label "evidence markarth-before"`). Then do the same with a build of
   `phase2/engine-loading` (no reference-shots needed). Keep both logs in `evidence/engine-loading/` with a
   LABEL.txt saying which crashed. If main does not crash in 2 tries, say so and keep the logs.

# Scope
Write only inside `C:/Modding/wkj-evidence` (throwaway `evidence/*` branches, the `evidence/` folder, its
`target/`). No pushes, no other branches, no source edits except part B's merge. python only as script
files.

# Report
Per part: paths, commits, what changed visibly, FLIP means, and the answer for A. Part B's resolution in
two lines. Total wall time.
