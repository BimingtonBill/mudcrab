# Goal
Re-review only the fix delta for open PR #95 (`fix(launcher): ready to play on every schema the engine loads; one schema
range in shared`): does the fix commit 899f0fd9 address the earlier findings without new problems?

# Context
- Delta (no git available, read the file): `local/review/pr-phase2-schema-range-shared/fix-delta.diff` (on top of the PR head 3c7533dd).
- Earlier findings it addresses: (1) `crates/shared/src/lib.rs` range test asserted `!supports(5)`, which fails once
  `WORLD_DATABASE_SCHEMA_VERSION` becomes 5 (open PR #105 bumps it to 5 and converter schema to 17); (2) the launcher test
  in `crates/launcher/src/conversion/mod.rs` used literal 15/14/3/2; (3) the roadmap doc and `docs/specs/modding/launcher.md`
  stated the old exact-schema rule or literal ranges; (4) no test for a report `schema_version` that does not fit u32.
- Constants: `shared::MIN_RUNTIME_CONVERTER_SCHEMA_VERSION` = 15, `shared::MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION` = 3,
  `shared::WORLD_DATABASE_SCHEMA_VERSION` = 4 (5 after #105), `converter::cache::CONVERTER_SCHEMA_VERSION` = 16 (17 after #105).
- Already checked: fmt clean, clippy -D warnings clean, nextest launcher + shared 118/118.

# Scope
Read only. Change nothing.

# Done when
- Each of (1)-(4) marked fixed / not fixed, with delta line evidence; the tests still pass after #105's bump (reason it
  through with the constants); any new issue listed.

# Report
- Verdict: ready / small fixes / fix first. Findings as `file:line - issue - fix`, at most 5. Under 250 words.
