# Goal
Wire the merged-terrain outputs into a conversion run: after the world database and the land textures exist, the converter builds the land texture arrays, one merged mesh per exterior cell, and the chunk files, and writes a terrain report. Phase 1 track, #186. Off by default behind a converter option until the engine half lands.

# Context
- Worktree: created by the launcher from `phase1/terrain-merge` at 20ac4831 (main f40ca1df plus the terrain libraries).
- Libraries to call (read them first): `crates/converter/src/esm/terrain_mesh.rs` (`build_cell_mesh`), `crates/converter/src/terrain_arrays.rs` (`build_land_texture_arrays`, `LandTextureSource`, `ArrayOptions`, `ArrayReport`), `crates/converter/src/terrain_chunks.rs` (`cell_mesh_key`, `PlacedCellMesh`, `write_terrain_chunks`). Shared contract: `crates/shared/src/terrain_mesh.rs` (paths `LAND_TEXTURE_TABLE_PATH`, `terrain_chunk_path`).
- Agreed format and rules: `C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/format-proposal.md` (section "v3").
- Where the pipeline has what you need (find by searching, cite file:line in your report): the merged records map and the load order with each plugin's masters (`esm/load_order.rs`, `LoadOrder::metadata[...].masters`; the record's `load_order` field), the winning LAND/LTEX/TXST records, each exterior cell's worldspace and grid (CELL XCLC; `esm/exporter.rs` writes `cells.grid_x/grid_y`), the converted texture paths (`crates/engine/src/world/database.rs:305-327` maps `Textures\X.dds` to `textures/x.ktx2`; the converter has the same mapping in `asset_path.rs`), the staging output root, and the conversion report. The LOD stage (`crates/converter/src/lod/`) is a good model of a post-database stage that reads the DB and writes extra outputs into staging.
- Default texture: the option `terrain_default_texture` = `white` (default) or `skyrim-ini`; for `skyrim-ini`, read `[Landscape] sDefaultLandDiffuseTexture` / `sDefaultLandNormalTexture` from the user's Skyrim.ini if a path to it is given in config, else `Dirt02.dds` / `Dirt02_N.dds` under `textures/landscape/`. Report which source was used.

# What to build
1. A config option (`config.rs` and the CLI in `main.rs`, following how other options such as `--no-lod` are done): `--merged-terrain` (off by default) and `--terrain-default-texture <white|skyrim-ini>`.
2. New file `crates/converter/src/terrain_pipeline.rs` with `pub fn build_merged_terrain(...) -> Result<TerrainReport>` called from the pipeline after the database and textures are in staging (when the option is on):
   - collect every winning LTEX (with its TXST diffuse/normal paths) and build the arrays with `build_land_texture_arrays` into the staging root;
   - for every exterior cell with a LAND record: compute `cell_mesh_key` (the LAND, its plugin's masters, and every LTEX the LAND names plus their TXST, present or not), build the mesh with `build_cell_mesh` (known LTEX = the table's entries), collect `PlacedCellMesh`, and write all chunk files with `write_terrain_chunks`;
   - interior cells are skipped (interiors have no LAND terrain);
   - a cell whose mesh fails to build is reported by cell id and reason and left out (the engine falls back for it); a whole-stage failure is an error.
3. `TerrainReport` (serialize it into the conversion report JSON, next to the LOD report): arrays and layer counts, skipped textures, cells written, cells over 6 slots, cells needing stride 16, cells using the default texture, cells left out with reasons, and timings for each step.
4. No schema bump in this task (the bump is batched later); say in the report where it would go.

# Tests
- An end-to-end converter test with the dummy-content plugin or synthetic records (look at how the LOD stage is tested): with the option on, staging gets `terrain/land-textures.json`, at least one array, and chunk files at `terrain_chunk_path` paths that read back with `read_terrain_chunk`; with the option off, no `terrain/` folder.
- The key changes when an LTEX the LAND names is edited.
- The key changes when the load order shifts: the same LAND and LTEX records, remapped to a different master index (as `esm/mod.rs` `remap_record_form_ids` does for a plugin inserted earlier), give a different key.

# Owned files
`crates/converter/src/terrain_pipeline.rs` (new), `crates/converter/src/lib.rs` (one `pub mod` line), and the minimal call-site and option edits in `crates/converter/src/pipeline.rs`, `crates/converter/src/config.rs`, `crates/converter/src/main.rs`, plus the report struct where the conversion report is defined (name the file you change). Keep the edits in those existing files small and say each one in the report.

# Checks (machine-wide build gate)
Heavy cargo goes through the gate, with the owner set: `BUILD_GATE_OWNER=p1 python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy --workspace --all-targets --all-features -- -D warnings` and `BUILD_GATE_OWNER=p1 python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets`. `cargo fmt --all -- --check` runs directly. Check `df -h /c` before building; below 15 GB, stop and report. If the gate refuses or a hold is listed for more than a few minutes, finish the code and report "written, unbuilt".

# Foundations check (state it)
No shortcuts (nothing silently dropped; every left-out cell and texture reported), flexible (any worldspace, any load order, no hard-coded FormIDs or paths), moddable (a mod's LAND/LTEX/TXST override changes the key; the option keeps the stage switchable). Say what shortcut, if any, remains.

# Rules
Never git stash, rebase, amend or push. Commit only your owned files by name, upstream style: `feat(converter): build merged terrain during a conversion (#186)`. Foreground commands only.

# Report (under 400 words)
Files and call sites changed (file:line), commit hash, tests and results, checks output lines, the foundations check, and the measured time of the terrain stage if a test shows it.
