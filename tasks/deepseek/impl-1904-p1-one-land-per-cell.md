# Goal
Every exterior cell gets exactly one LAND, chosen the way the game chooses: by load order. Used by both the cell cache and the merged-terrain stage, and checked before LOD so a bad load order fails in minutes. Phase 1 track (#186, and a pre-existing main bug).

# Context
- Evidence and rule: `local/research/terrain-merge/duplicate-land-2026-10-08.md` (read it first). In vanilla, 2 Tamriel cells (0x18DF, 0x18FD) have a second, different LAND added by Dawnguard.esm; the seam test shows the game draws the later plugin's LAND.
- Today: `crates/converter/src/esm/cell_cache.rs` `write_cell_cache` (about lines 9-84) inserts every LAND into a HashMap keyed by `record.cell_form_id.unwrap_or(record.form_id)`, so the last one in HashMap iteration order wins (nondeterministic). The merged-terrain stage (`crates/converter/src/terrain_pipeline.rs`, `build_merged_terrain`) builds one mesh per LAND, and `terrain_chunks.rs` `write_terrain_chunks` then fails "cell ... is written twice" after LOD has already run (55 min on full data).
- `RawRecord` (`crates/converter/src/esm/records/mod.rs`) has `load_order: u32` = the load-order slot of the plugin whose version of the record won, and `is_deleted()`.
- Worktree: launcher-made from `phase1/terrain-merge` at 5ce5dcc0.

# Rule (implement exactly)
For each cell id: take its LAND records that are not deleted. If there is one, use it. If there are several, use the one with the highest `load_order`; report every other one as a warning naming the cell, the kept LAND FormID and plugin slot, and each dropped LAND FormID and slot. If the highest `load_order` is shared by two or more of them (two different LANDs for one cell from the same plugin), that is an error naming the cell and the FormIDs: the game's order for that case is not established, and choosing silently would be wrong output. No FormID-based tie-break.

# What to build (3 items)
1. One function, e.g. `pub(crate) fn land_per_cell(records: &HashMap<u32, RawRecord>) -> Result<(BTreeMap<u32, &RawRecord>, Vec<String>)>` (cell id -> chosen LAND, warnings), in `crates/converter/src/esm/cell_cache.rs` (or a new `crates/converter/src/esm/land_choice.rs` declared in `esm/mod.rs`; say which). `write_cell_cache` uses it, and its warnings reach the conversion report the way its existing "dropped texture" warnings do (follow that path; say where they surface).
2. The merged-terrain stage uses the same function, so it builds one mesh per cell; and the check runs early: call it right after the database stage when `merged_terrain` is on (where `terrain_records` is kept, in `pipeline.rs` and `metadata.rs`), so an error stops the run before textures and LOD.
3. Tests: two plugins (synthetic `RawRecord`s are fine): a cell with a LAND at slot 0 and a different LAND at slot 1 uses slot 1's in the cell cache and in the terrain stage, with one warning; swapping the slots swaps the choice (same answer for any mod list); a deleted LAND is ignored; two LANDs for one cell at the same slot is an error; a cell with one LAND is unchanged. The existing cell-cache tests still pass unchanged.

# Owned files
`crates/converter/src/esm/cell_cache.rs`, optionally new `crates/converter/src/esm/land_choice.rs` plus its one `mod` line in `crates/converter/src/esm/mod.rs`, `crates/converter/src/terrain_pipeline.rs`, `crates/converter/src/pipeline.rs`, `crates/converter/src/metadata.rs`, and test files under `crates/converter/tests/` you add. Keep edits outside cell_cache.rs and terrain_pipeline.rs small.

# Checks (machine-wide build gate; check `df -h /c` first, below 15 GB stop)
- `cargo fmt --all -- --check`
- `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p converter --all-targets --all-features -- -D warnings`
- `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets`
No environment-variable prefixes on commands. A snapshot that changes must be explained line by line.

# Foundations check (state it)
No shortcuts (no silent pick, no FormID tie-break), flexible (any worldspace, any load order, any plugin set), moddable (a mod that adds a LAND to a cell wins by load order exactly as in the game). Say what, if anything, remains.

# Rules
Never git stash, rebase, amend or push. Foreground commands only. Commit your owned files by name, upstream style: `fix(converter): use one LAND per cell, chosen by load order`.

# Report (under 350 words)
Files and functions changed (file:line), where the warnings surface, commit hash, test names and results, check output lines, the foundations check.
