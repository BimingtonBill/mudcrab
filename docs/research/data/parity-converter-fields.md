# Parity: which record fields the Phase 1 converter reads

Read at commit `aaf060266c93be11026915f9bdf78086d6e63400` (branch `plus`); HEAD
has moved on since, and no `crates/converter/` file changed under this read.
Companion file: `parity-converter-fields.tsv`, 174 rows, each with a `file:line`
citation.

## Files read

`esm/exporter.rs`, `esm/mod.rs`, `esm/binary.rs`, `esm/extractors.rs`,
`esm/cell_cache.rs`, `esm/lighting.rs`, `esm/rooms.rs`,
`esm/directional_material.rs`, `esm/lodsettings.rs`, `esm/load_order.rs`,
`esm/mmap_reader.rs`, `esm/types.rs`, `esm/records/*.rs`,
`esm/records/record_type/*.rs`, `integration.rs`, `main.rs` and every file in
`src/bin/`. `lodsettings.rs` reads only the 16-byte `.lod` sidecar, not record
data; no bin reads a plugin.

## Counts

174 rows: `full` 99, `partial` 64, `raw` 8, `skipped` 2, `presence` 1. Forty-
eight four-letter record types, plus the `GRUP` header and the `XXXX` length
marker, and 89 distinct subrecord codes. The exporter dispatches on the type
string, so the `*` rows mean "every record type".

## Read

The export paths (exporter, cell_cache, lighting, rooms, extractors, binary)
reach 40 record types (plus the `*`, `GRUP` and `XXXX` rows). Every record of
every type is also stored verbatim, so a type with no dedicated arm still lands
in `records.data`. `LAND` is the one type read twice: its payload goes raw into
the `land` table and is decoded separately into the terrain cell cache. `GRAS`
`MODS` and `LAND` `BTXT`/`ATXT`/`VTEX` are each read on two paths too, once to
rewrite FormIDs and once to export.

## Touched, barely

Eight record types are named nowhere but the load-order remap: `PHZD`, `PARW`,
`PBAR`, `PBEA`, `PCON`, `PFLA`, which reach it only through the
placed-reference subrecord list (its FormIDs are rewritten, nothing is
decoded), and `AACT`/`CLFM`, which the remap matches only to exclude. All six
placed-type codes are absent from `exporter.rs`'s reference arm, so a reference
of those types gets no `references` row at all.

## Mentioned, not read

`DALC` appears in `lighting.rs`'s module comment (which says `space_lighting`
has no column for the six tints) and in test fixtures; no code matches it.
`records/{tes4,aact,clfm,achr,tree}.rs` implement `EsmRecord::parse` and
nothing calls them, which is the only thing that reaches
`SubrecordView::{get_obnd,get_vmad,get_model}` either; only
`get_alternate_textures` has a caller outside that dead branch (an integration
test). Those rows are in the TSV, flagged in `notes`.

## UNVERIFIED

- `how_read` for the remap rows. They read a FormID out of the bytes and write
  it back; no field is decoded for output. I called them `partial`.
- Whether the unused extractors are dead code or a path not yet wired up.
- Whether the bytes the converter *does not* read carry the values the fields
  are named for (`LIGH` `DATA` past offset 20, `WATR` `DNAM` outside 20..52,
  `XCLL`/`LGTM` offsets 0x28..0x48, `NAM0`'s three non-day colours,
  `FNAM`'s day/night maxima, `TREE` `CNAM`'s eight "unknown" floats). This is a
  code inventory; the vanilla dump is the other half.
- Memory of the format was not used. Where the code and the comment disagree,
  the code won.

## Join caution

`record_sig` and `subrecord_sig` carry `X|Y` when one arm of the code lists
alternatives; split the field before joining to a per-signature dump.
