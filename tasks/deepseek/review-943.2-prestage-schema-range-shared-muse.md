# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: fix(launcher): ready to play on every schema the engine loads; one schema range in shared
- What it claims (the PR text a reviewer will read): the PR text, pasted at the end of this brief
- Its diff against upstream main 1ad25cdc: `git diff 1ad25cdc...3c7533dd` (commit list: `git log --oneline 1ad25cdc..3c7533dd`). Read them with git (see below).
- **Read code only through git.** Upstream main's version of a file: `git show 1ad25cdc:<path>`; the PR's version: `git show 3c7533dd:<path>`. Your working folder is a DIFFERENT branch (the portal fork, with its own streaming code): never read its files or cite them as main's behaviour.
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
Read only. Change nothing. You may run `git show`, `git diff`, `git log`, `git grep` only.

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


# The PR text

> 🤖 Written by Claude (AI), checked by @BimingtonBill.

The launcher now calls a conversion ready to play whenever the engine would load it, including a converter schema 15 conversion with world database schema 3, which it refused before.

## Objective

After #72 the engine loads conversions from converter schema 15 (world database schema 3) as well as the current 16 (database 4), but the launcher's ready check still wanted the current schemas exactly. So a conversion made before the recent staging merge is playable from the command line, yet the launcher keeps Play switched off and offers to convert again.

## Details & Implementation

- The oldest schemas the runtime still loads are now defined once, in the `shared` crate that the engine, `world-inspect` and the launcher all use: converter manifest schema 15 and world database schema 3, plus one function that says whether a database schema is in range.
- The engine and `world-inspect` read those definitions instead of their own copies, so the engine's behaviour and messages do not change.
- The launcher's ready check accepts the same ranges: converter manifest 15 up to the converter's current schema, and world database 3 up to the current 4.
- The launcher now reads the conversion manifest as written. The converter's own loader treats an older manifest as incomplete, because that is how it decides what to redo on the next conversion; the engine does not, so the launcher follows the engine here.

<details><summary>More detail</summary>

- New in `shared`: `MIN_RUNTIME_CONVERTER_SCHEMA_VERSION = 15`, `MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION = 3` and `supports_runtime_world_database_schema(u32)`. The newest values stay where they are defined today: `shared::WORLD_DATABASE_SCHEMA_VERSION` and the converter's `CONVERTER_SCHEMA_VERSION`.
- The engine's own copies from #72 (`MIN_`/`MAX_RUNTIME_DATABASE_SCHEMA_VERSION`, `supports_runtime_database_schema`, the private converter minimum) are removed.
- The integration report's schema is read as a `u64` and converted with `u32::try_from`, so an out-of-range number is simply "not ready".
- The roadmap and `db-schema.md` now say where the ranges are defined and that the launcher uses the same ones.

</details>

## Verification & Testing

- **Environment**: OS: `Windows 11` | Backend: `DX12`
- **Commands executed**:
  - [ ] `cargo check --workspace`
  - [ ] `cargo test --workspace`
  - [x] `cargo fmt --all -- --check`
  - [ ] `cargo clippy --all-targets --all-features -- -D warnings`
  - [ ] Manual runtime testing: not run; the change is a ready check covered by unit tests.
  - Actually run: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` (708 passed, 15 skipped).
- **New tests:** a launcher test that builds outputs at each schema pair (converter 15 with database 3 is ready; 14, 17, database 2 and database 5 are not; the current pair still is), which failed on the old exact check; a `shared` test for both ends of the database range.

## For reviewers
- **Behaviour change:** the launcher offers Play for a complete converter-15 / database-3 conversion instead of treating it as needing conversion.
- The converter's own `check` command still wants the current schema exactly, so on such a folder Play is ready while Check reports a schema mismatch. I left that alone: Check verifies what this converter would write.
- When the converter schema is bumped with a migration, `MIN_RUNTIME_CONVERTER_SCHEMA_VERSION` in `shared` should be checked against what the engine still loads.


<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Bug Fixes**
  * Existing conversions using supported older schema versions can now pass readiness checks, while unsupported converter and world-database versions continue to be rejected.
  * Runtime schema validation now uses consistent supported-version ranges across the launcher and engine.

* **Documentation**
  * Updated schema guidance to reflect the accepted version ranges and compatibility behavior.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
