# Goal
Track: Phase 2 (for upstream). When `converter --resume-staging <dir>` is given a staging folder
whose conversion already finished (only the final publish failed), it publishes that staging
folder without re-walking and re-converting the assets.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`fix(converter): ...` or
  `feat(converter): ...`), no `[deepseek]` prefix, no project-internal references (portal,
  DeepSeek, task numbers) in code or docs.
- Publishing renames the staging folder over the output folder (`publish_directory` in
  `crates/converter/src/pipeline.rs`). On Windows that fails with "Access is denied" while any
  process holds a handle inside either folder (an engine reading `skyrim_world.db`, Explorer, a
  size scan). The staging folder is then kept, and the user re-runs with `--resume-staging`.
- Today `--resume-staging` (`crates/converter/src/config.rs`, `main.rs`, `pipeline.rs` around
  `let resumed = config.resume_staging.is_some();`) runs the whole `run_into` again into that
  staging folder, with the *output folder's* manifest as the reuse cache. Measured 2026-09-25 on a
  full Skyrim SE conversion: the first run finished its work in about 10 minutes and failed only at
  publish; the resume then re-converted 33,423 assets (12 minutes) before publishing.
- Find out what a finished staging folder holds (its `conversion-manifest.json`, the integration
  report, the world database, the cell cache) and what `run_into` writes last, so that "finished"
  can be told apart from "interrupted part-way".

# Scope
Owned files: `crates/converter/src/pipeline.rs`, `crates/converter/src/config.rs`,
`crates/converter/src/main.rs` (only if a message or the usage line changes), tests inside those
files, and your handoff note `docs/handoff/impl-579-p2-resume-publish-only.md`. No schema bump,
no new dependencies, no change to what a normal (non-resumed) run does.

# Design constraints
- Publish without converting only when the staging folder proves it finished: its manifest is
  `complete`, has the current `CONVERTER_SCHEMA_VERSION` and the current configuration hash, and
  every file the run writes last exists. Otherwise fall back to today's full resume.
- Before publishing, still run the checks a normal run does after converting, if they are cheap
  (e.g. the generated-artifact validation). If one is expensive, say which and why you skipped it.
- Print which path was taken ("staging is complete: publishing without re-converting" vs the
  reason it could not), so a user can tell.
- Keep the error when the staging folder does not exist.

# Done when
- Tests cover: a complete staging folder publishes without calling the conversion (assert via
  what the test can observe, e.g. a source asset whose conversion would fail, or a counter); a
  staging folder with `complete: false`, an older schema, or a different configuration hash takes
  the full path; a missing last-written file takes the full path.
- These pass in your checkout, using the shared warm check folder so they don't rebuild every
  dependency:
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo test -p converter --target-dir C:/Modding/shared-check-phase2`
- Your handoff note says what "finished" means now and what a user sees.

# Report
The diff summary per file, the rule for "finished", the test names and results, and anything you
were unsure of.
