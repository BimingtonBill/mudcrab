# Which fork fixes apply to upstream

**Status:** DeepSeek research-079 (read-only), 2026-09-23, against upstream `main` = `2105bfd`. Spot-checked by Claude: candidate 8 (`pipeline.rs` record_skip -> `manifest.complete`) and candidate 1 (no `CascadeShadowConfig` upstream) confirmed. Section numbers are candidates, not PRs.


**Method / caveats.** Read-only. Everything below is cited from the working directory `C:\Modding\wkj-phase2` (checked out at `main` = `origin/main` = `2105bfd`), except fork code, which I read through `git show <commit>:<path>` / `git show <commit> -- <path>`. The worktree carried one pre-existing staged change (`crates/shared/src/coordinates.rs`, per the session's `git status`); I did not touch it and cite no line from it. I could not read Bevy's source (the cargo registry is outside the permitted working directories), so two Bevy-internal facts are cited from the fork's own documentation rather than verified — flagged where they matter (candidates 1, 2, 4).

Fork commit shorthand: `8bd7a01`=1, `0475412`+`782f8dd`=2, `59f1f20`=3, `abaa14f`=4, `b4ce0d9`=5, `0ece570`+`68c19bb`=6, `2b69616`=7, `23f68f6`=8.

---

## 1. `8bd7a01` — shadow cascades sized for metres

**Verdict: applies.**

Upstream's sun is spawned in `setup_world` with shadows on and no cascade configuration of its own:

- `crates/engine/src/app.rs:1280-1287` — `DirectionalLight { illuminance: 12_000.0, shadow_maps_enabled: true, ..default() }, Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0))`. No `CascadeShadowConfig`.
- `CascadeShadowConfig` appears nowhere in upstream (`grep` over `crates/`: zero hits, in any crate), and `grep -i shadow` over `crates/engine/src` hits only `app.rs`'s five `shadow_maps_enabled: true` literals (`app.rs:497, 723, 889, 1094, 1283`).
- The world is in Creation units: `crates/engine/src/world/components.rs:3` (`CELL_SIZE: f32 = 4096.0`), camera far plane `CELL_SIZE * (stream_radius.max(1) + 2) as f32 * 2.0` at `app.rs:1269`, camera offset `(0, 1200, 2500)` at `app.rs:1266`.

So upstream asks Bevy for shadows in a world where Bevy's default cascades (metre-scale) cover the ground in front of the lens. The four other `DirectionalLight`s are the CI fixtures (`app.rs:494-501` material fixture, `:721`, `:887`, `:1092`), which are deliberately metre-scale test scenes and need nothing.

**Upstream PR content.** `crates/engine/src/app.rs` only: the `CREATION_UNITS_PER_METRE`/`SUN_SHADOW_*` constants, `fn sun_shadow_cascades(&EngineConfig) -> CascadeShadowConfig`, one element added to the `DirectionalLight` tuple at `:1280-1287`, optionally the explicit `DirectionalLightShadowMap` insert, and two tests in `mod tests`. **Must be rewritten, not applied:** the fork's doc comment and tests reference fork-only items — `crate::lights` (no such module upstream; `ls crates/engine/src` has no `lights.rs`), `config.terrain_radius` and `crate::config::MAX_TERRAIN_RADIUS` (upstream `EngineConfig` has only `stream_radius`, `crates/engine/src/config.rs:9`, default `2` at `:46`), and the distant terrain ring (fork-only, commit `5211bad`). The second test's ring half must be dropped. The `setup_world` hunk's anchor differs (the fork's version has bloom, fog/backdrop and a portal camera there); upstream's natural anchor is the light tuple itself, and the far plane at `app.rs:1269` is the honest upstream basis for the cascade range (the fork derives it from `stream_radius + 1` cells because upstream's `wanted_cells` grid is exactly `stream_radius` around the camera's cell: `crates/engine/src/streaming.rs:192-200`).

**Schema impact: none** (engine-only).

**CI-safe test: yes.** The fork's `the_sun_shadows_cover_the_full_detail_grid` and `the_shadow_range_follows_the_full_detail_grid_and_not_the_ring` are pure functions of `EngineConfig` — no game data, no GPU. Port the first; the second's ring half has no upstream analogue.

**Marco:** M3 (provar o caminho de renderização) — the sun's shadows are part of the render path the Marco 3 exit criteria ("teste de visibilidade passa…") and the M7 visual review depend on. **Risk to flag in the PR:** the fork's own analysis says this is the first configuration where the shadow pass draws the world at all, costing "about what the main opaque pass costs again" — upstream's M7 gates are `FPS >= 60`, so this PR needs a benchmark run before merge, and the `--benchmark-only` path (`app.rs:1221-1254`) is the cheapest way to measure.

