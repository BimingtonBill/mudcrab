# analysis-parity-converter-field-inventory: which Skyrim record fields does the Phase 1 converter read?

## Your assignment

Write an inventory of every Skyrim plugin record field the converter reads, as a TSV with `file:line` evidence for every row, plus a short summary note. Read only the converter code. Do not run it, do not edit any code.

## Context (read this first)

Mudcrab is a Rust/Bevy re-implementation of Skyrim. Phase 1 is the converter: `crates/converter/src/`. It reads Bethesda plugins (`.esm`/`.esp`) with its own parsers and writes converted data for the engine. We are building a parity matrix: for each record type and field, what vanilla Skyrim contains versus what the converter reads. Your job is the converter column only. A later step joins it to a dump of vanilla data (field names there differ from the converter's four-letter codes; that mapping is NOT your job).

Files to read, all under `crates/converter/src/`: `esm/exporter.rs` (largest, about 3800 lines), `esm/mod.rs`, `esm/cell_cache.rs`, `esm/lighting.rs`, `esm/rooms.rs`, `esm/extractors.rs`, `esm/binary.rs`, `esm/lodsettings.rs`, `esm/directional_material.rs`, `esm/types.rs`, `esm/records/*.rs`, `esm/records/record_type/*.rs`. Also find any other converter file that reads record or subrecord data: search the whole `crates/converter/` tree (including `src/bin/`) for four-letter byte-string signatures such as `b"DNAM"`, `*b"STAT"` or `"MODL"`. Report which files you read.

## What to produce

1. `docs/research/data/parity-converter-fields.tsv` with this header, tab separated:
   `record_sig<TAB>subrecord_sig<TAB>how_read<TAB>evidence<TAB>stored_in<TAB>notes`
   - `record_sig`: the four-letter record type (STAT, DOOR, CELL, REFR, WTHR, LTEX...). Use `*` only if the code treats any record type the same.
   - `subrecord_sig`: the four-letter subrecord (DNAM, MODL, XCLL...). Use `(header)` for the record header or flags, `(whole)` if the whole record body is passed on unparsed.
   - `how_read`: one of `full` (every byte of the subrecord is decoded), `partial` (only some fields; name which in notes), `presence` (only checks that it exists), `skipped` (the code matches the subrecord and deliberately ignores it), `raw` (kept as bytes, not decoded).
   - `evidence`: `path:line` of the code that matches the signature or reads the bytes. Every row needs one.
   - `stored_in`: the output struct, field or file the value ends up in, or `-` if it is dropped.
   - `notes`: byte offsets read, a one-sentence description, anything surprising. For `partial`, list which fields of the subrecord are read and which are not.
2. `docs/research/data/parity-converter-fields.md`: at most 500 words. How many record types and subrecords you found, which record types the converter reads at all, which it mentions but skips, files read, and a list of anything you could not determine (UNVERIFIED).

## Rules (all binding)

- Evidence only. Every TSV row cites a line you actually read. If you cannot tell what the code does with a subrecord, write `unknown` in `how_read` and say why in notes. Never fill a gap from memory of Skyrim file formats or from guessing.
- Record a subrecord once per code path that reads it; if two places read the same subrecord differently, write two rows.
- A subrecord that appears in a comment or a test only is not "read": leave it out, or mark `skipped` with the evidence if the code clearly drops it.
- No code reproduction beyond one-line signature names. This is our own code, but keep the note readable.
- Work from the current checkout (`git rev-parse HEAD`) and put that commit in the note.
- A reviewer will check twenty random rows against the code. Wrong rows cost more than missing ones.
- Do not edit anything under `crates/`.

## Done when

The TSV has a row for every signature the converter matches, the note lists the unknowns, and you reported the commit and the list of files read.
