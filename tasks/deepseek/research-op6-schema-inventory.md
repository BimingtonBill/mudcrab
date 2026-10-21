# research-op6-schema-inventory: what dev's converter writes beyond main, and what makes a main engine refuse it

Read `tasks/deepseek/context.md` first. Read-only research: write nothing in the repository except your report.

## Two checkouts

- **dev** (our tree): `C:/Users/Brannock/.t3/worktrees/mudcrab/opt-one-data-set` (branch `opt/one-data-set`, = `dev` b578ef33).
- **main** (upstream reference): `C:/Modding/Mudcrab/worktrees/op6-main-ref` (origin/main 27d48513, includes PRs 204, 223, 233).

Cite every claim as `<checkout>:<path>:<line>` with checkout `dev` or `main`.

## Background

dev's converter numbers its schema privately (`PORTAL_SCHEMA_BASE` 100_000 in `crates/converter/src/cache.rs`; `crates/shared/src/lib.rs`: world DB 100_701, converter 102_401) while main is at world DB 9, converter 26. dev is 17 commits behind main. Goal of the task: dev writes main's core tables and files byte for byte at main's numbers, and everything main lacks becomes a named, versioned extension listed in the conversion manifest.

A converter parity run on dummy fixtures (every flag off) found, besides schema_info:
- only in dev: tables `door_links`, `lod_block`, `lod_grid`, `lod_tree_instance`, `lod_tree_type`, `matos`, `space_lighting`, index `idx_lod_tree_block`
- differ: tables `lod_build`, `lod_chunks`, `statics`; files `lod/<ws>/<level>/cell_x_y.glb`, `meshes/generated.glb`; `conversion-manifest.json` (4 values), `lod-manifest.json` (3 values), `integration-report.json` (1 value differs, 7 only in dev)

## Questions (answer each, with citations)

1. **Writers.** For each item above, the dev code that writes it (function, file:line), and for the "differ" items the exact difference from main: extra columns (names, types), changed column order or types, different row content, extra glTF extension (name, which meshes, what fields), extra or changed JSON keys. Note whether main writes the same table with fewer columns or not at all.
2. **Readers.** For each dev-only table, column, glTF extension and JSON key: every engine or tool reader in dev (file:line), and whether the reader already tolerates its absence (probes for the table/column, Option, default) or fails/panics/assumes.
3. **Main's gate.** In main: every place the engine, `world-inspect`, launcher or LOD contract checks a schema number, manifest field, table presence, column set or file set before loading data (`supports_runtime_world_database_schema`, `MIN_RUNTIME_*`, `validate_lod_build_contract`, manifest parsing with `deny_unknown_fields` or strict serde, `PRAGMA table_info` checks, glTF loaders rejecting unknown `extensionsRequired`). For each: would a data set that has main's core plus extra tables, extra columns appended to a main table, extra JSON keys, and an extra glTF extension (in `extensionsUsed` only, not `extensionsRequired`) pass or fail? Quote the line that decides.
4. **Extra statics columns.** If dev adds columns to `statics` (or any main table), would moving them to a side table keyed by the same id change any dev query? List those queries.
5. **Main's 9/26 deltas.** What main changed from world DB 7 to 9 and converter 24 to 26 (tables, columns, files), so we know what dev must merge before its core can match.
6. **Manifest today.** The full shape of `conversion-manifest.json` and `lod-manifest.json` in main (struct, file:line, every field), and any existing place for optional or versioned payloads (for example ADR-0010 payloads, `lod_build` contract fields, PR 235 merged terrain).

## Report

Write `local/agents/runs/research-op6-schema-inventory/report.md`: one section per question, a table per question where it fits (item | dev writer | main equivalent | readers | tolerant of absence?), and a final list "risks for the extension design" (max 10 lines). No recommendations on the design itself beyond that list. Do not commit.
