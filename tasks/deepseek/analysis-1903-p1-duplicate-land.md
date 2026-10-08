# Goal
Find every exterior cell that has more than one LAND record in the vanilla Skyrim SE load order, and say which LAND the game uses, so the converter can pick one deterministically. Phase 1 track (#186, and a pre-existing cell-cache bug).

# Context
- A merged-terrain conversion failed: "cell 000018FD of worldspace 0000003C is written twice (grid -43,36 and grid -43,36)". Two distinct LAND records (different FormIDs) have the same parent CELL 0x000018FD in Tamriel.
- Today's cell cache has the same problem silently: `crates/converter/src/esm/cell_cache.rs:9-84` (`write_cell_cache`) keys LAND by `record.cell_form_id` in a HashMap and inserts every LAND, so with two LANDs for a cell the last one in HashMap iteration order wins (nondeterministic). Read that code in `C:/Modding/Mudcrab/worktrees/p1-terrain-merge`. The merged-terrain stage is `crates/converter/src/terrain_pipeline.rs`.
- The merge (`crates/converter/src/esm/mod.rs`) keys records by load-order FormID, so two different LAND FormIDs both survive; `RawRecord.load_order` says which plugin each came from.
- xEdit 4.1.5f is installed at `C:/Tools/SSEEdit`. Its dumper prints decoded records without a window: `C:/Tools/SSEEdit/SSEDump64.exe "-d:C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data\" <options> Skyrim.esm`. Write any dump output to `D:/Mudcrab/scratch/ssedit-dumps` (never into the game folder). Read only.
- Clean room: read and cite; never copy xEdit code.

# Do
1. Find cell 0x000018FD's LAND records: their FormIDs, which plugin defines each (Skyrim.esm, Update.esm, Dawnguard, HearthFires, Dragonborn), and how they differ (VHGT, layers). Use the dumper on the plugins involved, or parse the plugins with a short script if the dumper can't filter to one cell (state which).
2. Count every exterior cell in the vanilla load order with more than one LAND, by worldspace, with the plugins involved. A short Python script over the plugin files is fine if you write it under `D:/Mudcrab/scratch/ssedit-dumps/` (records are in GRUPs; LAND sits in the cell's temporary children group, type 9; compressed records use zlib).
3. What the game does with two LANDs in one cell: search xEdit's definitions and its check messages (TES5Edit source at tag xedit-4.1.5f, Core/wbDefinitionsTES5.pas and the error checks) and UESP for "multiple LAND" or "duplicate LAND"; cite what you find. If nothing documents it, say so.
4. Recommend a deterministic rule (for example: the LAND from the plugin latest in the load order; ties by FormID) with the evidence for it, and say whether the losing LAND should be reported as a warning.

# Report (under 400 words)
The cell 0x18FD finding, the full count table, the evidence for what the game does (cited), and the recommended rule. Files you wrote, with paths.
