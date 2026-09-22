# DeepSeek task queue

Briefs for DeepSeek worker agents (headless Claude Code on DeepSeek V4.1 Flash), run with the `deepseek-agents` skill's launcher. The setup is copied from the user's DOA Xbox360 UI project and adapted to OpenSkyrim. Claude or the user can launch them.

**Division of labour (delegation level 7, set by the user 2026-09-21):** DeepSeek does the research, reviews, analysis of completed runs, assigned documentation and most implementation, including multi-file changes within a crate, each through `tools/ds_impl.ps1` in its own worktree. Claude keeps integration, cross-crate architecture, the `crates/shared/` contracts, dependency changes and the acceptance/sign-off runs. Every worker patch gets an independent review and harness-run acceptance before it is merged; a tuning or calibration task (thresholds, geometry, heuristics fitted to samples) must also pass on examples the worker never saw. Follow `delegation-policy.md` for isolation and scoped permissions. Prioritize saving Claude's tokens, not minimizing DeepSeek cost.

## Before launching anything

- **Check which files are in flight.** Run `git log --stat -10` and check which files the last few commits touched, not only which feature area they belong to. Two workers can safely work on the same feature if they own different files. They can collide on the same file even when the features look unrelated.
- **Always safe:** a pure research brief (`docs/research/*.md`, `docs/research/data/*`, `docs/handoff/*.md`). The shared protections refuse `crates/`, `vendor/`, `Cargo.*` and `scripts/` to every research worker, so a research task cannot collide with implementation work even by accident. Analysing a finished run under `target/acceptance/` or `target/visual-baselines/` is equally safe.
- **Safe if the owned files are checked first:** a bounded implementation task (`tools/ds_impl.ps1`) whose `Owned files:` names files that no other in-flight work has touched recently. Good examples are a new module with no wiring yet (for example `crates/converter/src/esm/records/<rec>.rs` plus its tests), or a new `src/bin/` audit tool. Registering the module in `mod.rs` is the lead's one-line integration step. Never assign `crates/engine/src/app.rs`, `streaming.rs`, `render.rs` or anything in `crates/shared/`: those stay serial with the lead.
- **Pick the next task number from the filesystem, not from memory:** `Glob tasks/deepseek/[ti]*.md` right before writing a brief. In the DOA project two sessions picked the same number four minutes apart.

Numbering: `tNN-*` are research, analysis or documentation briefs; `iNN-*` are implementation briefs run through `tools/ds_impl.ps1`.

## Run a task

