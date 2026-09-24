# Goal
Track: Phase 2 (for upstream). Fix the converter findings from the 2026-09-24 standing audit that
also hold on upstream's `main`.

# Context
- Your checkout is cut from `main`. Use upstream's commit style (`fix(converter): ...`), with no
  `[deepseek]` prefix.
- The findings were filed against the fork (`portal` branch, converter schema 21, DB schema 5), so
  line numbers and schema numbers differ on `main`. **First confirm each one on `main`**; skip any
  that does not hold there, and say why in the report.
  1. **SC-01** `crates/converter/src/esm/exporter.rs`: the database schema version is stamped as a
     number literal instead of `shared::WORLD_DATABASE_SCHEMA_VERSION`, a second, unguarded copy of
     the contract. Use the constant.
  2. **CP-03** `crates/converter/src/pipeline.rs` (archive extraction failure): `manifest.failures`
     is keyed on the archive's full path (absolute in the documented invocation), while the same
     archive is keyed relative to the data folder two lines above (`strip_prefix`). The manifest is
     published, so a machine path lands in a published file. Key it relative like the other.
  3. **CP-04** `crates/converter/src/integration.rs` (`report.passed`): the verdict never consults
     `report.issues`, so a database whose models have no readable bounds passes with no warning,
     and a missing `cell_cache.rkyv` only fails through `cache_cells != terrain_cells`, which is
     `0 == 0` when there are no `land` rows. Make `passed` false when any issue was recorded, and
     check existing tests and fixtures for issues that are really warnings. If some issues must stay
     non-fatal, split them into a separate warnings list rather than weakening the rule, and say
     which.
  4. **CP-05** (low) `pipeline.rs`: `?` on a `spawn_blocking(...).await` join result, and the same
     shape for the rayon batch, turns a worker panic into a whole-run abort, so the failure is not
     recorded in `manifest.failures`. Record a panic as that archive's or file's failure instead,
     using the existing per-item error paths.
- Keep every change minimal and in the style of the surrounding code.

# Scope
Owned files: `crates/converter/src/esm/exporter.rs`, `crates/converter/src/pipeline.rs`,
`crates/converter/src/integration.rs`, tests next to them, and
`docs/handoff/impl-563-p2-audit-converter.md`. No engine changes, no schema bump, no new dependencies.

# Done when
- For each finding that holds on `main`: a test that fails on the old code and passes on yours
  (for CP-05, a test that makes one item panic and shows the run completes with that item in
  `manifest.failures`).
- `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets -- -D warnings` and
  `cargo test -p converter` pass.

# Report
- For each finding: holds on `main` or not (with the `main` file:line), what you changed, and the
  test's name.
