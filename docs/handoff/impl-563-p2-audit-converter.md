# impl-563 (Phase 2): converter findings from the 2026-09-24 standing audit

Task: re-confirm the converter findings filed against the fork's `portal` branch
(converter schema 21, DB schema 5) on upstream `main` (converter schema 19, DB
schema 3) and fix the ones that hold. Base commit: `2105bfd`.
Files changed: `crates/converter/src/esm/exporter.rs`,
`crates/converter/src/pipeline.rs`, `crates/converter/src/integration.rs`
(plus tests in each) and this note. No engine change, no schema bump, no new
dependency.

## Findings

### SC-01 — database schema version stamped as a literal — HOLDS on `main`

`crates/converter/src/esm/exporter.rs:18` (before the fix) was
`INSERT INTO schema_info(version) SELECT 3 ...` inside the `execute_batch`
schema DDL, while the constant is `shared::WORLD_DATABASE_SCHEMA_VERSION`
(`crates/shared/src/lib.rs:7`, `= 3`), and the same file validates against the
constant at `exporter.rs:331`. Two copies of the contract; a bump in one is
silently ignored by the other.

Fix: the insert left the batch and became a parameterised statement
(`params![shared::WORLD_DATABASE_SCHEMA_VERSION]`), so the stamp can only come
from the shared constant.

Test: `esm::exporter::tests::stamps_the_database_with_the_shared_schema_version`
— asserts `schema_info` holds exactly one row equal to
`shared::WORLD_DATABASE_SCHEMA_VERSION` after two `create_tables` calls (the
second call pins the `WHERE NOT EXISTS` guard the parameterised statement now
carries). The values agree today (`3 == 3`), so no behavioural test can fail on
the pre-fix code; this test fails the moment the constant is bumped without the
stamp following, which is the failure mode the finding describes.

### CP-03 — archive failure keyed on the archive's full path — HOLDS on `main`

`crates/converter/src/pipeline.rs:227-230` (before the fix) inserted
`archive.to_string_lossy().replace('\\', "/")` — the absolute path under the
documented invocation — into `manifest.failures`, while the same archive is
keyed relative to the data folder 40 lines above (`strip_prefix`, `:183-188`)
for the ingestion cache. `conversion-manifest.json` is published.

Fix: the failure key and message now use that same `archive_key`
(`pipeline.rs:229-243`). The message is `"{archive_key}: {error:#}"`.

Test: `pipeline::tests::keys_archive_failures_relative_to_the_data_directory`
(a `.bsa` of garbage in a temp data dir) — asserts `manifest.failures` has
exactly one entry, keyed `broken.bsa`. Fails on the pre-fix code (key was
`C:/Users/.../broken.bsa`).

Remainder (not in scope): the *detail* half of that message still embeds
whatever the error chain built, and `ArchiveExtractor::extract` formats
`archive_path.display()` (`crates/converter/src/archive/mod.rs:170`), so
absolute paths can still appear in message text. The key — the manifest's
identity for the archive — is now relative. File failures behave the same way
(`record_skip` prepends the relative path, inner converter errors may name
absolute sources).

### CP-04 — `report.passed` never consults `report.issues` — HOLDS on `main`

`crates/converter/src/integration.rs:195-199` (before the fix) computed the
verdict from counts only: `schema_version && missing_model_count == 0 &&
invalid_model_count == 0 && missing_texture_count == 0 && cache_cells ==
terrain_cells`. A missing `cell_cache.rkyv` records an issue (`:198`) but no
count, so with no `land` rows the verdict was `0 == 0` → passed. Models with no
readable bounds recorded an issue and no count, so they passed silently.

Fix: `passed` is now `schema_version == WORLD_DATABASE_SCHEMA_VERSION &&
issues.is_empty()` (`integration.rs:203-207`). Per the brief, the one issue that
must stay non-fatal got a separate list rather than a weaker rule:

- `IntegrationReport` gained `warnings: Vec<String>` (same 100-entry cap,
  `warning()` helper next to `issue()`).
