# Goal
Review the engine half of merged terrain (#186) on local branch `phase2/terrain-merge` (Mudcrab, Rust/Bevy 0.19, wgpu 29): each exterior cell's terrain drawn as one merged mesh with one material that samples land textures from per-class `texture_2d_array` bindings, instead of four quadrant meshes with their own materials. Purpose: fewer entities and draws for the render thread, for the 160 fps goal (6.25 ms frames). Large change (about 4,900 lines); the code is not yet compiled in its tests, so read for logic, not for compile errors.

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-terrain. Review `git diff c49aab04..HEAD` (eeb1b050, 86ff4dbf, 463d1460). The shared contract (5b909d4c, c49aab04) and the converter mesh builder (8d074309) are Phase 1's, already reviewed; use them as the spec: crates/shared/src/terrain_mesh.rs and the land texture table types.
- Read first: docs/specs/engine/merged-terrain.md and docs/handoff/claude-p2-terrain-merge-engine.md (the coder's design and open points).
- Key files: crates/engine/src/terrain_merge/{mod.rs, classes.rs, mesh.rs, material.rs, terrain_merged.wgsl, shader_check.rs}, crates/engine/src/streaming.rs, streaming/lod.rs, streaming/merged_tests.rs, profiling.rs, app.rs, examples/terrain_merge_probe.rs. Today's path for comparison: crates/engine/src/terrain.wgsl (or wherever the quadrant terrain shader lives) and the quadrant spawn in streaming.rs.
- Bevy 0.19 source is in the cargo registry (`%USERPROFILE%/.cargo/registry/src/*/bevy_*-0.19*`): check MaterialExtension specialisation, `AsBindGroup` with texture_2d_array and storage buffers, vertex buffer layout appending, and `RenderAssetUsages` for the array images.

# Scope
Read only. Change nothing.

# Done when
You have checked, with file:line:
1. Parity: the merged mesh equals the four quadrant meshes (positions, normals, UVs, colours, winding) and the shader blend equals today's (`sum(tex*w)/sum(w)`, base slot implied, missing normal = straight up, DX green flip). Name every visual difference you find (the coder names one: back faces where no slot has a normal map).
2. Weld and collider: edges registered identically, so merged and quadrant cells weld both ways; one collider per cell from the render mesh; readiness and LOD coverage correct on spawn and despawn.
3. Fallback: every way a cell can fail (no table, refused device, arrays late, missing chunk, unknown FormID, seam mismatch, corrupt chunk, wrong worldspace) ends in today's quadrant path with a counted, logged reason, never a missing or wrong cell. Arrays late must not cause permanent fallback.
4. Specialisation: shader defs per class set and stride; uniform control flow for textureSampleGrad derivatives; flat interpolation of slot ids is exact; storage buffer lookup word packing matches classes.rs.
5. Resources: array images keep a CPU copy (default asset usage) - is RENDER_WORLD-only possible here? Chunk LRU bound and mmap lifetime; anything per frame that should be per cell.
6. Tests: do they assert real behaviour (not tautologies)?

# Foundations check (answer explicitly)
What shortcut does this take, and what would break for a mod (more or other land textures, a mod LTEX overriding a vanilla one, a load-order change, a stale chunk after a mod changes a LAND record (#185: the engine does not check the cell's derived-data key)) or another worldspace? Anything hard-coded that should be data (the 48-texture allowance, nine slots per usage)?

# Report
Verdict (accept / fix needed), findings as path:line - issue - fix, ordered by severity; what you verified; what you could not verify.
