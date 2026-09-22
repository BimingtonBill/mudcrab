# Working agreement for AI assistants

<!-- ds-delegation:begin (managed by DeepSeek Workers/tools/ds_delegation.py; edit with --set) -->
## DeepSeek delegation level: 4 of 5 (Claude manages)

Claude plans, writes briefs, reviews and integrates; workers do the exploration and most coding, including multi-file changes in their own worktrees with owned-file lists. Claude avoids reading source beyond what review needs: ask a worker to locate, summarize or trace code instead. Keep several workers busy, and use a DeepSeek lead (-CanSpawn) for jobs that split into independent parts.

Web lookups: when a task needs information from outside the project (library or tool versions, changelogs, API documentation, an error message, how others solved something), send a DeepSeek websearch worker (-Kind websearch) instead of searching yourself; only a single quick search you need this minute is yours. Start it as soon as the question comes up and keep working while it runs. Give it everything it needs in the brief, nothing private, and check the claims you act on.

At every level Claude keeps: talking to the user, design and architecture decisions, integration into the main checkout, security-sensitive changes, and review of every worker result it acts on (spot-check the claims that matter; read the full diff of any edit). Whenever workers are running, Claude works in parallel on something that cannot collide with them (the next brief, integration plumbing, the previous review, its own tests and builds), and starts its own long builds and conversions in the background too; a worker run is not a wait.

Missing tools: before starting a job, check that the tools it needs are installed. If a compiler, runtime or package the best approach needs is missing, stop and ask the user to install it (say what it is for and give the exact install command) instead of working around it with a weaker approach or a hand-written substitute. Continue without it only if the user chooses to.

This block is generated. Change the level with `python "C:/Users/Brannock/.claude/skills/deepseek-agents/tools/ds_delegation.py" --set <1-5> --agents-md` from this folder, or ask Claude to. The full scale is in the deepseek-agents skill.
<!-- ds-delegation:end -->


Claude (Claude Code) leads the work on this checkout and delegates bounded tasks to DeepSeek worker agents. The setup is carried over from the user's DOA Xbox360 UI project. This file is how each session, and each assistant, sees what the others did.

## At the start of every session

1. Run `git log --oneline -15` and `git status`.
2. Read the **Handoff log** below, newest entry first.
3. If `git status` shows uncommitted changes you did not make, stop and ask the user before touching them. Another session or a worker may be mid-task. `tools/ds_status.ps1` shows whether a worker is running.

## While working

- Do not rewrite history (no amend, rebase or force-push on commits already made).
- **Division of labour (delegation level 4 of 5, set by the user 2026-09-21 as 7 of the old 0-10 scale; see the block above):** Claude plans, writes briefs, reviews and integrates; DeepSeek workers do the research, audits, analysis of completed profiling and acceptance runs, assigned documentation, and most implementation - including multi-file changes within a crate - each in its own worktree with an owned-file list (`tools/ds_impl.ps1`). Claude keeps integration, cross-crate architecture, the `crates/shared/` contracts, dependency changes, and the acceptance/sign-off runs. Every worker patch gets an independent review and harness-run acceptance before it is merged; a tuning or calibration task (thresholds, geometry, heuristics fitted to samples) must also pass on examples the worker never saw. Minimize Claude's investigation, review and repair load; DeepSeek cost is negligible to the user, so do not cut useful reasoning or review to save it. See `tasks/deepseek/delegation-policy.md`.
- Commit each meaningful step. Prefix the commit subject with who wrote the change: `[claude]`, `[deepseek]`, or `[chatgpt]` if ChatGPT/Codex joins. DeepSeek workers launched from `tasks/deepseek/` commit only their own paths and leave handoff notes in `docs/handoff/`.
- Task queue for DeepSeek workers and how to run it: `tasks/deepseek/README.md`. Review `[deepseek]` commits and their handoff notes before building on them.
- Before committing Rust changes, run what CI runs: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and the tests of the crates you touched.
- Keep `docs/` current. Findings go in the docs (`docs/research/`, `docs/roadmap/`), not only in chat.
- `local/` (worker state, checkouts, advisor notes) and `target/` are Git-ignored. Never commit them, and never commit Bethesda game data.
- Only one session runs the profiling or acceptance scripts (`scripts/phase2-*.ps1`) at a time. They need the GPU and a release build, and parallel runs spoil each other's timings.

