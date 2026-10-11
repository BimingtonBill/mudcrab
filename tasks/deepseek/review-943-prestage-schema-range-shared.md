# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: fix(launcher): ready to play on every schema the engine loads; one schema range in shared
- What it claims (the PR text a reviewer will read): `local/review/pr-phase2-schema-range-shared/pr-text.md`
- Its diff against upstream main 1ad25cdc: `local/review/pr-phase2-schema-range-shared/full.diff` (commit list: `local/review/pr-phase2-schema-range-shared/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `local/review/pr-phase2-schema-range-shared/base/` holds upstream main's version of every file the PR touches, and `local/review/pr-phase2-schema-range-shared/head/` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- Open PR #95, head `3c7533dd` (branch `phase2/schema-range-shared`), base = upstream main `1ad25cdc` (the PR's merge base
  a896b903; main has not touched these files since). CI on the head is green.
- What it does, in short: the engine accepts converted outputs from converter schema 15 up to the current one and world
  database schema 3 up to the current one; both minimums move into `crates/shared`, and the launcher's "ready to play" check
  (`crates/launcher/src/conversion/mod.rs`, `output_is_complete`) uses the same ranges, reading the manifest as written
  instead of through `ConversionManifest::load` (which marks an older schema incomplete for reuse).
- Overlap: TaylorTurnerIT's open PR #105 (terrain LOD) bumps the converter schema to 17 and touches
  `crates/converter/src/cache.rs`, `crates/engine/src/app.rs`, `crates/launcher/src/conversion/mod.rs` and
  `crates/shared/src/lib.rs`. After #105, this PR's test matrix may need 17/5 accepted and 18/6 rejected (with the newer
  database schema #105 brings). Say whether the PR's tests are written against constants (they follow a bump) or
  hard-coded numbers (they break), and what must change after #105.
- Known: converter `check` (Phase 1's code) insists on the current schema, so Check can report an older in-range output as
  a schema mismatch while Play is ready; the PR text should name this. Check that it does.

# Scope
Read only. Change nothing.

# Done when
You have checked, with diff line evidence:
1. **Correctness:** logic errors, edge cases, panics, integer or float overflow, off-by-one, wrong units.
2. **What used to work and now fails:** every behaviour change against main: defaults, flags, outputs, file formats, timing, a test that was weakened.
3. **Claims vs diff:** every number and statement in the PR text is backed by the diff or its evidence. Anything overstated or missing gets flagged (for example a claimed test that doesn't exist).
4. **Tests:** whether the new behaviour is covered, and what a reviewer would ask to see tested.
5. **Scope and size:** anything unrelated that should be split out; leftover debug code, local paths, TODOs.
6. **Overlap:** conflicts with other open work named in the context.

# Report
- Verdict line: ready to open / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 8, only ones you verified in the diff. Mark each **blocker**, **should fix** or **nit**.
- PR text problems (claims the diff doesn't support), as a separate short list.
- Under 450 words.
