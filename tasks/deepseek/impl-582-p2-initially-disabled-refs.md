# Goal
Track: Phase 2 (for upstream). References Skyrim starts disabled (record-header flag 0x800,
"Initially Disabled") are no longer drawn or streamed: the converter records each reference's
record flags, and the engine skips the initially-disabled ones.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`fix(converter): ...` /
  `fix(engine): ...`), no `[deepseek]` prefix, no project-internal references in code or docs.
- Measured on Skyrim.esm (a record walker reading the u32 flags at record offset 8):
  703,837 REFR/ACHR records; **4,072 carry 0x800 (Initially Disabled)**, 13 of them load doors
  (with XTEL); 3,161 of the 4,072 have no XESP (enable parent); 0 carry 0x20 (Deleted).
- Today the converter drops the flag: `crates/converter/src/esm/binary.rs` carries `header.flags`
  in `RawRecord`, but the REFR arm of `crates/converter/src/esm/exporter.rs` calls
  `insert_reference`, which takes only the subrecords, and the `references` table has no flags
  column. (Line numbers drift; find them.) XESP is parsed only for actors (`records/achr.rs`).
- Enable parents (XESP; 23,413 refs in Skyrim.esm) are **out of scope**: a ref whose enable parent
  is disabled is disabled too, with an "opposite" bit. Leave a comment where that would go.

# Design
- Converter: add `references.flags INTEGER NOT NULL DEFAULT 0` holding the record-header flags as
  stored; write it from `RawRecord` for every REFR (and ACHR if they share the table). Bump the
  world database schema (`shared::WORLD_DATABASE_SCHEMA_VERSION`, the one number both crates use)
  by one and update every place that states it (grep the current number in `crates/`, `scripts/`,
  `docs/specs/converters/db-schema.md`). Bump the converter schema too if the cache must rebuild
  the database (it is always rebuilt; check `crates/converter/src/cache.rs` and say what you found).
- Engine: where references are loaded for streaming (`crates/engine/src/world/database.rs` and its
  callers), skip references with `flags & 0x800 != 0`: they are not spawned, not used as lights,
  doors or colliders. One named constant with a comment.
- dummy-content: extend its generated plugin with one initially-disabled reference beside an
  enabled one, so the round-trip test proves the skip end to end.

# Scope
Owned files: `crates/converter/src/esm/exporter.rs`, `crates/converter/src/esm/binary.rs` (only if
needed), `crates/shared/src/lib.rs` (the schema constant), `crates/engine/src/world/database.rs`,
the engine file that consumes loaded references if the skip belongs there,
`crates/dummy-content/src/esm.rs`, the converter's fixture tests under `crates/converter/tests/`,
`docs/specs/converters/db-schema.md`, and `docs/handoff/impl-582-p2-initially-disabled-refs.md`.
If you need another file, say so in the report instead of editing it.

# Done when
- Tests: the exporter writes the flags; a database query returns them; the engine's reference
  loading skips an initially-disabled reference and keeps an enabled one; the dummy-content
  round trip shows it end to end; the schema-version tests updated.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo test -p converter -p engine -p shared -p dummy-content --target-dir C:/Modding/shared-check-phase2`

# Report
Diff summary per file, the new schema numbers and every place they changed, test names and
results, and anything left for enable parents.
