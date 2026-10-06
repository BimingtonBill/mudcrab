# Derived per-cell data: keying on input content (#185, converter input)

Status: design note, 2026-10-06. No code. The contract decision belongs to #185.

## Today

- **Assets are content-addressed.** Archives and files are hashed (sha256) into the ingestion
  cache, and each converted output records `source_hash` and `output_hash`
  (`crates/converter/src/cache.rs`). A changed mesh or texture is reconverted; an unchanged one is
  reused.
- **The world database is rebuilt whole** from the merged, winning records on every conversion.
  It keeps a checksum per plugin (`plugins.checksum`), but no list of what each cell was built from.
- **Per-cell derived data has no input key.** `cell_cache.rkyv` (terrain) is checked only against
  `CELL_CACHE_VERSION` (`crates/engine/src/world/cache.rs`). LOD has a content hash per chunk but
  one global `build_identity` (`pipeline.rs`, `build_identity`), so any plugin change rebuilds it all.
- **IDs depend on load order.** FormIDs are remapped into load-order numbering (#136), so a load
  order change renumbers every mod record even when nothing in it changed.

## Proposal

Give each cell an **input key**: a hash of everything its derived data is built from. Rebuild a
cell when its key changes, reuse it otherwise.

```
cell_key = H( derived_format_version, converter_schema, content_configuration,
              sorted[ (stable_id, record_hash) for the CELL, every placed reference in it,
                      and every base or shared record they point to ],
              sorted[ asset output_hash for every model, material and texture they use ] )
```

- **`record_hash`** is a hash of the winning record's bytes, with every FormID inside rewritten to
  its stable ID. Computed once per record during the merge (Skyrim.esm is about 250 MB; hashing it
  takes about a second).
- **`stable_id`** = (owning plugin's file name, the record's ID inside that plugin), the identity
  Skyrim itself uses. Inside the hash this names *which object* is meant; it is never used to
  decide whether something is stale. Staleness comes only from the hashed content, so a plugin
  edited in place but keeping its name still rebuilds the cells it touches.
- **Not in the key:** load-order slots, load-order FormIDs, absolute paths, file times, the MO2
  instance or profile identity, the output or staging paths.
- **Derived data stores stable IDs**, translated to load-order FormIDs when loaded. Otherwise a
  load order change would invalidate every cell even when no winner changed.
- **One-step replace:** new data is written beside the old and swapped at publish, as the
  converter already does for the whole output.

What this gives: reordering plugins without changing any winner rebuilds nothing; a mod that edits
one cell rebuilds that cell and the cells sharing its changed base records; a converter update
rebuilds everything, as it should.

## How it sits with the other open work

- **Chef's record work (#156-#160).** #159 and #160 keep every plugin's source bytes, immutable,
  and separate occurrence, owning and winning identity. That is exactly what `record_hash` and
  `stable_id` need: once it lands, the hash should come from the winning occurrence's retained
  bytes rather than from a second pass in the merge. The one thing to agree with Chef: his index
  exposes a per-record content hash and the stable ID of each winner. Until then the merge
  computes both, with the same definition, so the switch changes no keys.
- **MonoPT's MO2 profiles (#154).** A profile changes which files are inputs (load order, loose
  overrides, archives), and that is already covered by hashing content. #154 currently folds the
  MO2 instance, profile, paths and its config files' hashes into `configuration_hash`, so switching
  profile (or MO2 rewriting its ini) discards all reuse. For #185, `configuration_hash` should hold
  only settings that change output bytes; profile identity stays as provenance in the report, not
  in the key. A reviewer on #154 also found loose mod files missing from `source_hash`, which this
  scheme needs fixed, since loose files are inputs. Later, per-cell keys would let two profiles
  share every cell they agree on.
- **Saves (#90).** Saves need the same stable ID to survive load-order changes. One definition
  should serve both.

## What to measure first (#184)

How long the per-cell work the engine repeats today actually takes, and how long the key itself
costs (record hashing during the merge, joining a cell's inputs). If the key costs more than the
work it saves on small cells, the unit could be a group of cells, like LOD's chunks, rather than
one cell.

## Decisions for #185

1. The stable ID format (shared with #90).
2. Where `record_hash` comes from: the merge now, Chef's index later, same definition.
3. What counts as "content configuration" (the texture settings that change bytes do; MO2 and path
   identity don't).
4. The unit: one cell, or a group of cells.