From PowerShell in the project folder:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "C:/Users/Brannock/.claude/skills/deepseek-agents/ds-agent.ps1" -TaskFile "tasks/deepseek/<brief>.md" -Dir "C:/Modding/OpenSkyrimProject" -Label <brief>
```

`.deepseek-agents.json` in the project root supplies the rest: edit mode, effort max, 400 turns, a 150-minute timeout, the standard allowed-tools line, the read-only reference folders and the write-protected paths. Pass a flag only to override one of them. `-DryRun` prints the whole resolved policy without launching anything.

The output ends with `[ds-agent] session=<id> status=...`. Workers make one tool call per turn, so large tasks need 300-400 turns. If a worker stops with `error_max_turns`, resume it with `-Resume <id> -Task "Write your report and commit what you have, then continue."` and the same `-Dir`. A `402 Insufficient Balance` error means the DeepSeek account needs a top-up.

## What a worker may touch

- **Read:** the project, the cargo registry sources (every dependency's code at the version in use) and the Skyrim SE install. The list is in `.deepseek-agents.json` and described in `context.md`.
- **Write:** the project only, except `crates/`, `vendor/`, `Cargo.toml`, `Cargo.lock`, `.cargo/`, `.github/`, `scripts/`, the worker tooling, `AGENTS.md`, `CLAUDE.md` and `.deepseek-agents.json`. These are refused for the file tools and for shell redirects alike. `tests/` (the harness's Python tests), `docs/` and `tools/research/` are writable.
- **Shell:** `cargo check/test/clippy/tree`, `cargo fmt --check`, `python tools/*`, `python -m unittest`, and `git add/commit/status/diff/log`.
- Enforcement blocks the direct routes. A script a worker writes can still open any file, so the rules in `context.md` still matter.
- **Images:** workers can see them. A brief can point at a frame or a screenshot and ask what is in it.

## Implementation tasks (the pilot)

Bounded coding work runs through `tools/ds_impl.ps1`, not the launcher directly:

```powershell
tools/ds_impl.ps1 -Brief tasks/deepseek/i01-<name>.md    # run one task
tools/ds_impl.ps1 -List                                  # what is waiting for review
tools/ds_impl.ps1 -Integrate i01-<name>                  # copy its files into the main checkout
tools/ds_impl.ps1 -Discard i01-<name>                    # throw the checkout away
tools/ds_impl.ps1 -Post i01-<name>                       # re-run the checks on a finished checkout
```

Each task gets its own `git worktree` under `local/impl/<task>` from a recorded base commit. It keeps the shared read-only folders and deny rules, minus the files its brief assigns. All checkouts build into the main `target/` (`CARGO_TARGET_DIR`), so a worker does not recompile Bevy from scratch. The brief carries its own contract in `Owned files:` and `Acceptance:` lines, which the runner reads. For example:

```
Owned files: crates/converter/src/esm/records/weap.rs,crates/converter/tests/weap_record.rs
Acceptance: cargo test -p converter --test weap_record,cargo clippy -p converter --all-targets -- -D warnings
```

Workers never run git. The lead reviews the diff and integrates it. After the run the script reports anything touched outside the owned list, runs the acceptance commands in the checkout, and appends the task to `local/agents/pilot.csv`. `python tools/ds_report.py` summarizes that record.

Briefs follow `tasks/deepseek/templates/implementation.md`, and results must match `tasks/deepseek/result-schema.json`: base commit, a 300-600 word summary for the lead, critical evidence, unresolved assumptions, commands actually run, the next falsifying test, and changed files. `python tools/check_worker_result.py <result>` checks a result against that contract. `tasks/deepseek/analyst.md` analyses a completed run and `tasks/deepseek/reviewer.md` reviews a patch independently. Both take their target through `-Extra`.

## Unattended queue

`tools/run_ds_queue.ps1 -Briefs t02-a,t03-b -WaitFor t01-c` runs research briefs one after another, after the named running briefs finish. It resumes a turn-limited worker up to twice, checks the citations in what each worker committed, and updates the status column below. A brief whose first line has a backticked tools string passes it as `-AllowTools`; otherwise the config baseline applies. Briefs that edit the same files must not run at the same time.

## Checking a worker's work

- `python tools/check_citations.py docs/research/<note>.md` checks every `path:line` against the repository. `BROKEN` means the citation cannot be followed. `check` means the path resolves more than one way (every crate has a `lib.rs`).
- `tools/ds_status.ps1` shows the workers running now, with how long each has been quiet, then the last runs from `runs.csv`. `tools/ds_status.ps1 -Stop <label>` stops one properly.
- `tasks/deepseek/selftest.md` runs a short worker that checks the harness itself: search, a reference-folder read, an image, a refused write and an allowed write. Run it after changing the launcher, the model, the config or the permission rules.

## Watching and stopping a worker

Each running worker writes `local/agents/<label>.json` with its `claude.exe` pid and the exact `taskkill` line that stops it. The file is removed when the worker finishes. Every finished run appends a row to `local/agents/runs.csv` (status, turns, tokens, seconds, denied tools).

Killing the queue runner or the `ds-agent.ps1` PowerShell process does **not** stop the worker. The headless `claude.exe` keeps running and keeps committing. Use `tools/ds_status.ps1 -Stop <label>`, then check `git log` for commits it made after the stop.

## Rules every brief includes

- Read `tasks/deepseek/context.md` first.
- Commit only your own files with `git add <paths>` then `git commit -m "[deepseek] <summary>" -- <paths>`. The `-- <paths>` form commits only those paths, so parallel workers do not sweep up each other's changes. Retry if `index.lock` exists.
- Write a handoff note to `docs/handoff/<task-name>.md`: what you did, what you tested, what is untested, what to do next. Do not edit `AGENTS.md`.
- Never commit game data or anything under `local/` or `target/`. Never run the profiling or acceptance scripts: those runs belong to the lead.

## Queue

| Brief | Goal | Status |
| --- | --- | --- |
| `t01-phase2-status-audit.md` | True status of Phase 2 milestones M0-M8 from the code; the conversion command; the acceptance gates likely to fail | done, reviewed by Claude (claims spot-checked: grid preflight, baseline warning, no NiTriStripsData parser) - `docs/research/phase2-status.md` |
| `t02-worldspace-transition-demo.md` | Alftand -> Blackreach seamless-transition demo: route IDs from the data, converter/DB/engine gaps, recipe, Markarth fallback (requested by the QnA session) | done, reviewed by Claude (spot-checked: no XTEL in code, exterior-only plan_cells, statics STAT/MSTT/FURN only, fixed worldspace_id) - `docs/research/worldspace-transition-demo.md`; DB queries in its section 7 still to run on the real conversion |
| `impl-003-bounds-check-precision.md` | Spawned-bounds check lost f32 precision far from the origin (MineScaffoldFreeRopeEnd01 rejected in the real-asset stability run); compose root-relative transforms | done, accepted without correction after review (`7195065`); failing-before test verified. Note: `crates/engine/src/app.rs:953` keeps a second world-space copy, used only by an origin fixture |
| `research-004-terrain-blending-seams.md` | Root cause of blocky terrain blending and grid-line seams in the real-asset screenshots (M4 blocker) | done, reviewed (`b3cb997`; spot-checked the per-cell water plane at `streaming.rs:474` and the tangent-packed weights vs Bevy's tangent renormalization). Fix queued after impl-006 merges, since both touch `streaming.rs` |
| `impl-005-converter-door-links.md` | Converter: XTEL door links, six more base types, schema v4 | done, accepted without correction (`df91524`) |
| `impl-006-engine-door-streaming.md` | Engine: active cell, interiors, seamless door crossings | done, accepted without correction (`7a155fa`) |
| `impl-007-player-controller.md` | Engine: first-person player controller (--walk) | done, accepted without correction (`be8c2dc`); found the shared target-dir flaw |
| `impl-009-door-portals.md` | Hide load doors, isolate pre-streamed cells, portal view into the destination | done, accepted (`f219a05`; lead fixed a startup-order panic the worker could not run) |
| `impl-010-terrain-blending.md` | Smooth terrain layer blending; stretched terrain textures | done, accepted (`0ece570`; root cause: ClampToEdge sampler; lead fixed a WGSL type error found at first runtime compile) |
| `research-011-transparent-and-misplaced-meshes.md` | Why objects render transparent or misplaced | done, reviewed (`bc54a17`; spot-checked `alpha_contract` vertex-alpha -> Blend) |
| `impl-012-converter-alpha-and-markers.md` | Converter: vertex alpha is not blend, keep blend factors, drop editor markers | done, accepted without correction (`b4ce0d9`; lead bumped CONVERTER_SCHEMA_VERSION to 15) |
| `impl-013-glow-blending-and-terrain-base.md` | Engine: additive/multiplicative glows, textureless terrain bases, clean door labels | launched 2026-09-22 |
| `impl-008-terrain-weld-and-water.md` | Weld vanilla terrain seams instead of dropping cells; no dry water planes | done, accepted without correction (`68c19bb`) |

Update the status column when you launch or finish a task.
