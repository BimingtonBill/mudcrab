# Goal
Track: Phase 2 (for upstream). Distant LOD step 4b: the engine draws Skyrim's distant tree
billboards (`.btt` tree LOD), so forests continue past the full-detail cells instead of ending at
their edge.

# Context
- Your checkout is cut from `phase2/lod-engine-objects` (steps 3 and 4a: terrain and object LOD
  blocks). Use upstream's commit style (`feat(engine): ...`), with no `[deepseek]` prefix.
- The design: `C:/Modding/MudcrabProject/local/research-557/DESIGN.md`, section 3 ("Tree
  billboards"), 4, 5, 6 and 8 ("4b"). Follow it, with these decisions from the lead:
  1. **Geometry:** the converter already publishes one crossed-quad GLB per tree type (`MASK`,
     double-sided); draw those as they are. No new WGSL.
  2. **Instances are entities batched by Bevy:** one entity per instance under its tree-block root,
     sharing the type's `Mesh3d` and `MeshMaterial3d<StandardMaterial>` handles from a
     `LodBillboardCatalog` built once per worldspace (spawn each type's `Scene(0)` as a prototype,
     read the handles on `WorldInstanceReady`, despawn the prototype). No `Name` on instances.
     `NotShadowCaster`.
  3. **Placement:** through the shared coordinate contract (`creation_to_runtime_vector`,
     `creation_euler_to_runtime_quaternion([0, 0, rotation])`), block-relative under a root placed
     like a level-4 block (level 4's depth offset).
  4. **Residency:** tree blocks are their own `LodBlockKind::Trees` at level 4 with their own band
     (`lod_tree_distance`, default 75 000 as in every shipped preset but Low), planned by the
     existing `plan_blocks` (terrain-style nested rules are fine: trees have one level). Their
     availability and payload come from `lod_tree_instance` (and `lod_tree_type` for the mesh
     path), not `lod_block`.
  5. **Hiding near the player, per instance:** hide an instance while its own cell is *loaded*,
     using the same `LoadedCells` resource the terrain clip and object segments use (cells whose
     full-detail terrain is showing), not a camera square. Store each instance's absolute cell at
     spawn. Only walk the resident instances when `LoadedCells` changes.
  6. **Budget:** spawn instances across frames with a per-frame cap
     (`lod_tree_instances_per_frame`, default in the design), counted in the shared commit budget.
     A tree block's root is shown once all its instances are spawned.
  7. `--lod` turns trees on with the other kinds; `--lod-trees off` turns them off alone.
- Step 2 data (converted, read-only): `C:/Modding/SkyrimConverted-upstream15/` has 120 tree types
  and 43,333 instances in `lod_tree_type` / `lod_tree_instance`. Tree-instance block keys are
  absolute cell coordinates (see design section 9, finding 4).
- Released LOD roots use `try_despawn`, as in the rest of `lod.rs`.

# Scope
Owned files: `crates/engine/src/lod.rs`, `crates/engine/src/world/database.rs` (the Trees kind,
the instance and type queries), `crates/engine/src/config.rs` (the tree options and their tests),
`crates/engine/src/streaming.rs` (tree counters in `StreamingMetrics` only), `crates/engine/src/app.rs`
(`--lod-fixture` tree rows only, if the fixture needs them), `docs/handoff/impl-568-p2-lod-trees.md`.
No converter changes, no shader changes, no schema bump, no new dependencies.

# Done when
- Tests that fail on the old code and pass on yours:
  1. A synthetic `lod_tree_instance` / `lod_tree_type` database yields the right tree blocks and
     instance rows (ordered for determinism), and a `tree_index` with no type drops its instances
     and counts one failure.
  2. Placement: an instance at a known Creation position and rotation lands at the expected
     block-relative translation and yaw (use the shared contract's own tests as the reference).
  3. Hiding: with an instance's cell in `LoadedCells` it is hidden, and it is shown again when the
     cell leaves.
  4. Spawning respects the per-frame cap, and the block root shows only once every instance exists.
  5. `--lod-trees off` parses and removes trees from the plan; `--lod` alone keeps them on.
- With `--lod` off nothing changes, and every existing LOD test still passes.
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
  `cargo test -p engine` pass.
- Do not run the engine window; the lead runs the real-data render.

# Report
- Files changed and the approach in 6 lines.
- Each test's name and what it proves.
- Anything in the design you changed or left out, and why.