## At the end of every session

Commit your work, then add a dated entry at the top of the handoff log: what changed, what was tested, whether a worker is still running, and the next step.

## Handoff log

### 2026-09-23 (overnight, in progress) - Claude (visual gaps, animated doors and seamless crossing, demo for contributors, upstream PR plan)

- **User's goals for the night:**
  1. all the fixes and visual gaps;
  2. animated doors, with seamless cell-to-cell walking through portals;
  3. prepare the demo for other contributors, without publishing it;
  4. general project work;
  5. an audit and the design of a reasonable upstream PR.
- **Landed so far:**
  - `23a95a2` impl-023: the door front comes from the return link's `XTEL` (review-035 running).
  - `cc53baf` docs-030: `docs/demo/README.md` and `tools/demo/*.ps1`. Claude ran both dry runs.
  - `6998431` research-028: the fixture "hang" is by design; it needs a benchmark window.
  - `4ce6f1f` research-027: `docs/design/animated-doors-and-seamless-crossing.md`.
  - research-029: `docs/pr/upstream-pr-plan.md`, a series of 9 PRs, with the cleanup and
    exclusion lists.
  - research-031: `docs/research/reconversion-disk-budget.md`.
  - `79ce3be`: the visual-gaps spec from lead-026, in `docs/research/visual-gaps-spec.md`.
- **Running:**
  - impl-032: converter NIF Open/Close → glTF clips;
  - impl-033: door state machine; no DB column, clips found by name through `Gltf::named_animations`;
  - impl-034: seamless crossing;
  - impl-036: emissive exposure, HDR + bloom, light reach cap;
  - impl-037: empty marker models;
  - impl-038: converter `space_lighting`, `matos`, the `statics` DNAM columns;
  - review-035.
- **Disk:** each ds_impl worktree seeds about 11 GB of `target/`. Discard a worktree as soon as it
  is integrated. That alone freed 45 GB tonight.
- **Reconversion: blocked on a user decision.** The code is ready. Converter schema 16 is in
  `97eefa4`: door clips, `space_lighting`, `matos`, and a migration that keeps cached textures.
  `WORLD_DATABASE_SCHEMA_VERSION` stays 5, because the new tables are additive and probed.
  - Claude's attempts to delete `C:\Modding\SkyrimConverted\vfs` and `.ingestion-cache` (47 GB,
    regenerable) were refused by the permission classifier as irreversible, and so was
    inspecting drive D: for a fresh conversion. With 74 GB free, an in-place run does not fit
    without that deletion.
  - The user chooses:
    - **(a)** delete those two folders, then run
      `target\release\converter.exe "<SkyrimSE>\Data" C:\Modding\SkyrimConverted --cpu-jobs 16 --report-json C:\Modding\SkyrimConverted-conversion-report.json`.
      This is fast, since textures are reused.
    - **(b)** approve a fresh conversion to D:, about 5 h.
  - After it publishes, set the engine's `converter_schema_version()` (`app.rs`) to **16**.
    Until then the engine stays at 15 on purpose, so the current assets keep working.
- **Upstream:** `origin/main` has moved 16 commits ahead (`dummy-content`, ADRs, CI). A trial merge
  in `local/upstream-trial` has one test-only conflict in `bsa.rs`, now resolved; its build is
  being checked.

### 2026-09-22 (night) - Claude (rotation fix, distant terrain, closed doors)

- **User asks:** the Ruined Tower was rotated wrongly in the side-by-side comparison, so check rotation and placement. Also increase the render distance so distant terrain shows.
- **Rotation (`933b95f`, Claude, shared contract):** Creation `REFR` angles turn clockwise. Objects are now rotated by `Rx(-x) Ry(-y) Rz(-z)`, the inverse of the old conversion.
  - Yaw was proven from the door links: 88% of doors were consistent against 42% under the old convention.
  - Tilt and order were confirmed by research-023 (`22ad24c`): on 82,731 tilted references the median error against the terrain normal is 8.2°, and every alternative is 19-32°. Scale and position are applied correctly.
  - Objects and the arrival camera now share one convention.