---

## 2. `0475412`, `782f8dd` — entity commands on streamed entities must be fallible

**Verdict: partly applies.** The identical code shape is upstream's, at 13 sites, but I could not construct a reachable crash on `main`, because upstream has exactly one entity despawner and it is ordered first in the same chain as every site.

Upstream evidence:

- `crates/engine/src/streaming.rs:51-62` — one chained set: `(plan_cells, collect_cells, track_asset_readiness, track_surface_readiness, update_render_origin, validate_streaming_lifecycle).chain()`.
- `crates/engine/src/streaming.rs:229` — `commands.entity(*root).despawn()` when a cell leaves the unload radius (inside `plan_cells`, the first system of the chain). `despawn` is recursive in Bevy 0.16+, so the whole cell subtree goes.
- The non-fallible commands on that subtree: `streaming.rs:675`, `:709`, `:738`, `:767` (`remove::<PendingAssetProfile>()`), `:803-804`, `:822-823` (terrain validated/failed: `insert(Visibility::…)` + `remove::<PendingTerrainProfile>()`), `:840-841`, `:856-857` (water), `:1224` (`hide_partial_scene`'s `insert(Visibility::Hidden)` on descendants).
- Grep for `despawn` across all of `crates/` returns **only** `streaming.rs:229` — there is no second despawner upstream, and no `try_insert`/`try_remove`/`try_despawn` anywhere.
- The scene-ready observer uses a query, not commands: `streaming.rs:624-631` (`pending.get_mut(ready.entity)` returns `Err` for a dead entity).
- No upstream system outside `streaming.rs` queues entity commands against streamed entities: `grep "Commands" crates/engine/src/app.rs` gives only fixture setup (`:277, :406, :589, :778, :1017, :1222, :1257`) and the screenshot spawn (`:1420, :1468-1470`).

So: `plan_cells` despawns, every reader of those components is explicitly chained after it, and Bevy inserts a sync point between explicitly ordered systems (the fork's own note, `git show portal:crates/engine/src/model_animation.rs` lines 917-935 of that file, and its test that passes with *and* without the `try_` forms — i.e. no command is ever queued against a dead entity in a chained layout). The crash the fork actually chased and reproduced was in a fork-only system: `Engine::validate` reported `when applying buffers for system engine::portal::isolate_cells` (fork `crates/engine/src/portal.rs`, `isolate_cells`), on an entity a fork-only space swap had despawned — and `portal.rs`, `transition.rs`, `door_animation.rs`, `model_animation.rs`, `player.rs` do not exist upstream at all (`ls crates/engine/src`).

**Upstream PR content.** `crates/engine/src/streaming.rs` only — the 13 sites above become `try_insert`/`try_remove`. The fork's hunks (in `782f8dd`) are textually the same lines and apply mechanically, modulo line offsets; its `app.rs`/`player.rs`/`portal.rs`/`transition.rs`/`door_animation.rs` hunks have no upstream target (upstream's `app.rs` has no `update_atmosphere`, that is fork-only).

**Schema impact: none.**

**CI-safe test: none that proves the fix.** The fork states its own test "passes with the `try_` forms and without them" because the sync point between ordered systems removes the interleaving; I found no way to build the failing interleaving in a Bevy `App` test without reaching into command-buffer flush ordering. Port the fork's test only as a regression guard, labelled as such.

**Which Marco:** M5 (endurecer streaming e ciclo de vida: "zero entidades órfãs", robustez). **Honest recommendation: send this last, or not at all** — upstream cannot currently fire it, so it is defensive hardening plus a comment explaining why the sites are fallible (the fork's rationale, "a reference whose cell went away wants nothing done to it", is the transferable part).

---

## 3. `59f1f20` — glossiness is a Blinn-Phong exponent

**Verdict: applies**, verbatim.

- `crates/converter/src/material.rs:414-417` — `let mut pbr = serde_json::json!({ "baseColorFactor": …, "metallicFactor": 0.0, "roughnessFactor": (1.0 - (material.glossiness / 100.0).clamp(0.0, 1.0)) })`. This is the exact line the fork replaced.
- The upstream schema is 14: `crates/converter/src/cache.rs:11` (`CONVERTER_SCHEMA_VERSION: u32 = 14`), migration arm at `cache.rs:61`, `crates/converter/src/pipeline.rs:77`, engine's copy at `crates/engine/src/app.rs:1215-1218`, and the contract is asserted in the acceptance preflight with `manifest.complete` (`scripts/phase2-acceptance.ps1:162-170`).
- Upstream's fixture/test to update: `material.rs:1129` (`glossiness: 32.0`) and `material.rs:1312-1315` (`assert!((roughness - 0.68).abs() < 1e-6)`).
- `validate_material` already rejects negative/non-finite glossiness (`material.rs:1017`, `:1048`, `:1057`), so the fork's guard function is belt-and-braces, not a behaviour change.

**Upstream PR content.** `material.rs` (new `perceptual_roughness()` + the one-line change at `:417` + four tests + the 0.68→0.4924791 test edit), `cache.rs` (14→**15**, migration arm `matches!(schema_version, 12..=14) && CONVERTER_SCHEMA_VERSION == 15`, the migration test sweep), `pipeline.rs:77` (`12 | 13`→`12..=14`), `engine/src/app.rs:1218` (14→15), and the docs that hard-code the number: `docs/roadmap/02-core-engine.md:93`, `docs/roadmap/02-integration-and-acceptance.md:8`, `docs/roadmap/02-visual-fidelity-implementation-plan.md:230,267`. The fork's hunks apply cleanly (same lines); the *numbers* must be rewritten (fork is 18, upstream 14) and the fork's comment text about schemas 15-18 trimmed.

**Schema impact: yes** — every material JSON in every GLB changes, so bump 14→15 and reconvert; until then upstream's engine refuses the assets (`app.rs:1196-1200`). The fork's point stands: one bump can carry more than one publication change.

**CI-safe test: yes.** Pure-function tests on `perceptual_roughness` (value table, strict monotonicity, finite/in-unit guard) plus the changed fixture assertion — all use synthetic `ValidatedNifMaterial` fixtures, no game data.

**Which Marco:** M2 item 2 ("flags visuais necessárias ao conjunto estático: … parâmetros de gloss/roughness").

---

## 4. `abaa14f` — vertex-colour alpha is not opacity

**Verdict: applies.** Upstream has both halves of the data path; only the fix is missing.

- The converted GLBs carry `COLOR_0` as a `Vec4`: `vendor/project-wormhole-nif/src/model/model.rs:175-184` (`if !colors.is_empty() { append_attribute(&colors, F32, Vec4, …, glTF::Semantic::Colors(0), "COLORS") }`).
- Upstream publishes alpha-tested materials with a cutoff: `crates/converter/src/material.rs:430-433` (`"OPAQUE" | "MASK" | "BLEND"`) and `:449-450` (`if alpha_mode == NifAlphaMode::Cutout { output["alphaCutoff"] = … }`), with the cutoff test at `:1301-1303`.
- Upstream's engine does **not** touch vertex colours: `crates/engine/src/render.rs` (498 lines) has no `ATTRIBUTE_COLOR`/alpha code at all; the only colour writer anywhere upstream is the terrain mesh (`crates/engine/src/streaming.rs:1475-1506`, `Float32x4` with alpha `1.0`).
- Bevy's PBR shader is what multiplies the two (the fork cites `bevy_pbr-0.19.0/src/render/pbr_fragment.wgsl:54-55, :194` and `pbr_functions.wgsl:119-127`; **I could not read Bevy's source to re-verify this** — no read access to the cargo registry). The fork verified the effect on screen (trees are dots before, canopies after) and the same Bevy 0.19 + same converted data is upstream's situation.

**Upstream PR content.** Upstream has no equivalent code, so this is *new* code, not a hunk: `crates/engine/src/render.rs` — `force_opaque_vertex_alpha(&mut Mesh) -> Result<usize, &'static str>` (pure), `force_opaque_vertex_colours` (reads `AssetEvent<Mesh>`, rewrites only when something changed, so it does not chase its own `Modified` event) and one registration in `VercidiumRendererPlugin::build` (`crates/engine/src/render.rs:32-60`) in `PostUpdate` after `AssetEventSystems` (upstream's plugin adds its systems with `.add_systems(Update, (…))` at `render.rs:42-49`, so this is an added `.add_systems(PostUpdate, …)` — no conflict). Alternative and worse: fix it in the converter by publishing `COLOR_0` opaque, which loses the Skyrim shader parameter the fork deliberately keeps and forces a schema bump + reconversion. **I could not determine** whether Bevy widens a `Vec3` `COLOR_0` to alpha 1.0 (which would make a "drop the alpha" converter fix work); the fork's evidence is that the glTF loader widens `COLOR_0` to `Float32x4`.

**Schema impact: none** for the engine-side fix (the fork's commit says "No reconversion needed"); **yes** if done converter-side with the other publication changes.

**CI-safe test: yes.** `force_opaque_vertex_alpha` takes a `Mesh` and returns the count of vertices changed: build a mesh with `Float32x4` colours whose alphas are 0.25/0.0, assert RGB bit-identical, alpha `1.0`, count == 2; plus "no colour attribute → `Ok(0)`" and "3-component colours are not widened". No game data. (The fork's tests for this are in `render.rs`.)

**Which Marco:** M1 item 1 (the NIF export includes "cores"), M2 item 2 (alpha test), and M7's rural visual review — this is literally vegetation rendering.

---

## 5. `b4ce0d9` — vertex alpha is not blend; keep blend factors; drop editor markers

**Verdict: applies (all three parts).**

- *Vertex alpha ≠ blend.* `crates/converter/src/material.rs:990-992`: `let shader_requires_blend = shader_flags_1 & (SLSF1_VERTEX_ALPHA | SLSF1_SCREENDOOR_ALPHA_FADE) != 0 || shader_flags_2 & SLSF2_PREMULTIPLIED_ALPHA != 0;` — with `SLSF1_VERTEX_ALPHA` at `:16`. Upstream even has tests asserting the wrong behaviour: `material.rs:1479` and `:1483` (`alpha_contract(None, SLSF1_VERTEX_ALPHA, 0).0` must become `Opaque`).
- *Blend factors.* Upstream publishes the `OPEN_SKYRIM_material` extension without them (`material.rs:654-710` region; `premultiplied_alpha` and `screen_door_alpha_fade` are the only blend-ish fields, `:576-577`). The vendored crate already exposes the raw flag word the decoder needs — `vendor/project-wormhole-nif/src/nif_flags.rs:181` (`AlphaFlags::raw()`), with `test_enabled()`/`blend_enabled()` used at `material.rs:983-987` — so **no vendor change is required**.
- *Editor markers.* Upstream has no marker-shape handling anywhere: `grep -i marker` over `crates/converter/src/{material,mesh,lib}.rs` and `grep -rn "editor_marker\|EditorMarker"` over `crates/` both return nothing. Excluded shapes only receive a non-rendering material (`material.rs:249-270`: `"Mudcrab non-rendering excluded geometry"`, `MASK`, `alphaCutoff 1.0`, `baseColorFactor [0,0,0,0]`) while their geometry stays in the GLB — and no upstream rule excludes a shape *named* `EditorMarker`. `crates/converter/src/mesh.rs:437-448` (`is_declared_geometry_block`) and `:450-461` (`is_deferred_dynamic_mesh`) are path/block-type rules only. Upstream's *documented* policy is nevertheless to exclude them: `docs/roadmap/02-core-engine.md:59-61` — "Runtime static-world discovery therefore excludes sky/weather geometry, editor marker meshes and the `Effects`/`Markers` trees" — which today is implemented only for whole model paths, so a marker authored *inside* a legitimate model is still drawn.

**Upstream PR content.** `crates/converter/src/material.rs`: drop `SLSF1_VERTEX_ALPHA` from the blend decision and fix the two tests at `:1479-1483`; add `NifBlendFactor` + `blend_factors()` (pure, testable), thread it through `publish_material` (`:414+`) and `publish_skyrim_extension` (`:654+`), add `blend_factors` to `ValidatedNifMaterial` (`:119-140`) and to both builders (`:779`, `:849`); add `is_editor_marker_shape` and the exclusion in `build_shape_material` (`:757+`). `crates/converter/src/mesh.rs`: `drop_editor_marker_geometry` and its two call sites (upstream's `convert` body at `mesh.rs:87-117`; note upstream keeps `let output = glb_output_path.as_ref()` *inside* the empty-scene branch at `:96` and again at `:102`, where the fork hoists it — a one-line reshuffle, not a conflict). `cache.rs`/`app.rs`/docs: the schema bump. **Upstream overlap to name:** `8b5bf80` (prune dangling texture URIs) added +397 lines to `mesh.rs` and +61 to `pipeline.rs` *after* the fork point, but its `mesh.rs` hunks are a new `impl MeshConverter` region at `mesh.rs:196+` and tests at `:1669+` — **not** the `convert` body this PR touches, so no textual conflict, only offsets; if both PRs are queued, land the prune first.

**Schema impact: yes** — published materials and the exported mesh set change; bump, reconvert. (The same bump can carry candidates 3 and, if done converter-side, 4.)

**CI-safe test: yes, all three parts.** `alpha_contract` and `blend_factors` are pure functions over a synthetic `NiAlphaProperty` (upstream's `fixture(...)` helpers at `material.rs:1108-1130` build these); `is_editor_marker_shape` is a pure string predicate (assert `"EditorMarker"`, `"EditorMarker:12"`, `" editormarker "` are markers and `"DoorLeft:12"` is not); `drop_editor_marker_geometry` operates on a fixture `project_wormhole_nif::model::all::Model` (the fork's `fixture_model`/`static_mesh` helpers, no NIF files) and asserts the renumbering of surviving mesh indices.

**Which Marco:** M2 item 2 (alpha test/blend), M1 item 4 (an exclusion must not become a panic or a partial GLB) and the marker policy in `02-core-engine.md:59-61`. The blend-factor half is what M2 item 1's "regras explícitas por shader" is about for additive torch glows and multiplicative decals.

---

## 6. `0ece570` + `68c19bb` — terrain: repeating samplers, bilinear weights, seam weld

**Verdict: applies (all three parts).**

**(a) ClampToEdge on tiled layers.**
- `crates/engine/src/render.rs:219` — `let handle = asset_server.load(path.to_owned());` — no sampler settings, so Bevy's linear+clamp default applies.
- The shader tiles: `crates/engine/src/render.rs:232` (`tiling_and_layer_count: Vec4::new(8.0, 8.0, …)`) and `crates/engine/src/shaders/terrain.wgsl:50` (`let uv = in.uv * terrain.tiling_and_layer_count.xy;`), sampled at `terrain.wgsl:51-56`. Everything past the first tile is edge texels.

**(b) Wrong weight interpolation.**
- `crates/engine/src/shaders/terrain.wgsl:30-48` takes weights 1-3 from `in.world_tangent` (a direction plus its magnitude, which Bevy renormalizes between stages) and weights 4-5 from `uv_b`.
- The packing is in `crates/engine/src/streaming.rs:1425-1430` (the `overlay_weights` 17×17 grid is built from `layer.weights`, i.e. the VTXT opacities) and `:1461-1474` + `:1504-1505` (packed into `ATTRIBUTE_TANGENT` and `ATTRIBUTE_UV_1`). The correct data is already in the cache: `crates/engine/src/world/cache.rs:21-27` (`TerrainLayerSnapshot { …, weights: Vec<(u16, f32)> }`), produced by `crates/converter/src/esm/cell_cache.rs:189-227` (VTXT parsed and validated against 17×17).
- So the fork's fix (weights in the material uniform, bilinear `grid_weight` in WGSL) is portable engine-side with **no converter change**.

**(c) One-point seams reject a whole cell.**
- `crates/engine/src/streaming.rs:1511-1606` — `validate_and_register_terrain_edges` returns `Err("terrain edge does not match neighbor …")` when any point differs by more than `0.01` (`:1592-1601`), and the caller turns that into `CellStatus::Failed` (`:321-336`), dropping the cell, its terrain and its references.
- The fork's `68c19bb` welds the arriving cell onto the resident neighbour below a 512-unit delta and still rejects wrong-length edges, non-finite heights and larger deltas — its own diff shows upstream's exact code being replaced.
- Part of the same commit — no water plane on a dry cell (`terrain_reaches_water`) — replaces upstream's unconditional spawn at `streaming.rs:470-517` (`if let Some(height) = terrain.water_height.filter(…)`), which is M4 item 4's z-fighting/grid-lines defect.

**Upstream PR content.** (a) `render.rs` — `load_builder().with_settings(ImageLoaderSettings { sampler: ImageSampler::Descriptor(…) })` at `:219` plus a `bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor}` import (upstream's import block, `render.rs:5-20`, has no `image::` line). (b) `render.rs` (`TerrainSettings` grows `quadrant_origin`/`weight_source`/`weights[365]` — the fork's uniform is 369 `Vec4` ≈ 5.9 KB per material and every quadrant has its own material, so the PR should say so), `shaders/terrain.wgsl` (`WEIGHT_GRID_SIDE`/`WEIGHT_GRID_WORDS`, `packed_weight`, `grid_weight`, the `weight_source` branch), `streaming.rs` (`quadrant_overlay_weights` factored out of `:1425-1430`; the vertex packing stays as the fixture fallback, which upstream's `TerrainExtension::fixture` at `render.rs:241-255` and `TerrainExtension::default` at `:258-274` still need — the fork's `vertex_weights_only` exists for exactly that). (c) `streaming.rs` — `TerrainEdges::of`, `TerrainEdgeSide`, the weld in `validate_and_register_terrain_edges` (signature becomes `&mut TerrainSnapshot`), `terrain_reaches_water`, and the `terrain_seam_points_welded` metric. All three hunk-sets apply to upstream's file text (the fork's `68c19bb` context is upstream's code verbatim); (b) collides with nothing in `8b5bf80` (that commit touches `mesh.rs`/`pipeline.rs`).

**Schema impact: none** — no converter change; the weld edits the in-memory snapshot, not the cache.

**CI-safe test: yes, all three.** (a) assert the descriptor's address modes are `Repeat` (the fork's test), or load an image through an in-memory asset server and read `Image::sampler` back. (b) `weight_field`/`TerrainSettings::for_quadrant` over a fixture `TerrainLayerSnapshot` list, plus a cross-check that `WEIGHT_FIELD_WORDS`/`OVERLAY_WEIGHT_WORDS` equal the WGSL constants (the fork's tests do this with `TerrainLayerSnapshot` fixtures). (c) synthetic `TerrainSnapshot` + `TerrainContinuity`: a neighbour registered with a 24-unit one-point difference, assert the arriving cell's heights were welded, that >512 still fails, and that a dry cell gets no `WaterSurface` entity (the fork has tests in `streaming.rs`; `validate_and_register_terrain_edges` is a pure function of the snapshot and the continuity map).

**Which Marco:** M4 — item 1 (seams: "rejeitar … seams acima da tolerância" — welding below a tolerance is the same policy read correctly), item 2 ("Comparar os pesos enviados ao shader com os seis canais do cache e corrigir orientação de UV, escala, seleção de layer e mistura da camada base" — this is exactly (a)+(b)), item 4 (water).

---

## 7. `2b69616` — empty marker models must not fail the bounds gate

**Verdict: applies** — the gate is upstream's; only the trigger population is narrower upstream.

- `crates/engine/src/streaming.rs:942-944`, in `validate_spawned_transforms_and_bounds`: `let expected = expected.ok_or_else(|| { "converted model has no validated aggregate bounds; reconvert the asset".to_owned() })?;` The call is in the readiness scan at `:678-713`, and the failure path records the asset failure, hides the scene and increments `transform_bounds_validation_failures` (`:691-711`).
- Where a missing `ExpectedModelBounds` comes from: `crates/engine/src/streaming.rs:533-553` inserts it only `if reference.bounds_valid`; `crates/converter/src/integration.rs:102-117` sets `bounds_valid = 1` only when `MeshConverter::glb_bounds(path)` succeeds, and `crates/converter/src/mesh.rs:1180-1191` fails an empty scene with `ensure!(self.populated, "GLB contains no bounded POSITION accessor")` — leaving `statics.bounds_valid` at its `DEFAULT 0` (`crates/converter/src/esm/exporter.rs:55`, mirrored in the runtime `CREATE TABLE` at `crates/engine/src/app.rs:231`).
- Upstream *does* write empty scenes: `crates/converter/src/mesh.rs:87-101` (`ensure!(is_deferred_dynamic_mesh(nif_path) || no declared geometry block)` → `write_glb_atomic(output, &empty_scene_glb(&name))`), with `empty_scene_glb` at `:463-482` and the predicates at `:437-461`. So a `STAT` whose model is an empty scene is spawnable, unbounded, and fails the gate.
- The fork's *particular* trigger (marker shapes dropped by its own converter change) does not exist upstream — which is why the reachable population upstream is narrower. **I could not measure it** without running a conversion on game data.

**Upstream PR content.** `crates/engine/src/streaming.rs`: the `empty_model_references` metric, the early return in `track_asset_readiness` before `validate_spawned_asset` (upstream's call sites at `:678-742` are the hunk context), `TransformValidationSummary::empty_model`, the `expected == None` branch that counts `spawned_primitive_count(root, children, primitives)` and only fails when the spawned scene really holds a primitive, and the `spawned_primitive_count` helper. No signature change is needed: upstream already passes `children` and `primitives` into the function (`:932-935`). Hunks apply with only offset drift.

**Schema impact: none.**

**CI-safe test: yes** — the fork's `empty_model_app` builds a Bevy `App` with `AssetPlugin` + `WorldSerializationPlugin` and registers an empty `World` as the asset, then spawns a reference and runs the real `track_asset_readiness`; it asserts `empty_model_references == 1` and `transform_bounds_validation_failures == 0`, and that a scene *with* a primitive but no bounds still fails. No game data (in-repo `dummy-content` is not even needed).

**Which Marco:** M0 item 2 ("não agregar conteúdo não renderizável com falhas reais") and M5 ("zero falhas de streaming"); a false failure here also blocks M3/M6 (the readiness gate feeds `capture_acceptance_screenshot`'s readiness condition) and M7's screenshots.

---

## 8. `23f68f6` — a pruned dangling texture is not an incomplete conversion

**Verdict: applies, and it is a bug upstream introduced in `8b5bf80`.** On `main` a stock Skyrim install cannot produce a complete conversion, so the converter exits non-zero, the launcher never settles, and the engine refuses to start.

Upstream evidence:

- `crates/converter/src/pipeline.rs:294-330` — after the texture stage, `MeshConverter::prune_dangling_texture_uris(staging)` (the `8b5bf80` prune), and for **each** removed URI: `eprintln!("warning: pruned dangling texture …")` then `batch.record_skip(ProgressStage::Textures, …, eyre!("texture {uri} has no converted artifact; reference pruned"))`.
- `crates/converter/src/pipeline.rs:737-759` — `record_skip` inserts into `manifest.failures` (`:756`), pushes a warning (`:757`) and does `self.report.skipped += 1` (`:758`).
- `crates/converter/src/pipeline.rs:381` — `manifest.complete = report.skipped == 0 && report.warnings.is_empty();`.
- Downstream, that one boolean is three hard gates: `crates/converter/src/main.rs:99-105` (`bail!` → non-zero exit), `crates/launcher/src/handlers.rs:22-40` (assets count as converted only when `manifest.complete && schema == …`), `crates/engine/src/app.rs:1187-1200` (`ensure!(manifest.schema_version == converter_schema_version() && manifest.complete, …)` unless `--allow-incomplete-assets`), and the acceptance tooling: `scripts/phase2-acceptance.ps1:162-170` (preflight `conversion-complete`) and `scripts/phase2-visual-baseline.ps1:44-46`.
- Scale, from upstream's own commit message: "225 dangling URIs across 247 of 25,388 GLBs".

**Upstream PR content.** `crates/converter/src/pipeline.rs` — a `pruned_texture_references` counter on `PipelineReport`, `fn conversion_is_complete(&PipelineReport)`, the new async `record_pruned_texture_reference` (progress event + counter + manifest map entry, no warning, no `failures` insert), and the call-site swap at `:317-328`; `crates/converter/src/cache.rs` — `pruned_texture_references: BTreeMap<String, Vec<String>>` with `#[serde(default)]` on `ConversionManifest` (`:34-47`) and a test that a pre-existing manifest without the key still loads; `crates/converter/src/main.rs` — the summary line after `:96-98`. The hunks are upstream's own text verbatim (I diffed the fork's pre-fix side against `pipeline.rs:294-330`), so they apply cleanly; only the fork's schema-18 wording and its `docs/handoff/` file are fork-specific. **One review point to raise:** the fork keeps the progress message `"Asset skipped"` for pruned references, so any consumer keying on that message still counts them (upstream's own test at `pipeline.rs:1341` filters on exactly that string) — the PR should either give pruned references their own message or say why not.

**Schema impact: no bump and no reconversion.** The manifest is rewritten every run and the new key is `#[serde(default)]` (the fork's `manifests_written_before_pruned_reference_tracking_still_load` test proves old files load); the converter's *artifacts* (GLBs, world DB, cell cache) are unchanged. The one semantic output change is that pruned URIs no longer appear in `manifest.failures`, moving to `pruned_texture_references`.

**CI-safe test: yes.** `conversion_is_complete` is a pure predicate (pruned-only → complete; one skip → not; one warning → not), and the fork's `#[tokio::test] publishes_meshes_with_missing_textures_and_stays_complete` runs the real pipeline over `dummy_content::nif::StaticShape` fixtures with a deliberately absent normal texture into a temp dir — the in-repo `dummy-content` crate exists for exactly this. No game data.

**Which Marco:** M0 item 2 (non-renderable content must not be aggregated with real failures) and M6/M7 — without it the acceptance preflight's `conversion-complete` check can never pass on a real install, so no campaign can produce `accepted`.

---

## Proposed PR grouping and order

Two ordering constraints drive everything: (i) nothing can be accepted until real-asset conversions come out complete (candidate 8) and reachable models stop failing the bounds gate (candidate 7); (ii) schema bumps force a full GLB rebuild, so all publication changes should share one bump and one reconversion.

| # | PR | Candidates | Files | Bump |
|---|----|-----------|-------|------|
| 1 | `fix(converter): a pruned dangling texture does not make a conversion incomplete` | 8 | `converter/pipeline.rs`, `cache.rs`, `main.rs` | no |
| 2 | `fix(engine): an empty converted model is not a bounds failure` | 7 | `engine/streaming.rs` | no |
| 3 | `fix(converter): skyrim material publication — gloss exponent, vertex-alpha blend, blend factors, editor markers` (one PR, two or three commits) | 3 + 5 (+4 if done converter-side) | `converter/material.rs`, `mesh.rs`, `cache.rs`, `pipeline.rs`, `engine/app.rs`, 3 roadmap docs | **14→15** + one reconversion |
| 4 | `fix(engine): vertex-colour alpha is not opacity` | 4 (engine-side) | `engine/render.rs` | no |
| 5 | `fix(engine): terrain blending — repeating layer samplers and bilinear VTXT weights` | 6a + 6b | `engine/render.rs`, `shaders/terrain.wgsl`, `engine/streaming.rs` | no |
| 6 | `fix(engine): weld small terrain seams instead of rejecting the cell` | 6c (+ dry-cell water plane) | `engine/streaming.rs` | no |
| 7 | `fix(engine): size the sun's shadow cascades for Creation units` | 1 | `engine/app.rs` | no |
| 8 | `chore(engine): make entity commands on streamed entities fallible` (optional) | 2 | `engine/streaming.rs` | no |

Order: 1, 2 (unblock), then 3 (and run the reconversion once; 4 and 5/6/7 can be developed in parallel because they are engine-only), then 4, 5, 6, 7 in whatever order the maintainers want measured — 7 is the one that needs a benchmark before merge. Send 8 last, or fold it into 2's PR as a second commit with a comment explaining that no repro exists on `main` today.

## What I could not determine

1. **Bevy 0.19 internals.** I could not read the cargo registry (`C:\Users\Brannock\.cargo` is outside the permitted working directories), so: Bevy's default `CascadeShadowConfig` bounds (10/24.7/60.8/150) and `DirectionalLightShadowMap`'s default size (does the fork's explicit `size: 2048` insert change anything?), Bevy's `alpha_discard` path multiplying `vertex_color.a`, and `bevy_ecs`'s automatic sync-point insertion rule. The last one is load-bearing for candidate 2's verdict; I relied on the fork's recorded observation (`git show portal:crates/engine/src/model_animation.rs`, lines 917-935) and on the fact that upstream has exactly one despawner, ordered first in the chain.
2. **Reachability counts that need a real conversion run** (out of scope, and `local/` is off-limits): how many upstream references have `bounds_valid = 0` *and* an empty-scene model (candidate 7), how many statics reference `effects/`-category or no-geometry NIFs, and the per-cell seam distribution candidate 6c is fitted to. The fork's numbers (182 pruned meshes; 2 empty-model references in Blackreach; 24-unit and 16-unit seams) are its own measurements on its own data.
3. **Whether `MeshConverter::glb_bounds` is *the only* path that can leave `bounds_valid = 0`** for a model that really has geometry; I read `integration.rs:87-118` only.
4. **Candidate 4's converter-side alternative.** Whether Bevy widens a three-component `COLOR_0` to alpha 1.0 (which would make "publish the colour without alpha" a viable, bump-free variant if it rode the schema-15 PR). The fork's evidence is that the loader widens `COLOR_0` to `Float32x4`, which suggests `Vec3` is handled but I did not confirm the alpha it produces.
5. **Candidate 5's skeletal-mesh interaction** upstream: the fork's `drop_editor_marker_geometry` deliberately leaves skinned models alone ("their meshes are matched to source shapes positionally"); upstream exports skeletal meshes too (`crates/converter/src/mesh.rs:87`, `vendor/project-wormhole-nif/src/model/model.rs:468+`), and I did not verify that no reachable `STAT`/`MSTT`/`FURN` model is skeletal-with-markers.
6. **The fork's claim that its crash fired on `portal.rs` rather than in `streaming.rs`** is from its commit messages and handoff docs; I verified that `portal.rs` is fork-only and that the converted sites in `streaming.rs` are upstream's, but I did not reproduce the crash or read its logs.

No files were changed.

