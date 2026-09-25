# research-184 - Is the Riverwood Trader's "upper door" a reference Skyrim starts disabled? Does the converter draw disabled references?

Track: found on portal; the converter part belongs to Phase 2. Read-only: change nothing.

## Goal

impl-182 found a second load door, `00070E69` (model `FarmhouseLDoor01`, linked into cell `000133C9` RiverwoodRiverwoodTrader), 224 units above the Riverwood Trader's front door `0001341F`, and the demo draws and uses it. Skyrim's Riverwood Trader has no upper-storey door that we know of. Find out what that reference really is, and whether the converter publishes references the game starts disabled.

## Questions

1. The `REFR` record `00070E69` in `Skyrim.esm` (and any override in `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`): its record header flags (0x800 Initially Disabled, 0x20 Deleted, 0x10000 ...?), `XESP` enable parent (form id, the "opposite" flag), `XTEL`, position. The same for `0001341F` for comparison. Say which tool you used and show the bytes or the decoded fields.
2. If it is initially disabled or has an enable parent: what enables it (the parent's own flags, a quest if you can tell)?
3. The converter: does it read REFR record flags or `XESP` at all? Find where references are read (`crates/converter/`, the vendored parser under `vendor/` or `crates/` if any) and say, with file:line, whether an initially-disabled reference or a deleted one is skipped, stored with a flag, or published as a normal reference. Check the world DB schema (`crates/shared/`) for any column that could carry it.
4. How many references in `Skyrim.esm` are initially disabled, and how many of those are load doors (`XTEL`) or in Riverwood's cells? A count is enough; list up to 20 in Riverwood with form id, base, and XESP.

## Context

- Skyrim SE: `C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data` (read-only). The converted world DB: `C:\Modding\SkyrimConverted\skyrim_world.db` (SQLite; read only with `python`'s sqlite3 in `mode=ro` URI; never open it read-write).
- Look for existing ESM-reading scripts under `tools/` before writing your own; a new script goes in your scratch folder, not the repository.
- Do not read under `local/` except `local/research/`.

## Report

Answers to 1-4 with evidence (bytes, file:line, query and its output), and one line: what the engine should do (skip initially-disabled references, skip only disabled load doors, honour enable parents, or nothing), and which track the change belongs to.
