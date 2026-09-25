# Goal
Track: Phase 2 (for upstream). Distant LOD tree billboards are shaded the way Skyrim's DistantTree
shader shades them, so a distant forest is as dark as the full-detail trees next to it instead of
pale grey-green.

# Context
- Your checkout is cut from `phase2/lod-engine-trees`. Upstream commit style (`fix(engine): ...`), no
  `[deepseek]` prefix, no project-internal references (portal, DeepSeek, task numbers) in code or docs.
- The billboards: `crates/engine/src/lod.rs`, `harvest_tree_billboard_prototype` (~line 1566). Today
  every billboard vertex gets an up (+Y) normal and a `StandardMaterial` (roughness 1, metallic 0,
  `cull_mode = None`), so a billboard is lit like flat open ground: sun × N·L (≈0.7 with the default
  sun) plus a bluish ambient. Measured in a render: billboards come out about sRGB (139,141,135)
  while the full-detail pines beside them are about (65-75, 70-78, 57-64); the atlas itself
  (`textures/terrain/tamriel/trees/tamrieltreelod.dds`) is dark green, median sRGB (61,63,39).
- How vanilla shades them (read `local/research/distant-tree-lighting.md`; the key lines were
  checked against Community Shaders' `package/Shaders/DistantTree.hlsl`, lines ~212-264): the sun
  term has **no N·L**: `sunColour × shadow × 0.5`; ambient is sampled along a **camera-facing**
  normal (from screen-space derivatives); the result is `(sun term + ambient) × albedo`. No specular.
- An existing custom material to copy the pattern from: `LodTerrainMaterial =
  ExtendedMaterial<StandardMaterial, LodTerrainExtension>` in the same file, with its shader in
  `crates/engine/src/shaders/lod_terrain.wgsl`.
- Choose the smallest faithful implementation, and say why: for example a material extension whose
  fragment shader computes the vanilla formula from Bevy's directional light and ambient, or another
  approach that gives the same result. Keep the billboards batched per tree type as they are now.

# Scope
Owned files: `crates/engine/src/lod.rs`, a new shader under `crates/engine/src/shaders/` if you use one,
`docs/design/distant-lod.md` only if it describes the tree shading (read it on this branch first), and
your handoff note `docs/handoff/impl-574-p2-tree-billboard-lighting.md`. No converter changes, no
schema change, no new dependencies.

# Done when
- The billboard shading follows the vanilla formula above (no N·L on the sun term, the 0.5 factor,
  ambient added, times albedo, no specular), with a comment citing the source of the formula.
- A unit test pins what can be tested on the CPU side (e.g. the material/extension settings, or a
  pure function computing the lighting factor), and fails on the old up-normal shading.
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
  `cargo test -p engine` pass.

# Report
The approach you chose and why; files changed; the test names; the check output. The lead renders
and compares on screen, so say what you expect a render to show.
