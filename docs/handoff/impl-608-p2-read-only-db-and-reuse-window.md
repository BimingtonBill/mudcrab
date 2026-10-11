# impl-608-p2-read-only-db-and-reuse-window

Branch: `impl-608-p2-read-only-db-and-reuse-window` (cut from `main`, tracked for upstream).
Part (1) — read-only engine database opens — was **dropped by the lead**: `phase2/audit-engine`
`9e40e93` already makes every engine-side open read-only. Only part (2) was done here.

## Part (2): one converter reuse window

- `crates/converter/src/cache.rs` — new `pub fn reusable_previous_schema(schema_version: u32) -> bool`
  next to `CONVERTER_SCHEMA_VERSION` (14). The condition
  `matches!(schema_version, 12 | 13) && CONVERTER_SCHEMA_VERSION == 14` now lives only there; the
  doc comment carries the old "schemas 13/14 change only NIF material publication and LAND
  normalization" rationale.
- `crates/converter/src/cache.rs` `ConversionManifest::load` — calls the function instead of the
  inline `matches!`; behaviour unchanged (schema 12/13 manifests keep non-GLB entries, are marked
  incomplete, and GLBs are dropped; any other version is discarded wholesale).
- `crates/converter/src/pipeline.rs` `AssetPipeline::run_async` — the previous manifest's
  configuration hash is still compared against `configuration_hash_for_schema(..., previous_schema)`
  only when `reusable_previous_schema(previous_schema)`.
- Tests: `cache::tests::reuse_window_is_only_the_recent_migrations` iterates schema 0..=15 and pins
  the reusable set to exactly `[12, 13]`; `cache::tests::recent_schema_migrations_reuse_only_unchanged_asset_kinds`
  (pre-existing) still covers the manifest behaviour for 12 and 13.

`grep -rn "12 | 13"` over the repo now matches only `crates/converter/src/cache.rs:26` (the function).

## Checks run (this worktree)

| command | result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p converter --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings` | clean |
| `cargo nextest run -p converter --target-dir C:/Modding/shared-check-phase2` | 130 passed, 8 skipped |
| widened the window to `11 \| 12 \| 13` and re-ran the new test | fails with `left: [11, 12, 13]`, `right: [12, 13]` (pin works; then reverted) |

Notes: `-p engine` was not run because part (1) was dropped and no engine file changed. In this
checkout (cut from `main`) `Connection::open(` still appears at `crates/engine/src/app.rs:225`,
`crates/engine/src/app.rs:1308`, `crates/engine/src/bin/world-inspect.rs:239` and in the three
fixture builders at `crates/engine/src/world/database.rs:382,401,446`; the fix for the first three is
`phase2/audit-engine` `9e40e93`, not this branch.

This note is left untracked: `docs/handoff/` does not exist on upstream `main`, and a Phase 2
hand-off note does not belong in an upstream commit.
