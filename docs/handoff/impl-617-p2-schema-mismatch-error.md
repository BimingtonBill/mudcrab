# impl-617 — the runtime names the reason it refused a converted asset set

Phase 2 (upstream engine). `crates/engine/src/app.rs`, `validate_runtime_assets`.

## Problem

One `ensure!` covered three situations ("incomplete or stale"), and the integration gate's
`ensure!` joined two more ("schema differs" and "report did not pass"). The player saw the same
sentence whether the converter was old, the engine was old, or the conversion had stopped early,
and none of the messages said what to run.

## Change

- `AssetSetRejection` (five variants) plus one message builder, `asset_set_rejection_message`
  (`crates/engine/src/app.rs:1273`). It names the check, what it found, what it expected, and the
  fix. `validate_runtime_assets` classifies and calls it; `converter_schema_version()` is unchanged.
- The reconvert command is printed with the real assets directory the engine was given and a
  placeholder only for the Skyrim Data folder, which the engine cannot know.

Arguments were copied from the converter's own parser, not invented:
`usage()` — `converter <Skyrim Data> [output directory] [--cpu-jobs N] [--io-jobs N] [--fail-fast]
[--invalidate-cache] [--no-verify-cache] [--resume-staging DIR] [--report-json FILE]`
(`crates/converter/src/main.rs:225`; the positional parse is `:194`–`:201`). The engine's own
option is `--allow-incomplete-assets` (`crates/engine/src/config.rs:163`). No `OPENSKYRIM_*`
environment variable configures either tool: `OPENSKYRIM_SKYRIM_DATA` and the `*_FIXTURE` names are
read only by opt-in `#[ignore]`d tests (e.g. `crates/converter/src/archive/bsa.rs:382`).

## Messages (example assets path `C:\Modding\SkyrimConverted`)

Old converter (13 < 14):

```
converted assets are stale: C:\Modding\SkyrimConverted\conversion-manifest.json was written by converter schema 13, but this engine requires converter schema 14; reconvert with `cargo run --release -p converter -- <Skyrim Data folder> "C:\Modding\SkyrimConverted"` (the converter reuses unchanged work, so a reconvert is usually quick)
```

New converter (15 > 14):

```
converted assets are newer than this engine: C:\Modding\SkyrimConverted\conversion-manifest.json was written by converter schema 15, but this engine understands only converter schema 14; update the engine and rebuild it (`cargo build --release -p engine`), or reconvert with a converter at schema 14
```

`complete: false` at the current schema:

```
the asset conversion did not finish: C:\Modding\SkyrimConverted\conversion-manifest.json reports complete=false at converter schema 14, so the converter stopped early or skipped inputs; rerun `cargo run --release -p converter -- <Skyrim Data folder> "C:\Modding\SkyrimConverted"` (it reuses unchanged work), or start the engine with --allow-incomplete-assets to use what is there
```

World database schema in the report (2 vs 3):

```
the converted assets use a different world database schema: C:\Modding\SkyrimConverted\integration-report.json reports world database schema 2, but this engine requires 3; reconvert with a converter built from the same revision as this engine: `cargo run --release -p converter -- <Skyrim Data folder> "C:\Modding\SkyrimConverted"`
```

Report ran and failed:

```
the asset integration report did not pass: C:\Modding\SkyrimConverted\integration-report.json reports passed=false; read that report for the failing check, then reconvert with `cargo run --release -p converter -- <Skyrim Data folder> "C:\Modding\SkyrimConverted"`
```

## Tests (`crates/engine/src/app.rs`, `mod tests`)

Helper-level, one per message, no filesystem:

- `older_converter_schema_names_both_versions_and_the_reconvert_command`
- `newer_converter_schema_names_both_versions_and_the_engine_rebuild`
- `incomplete_conversion_names_the_rerun_and_the_incomplete_assets_option`
- `world_database_schema_mismatch_names_both_versions_and_the_reconvert_command`
- `failed_integration_report_points_at_the_report_file`

End-to-end through `validate_runtime_assets` with a temporary asset set, and the passing case:

- `stale_incomplete_and_failed_runtime_assets_report_distinct_reasons` (replaces
  `rejects_stale_or_incomplete_runtime_assets`; also asserts the four errors differ pairwise)
- `accepts_current_complete_runtime_assets` (unchanged)

Gates: `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings`,
`cargo nextest run -p engine` (57 passed).

## Open points

- The world database schema mismatch has one message for both directions; the fix offered
  (reconvert with a converter at the same revision as the engine) is right when the converter is
  behind, and a no-op when the engine is behind.
- `converter_schema_version()` (`crates/engine/src/app.rs:1318`) still hand-mirrors
  `converter::cache::CONVERTER_SCHEMA_VERSION` (`crates/converter/src/cache.rs:11`) with no test
  comparing them; both read 14 today. Out of scope here.