- "model has no static bounds ..." moved from `issues` to `warnings`
  (`integration.rs:112-121`). Evidence that this is a real, expected condition
  and not a defect: the retail-tree report at
  `C:\Modding\SkyrimConverted\integration-report.json` has
  `unbounded_model_count: 427`, `issues` filled to the 100 cap with
  "model has no static bounds", and `passed: true`. Making those fatal would
  flip a full conversion to `passed: false` and make the engine refuse the
  assets (`engine/src/app.rs:1208`). Every other issue (missing converted
  model/texture, terrain-cache mismatch, missing cell cache) was already fatal
  through a count except the cell cache, which is now fatal by the rule.

Tests: `integration::tests::fails_when_the_cell_cache_is_missing_even_with_no_terrain`
(fails on the pre-fix code: it passed while `cache_cells == terrain_cells == 0`)
and `integration::tests::reports_models_without_readable_bounds_as_warnings`
(pins the split: `passed`, `warnings` non-empty, `issues` empty — the last
assertion fails on the pre-fix code).

### CP-05 — worker panics abort the run — HOLDS on `main`

`pipeline.rs:202-203` (before the fix) `?`-ed the `spawn_blocking(...).await`
join result, and `pipeline.rs:728-730` the rayon batch join; a panic in either
worker returned `Err` from `run_into` and the failure never reached
`manifest.failures`.

Fix:
- Archive: the join error becomes that archive's error and flows through the
  existing per-archive arm — recorded in `manifest.failures` (under
  `archive_key`) and `report.warnings` when `fail_fast` is off, and a run abort
  when `fail_fast` is on (`pipeline.rs:192-210`).
- Rayon batch: the per-item body runs under
  `std::panic::catch_unwind(AssertUnwindSafe(...))` (`pipeline.rs:507`), so a
  panicking item is sent through the normal outcome channel as an error for
  *that* file and lands in `manifest.failures` via `record_skip` (the
  `if let Err(panic) = item` arm, `pipeline.rs:633-656`); the message carries
  the panic payload, e.g.
  `scripts/panic-on-convert.pex: asset conversion worker panicked:
  test-injected asset worker panic`. A batch-level join error that is not an
  item panic (pool/machinery) is recorded against `"<kind>/*"` and only aborts
  the run under `fail_fast`.

Test hook: under `#[cfg(test)]` only, a file whose name starts with
`panic-on-convert.` panics at the top of the item body
(`pipeline.rs:509-515`). It is compiled out of every non-test build; without a
panicking input there is no way to exercise the recording path.

Test: `pipeline::tests::records_a_panicking_asset_as_a_failure_instead_of_aborting`
— two `.pex` scripts, one named `panic-on-convert.pex`; asserts the run returns
`Ok`, `converted == 1`, `skipped == 1`, the good script converted, and
`manifest.failures["scripts/panic-on-convert.pex"]` is present and says
"panicked" (failures are keyed by canonical *source* key). Fails on the pre-fix
code (the run returns `Err`).

The archive-side join change has no test: no input makes extraction panic, and
the test hook above is only wired into the item body.

## Verification

- `cargo fmt --all -- --check` — clean.
- `cargo clippy -p converter --all-targets -- -D warnings` — clean.
- `cargo test -p converter` — 106 passed, 0 failed, 7 ignored.
- `cargo check --workspace` — clean.
- Old-code check: with the three fixes temporarily reverted (literal-free
  passed-rule, absolute archive key, panic raised outside `catch_unwind`), the
  suite reported exactly `keys_archive_failures_relative_to_the_data_directory`,
  `records_a_panicking_asset_as_a_failure_instead_of_aborting` and
  `fails_when_the_cell_cache_is_missing_even_with_no_terrain` FAILED
  (`103 passed; 3 failed`), then passed again after restoring. SC-01 has no such
  check, for the reason given above.

## Not done / unsure

- The engine, launcher and `asset-closure` binaries read
  `integration-report.json` with their own structs or untyped JSON, so the new
  `warnings` field is additive for them; the panic-in-the-item-body closure is
  a larger re-indent than the rest of the change.
- Whether the fork's `portal` converter records the same 427-unbounded-model
  population was not re-measured on `main` (that would need a full conversion);
  the argument that bounds are non-fatal rests on the portal report above.
