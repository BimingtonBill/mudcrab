# Goal
Add the converter code that builds the land-texture arrays and their lookup table (`shared::terrain_mesh::LandTextureTable`) for the merged terrain (#186), with tests. Phase 1 track. This task builds and writes the arrays from already-converted land textures; wiring it into the conversion pipeline is a later task.

# Context
- Your worktree is based on branch `phase1/terrain-merge` at 5b909d4c (main f40ca1df plus the shared types).
- Agreed format (read first): `C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/format-proposal.md`, especially the last section "v3". The types are in `crates/shared/src/terrain_mesh.rs` (do not change them; report if one must change).
- Measured vanilla input (`C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/land-textures-2026-10-07.txt`): diffuse 52 BC1 1024, 35 BC3 1024 (one stored UNORM), 2 BC1 2048, 1 BC3 2048; normal 87 BC3 1024, 3 BC3 2048; all with full mip chains; KTX2 files with zstd supercompression (scheme 2), as `crates/converter/src/texture.rs` writes them.
- Useful existing code: `crates/converter/src/texture.rs` (`supercompress_ktx2_levels` :604, `publish_ktx2_file` :351, `encode_rgba_mips` :131, `encode_2d_surface` :733, `max_mip_levels` :989), the `ktx2` crate (already a dependency) for reading headers and level indexes, `zstd` for (de)compression, `image_dds`/`intel_tex_2` for decode and BC7 encode. Look at how texture.rs builds a KTX2 (DFD, level index, supercompression) and reuse it; a KTX2 array sets `layerCount` = layers, and each mip level's data is all layers of that level back to back.

# What to build
New file `crates/converter/src/terrain_arrays.rs` (add `pub mod terrain_arrays;` to `crates/converter/src/lib.rs`):
1. `pub struct LandTextureSource { ltex_form_id: u32, diffuse: PathBuf, normal: Option<PathBuf> }`: paths to converted `.ktx2` files.
2. `pub fn build_land_texture_arrays(sources: &[LandTextureSource], default: Option<LandTextureSource>, output_root: &Path, options: &ArrayOptions) -> Result<(LandTextureTable, ArrayReport)>`:
   - Entry 0 is the default texture: `default` if given, else a 4x4 white diffuse (no normal) in the 1024 BC1 diffuse class only if no other class... simpler: else a white texture upscaled into the smallest class that already exists for diffuse (or 1024 BC1 if none). Say in the report which was used.
   - Class of each texture: usage (diffuse sRGB / normal linear), block format, size class. Formats kept as-is: diffuse BC1, BC3, BC7; normal BC3, BC5, BC7 (`shared::terrain_mesh::DIFFUSE_FORMATS`/`NORMAL_FORMATS`). A source whose format is not in its list is decoded and re-encoded to BC7 (report: "re-encoded: <format> -> BC7"). Size class = the smallest of 1024/2048/4096 that is at least the source's larger side. A source exactly at a class size with a full mip chain is copied block for block (all mip levels, zstd-decompressed, appended as the next layer). A source smaller than its class, non-square, or with an incomplete mip chain is decoded, upscaled (Lanczos3 or the converter's existing resize), mips regenerated, and encoded to its class's format if that encoder exists, else BC7 class; reported with before/after size. A source above 4096: error naming it, unless `options.allow_downscale` is true, then downscale to 4096 and report it as "downscaled (user option)".
   - Each class becomes one KTX2 array file `terrain/land-<usage>-<format>-<size>.ktx2` under `output_root`: vkFormat = the sRGB variant for diffuse, UNORM for normal (the block data is the same); layerCount = members; at least 2 layers (pad a single member with a copy of itself); at most `options.max_layers_per_array` (default 2048), above which return an error naming the class; zstd-supercompressed like other converter KTX2 files.
   - The table: `format_version = TERRAIN_MESH_FORMAT_VERSION`, `max_layers_per_array`, only the classes used, entries dense by id in a stable order (default first, then sources sorted by LTEX FormID), and it must pass `LandTextureTable::validate()` before returning.
   - Write `terrain/land-textures.json` (serde_json, pretty) next to the arrays.
3. `pub struct ArrayReport`: per texture what happened (copied / upscaled / re-encoded / downscaled), the default's source, and the class list with layer counts and byte sizes.

# Tests (same file, with small synthetic textures; build KTX2 inputs with the converter's own writer so they look like real output)
- Two BC1 1024-equivalent sources (use 8x8 or 16x16 with full mips and treat the class size as a parameter in tests, e.g. `options.size_classes = [8, 16, 32]`, so tests stay tiny; production uses `LAND_TEXTURE_SIZE_CLASSES`) are copied exactly: decompress the output array and compare each layer's bytes with the source levels.
- A BC3 source and a BC1 source land in different classes; a single-member class is padded to 2 layers.
- A smaller source is upscaled into the next class and reported; a larger-than-max source errors without the downscale option and is downscaled with it.
- An unsupported format (for example RGBA8) is re-encoded to BC7 and reported.
- The table validates, ids are dense with the default at 0, and the JSON round-trips.
- The diffuse array's KTX2 header has the sRGB vkFormat, the normal array's the UNORM one.

# Owned files
`crates/converter/src/terrain_arrays.rs` (new) and the one `pub mod terrain_arrays;` line in `crates/converter/src/lib.rs`. Do not touch any other file (another coder owns `crates/converter/src/esm/terrain_mesh.rs` and `esm/mod.rs` in parallel; the lead owns `crates/shared`).

# Checks (machine-wide build gate, AGENTS.md 61ccd248)
Every heavy cargo command goes through the gate: `python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py run -- cargo clippy --workspace --all-targets --all-features -- -D warnings` and `python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets` (`cargo fmt --all -- --check` is light and runs directly). Before the first heavy command, run `python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py status`: if it lists any hold, do NOT build; finish writing the code and tests, commit nothing, and report "written, unbuilt" with your file list. Check `df -h /c` and `df -h /d` before building; below 15 GB, stop and report.

# Foundations check (state it in your report)
No shortcuts (nothing silently dropped, capped or downscaled; errors are loud), flexible (no hard-coded texture names, paths or counts; any mod list), moddable (any texture set a mod brings fits a class or is reported). Say what shortcut, if any, your code takes and what would break for a mod.

# Rules
Never git stash, rebase, amend or push. Commit only your owned files by name, in upstream style: `feat(converter): build land texture arrays for merged terrain (#186)`. Foreground commands only, no background waits.

# Report (under 300 words)
Files changed, commit hash, tests and results, the checks' output lines, the foundations check, and anything in the shared types that needs changing.
