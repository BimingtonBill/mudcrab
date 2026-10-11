# Goal
Is upstream PR #89 (eclepso, `fix(converter): validate plugin records and preserve load-order identities`, closes issue #80) correct and safe to merge into `main`? Answer as an independent reviewer for a maintainer who will merge it.

# Context
- Checkout: this folder is the PR's head (6d9ee47c), based on current `main` a896b903. The full diff against main is saved in `review-diffs/pr89.diff` (2005 lines, 12 files, +1485/-116). If `git` works, `git diff a896b903...HEAD` gives the same.
- Files: `crates/converter/src/esm/{binary,cell_cache,exporter,extractors,load_order,mod}.rs`, `crates/converter/src/pipeline.rs`, three new tests in `crates/converter/tests/`, a snapshot, and `docs/specs/converters/db-schema.md`.
- What the PR claims (check each):
  1. Rejects incomplete subrecords/records: header and payload bounds, `XXXX` extended lengths, group boundaries, compressed record headers, declared decompressed size, zlib stream completion (missing checksum trailer rejected).
  2. Walks nested groups with an explicit stack (no recursion), keeps file order, restores parent CELL/WRLD context for sibling groups.
  3. Assigns full and light (ESL) plugin slots independently; checks duplicate filenames and master ordering; remaps LTEX, LAND (VTEX is an array of 4-byte FormIDs; BTXT/ATXT 8-byte layout), CELL/WRLD and GRAS references into load-order space.
  4. `formid_map` exports the lowercase owning plugin and plugin-local ID (instead of `merged`), separately from the winning override's priority; db-schema.md updated to match.
  5. GMST overrides matched by case-insensitive EditorID; first definition keeps identity; aliases kept through deletion/restoration; unknown header-only deletion warns and is skipped; conflicting aliases rejected.
  6. Database and terrain cache built from the same merged record set; asset configuration hash unchanged so meshes/textures/scripts stay reusable (world data regenerates).
- Already checked: CI `tests_pass` is green; CodeRabbit reviewed it. Don't repeat style nits; look for real defects.
- Skyrim facts you may rely on: FormID top byte is the master index into the plugin's own master list (index == master count means the plugin itself); ESL/light plugins use 0xFE with a 12-bit slot in bits 12-23 and a 12-bit local ID; record header is 24 bytes (type, dataSize, flags, formID, vc info, version, unknown); flag 0x00040000 = compressed (data starts with a u32 decompressed size); group header 24 bytes with groupSize including the header; subrecord header 6 bytes (type, u16 size), `XXXX` gives a u32 size for the next subrecord whose own u16 size is then 0.

# Scope
- Read only. Read the diff and the touched files. Run `git` read commands if they work. Do not build or edit.
- Do not fix anything: report.
- Priorities, in order: (a) correctness bugs that would produce wrong world data or reject valid real plugins (Skyrim.esm, Update.esm, Dawnguard/HearthFires/Dragonborn, CC .esl files, user mods); (b) FormID remapping off-by-one or light-slot mistakes; (c) behaviour changes a maintainer must know about (schema/contract change in `formid_map`, does it need a schema version bump or reconversion note? does the engine or launcher read `formid_map`'s owner column anywhere — grep `formid_map`, `merged` across `crates/`); (d) error paths that turn a recoverable oddity in real data into a hard failure; (e) tests that don't test what they claim.

# Done when
- Each of the 6 claims is marked checked (with file:line evidence) or not checked.
- The verdict is accept, fix first, or reject.

# Report
- Findings as `path:line - problem - why it matters`, most serious first, each tagged blocking / should-fix / nit.
- A short "for the maintainer" list: behaviour changes, reconversion needed or not, schema implications.
- What you ran and what it printed; what you only read.

# Earlier review (check these are fixed)
A first review (2026-09-30) asked four things; the newest commit 6d9ee47c says it addresses them. Mark each fixed / not fixed with evidence:
1. A deleted GMST with no EDID (xEdit-cleaned bare header) failed with "has no EditorID" because the check ran before `is_deleted()`. More broadly: does one bad record in an old mod still stop the whole conversion?
2. LAND `VTEX` treated as 4-byte FormIDs: is that cited and right (xEdit `wbDefinitionsTES5.pas`: VTEX is an array of FormIDs to LTEX)?
3. `docs/specs/converters/db-schema.md` still described `plugin_name = 'merged'`.
4. Four new `*_revision` keys changed the asset configuration hash, forcing every mesh and texture to reconvert. Are they gone, and is the hash really unchanged (compare the config-hash inputs on main vs the PR)?
