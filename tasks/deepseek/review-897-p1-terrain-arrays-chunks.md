# Goal
Review two unbuilt, uncommitted converter changes for the merged terrain (#186) before the lead builds and commits them. Phase 1 track. Read only. Find what is wrong, what shortcut each takes, and what would break for a mod or another worldspace.

# Context
- Agreed format: `C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/format-proposal.md` (the "v3" section governs). Shared types: `crates/shared/src/terrain_mesh.rs` in either worktree.
- Change A (texture arrays), worktree `C:/Modding/Mudcrab/mudcrab/local/impl/impl-895-p1-land-texture-arrays`: new `crates/converter/src/terrain_arrays.rs` (about 1,800 lines). Brief: `C:/Modding/Mudcrab/mudcrab/tasks/deepseek/impl-895-p1-land-texture-arrays.md`.
- Change B (chunk files and keys), worktree `C:/Modding/Mudcrab/mudcrab/local/impl/impl-896-p1-terrain-chunk-files`: new `crates/converter/src/terrain_chunks.rs`. Brief: `C:/Modding/Mudcrab/mudcrab/tasks/deepseek/impl-896-p1-terrain-chunk-files.md`.
- Rules for the format: never silently lower quality (no downscale without the explicit option, report every non-copy); keep the source block format when it is in the usage's list (diffuse BC1/BC3/BC7, normal BC3/BC5/BC7); size classes 1024/2048/4096, placement only upward; arrays at least 2 layers, at most `max_layers_per_array`; diffuse arrays carry the sRGB vkFormat, normal arrays UNORM; the table lists only used classes, dense ids, entry 0 the default.
- Known points to judge: A errors on UASTC sources; A trims mip chains to the most common depth; B's key does not hash the record header flags (for example a LAND marked deleted or a changed flag in a mod override).

# Questions
1. Correctness: KTX2 array layout (level index, per-level layer order, DFD, supercompression), the class placement rules, block copy exactness, sRGB/UNORM labels; B's key (completeness for #185: what change in a mod would NOT change the key?), chunk grouping for negative coordinates, atomic write.
2. Foundations check: what shortcut does each take, and what would break for a mod or another worldspace? Are any limits hard-coded?
3. Tests: would they catch a broken implementation? Name a mutation each would miss.
4. Cost: memory and time for the vanilla set (90 diffuse + 90 normal at 1024/2048) and for a big mod list.

# Scope
Read only. No cargo, no builds, no writes.

# Report
Under 500 words: a verdict per change (accept / small fixes / fix first), then findings as `file:line - issue - fix`, most severe first.
