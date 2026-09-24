# Goal
Track: Phase 2 (for upstream). Distant LOD step 4a: the engine streams Skyrim's far-away object
blocks (buildings, rocks, bridges baked into `.bto` LOD meshes) beside the terrain blocks it
already draws with `--lod`, and drops the non-rendering primitives LOD scenes carry.

# Context
- Your checkout is cut from `phase2/lod-engine-terrain` (step 3). Use upstream's commit style
  (`feat(engine): ...`, `fix(engine): ...`), with no `[deepseek]` prefix.
- The design is `C:/Modding/OpenSkyrimProject/local/research-557/DESIGN.md`. Read sections 0, 1,
  2, 4, 5, 6 and 8 ("4a"), and 9. It was written before two changes that landed tonight, which
  override it where they differ:
  1. **Blocks are clipped under full-detail cells.** `crates/engine/src/lod.rs` now draws every
     terrain block with `LodTerrainMaterial` (`ExtendedMaterial<StandardMaterial,
     LodTerrainExtension>`, shader `crates/engine/src/shaders/lod_terrain.wgsl`). Fragments over
     any cell whose full-detail terrain is showing are discarded, in the main pass and in the
     depth prepass (`ClipMask`, `update_lod_clip`). Object blocks use the same clip. This
     replaces the design's "leave the overlap to the depth offset": a simplified LOD building
     that is bigger than the real one would poke through it.
  2. **Terrain blocks shade from a model-space normal map.** The extension takes the block's
     `_n` texture off the base material (`model_space_normal`; `settings.window.z` = 1). Object
     blocks are different: their GLBs carry `NORMAL` and a tangent-space `normalTexture`
     (checked on `meshes/terrain/tamriel/objects/tamriel.4.4.-12.glb`). An object block keeps
     its base material's normal map, and the extension is built with `model_space_normal: None`,
     so the shader's `window.z == 0` path keeps `pbr_input_from_standard_material`'s normal.
- Per-kind material policy, which settles design §2 "Material":
  - Terrain: exactly what `track_lod_readiness` does today.
  - Objects: clone the GLB material into a `LodTerrainMaterial` with the clip extension and
    `model_space_normal: None`. Keep the GLB's `cull_mode`, `double_sided`, roughness and normal
    map. Set `alpha_mode: Mask(0.5)`, which the clip needs so the prepass runs it (see the
    comment in `track_lod_readiness`).
  - A primitive whose GLB material is not opaque is left alone, as today.
- The exclusion filter (design §9.1). Terrain LOD GLBs carry non-rendering primitives: a `WATER`
  node mesh whose material is `"OpenSkyrim non-rendering excluded geometry"` (MASK, alpha 0) and
  whose primitive extras carry `openSkyrim.materialExclusion`. The cell path filters these
  through `GltfExtras` (`has_explicit_material_exclusion` in `crates/engine/src/streaming.rs`).
  LOD readiness must hide them (`Visibility::Hidden`) the same way, for both kinds, and count
  them. Reuse the streaming helper; make it `pub(crate)` if needed.
- Selection (design §2 "Selection"): object blocks use the same planner, rules and bands as
  terrain. `plan_lod_blocks` plans `LodBlockKind::Terrain` only today; it must plan each enabled
  kind. `LodBlockKind::Objects` already exists in `world/database.rs`, and `lod_block` rows of
  kind `objects` are already in the table. Objects are on whenever `--lod` is. `--lod-objects off`
  turns them off. No per-kind distance flags yet: objects use `lod_bands`.
- Depth offset, block translation, hysteresis, the commit budget, rebase, the lifecycle
  validator and the metrics all already work per `LodBlockKey` (which includes the kind). Check
  each one handles `Objects`, and extend the counters where the design lists object metrics.
- Converted data for a real-data look: `C:/Modding/SkyrimConverted-upstream15/` (DB schema 4,
  717 Tamriel object blocks under `meshes/terrain/tamriel/objects/`). The lead runs the real-data
  render; you need no GPU.

# Scope
Owned files:
- `crates/engine/src/lod.rs`
- `crates/engine/src/shaders/lod_terrain.wgsl` (only if the object path needs it; say why)
- `crates/engine/src/config.rs` (the `--lod-objects` switch and its test)
- `crates/engine/src/world/database.rs` (only if the object rows need a query change)
- `crates/engine/src/streaming.rs` (only to share `has_explicit_material_exclusion` and add
  object counters to `StreamingMetrics`)
- `crates/engine/src/metrics.rs` (only for new counters)
- `crates/engine/src/app.rs` (only for `--lod-fixture` object rows, if the fixture needs them)
- `docs/handoff/impl-558-p2-lod-objects.md`

No converter changes, no schema bump, no new dependencies, no trees (that is step 4b).

# Done when
- Tests that fail on the old code and pass on yours:
  1. The plan for a camera cell includes object blocks from the table, with the same band,
     coverage and hysteresis rules as terrain, and none when objects are turned off.
  2. `--lod-objects off` parses, and `--lod` alone leaves objects on.
  3. The exclusion check recognises the primitive extras the converter writes
     (`{"openSkyrim":{"materialExclusion":"..."}}`) and rejects others.
- With `--lod` off nothing changes: no new systems run, and the existing LOD-off tests still pass
  unchanged.
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
  `cargo test -p engine` pass.

# Report
- Files changed and the approach in 5 lines.
- Each test's name and what it proves.
- Anything in the design you found wrong, or left out and why.