- **Reference poses:** re-fitted by research-024 (`6804978`, `9d6d844`): 1 matched, 15 close, 10 area only. The Alftand exteriors are shot from the north side of the ravine. The Ruined Tower door faces east.
- **Closed doors (`f9c439e`, impl-022):** a load door draws closed unless the portal is rendering through it.
- **Distant terrain (`5211bad`, impl-021 plus a review round):** a terrain-only ring out to `--terrain-radius` (default 8), with distance fog and `NotShadowCaster` on the ring.
  - Frame rate on the Pale view: 180 FPS without the ring, about 97 FPS at radius 8 (shared GPU, rough).
  - Claude's change: benchmark runs keep the ring off unless `--terrain-radius` is given. A default benchmark passed with 0 commit-budget violations.
  - DeepSeek review-025 verdict: accept after fixes.
  - Unexplained: Claude's own `--streaming-fixture --terrain-radius 8` run did not exit within 400 s. The worker's fixture run validated, but the ring terrain was not exercised there. Check this before the Phase 2 runs.
- **Harness:** `tools/ds_impl.ps1` synced to skill v1.3 (`54645f9`).
- **Queued, not started:** impl-023 (`tasks/deepseek/impl-023-door-front-from-xtel.md`). The portal takes a door's front from the model axis, which is wrong for many door models (the tower door opens only from its back). The fix takes the front from the return link's `XTEL`.
- **Open rendering gaps:** glow/emissive, per-cell lighting templates and interior fog, the snow shader (roofs render bronze), overcast sky and weather, ice reading as rock, `BlackPlane01` voids, two Blackreach models that fail validation.
- **No workers running.**

### 2026-09-22 (late) - Claude (reference screenshots and matched renders)

- **User ask:** collect Skyrim screenshots of the demo's locations, then render the same compositions in OpenSkyrim for comparison.
- **References:** 26 UESP screenshots (CC BY-SA; download approved by the user) in `local/reference/uesp/` (git-ignored, `SOURCES.txt`).
- **Built:** design `docs/design/reference-shots.md` (the shots-file contract). impl-017 `--shots <file> [--shots-out <dir>]` renders exact poses once streaming settles (`5e53712`, DeepSeek, verified at runtime by Claude). research-018 wrote the first poses, `tools/compare_shots.py` and the mapping/fitting scripts (`adbea02`, `4050696`).
- **First renders:** `local/reference/cmp1/`. The Ruined Tower matches; most other poses are rough (walls, and some pitch signs flipped).
- **Found:** the Alftand01 arrival is washed out to white at eye height. The suspect is the stopgap camera lantern (3e9) in `update_atmosphere`. It showed once crossings landed at eye height (`4b1dd7a`).
- **Done:** research-020 refined all 26 poses by rendering: 1 matched, 12 close, 13 area only (`036efbe`; compare-sheet fix `64129bb`). impl-019 calibrated the lighting (`efa5351`):
  - the lantern is removed;
  - Bevy's 4π on point lights was missing from `lights.rs`;
  - the ambient levels and colours were re-measured.
  Claude's holdout check on the refined poses: Alftand02 is 1.8x too dark (it was 8x), AlftandWorld 1.2x, Blackreach 1.5x too bright; Tamriel is unchanged. `.deepseek-agents.json` now has `implAllowTools` with `cargo build` and the release engine.
- **Renders:** refined poses with the new lighting are in `local/reference/cmp-lit/` (with `_sheet.jpg`). Measure with `python tools/research/exposure_stats.py --shots <file> --render <dir>`.
- **Rendering gaps the comparison found** (in `docs/research/reference-shot-poses.md`):
  - no emissive/glow (Blackreach);
  - no per-cell lighting templates (`XCLL`/`LTMP`) or fog, so the lighting is flat;
  - ice reads as rock;
  - no snow shader or weather;
  - black voids (`BlackPlane01`?);
  - two Blackreach models fail validation (`WispAmbush`, `FrostSpiderAmbush01`);
  - one 4334-radius `FalmerCityLight02NS` floods the Blackreach ceiling orange.
- **Next:** the user picks the order. Glow first is recommended.

