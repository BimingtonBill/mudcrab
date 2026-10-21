# analysis-parity-engine-field-use: which converted columns does the Phase 2 engine read?

## Your assignment

Write an inventory of every column the converter writes to its output (SQLite tables and the rkyv cell cache) and say whether the engine reads it, with `file:line` evidence for every row. Read code only. Do not run the converter or the engine, do not edit any code.

## Context (read this first)

Mudcrab is a Rust/Bevy re-implementation of Skyrim. The converter (`crates/converter/`) reads Bethesda plugins and writes `skyrim_world.db` (SQLite) and `cell_cache.rkyv`. The engine (`crates/engine/`, with shared types in `crates/shared/`) loads them. We are building a parity matrix: for each record field, what vanilla Skyrim contains, whether the converter reads it, and whether the engine uses it. A previous worker produced the converter half: `docs/research/data/parity-converter-fields.tsv` (read it; its `stored_in` column names output tables and columns). Your job is the engine half. Another note, `docs/research/data/parity-converter-fields.md`, explains that file.

Where to look:
- Output schema: `CREATE TABLE` statements and `INSERT` statements in `crates/converter/src/esm/exporter.rs`, `crates/converter/src/esm/cell_cache.rs`, `crates/converter/src/esm/lighting.rs`, `crates/converter/src/esm/rooms.rs`, and anything else under `crates/converter/src/` that writes a table or an rkyv structure (search for `CREATE TABLE`, `INSERT`, `rkyv::`, `Archive`). The schema version constant says which layout is current.
- Engine readers: `crates/engine/src/` (for example `world/database.rs`, `streaming.rs`, `snow.rs`, lighting, water and terrain code) and `crates/shared/src/`. Search for table names, column names and the rkyv type names. SQL strings in the engine name columns: `SELECT ... FROM <table>`.

## What to produce

1. `docs/research/data/parity-engine-use.tsv`, tab separated, header:
   `table<TAB>column<TAB>written_at<TAB>engine_use<TAB>engine_evidence<TAB>notes`
   - One row per column of every table the converter writes, and one row per field of each rkyv structure it writes (use `rkyv:<TypeName>` in `table`).
   - `written_at`: `path:line` of the `CREATE TABLE` or the struct field.
   - `engine_use`: `consumed` (engine reads it and the value changes what is drawn, spawned, simulated or loaded), `read_unused` (selected or deserialized but nothing uses the value), `not_read` (no engine code reads it), or `unknown` (say why in notes).
   - `engine_evidence`: `path:line` of the SQL/struct access, and in notes the line where the value is used when it differs. Every row needs evidence; for `not_read` cite the file you searched (for example "searched crates/engine/src and crates/shared/src for the column name").
   - `notes`: one sentence at most; for `consumed` say what it affects.
2. `docs/research/data/parity-engine-use.md`, at most 400 words: counts per `engine_use`, the tables with the most `not_read` columns, files read, and anything UNVERIFIED.

## Rules (all binding)

- Evidence only. Never infer from a column's name or from memory of how Skyrim works. If a column is read through a helper or a `SELECT *`, follow it to the field use; if you cannot, write `unknown` and say why.
- Work from the current checkout (`git rev-parse HEAD`) and put the commit and the schema version in the note.
- A reviewer will check twenty random rows against the code. Wrong rows cost more than missing ones.
- Do not edit anything under `crates/`. Do not run git.

## Done when

Every table and rkyv structure the converter writes has rows, the note lists unknowns, and you reported the commit and the files read.
