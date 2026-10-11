# Goal
Track: Phase 1. Find which converter code path leaves some `vfs/` files as separate copies of a
cache blob with identical bytes, instead of hard links, after the hard-link change. Read-only
analysis; name the path and propose the smallest fix with a test.

# Context
- Read `tasks/deepseek/context.md` first.
- Code (read-only) at `C:/Modding/wkj-phase1`, branch `phase1/vfs-hard-links` (head 3fcf9f2):
  `crates/converter/src/archive/mod.rs` (`extract_cached`, `persist_cache_blobs`,
  `restore_cached_files`, `copy_if_missing`, `copy_file`, `atomic_write`),
  `crates/converter/src/cache.rs` (`link_or_copy`), `crates/converter/src/pipeline.rs`
  (the extraction loop in load order, `overlay_loose_assets`).
- Data (read-only; nothing is converting now, but never write, rename or delete anything there):
  `D:/Mudcrab/p1-timing/SkyrimConverted/` is the output of TWO reconversions in a row
  (first with the hard-link converter, then with a later build that has the same extraction code).
  Its `vfs/` has 8,054 files (1.75 GB) with link count 1 whose bytes equal an existing blob in
  `.ingestion-cache/sha256/<xx>/<hash>` (also link count 1), e.g.
  `vfs/textures/actors/character/facemods/skyrim.esm/00000a68_0.dds` (not a loose file in the game
  Data folder). 155,008 other vfs files have link count 2 as expected. The manifest
  `conversion-manifest.json` lists per archive (`archives`) its files and hashes; the game archives
  are read-only in `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data`.
- Tools: `tools/research/phase1_size/nlink_hist.py <dir>` (link-count histogram). Write any new
  scripts under `tools/research/phase1_size/` and run them as
  `python tools/research/phase1_size/<script>.py`; never `python -c`.

# Questions
1. What do the 8,054 files have in common: which archive(s) list their path in the manifest (one
   archive or several; a path listed by two archives means a later archive overrode it), file type,
   whether their blob is shared by several paths, whether their archive was a cache hit?
2. Trace the code for that case and name the exact path (`path:line`) that writes a copy or leaves
   the vfs name and the blob as two files. Consider: a path extracted by an earlier archive and
   overridden by a later one with identical or different bytes; `copy_if_missing` skipping the
   blob→blob copy when the new blob already exists; the loose overlay; the NTFS limit of 1023 links
   per file; `link_or_copy`'s copy fallback.
3. The smallest fix and a unit test that fails before it (describe it; do not edit code).

# Scope and rules
Read-only. Write only `local/research/phase1-unlinked-vfs.md` (under 700 words) and scripts under
`tools/research/phase1_size/`. No cargo builds or runs of the converter. Do not commit. Then a
summary under 250 words as your final message.