### 2026-09-22 (day) - Claude (terrain textures, see-through meshes, open doors with portals)

- **User asks:** work on terrain and terrain textures; doors not rendered (or open) so you can see into the next interior; fix objects that render transparent or misplaced.
- **Merged (DeepSeek, reviewed and integrated by Claude):** impl-009 portals + open load doors + pre-streamed cells kept out of the active space (`f219a05`); impl-010 terrain - root cause of the stretched textures was the ClampToEdge sampler on tiled layers, now Repeat; bilinear layer weights in the material uniform (`0ece570`); impl-012 converter - vertex alpha no longer means blend (2,632 materials were see-through), blend factors kept, EditorMarker shapes dropped (`b4ce0d9`). research-011 (`bc54a17`) diagnosed the transparency/misplacement classes.
- **Claude fixes found only at runtime:** portal startup ordering panic; WGSL u32->f32 in terrain.wgsl; portal quad was a floor-oriented `Plane3d`; every crossing arrived facing the return door (`arrival_camera_rotation`, `a49b4df`). Verified with `--demo-tour`: 4/4 crossings, walk test, and a doorway screenshot showing AlftandWorld through the Alftand02 door (`local/demo/portal5/02-door.png`); the tour now saves the portal camera's own image per door.
- **Harness:** `claude.exe` lives under the MSIX package folder, so launches need the updated skill (fixed by the DeepSeek Workers session); `tools/ds_impl.ps1` skips acceptance when no worker ran and seeds each task's `target/` from the lead's debug build (ported from the harness). Pending: sync the other tools with the harness at a clean point.
- **Later the same day:** schema-15 assets published (see-through ice/rock and editor markers gone, verified on screen); impl-013 glows/terrain bases/labels (`8d4160b`); tools synced with the harness (`c17fc48`); impl-014 converter `lights` table + `references.radius_override`, DB schema v5 (`5cd1bf4`); impl-015 engine point lights with a nearest-64 budget (`2acd409`); impl-016 auto-load doors cross on contact (`83ec4cd`). All DeepSeek, all accepted without correction.
- **Claude runtime fixes:** `ds_impl -Integrate` now stamps copied files (a 1 s "build" had reused a stale binary); lights never rendered - model-less LIGH references lacked `Visibility` (B0004) - and were ~100x too dim for the camera exposure (`EXPOSURE_CALIBRATION = 50` in `lights.rs`, the brightness knob); `update_atmosphere` never applied when starting inside Blackreach (`188e184`).
- **Data state:** `C:\Modding\SkyrimConverted` is converter schema 15 / DB schema 5 (504 lights, 15,167 lit references, 315 auto-load doors). A publish can fail with "Access is denied" if anything holds `skyrim_world.db` open (a worker script did); `converter.exe ... --resume-staging <staging dir>` republishes.
- **Verified:** `--demo-tour` 4/4 crossings + walk test on the final build; screenshots in `local/demo/lights3/`. The user is running agentic tests on this PC, so no FPS/timing numbers from today are valid.
- **Evening fix (Claude):** the black Alftand01 -> Alftand02 (and Tamriel -> Alftand01) portals were a heading-sign bug, not rendering: a Creation `XTEL` heading turns *clockwise* (forward = `(sin z, cos z)`; every route link puts the arrival in front of its destination door along that), while `arrival_camera_rotation` reused the counter-clockwise object convention plus a half turn, which only happened to fit AlftandWorld. Now `Quat::from_rotation_y(-z)`, with a regression test built from the four real links. Crossings also land the camera at eye height (XTEL is the feet point; fly mode and the tour had sat on the floor). Tour 4/4, screenshots `local/demo/portalfix2/`. Open question: objects still use the counter-clockwise `Rz` convention and look right on screen; not re-verified against the clockwise heading finding.
- **Known gaps:** lights overexpose right next to a source (inverse-square; tune `EXPOSURE_CALIBRATION` or add a tonemapping/exposure pass); GPU clustering ignores render layers, so a portal destination's lights can faintly reach the active space (impl-015's note).
- **Next:** the user's look at the lit demo; then the Phase 2 shakedown and visual review (no GPU-heavy runs while the user's agentic tests run).

