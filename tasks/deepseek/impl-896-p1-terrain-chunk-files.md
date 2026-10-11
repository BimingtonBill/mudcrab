# Goal
Add the converter code that keys merged terrain cell meshes (#185 derived-data keys) and writes them into terrain chunk files, with tests. Phase 1 track, #186 (terrain merge). Pipeline wiring (calling this during a conversion) is a later task; this one adds the library functions only.

# Context
- Your worktree is based on `phase1/terrain-merge` at c49aab04 (main f40ca1df plus the shared terrain types, the cell-mesh builder 8d074309 and the path helpers).
- Agreed format: `C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/format-proposal.md` (read the "v3" section). Types and paths: `crates/shared/src/terrain_mesh.rs` (`TerrainCellMesh`, `TerrainChunkFile`, `TERRAIN_MESH_FORMAT_VERSION`, `terrain_chunk_path`, `terrain_chunk_for_cell`). Do not change shared; report if it must change.
- The cell mesh comes from `crates/converter/src/esm/terrain_mesh.rs` (`build_cell_mesh(cell_id, record, known_ltex, key)`); it takes the key as an argument.
- Hashing: `crates/converter/src/cache.rs` has `hash_bytes` (SHA-256, hex). Records: `crates/converter/src/esm/records/mod.rs` (`RawRecord`); find how a record's plugin and that plugin's master list are known after merging (search `masters`, `plugin` in `crates/converter/src/esm/`). The #185 rule (measured and accepted 2026-10-06): a record's hash = its raw bytes plus the declaring plugin's master list; a derived item's key also includes its format version.
- rkyv usage example: `crates/converter/src/esm/cell_cache.rs` (`write_cell_cache`, `validate_cell_cache`) writes and validates an rkyv file; follow it.

# What to build
New file `crates/converter/src/terrain_chunks.rs` (add `pub mod terrain_chunks;` to `crates/converter/src/lib.rs`):
1. `pub fn cell_mesh_key(land: &RawRecord, land_masters: &[String], textures: &[(&RawRecord, &[String])]) -> [u8; 32]`: SHA-256 over, in a fixed order with length prefixes so no two inputs collide: `TERRAIN_MESH_FORMAT_VERSION`, the LAND record's type, FormID and serialized subrecords, its plugin's master list, then each LTEX and TXST record the cell uses (sorted by FormID) with its master list. Return the raw 32 bytes. Document exactly what goes in.
2. `pub struct PlacedCellMesh { worldspace_id: u32, grid_x: i32, grid_y: i32, mesh: TerrainCellMesh }`.
3. `pub fn write_terrain_chunks(output_root: &Path, cells: Vec<PlacedCellMesh>) -> Result<ChunkWriteReport>`: group cells by `terrain_chunk_path`, sort each chunk's cells by `cell_id` (deterministic output), refuse a duplicate cell_id in a worldspace (error naming it), check every mesh's structure (call `TerrainCellMesh::validate` with an ids map built from the mesh's own slots, as `build_cell_mesh` does; the table check happens at load), serialize `TerrainChunkFile { format_version, worldspace_id, cells }` with rkyv, and write each file atomically (write to `<path>.tmp`, then rename over the target).
4. `pub fn read_terrain_chunk(path: &Path) -> Result<TerrainChunkFile>`: read and validate (rkyv bytecheck), refuse a wrong format_version with a clear error.
5. `pub struct ChunkWriteReport`: files written per worldspace, cells written, total bytes.

# Tests (same file)
- Key: the same inputs give the same key; changing one LAND subrecord byte, a master name, an LTEX record, or the order of textures given (sorted internally, so the order must NOT change the key) behaves as documented; a length-prefix test shows ("ab","c") and ("a","bc") differ.
- Write two worldspaces' cells (build meshes with `build_cell_mesh` from small synthetic LAND records, as `esm/terrain_mesh.rs`'s tests do) into a temp dir: files appear at `terrain_chunk_path` paths, cells in negative grid coordinates land in the right chunk, reading back gives equal meshes sorted by cell_id.
- A duplicate cell_id errors; a corrupted file fails `read_terrain_chunk`; a wrong format_version is refused.

# Owned files
`crates/converter/src/terrain_chunks.rs` (new) and the one `pub mod terrain_chunks;` line in `crates/converter/src/lib.rs`. Another coder adds `pub mod terrain_arrays;` to the same lib.rs in parallel: add only your one line, next to the other `pub mod` lines.

# Checks (machine-wide build gate, AGENTS.md 61ccd248)
Every heavy cargo command goes through the gate: `python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py run -- cargo clippy --workspace --all-targets --all-features -- -D warnings` and `python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets` (`cargo fmt --all -- --check` is light and runs directly). Before the first heavy command, run `python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py status`: if it lists any hold, do NOT build; finish writing the code and tests, commit nothing, and report "written, unbuilt" with your file list. Check `df -h /c` and `df -h /d` before building; below 15 GB, stop and report.

# Foundations check (state it in your report)
No shortcuts (nothing silently dropped; errors are loud), flexible (any worldspace, any load order, negative coordinates), moddable (a mod's override of LAND, LTEX or TXST changes the key, so the cell is rebuilt). Say what shortcut, if any, the code takes and what would break for a mod or another worldspace.

# Rules
Never git stash, rebase, amend or push. Commit only your owned files by name, upstream style: `feat(converter): key and write merged terrain chunk files (#186)`. Foreground commands only.

# Report (under 300 words)
Files, commit hash, tests and results, checks output lines, the foundations check, and how the master list of a record's plugin is found.
