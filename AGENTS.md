# Working agreement for AI assistants

<!-- ds-delegation:begin (managed by DeepSeek Workers/tools/ds_delegation.py; edit with --set) -->
## DeepSeek delegation level: 7 of 10 (Claude plans and reviews)

Claude plans, writes briefs, reviews and integrates; workers do the exploration and most coding, including multi-file changes in their own worktrees with owned-file lists. Use a DeepSeek lead (-CanSpawn) for jobs that split into independent parts, and -Crosstalk for workers whose work touches.

At every level Claude keeps: talking to the user, design and architecture decisions, integration into the main checkout, security-sensitive changes, and review of every worker result it acts on (spot-check the claims that matter; read the full diff of any edit).

Missing tools: before starting a job, check that the tools it needs are installed. If a compiler, runtime or package the best approach needs is missing, stop and ask the user to install it (say what it is for and give the exact install command) instead of working around it with a weaker approach or a hand-written substitute. Continue without it only if the user chooses to.

This block is generated. Change the level with `python "C:/Users/Brannock/.claude/skills/deepseek-agents/tools/ds_delegation.py" --set <0-10> --agents-md` from this folder, or ask Claude to. The full scale is in the deepseek-agents skill.
<!-- ds-delegation:end -->


Claude (Claude Code) leads the work on this checkout and delegates bounded tasks to DeepSeek worker agents. The setup is carried over from the user's DOA Xbox360 UI project. This file is how each session, and each assistant, sees what the others did.

## At the start of every session

1. Run `git log --oneline -15` and `git status`.
2. Read the **Handoff log** below, newest entry first.
3. If `git status` shows uncommitted changes you did not make, stop and ask the user before touching them. Another session or a worker may be mid-task. `tools/ds_status.ps1` shows whether a worker is running.

## While working

- Do not rewrite history (no amend, rebase or force-push on commits already made).
- **Division of labour (delegation level 7, set by the user 2026-09-21; see the block above):** Claude plans, writes briefs, reviews and integrates; DeepSeek workers do the research, audits, analysis of completed profiling and acceptance runs, assigned documentation, and most implementation - including multi-file changes within a crate - each in its own worktree with an owned-file list (`tools/ds_impl.ps1`). Claude keeps integration, cross-crate architecture, the `crates/shared/` contracts, dependency changes, and the acceptance/sign-off runs. Every worker patch gets an independent review and harness-run acceptance before it is merged; a tuning or calibration task (thresholds, geometry, heuristics fitted to samples) must also pass on examples the worker never saw. Minimize Claude's investigation, review and repair load; DeepSeek cost is negligible to the user, so do not cut useful reasoning or review to save it. See `tasks/deepseek/delegation-policy.md`.
- Commit each meaningful step. Prefix the commit subject with who wrote the change: `[claude]`, `[deepseek]`, or `[chatgpt]` if ChatGPT/Codex joins. DeepSeek workers launched from `tasks/deepseek/` commit only their own paths and leave handoff notes in `docs/handoff/`.
- Task queue for DeepSeek workers and how to run it: `tasks/deepseek/README.md`. Review `[deepseek]` commits and their handoff notes before building on them.
- Before committing Rust changes, run what CI runs: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and the tests of the crates you touched.
- Keep `docs/` current. Findings go in the docs (`docs/research/`, `docs/roadmap/`), not only in chat.
- `local/` (worker state, checkouts, advisor notes) and `target/` are Git-ignored. Never commit them, and never commit Bethesda game data.
- Only one session runs the profiling or acceptance scripts (`scripts/phase2-*.ps1`) at a time. They need the GPU and a release build, and parallel runs spoil each other's timings.

## At the end of every session

Commit your work, then add a dated entry at the top of the handoff log: what changed, what was tested, whether a worker is still running, and the next step.

## Handoff log

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