### 2026-09-22 - Claude (overnight: walkable Alftand -> Blackreach demo; Phase 2 sign-off paused)

- **The user's goal for the night:** "get this cool Blackreach demo up and running, extra points if it's interactable and I can walk through the level". Done: `play-blackreach-demo.cmd` (or `target\release\engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk`) starts on foot at the Alftand exit; four load doors (E) lead down to Blackreach with no loading screen. `--demo blackreach` (or `play-blackreach-shortcut.cmd`) starts inside Blackreach.
- **Verified:** `--demo-tour <dir>` walks the route by itself, screenshots every place and door, then holds W in Blackreach. Final run on `3de1505`: 4/4 crossings, walk test 600 units grounded, 0 errors, 170-180 FPS. Screenshots: `local/demo/final/`.
- **Built by DeepSeek workers, reviewed and integrated by Claude:** impl-005 (converter: `XTEL` -> `door_links`, DOOR/ACTI/FLOR/CONT/TREE/LIGH models, schema v4), impl-006 (engine: `ActiveCell`, interiors, rebase guard, `TransitionPlugin` pre-streams and crosses), impl-007 (`PlayerPlugin`: mouse look, walking via Bevy mesh ray casting, E to open, F to fly), impl-008 (weld vanilla one-point terrain seams instead of dropping cells; no water plane on dry cells). All four accepted without correction. Claude: design + `doors.rs` contract, CLI (`--walk`, `--demo`, `--start-position/--start-yaw`, `--demo-tour`), app wiring, demo tour, atmosphere (sky / underground ambient / lantern), objective text, launchers.
- **Data:** `C:\Modding\SkyrimConverted` reconverted at schema v4 (complete, integration passed, 2204 door links all resolved; the four route doors match research t02 exactly).
- **Harness fix:** `tools/ds_impl.ps1` now gives every checkout its own `target/`; a shared one let parallel engine workers run each other's test binaries (found by impl-007).
- **Known gaps:** the terrain layer blending is still blocky (research-004's Defect A, fix recipe in `docs/research/terrain-blending-seams.md`, not yet implemented); `LIGH` references are not real lights (interiors use ambient + a camera lantern, Blackreach reads dark blue); load doors are opened with E only (Skyrim's auto-load doors fire on contact); no skinned meshes, NPCs or physics (Phase 4).
- **Phase 2 sign-off (paused for the demo):** shakedown on real assets passed every gate except the visual review; the terrain fixes above are what the visual review needs. Test locations chosen from data: rural (0,0), dense (4,-21), water (21,-3). Next: implement the blending fix, rerun the shakedown, user reviews screenshots, then two full campaigns.

### 2026-09-21 - Claude (clone, toolchain, DeepSeek harness)

- **Cloned** `realfakenerd/OpenSkyrim` at `8919096` into `C:\Modding\OpenSkyrimProject`.
- **Toolchain installed** on this machine: Visual Studio 2022 Build Tools (C++ workload) and Rust stable 1.98.1 (MSVC) via rustup. CMake and Ninja were already present.
- **DeepSeek harness** copied from the DOA Xbox360 UI project and adapted: `.deepseek-agents.json`, `tasks/deepseek/` (context, cookbook, delegation policy, reviewer, run analyst, advisors, self-test, implementation template, result schema), `tools/ds_impl.ps1` (isolated worktree per implementation task, sharing `target/`), `tools/ds_status.ps1`, `tools/run_ds_queue.ps1`, `tools/ds_report.py`, `tools/check_worker_result.py` (now recognises `cargo` commands), `tools/check_citations.py`, and their tests (`python -m unittest discover -s tests`).
- **Verified:** `cargo check --workspace` passes (5 min cold, only a future-incompat warning from the `proc-macro-error2` dependency). The harness self-test passed 6/6 (`local/agents/runs.csv`).
- **Skyrim SE** is installed at `C:\Games\Steam\steamapps\common\Skyrim Special Edition`. Workers can read it (in `readOnlyDirs` and described in `tasks/deepseek/context.md`) but never write to it.
- **Private project:** the user wants none of this public. Commit locally only, never push, never open PRs against the upstream repository.
- **Next:** run the harness self-test (`tasks/deepseek/selftest.md`), then choose the first tasks.
