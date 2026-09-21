# t01 - Phase 2 status audit: what is really done, milestone by milestone

Read `tasks/deepseek/context.md` first. This is a **research** task: you write documentation only,
never code.

## Goal

The lead is starting the Phase 2 sign-off on this machine with the user's real Skyrim SE install.
The plan is `docs/roadmap/02-completion-plan.md` (in Portuguese): milestones M0-M8 and one exit
criterion - an `accepted` verdict from `scripts/phase2-acceptance.ps1`. The docs claim some
milestones are done (M1 says "completo em 2026-09-10"), and the last 20 commits say "phase2
completion". Establish, **from the code and not from the docs**, the true status of every milestone,
so the lead knows which gates will fail before spending hours on a real campaign.

## What to do

For each milestone M0 to M8, and for each numbered implementation item inside it:

1. Find the code that implements it (`crates/`, `scripts/phase2-*.ps1`, `.github/workflows/`), or
   establish that none exists. Use `Grep`/`Glob`; cite `path:line`.
2. Find the test that proves it, if any (`#[test]` functions, the robustness cases in the acceptance
   script). A test that exists is not a test that passes: you may run `cargo test -p <crate>` for a
   crate whose status you are judging, and report the real result. The build shares `target/`; if
   it waits for a lock, wait.
3. Classify: **done** (code + test), **code only** (no test), **partial** (say what is missing),
   **missing**, or **needs real data / GPU / a human** (cannot be judged without them - say what
   run would judge it).
4. Note every place where the docs (`02-completion-plan.md`, `02-core-engine.md`,
   `02-acceptance.md`, `docs/roadmap/README.md`, `README.md`) disagree with the code.

Then, specifically for the lead's next step:

5. **How to convert the real game data.** From `crates/converter/src/main.rs`, `config.rs` and
   `pipeline.rs`: the exact command line to convert the install at
   `C:/Games/Steam/steamapps/common/Skyrim Special Edition` (which plugins, which output folder,
   which options), what files the output folder must contain for `phase2-acceptance.ps1`'s preflight
   (it checks `conversion-manifest.json`, `integration-report.json`, `skyrim_world.db`,
   `cell_cache.rkyv`), and roughly how long and how much disk it may take if anything in the code
   or docs says so. Do **not** run the converter.
6. **The acceptance campaign's gates, as a checklist.** From `scripts/phase2-acceptance.ps1`: every
   condition that leads to `rejected` or `accepted-with-warnings`, with `path:line`. Mark the ones
   you expect to fail today and why.
7. The grid coordinates the docs give for the rural, dense and water scenarios, and whether the
   code hard-codes any.

## Scope

- Write only `docs/research/phase2-status.md` and `docs/handoff/t01-phase2-status-audit.md`.
- Read-only everywhere else. Do not run `scripts/`, the converter or the engine.
- The Skyrim install is readable; you do not need it for this task beyond confirming which plugins
  exist in `Data/`.

## Done when

- `docs/research/phase2-status.md` has a table with one row per milestone item (M0.1, M0.2, ... M8.4):
  item, status, code evidence, test evidence, notes. Then sections for points 5-7, then "Doc/code
  disagreements", then "Risks for the first real campaign", ranked.
- `python tools/check_citations.py docs/research/phase2-status.md` reports nothing broken.
- Both files committed: `git add <paths>` then
  `git commit -m "[deepseek] t01: Phase 2 status audit" -- <paths>`.

## Report

Your final message: 300-600 words for the lead - the milestones that are genuinely done, the ones
that are not, the conversion command, and the three gates most likely to fail first. Every
consequential claim with `path:line`.
