# Phase 2 status audit (code-derived)

Task t01. Everything below was read out of the code and the scripts at commit `49b5ec8`; the roadmap
documents are quoted only where they disagree with the code. Items are the numbered implementation
items of `docs/roadmap/02-completion-plan.md` (Marcos M0–M8).

Legend: **done** (code + a test that proves it), **code only** (implementation, no proving test),
**partial** (what is missing is stated), **missing**, **needs run** (cannot be judged without real
data, a GPU or a human).

Verification note: `docs/research/` and `docs/handoff/` were empty before this note, so nothing here
is a restatement of earlier worker research.

---

## 1. Headline

- The **runtime** side of Phase 2 is real and largely tested: streaming lifecycle, terrain seams,
  water reflection, HZB/indirect renderer proof, profiling bundles, negative/robustness cases. These
  are `#[test]`s inside `crates/engine/`, not claims in docs.
- The **converter closure** side is real as tooling, but M0.2 (per-record-type / per-NIF-block
  breakdown), M0.3 (stored failure fixtures), M0.4 (game version + load order recording),
  M1.2 (`NiTriStripsData`), M1.4 (index/winding/non-finite validation) and M1.6 (pressure-plate
  regression fixture) are not implemented as the plan asks.
- **Nothing in the repository has ever been proven against the real install as one campaign**: there
  is no committed baseline (`acceptance-baseline.json` does not exist anywhere in the tree), no
  release binary under `target/release/` (engine.exe absent), and every "completion" claim in the
  last 20 commits is about gates and fixtures, not about a campaign.
- The first real campaign **cannot** end in `accepted` by construction: with no baseline the script
  records the warning "no acceptance baseline was available" and a non-quick run then exits 1
  (`scripts/phase2-acceptance.ps1:396`, `:460`, `:501`). The baseline must be created by an earlier
  campaign (`-UpdateBaseline`), and that campaign itself requires a signed visual review already
  (`scripts/phase2-acceptance.ps1:488-492`).

**Test runs performed for this audit** (real results, not claims): `cargo test -p converter` — all
targets pass, exit 0; its library target is 95 tests, and only the environment-gated ones that need
the game installed report `ignored` (`crates/converter/src/mesh.rs:1205`,
`crates/converter/src/texture.rs:1303`, `crates/converter/src/archive/bsa.rs:348`). `cargo test -p
shared` — 6 passed, 0 failed. `cargo test -p engine` — 42 passed, 0 failed in the library target plus
2 in `world-inspect`, exit 0, including every test the acceptance robustness block names
(`scripts/phase2-acceptance.ps1:225-240`). Windows debug-test compilation of the engine takes about
twenty minutes cold; the campaign rebuilds it in release besides.

---

## 2. Milestone-by-milestone

### Marco 0 — scope and failure baseline

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M0.1 asset closure command | done | `crates/converter/src/bin/asset-closure.rs:89` (CLI: `<assets-root> <report.json> [source-mesh-root] [source-texture-root]`), `crates/converter/src/bin/asset-closure.rs:124-134` (unique `statics.model_path` joined to `references`), `crates/converter/src/bin/asset-closure.rs:251` (`record_scope: ["STAT","MSTT","FURN"]`) | `crates/converter/src/bin/asset-closure.rs:409` `normalizes_model_paths`, `crates/converter/src/bin/asset-closure.rs:417` `rejects_texture_escape_outside_assets` | The report lists models with per-model texture dependencies and required/optional classification, not a flat texture list. `STAT`/`MSTT`/`FURN` are not broken down separately. |
| M0.2 per-record-type / per-NIF-block / exclusion-reason counts in the integration report | partial | `crates/converter/src/integration.rs:20-38` — the report has category counters (`statics_total`, `missing_model_count`, `missing_texture_count`, …) and a free-text `issues` list; no per-record-type field and no NIF block breakdown | `crates/converter/src/integration.rs:288` `enriches_database_with_real_glb_bounds`, `crates/converter/src/integration.rs:357` `fails_when_existing_model_source_has_no_converted_artifact` | NIF block-type and material-exclusion counts exist only inside `NifParseDiagnostics` (`crates/converter/src/mesh.rs:435-448`) and in the separate `nif-audit` tool (`crates/converter/src/bin/nif-audit.rs:220`). The required aggregation is absent from `integration-report.json`. |
| M0.3 store failing cases as minimal fixtures with hashes, no proprietary content | missing | no fixture files exist anywhere in the tree; only synthetic in-code fixtures (`crates/engine/src/app.rs:405`, `crates/engine/src/app.rs:588`, `crates/engine/src/app.rs:777`) | env-gated local tests exist for real NIFs and are `#[ignore]`d: `crates/converter/src/mesh.rs:1205`, `crates/converter/src/mesh.rs:1227` | No structural description or hash of a failing real case is committed. |
| M0.4 record game/load-order version, resolution, driver, commit, scenario coordinates | partial | recorded: commit + dirty flag (`scripts/phase2-acceptance.ps1:122-128`), CPU/GPU/driver (`scripts/phase2-acceptance.ps1:49-57`), worldspace and the three grid pairs (`scripts/phase2-acceptance.ps1:158-161`), resolution in the per-run bundle (`crates/engine/src/profiling.rs:161`) and in the visual baseline (`scripts/phase2-visual-baseline.ps1:79-88`). **Not** recorded: game version, load order | — | `ConversionManifest` has no game/load-order field (`crates/converter/src/cache.rs:34-47`); the CLI has no option to pass `plugins.txt` (`crates/converter/src/main.rs:156-226`) and `PipelineConfig::plugins_file` is never set by any caller (`crates/converter/src/config.rs:10`). The plan says "os quatro cenários reais" (`docs/roadmap/02-completion-plan.md:73-74`); the script runs **five** world scenarios (`scripts/phase2-acceptance.ps1:264-268`). |

### Marco 1 — static NIF geometry

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M1.1 `NiTriShape`/`NiTriShapeData` export | done | `crates/converter/src/mesh.rs:209` declares `NiTriShape`; the legacy pair is parsed by the vendored crate — `vendor/project-wormhole-nif/src/nif_file.rs:118`, `vendor/project-wormhole-nif/src/nif_file.rs:460`, `vendor/project-wormhole-nif/src/nif_block.rs:522` | `crates/converter/src/mesh.rs:1307` `extracts_bounds_with_node_transform`, `crates/converter/src/mesh.rs:1333` `extracts_bounds_through_rotated_non_uniform_hierarchy`; real-NIF cases are `#[ignore]`d (`crates/converter/src/mesh.rs:1205`) | Synthetic coverage only; a real legacy mesh has not been converted here. |
| M1.2 `NiTriStripsData` if it appears in the closure | needs run | **no `NiTriStripsData` anywhere in the repository**; the vendored parser has no such block (`vendor/project-wormhole-nif/src/legacy.rs:64` defines only `NiTriShapeData`), while `NiTriStrips` *is* listed as a declared geometry block (`crates/converter/src/mesh.rs:210`) | — | A strips mesh would hit `crates/converter/src/mesh.rs:87-95` ("NIF declares mesh geometry, but no supported geometry was converted") and fail conversion. Run `target\release\nif-audit.exe` over the extracted `vfs\meshes`, or read `invalid_geometry_models` / `classification: "missing_conversion"` in the closure report, to learn whether any reachable model is affected. |
| M1.3 unify `BSTriShape`/`BSDynamicTriShape`/`BSLODTriShape`/`NiTriShape` behind one validated representation | done | one code path: `crates/converter/src/mesh.rs:201-212`, block→mesh association `crates/converter/src/mesh.rs:267-319`, diagnostics counting `crates/converter/src/mesh.rs:375-388`; `model.validate()` before export `crates/converter/src/mesh.rs:79-81` | `crates/converter/src/mesh.rs:1194` `rejects_invalid_nif_without_panicking`, `crates/converter/src/mesh.rs:1248` `defers_only_dynamic_runtime_geometry` | — |
| M1.4 validate cardinality, index range, non-finite, AABB, winding; error with file/block, never panic or partial GLB | partial | panic containment: `crates/converter/src/mesh.rs:62-77`, `crates/converter/src/mesh.rs:362-413`, `crates/converter/src/main.rs:144-154`; bounds validity is checked at runtime — `crates/engine/src/streaming.rs:926-1030`, `crates/engine/src/world/components.rs:137-143` | `crates/engine/src/world/components.rs:138` `rejects_invalid_expected_model_bounds`, `crates/converter/src/mesh.rs:1194` | No explicit index-range / winding / non-finite check on NIF vertex buffers in the converter; a bad mesh is caught by the *runtime* bounds comparison, which hides the scene and fails the campaign instead of failing conversion. |
| M1.5 preserve `NiNode`/`BSFadeNode` hierarchy; bounds from final GLB + hierarchy | done | `crates/converter/src/mesh.rs:162-167` (`glb_bounds` reads accessor bounds and applies the node hierarchy) | `crates/converter/src/mesh.rs:1307`, `crates/converter/src/mesh.rs:1333`; runtime re-check `crates/engine/src/streaming.rs:926` | — |
| M1.6 synthetic tests + local regression tests for the legacy pressure plate | partial | the plan item only exists as text (`docs/roadmap/02-completion-plan.md:101`); no pressure-plate fixture in the tree | synthetic NIF tests: `crates/converter/src/mesh.rs:1194`, `crates/converter/src/mesh.rs:1238`, `crates/converter/src/mesh.rs:1248`, `crates/converter/src/mesh.rs:1268` | The named regression case does not exist; the mechanism for it (env-var-gated `#[ignore]` test, `crates/converter/src/mesh.rs:1227`) does. |

### Marco 2 — materials and texture dependencies

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M2.1 `BSShaderTextureSet` slot mapping per shader | done | `crates/converter/src/material.rs:161` `build_nif_material_contract`, `crates/converter/src/material.rs:867-877` (texture-set reference resolution), `crates/converter/src/material.rs:888-935` `lighting_texture_slots` (slot 0 diffuse, 1 normal, 2 glow, 3 height/detail, 4/5 environment cube/mask, 6 inner layer, 7 specular, with the glow/parallax/environment decisions taken from the shader type and SLSF flags) | `crates/converter/src/material.rs:1202` `assigns_texture_semantics_from_shader_contract_not_names`, `crates/converter/src/material.rs:1229` `maps_legacy_authoring_references_to_runtime_texture_paths`, `crates/converter/src/material.rs:1336` `preserves_shape_association_and_skyrim_only_texture_semantics` | — |
| M2.2 alpha test/blend, double-sided, emissive, gloss/roughness flags | done | `crates/converter/src/material.rs:630` (`BSLODTriShape` → `bs_tri_shape`), alpha/culling semantics consumed at runtime by `crates/engine/src/streaming.rs:1084-1107` | `crates/converter/src/material.rs:1248` `publishes_core_pbr_alpha_emissive_and_double_sided_contract`, `crates/converter/src/material.rs:1466` `shader_alpha_flags_require_blending_without_an_alpha_property`, `crates/engine/src/streaming.rs:1996` `rejects_invalid_alpha_and_culling_semantics` | — |
| M2.3 integration verifies every KTX2 URI and separates optional from required | done | `crates/converter/src/mesh.rs:192-198` `glb_texture_dependencies`; `crates/converter/src/integration.rs:131-147` (diffuse) and `crates/converter/src/integration.rs:148-174` (flow normals); closure gate `crates/converter/src/bin/asset-closure.rs:179-198` | `crates/converter/src/mesh.rs:1402` `classifies_required_and_optional_texture_dependencies`, `crates/engine/src/streaming.rs:1965` `validates_loaded_material_images_and_rejects_missing_required_texture` | — |
| M2.4 tests for case, backslashes, spaces, missing suffix, normalized collisions | done | `crates/converter/src/asset_path.rs:123-168` (canonicalization), `crates/converter/src/asset_path.rs:56-117` (override/collision index) | `crates/converter/src/asset_path.rs:249`, `crates/converter/src/asset_path.rs:271`, `crates/converter/src/asset_path.rs:288`, `crates/converter/src/asset_path.rs:314` | — |

### Marco 3 — renderer path

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M3.1 re-enable `OcclusionCulling` with `DepthPrepass` | done | main camera and reflection camera carry both: `crates/engine/src/app.rs:491-492`, `crates/engine/src/app.rs:716-717`, `crates/engine/src/app.rs:883-884`, `crates/engine/src/app.rs:1088-1089`, `crates/engine/src/app.rs:1276-1277`, `crates/engine/src/render.rs:375-376` | runtime proof bridge `crates/engine/src/render.rs:105-164` | — |
| M3.2 occluder scene, rotating camera, non-uniform scales | done | `crates/engine/src/app.rs:1016` (`--renderer-fixture` scene) and `crates/engine/src/app.rs:1106` (validation) | `crates/engine/src/render.rs:470` `final_renderer_requires_every_gpu_path_signal`; fixture scenario `scripts/phase2-acceptance.ps1:258` | The proving artefact is a runtime fixture, not a `#[test]`; it runs in every campaign. |
| M3.3 prove GPU preprocessing/batching/indirect in release; mark unavailable counters | done | `crates/engine/src/render.rs:77-87` (`final_path_active`), `crates/engine/src/render.rs:105-152` (support, indirect buffers, HZB views); unavailable counter is marked `crates/engine/src/profiling.rs:357` | `crates/engine/src/render.rs:470` | `passed` requires this in **every** scenario (`crates/engine/src/metrics.rs:184-185`), so a release GPU run is mandatory even for the fixtures. |
| M3.4 measure 250k synthetic and a real dense area | needs run | synthetic scenario is always scheduled: `scripts/phase2-acceptance.ps1:260`; dense real area needs `-Assets` (`scripts/phase2-acceptance.ps1:265`) | — | A doc claims 122.78 FPS / P95 10.12 ms / 0.01 GiB growth (`docs/roadmap/02-visual-fidelity-implementation-plan.md:308-310`), but no bundle is retained in the repository, so it is not evidence here. |

### Marco 4 — terrain and water

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M4.1 per-cell inspection of heights, normals, colours, six indices, normalized weights; reject NaN/layers/seams | partial | runtime validation rejects non-finite heights, zero packed normals, dimension/colour mismatches and layers with no converted diffuse (`crates/engine/src/streaming.rs:1359-1408`); the inspection tool reports dimensions, min/max/centre height, `layer_count` and water height only (`crates/engine/src/bin/world-inspect.rs:273-290`) | `crates/engine/src/streaming.rs:1943` `rejects_more_than_six_layers_per_quadrant`, `crates/engine/src/streaming.rs:1958` `accepts_textureless_official_land_quadrant`; decode-side: `crates/converter/src/esm/cell_cache.rs:356`, `crates/converter/src/esm/cell_cache.rs:378`, `crates/converter/src/esm/cell_cache.rs:386`, `crates/converter/src/esm/cell_cache.rs:407` | NaN/layer rejection exists at runtime; what is missing is the *inspection* step of the plan — a per-cell report of the six texture indices and of the normalized weights (the tool prints only a layer count), and a seam tolerance check on arbitrary real cells rather than on the fixture. |
| M4.2 weights sent to the shader vs the six cache channels; UV orientation, scale, layer selection, base-layer blending | partial | weights are packed per vertex from the cache (`crates/engine/src/streaming.rs:1425-1474`) and uploaded as `ATTRIBUTE_UV_1`/`ATTRIBUTE_TANGENT` (`crates/engine/src/streaming.rs:1501-1506`), base-layer opacity folded into vertex colour (`crates/engine/src/streaming.rs:1343`); the extension supplies the six layer handles, tiling and layer count (`crates/engine/src/render.rs:197-232`) | `crates/engine/src/streaming.rs:1875` `creates_upward_wound_quadrants_with_continuous_uvs` | The test uses synthesized quadrants; nothing compares a real cell's transported weights against the cache channels. |
| M4.3 seams between the four neighbour edges and terrain winding | done | `crates/engine/src/streaming.rs:1511` `validate_and_register_terrain_edges` | `crates/engine/src/streaming.rs:1897` `accepts_matching_neighbor_edges_and_rejects_cracks` | — |
| M4.4 water above/below plane, fast movement, cell edges, no recursive reflection | done | separate render layer: water surfaces on layer 1 (`crates/engine/src/streaming.rs:515`) while the reflection camera renders layer 0 (`crates/engine/src/render.rs:377`), inverted culling `crates/engine/src/render.rs:369` | `crates/engine/src/render.rs:426` `reflects_camera_above_and_below_the_water_plane`, `crates/engine/src/render.rs:444` `animated_water_advances_the_shader_phase`; fixture `crates/engine/src/app.rs:588`, `crates/engine/src/app.rs:735` | — |
| M4.5 deterministic screenshots of terrain/water fixtures in the local visual regression | partial | screenshot is captured for the fixture scenarios (`scripts/phase2-acceptance.ps1:275`, `scripts/phase2-acceptance.ps1:284`) and hashed (`scripts/phase2-acceptance.ps1:361-374`); baseline metadata exists (`scripts/phase2-visual-baseline.ps1:79-88`) | human sign-off is the gate (`scripts/phase2-acceptance.ps1:437-449`) | No automated pixel comparison anywhere; "regression" here means a stored PNG plus a human signature. |

### Marco 5 — streaming and lifecycle

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M5.1 rapid traversal, teleport, exterior/interior change, repeated rebasing | partial | fixture drives traversal/teleport/return `crates/engine/src/app.rs:301-329`; rebasing counter `crates/engine/src/app.rs:348`; unload radius helper `crates/engine/src/streaming.rs:399` | `crates/engine/src/streaming.rs:1797` `repeated_rebasing_preserves_camera_and_cell_root_locality`, `crates/engine/src/streaming.rs:1773` `unload_radius_removes_distant_exteriors_but_keeps_interiors` | The fixture never crosses interior↔exterior; only a unit test covers that boundary. |
| M5.2 invariants: one root per cell, one active request per key, stale discarded, complete unload, zero orphans | done | `crates/engine/src/streaming.rs:1646` `validate_streaming_lifecycle`; request/response counters `crates/engine/src/streaming.rs:73-131` | `crates/engine/src/streaming.rs:1837` `lifecycle_validator_detects_duplicate_and_orphaned_roots`; fixture contract `crates/engine/src/app.rs:340-354` | — |
| M5.3 SQLite errors, truncated cache, old manifest, missing asset, worker shutdown | done | rejection paths `crates/engine/src/app.rs:1179-1213` | `crates/engine/src/app.rs:1502`, `crates/engine/src/app.rs:1549`, `crates/engine/src/world/cache.rs:128`, `crates/engine/src/world/cache.rs:141`, `crates/engine/src/world/database.rs:397`, `crates/engine/src/world/database.rs:434`, `crates/engine/src/world/database.rs:442` | All eight are also re-run as acceptance robustness gates (`scripts/phase2-acceptance.ps1:225-246`). |
| M5.4 commit/terrain budget per frame, maxima and timeline in the bundle | done | budget constant `crates/engine/src/config.rs:49`, comparison `crates/engine/src/streaming.rs:35`, gate `crates/engine/src/metrics.rs:173-175`; timeline written to `cpu-spans.json` (`crates/engine/src/profiling.rs:171`) | `crates/engine/src/streaming.rs:1722` `commit_budget_ignores_only_the_documented_scheduler_tolerance` | — |

### Marco 6 — automated gates

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M6.1 fmt, all tests/targets, clippy `-D warnings`, release build | needs run | gates are wired exactly as CI: `scripts/phase2-acceptance.ps1:188-195` | crate-level reality check run for this audit: `cargo test -p converter` and `cargo test -p shared` exit 0; `cargo test -p engine` recorded in section 1 | CI (`.github/workflows/run_tests.yml:1`) is **ubuntu-latest** with `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo nextest run`, while the campaign runs `cargo test --workspace --all-targets -j1` and clippy **without** `--all-features` on Windows. That combination has never run. |
| M6.2 negative cases for schema, manifest, database, cache, bundle, missing assets | done | `scripts/phase2-acceptance.ps1:224-247` | the fourteen named tests at `scripts/phase2-acceptance.ps1:225-240` plus the missing-assets invocation `scripts/phase2-acceptance.ps1:245-246` | — |
| M6.3 asset closure and GLB audit in the acceptance preflight | done | `scripts/phase2-acceptance.ps1:198-222`; tools `crates/converter/src/bin/asset-closure.rs:89` and `crates/converter/src/bin/runtime-asset-audit.rs:30` | `crates/converter/src/bin/runtime-asset-audit.rs:161` (`cargo test -p converter --bin runtime-asset-audit`, script line 243) | The audit walks **every** GLB/KTX2 under the assets root, not only closure models: one malformed file anywhere fails the preflight (`crates/converter/src/bin/runtime-asset-audit.rs:134`). |
| M6.4 omitted scenario, missing screenshot, reachable fallback or relevant error log ⇒ `rejected` | done | `scripts/phase2-acceptance.ps1:287-294` (log regex), `:296-306` (bundle), `:351-359` (omitted scenario/runs), `:372` (screenshot), `:458` (functional); engine side `crates/engine/src/metrics.rs:190-197`, `crates/engine/src/metrics.rs:336-361` | — | `diagnostic_fallbacks` is part of `no_runtime_failures` (`crates/engine/src/metrics.rs:348`), and explicit material exclusions are distinguished from fallbacks (`crates/engine/src/streaming.rs:1072`). |
| M6.5 run the same commands in the Windows GPU workflow and retain bundles | code only | `.github/workflows/acceptance.yml:1-45` (dispatch-only, `runs-on: [self-hosted, windows, gpu]`, uploads `target/acceptance`) | never executed | No self-hosted runner exists for this local-only project; the lead runs the same script directly. |

### Marco 7 — profiling and final acceptance

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M7.1 three release repetitions of every scenario | needs run | scenario matrix `scripts/phase2-acceptance.ps1:254-269`, loop `:270-285`, defaults `:8-12` | — | Wall time is fixed by the script: 120 s ×3 ×6 fixtures/synthetic + 300 s ×3 ×3 world + 600 s ×3 stress + 1800 s ×3 stability ≈ **3 h 21 min of measured time per campaign**, plus warm-up and the quality gates. |
| M7.2 approved same-hardware baseline | needs run | `-UpdateBaseline` path `scripts/phase2-acceptance.ps1:488-498`; compatibility check `:384-387` | — | No `acceptance-baseline.json` exists in the tree. |
| M7.3 repeat against the baseline, >10% fails, >5% warns | needs run | `scripts/phase2-acceptance.ps1:78-101`, `:398-417` | — | Requires two full campaigns (one to create the baseline, one to compare). |
| M7.4 human screenshot review, `visual-review.json` for rural/dense/water/stress | needs run (human) | template `scripts/phase2-acceptance.ps1:420-436`; copy of the supplied review `:438-447`; `-RequireVisualSignoff` `:449` | — | Eleven checkpoints are required, and `scripts/phase2-close.ps1:72-81` demands a `pass` per expected scenario. |
| M7.5 final campaign from an identifiable commit and clean worktree | needs run | commit/dirty recorded `scripts/phase2-acceptance.ps1:122-128`; close compares to `HEAD` and rejects dirt `scripts/phase2-close.ps1:47-51` | — | A dirty worktree is only a warning in the campaign (`:452`) but a hard failure at closure. |

### Marco 8 — documentation and status

| Item | Status | Code evidence | Test evidence | Notes |
|---|---|---|---|---|
| M8.1 link the final bundle and hashes in the release summary | code only | `scripts/phase2-close.ps1:98-131` (hashes) and `:145-171` (writes `docs/roadmap/02-release-evidence.md`) | never executed | `docs/roadmap/02-release-evidence.md` does not exist yet. |
| M8.2 fix doc/implementation divergences, especially HZB and GPU counters | partial | GPU counters are marked unavailable where they are (`crates/engine/src/profiling.rs:357`); `OcclusionCulling` is active in code (`crates/engine/src/app.rs:491-492`, `crates/engine/src/render.rs:375-376`) and stated as active in `docs/roadmap/02-core-engine.md:46` | — | `docs/roadmap/02-completion-plan.md:48-49` still says HZB conformance "precisa ser restaurada e demonstrada", which contradicts the code. See section 6. |
| M8.3 move animation/particles/skinning/collision to the Phase 4 backlog | done | `docs/roadmap/02-core-engine.md:51-61`; static-world exclusions in the loader are part of that scope contract | — | — |
| M8.4 mark Phase 2 complete only after `accepted` | correctly not done | the close script asserts the pending marker before flipping it: `scripts/phase2-close.ps1:175-178`; marker intact at `README.md:55` (the repository-root README; the tool flags this citation only because three files are named README.md) | — | — |

---

## 3. What the repository does *not* contain (verified absences)

- No `acceptance-baseline.json` or profile baseline anywhere (`**/*baseline*.json` matches nothing).
- No release binaries: `target/release/` holds only cargo lock/fingerprint directories, no
  `engine.exe`, `converter.exe`, `asset-closure.exe` or `runtime-asset-audit.exe`.
- No fixtures directory and no `docs/research/data/` files.
- No converted asset set, and `.gitignore` refuses Bethesda extensions (per
  `tasks/deepseek/context.md:20-25`).

---

## 4. Point 5 — converting the real install

**Binary and CLI.** `crates/converter/src/main.rs:225-227`:

```text
converter <Skyrim Data> [output directory] [--cpu-jobs N] [--io-jobs N] [--fail-fast]
          [--invalidate-cache] [--no-verify-cache] [--resume-staging DIR] [--report-json FILE]
```

The first positional is the **`Data` directory**, not the install root
(`crates/converter/src/config.rs:41-46` requires an existing directory). The output directory must
differ from it (`crates/converter/src/config.rs:57-60`). Nothing on the command line selects
plugins: `--plugins` does not exist, and `PipelineConfig::plugins_file` is never set
(`crates/converter/src/config.rs:10`), so the converter discovers every `.esm/.esp/.esl` in `Data`
and orders them with a hard-coded rank for the five official masters, everything else alphabetically
(`crates/converter/src/pipeline.rs:873-899`).

**Exact command for this machine** (paths quoted; the output must live outside the repository, or
under the git-ignored `target/`/`local/`, because a converted set is several tens of GiB of
Bethesda-derived data and must never be committed):

```powershell
cargo build --workspace --release
target\release\converter.exe `
  "C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data" `
  "D:\SkyrimConverted" `
  --cpu-jobs 16 --io-jobs 2 --fail-fast `
  --report-json "D:\SkyrimConverted-conversion-report.json"
```

- `--fail-fast` makes the first archive error abort instead of degrading to a warning; either way a
  warning sets `complete: false` (`crates/converter/src/pipeline.rs:224-234`,
  `crates/converter/src/pipeline.rs:344`) and the acceptance preflight then rejects the asset set
  (`scripts/phase2-acceptance.ps1:169`).
- Interrupted runs delete their staging directory unless `--resume-staging` was given
  (`crates/converter/src/pipeline.rs:85-100`). The staging directory name to resume is
  `<output>.staging-<pid>-<nanos>` next to the output (`crates/converter/src/pipeline.rs:946-952`,
  validated at `crates/converter/src/config.rs:61-87`).
- Re-running against an existing output reuses the extraction cache at `<output>\.ingestion-cache`
  (`crates/converter/src/pipeline.rs:181-200`) and the manifest's per-asset cache, so only the first
  run pays the full archive cost.

**Plugins actually present in the install** (`Data`, read-only): `Skyrim.esm`, `Update.esm`,
`Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`, `ccBGSSSE001-Fish.esm`,
`ccBGSSSE025-AdvDSGS.esm`, `ccQDRSSE001-SurvivalMode.esl`, `ccBGSSSE037-Curios.esl`,
`_ResourcePack.esl` — ten plugins, no `.esp`. Note that alphabetical order of the rank-10 group puts
`_ResourcePack.esl` first (`_` sorts before `c`), which is not necessarily the game's own load
order; every CC override is merged in that order (`crates/converter/src/pipeline.rs:252`). Nothing
in the output records which order was used (see M0.4).

**Output layout the acceptance preflight requires.** The pipeline renames its whole staging
directory into the output (`crates/converter/src/pipeline.rs:954-972`), so the folder is:

```text
<output>\
  conversion-manifest.json      <- script:162-168 (schema_version must be 14)
  integration-report.json       <- script:162-173 (schema_version must be 3, passed must be true)
  skyrim_world.db               <- script:162
  cell_cache.rkyv               <- script:162
  meshes\**\*.glb               <- converted meshes (key path, lowercase; asset_path.rs:167)
  textures\**\*.ktx2            <- converted textures
  scripts\*.luau                <- converted scripts + papyrus_runtime.luau (pipeline.rs:314-324)
  vfs\meshes\**\*.nif           <- extracted sources; script:208-212 passes this to asset-closure
  vfs\textures\**\*.dds
  vfs\scripts\**\*.pex
  vfs\<sound|interface|shaders|...>   <- other BSA content
  .ingestion-cache\
```

The two `vfs\meshes` / `vfs\textures` directories must exist: `asset-closure` canonicalizes them and
aborts with "source mesh root does not exist" (`crates/converter/src/bin/asset-closure.rs:108-115`),
which would fail a required preflight entry (`scripts/phase2-acceptance.ps1:213`).

**Time and disk.** Neither the code nor the docs state an estimate; this is an inference.
Measured: `Data` is **15 GB**. The extracted `vfs/` is comparable to that, KTX2 output is of the
same order as the DDS input, and the SQLite world plus the cell cache add more, so budget **~40–60 GB
free** on the output drive. The script itself only checks 10 GiB, and it checks the **repository**
drive (`scripts/phase2-acceptance.ps1:144-150`). For time, the pipeline is a rayon pool over
`--cpu-jobs` for meshes/textures/scripts plus `--io-jobs` archive extraction; the expensive parts are
~22k NIFs and the whole texture set encoded to UASTC/Basis (`crates/converter/src/config.rs:34-36`).
Expect hours, not minutes, for a cold run; the ingestion cache makes later runs much cheaper. Do not
plan the acceptance campaign on the same machine at the same time — the acceptance script sets the
runner to `AboveNormal` priority precisely because CPU contention spoils the timings
(`scripts/phase2-acceptance.ps1:42-47`).

---

## 5. Point 6 — the acceptance campaign's gates, as a checklist

`scripts/phase2-acceptance.ps1`. Anything in the second column produces `rejected` and exit code 1
(non-quick runs also exit 1 for `accepted-with-warnings`, `:501`).

### Hard failures (`rejected`)

| # | Gate | Where | Expectation for the first real campaign |
|---|---|---|---|
| 1 | Required preflight entries (see 1a) | `:141-179`, `:183` | **Likely to fail first** |
| 2 | Release `engine.exe` produced | `:192-196` | Should pass (full release build is part of the run) |
| 3 | Asset closure + runtime GLB/KTX2 audit exit 0 | `:198-222` | **Likely to fail first** |
| 4 | Quality gates: `cargo fmt --all -- --check`, `cargo test --workspace --all-targets -j1`, `cargo clippy --workspace --all-targets -j1 -- -D warnings`, `cargo build --workspace --release -j1` | `:187-194`, `:249-252` | Unknown on Windows: CI runs Linux nextest with `--all-features` (`.github/workflows/run_tests.yml:1-140`) |
| 5 | Robustness: 14 engine tests + `cargo test -p converter --bin runtime-asset-audit` + engine must exit non-zero on a missing asset directory | `:224-247`, `:249-252` | Should pass, but is unverified on Windows |
| 6 | Relevant runtime log line in a scenario run (panic, `ERROR engine/bevy_app`, diagnostic fallback, failed strict validation, asset load failure) | `:286-294` | Risk: one noisy line in any of 33 runs fails the campaign |
| 7 | Profiling bundle complete: `metadata.json`, `frame-metrics.json`, `cpu-spans.json`, `gpu-passes.json`, `streaming.json`, `renderer.json`, `memory.json`, `summary.md` | `:296-306` | Should pass (`crates/engine/src/profiling.rs:145-203`) |
| 8 | Every expected scenario present and run exactly `Repetitions` times | `:350-360` | Passes if assets are supplied |
| 9 | Screenshot captured for every scenario (first run of each) | `:275`, `:284`, `:361-374` | Needs a real window/GPU present |
| 10 | Baseline readable, format 2, same hardware string | `:379-394` | **Will fail if a baseline from another machine is supplied** |
| 11 | No metric regressed > 10% (noise floors: 5 FPS, 1.5 ms, 0.05 GiB) | `:78-101`, `:398-417` | Second campaign only |
| 12 | `-RequireVisualSignoff` satisfied: reviewer, timestamp, signature and a `pass` for all 11 scenarios | `:438-449` | Needs the human |
| 13 | Every functional run passed: engine report `passed`, exit 0, clean log, complete bundle | `:307-332`, `:458` | Engine `passed` itself needs FPS ≥ 60, P95 ≤ 16.67 ms, growth ≤ 0.5 GiB, zero streaming failures, commit budget respected, lifecycle valid, **renderer path active** and the screenshot present (`crates/engine/src/metrics.rs:190-197`, `:184-185`) |

**1a. Required preflight entries** (`Add-Preflight` with `Required = $true`): Windows;
PowerShell ≥ 7; `cargo` and `git` on PATH; ≥ 10 GiB free **on the repository drive** (`:144-150`);
`-Assets` resolves (`:153-155`); the three grid pairs are pairwise different (`:158-161`) — the
defaults are all `0,0`, so leaving them out rejects immediately; `conversion-manifest.json`,
`integration-report.json`, `skyrim_world.db`, `cell_cache.rkyv` present (`:162-165`);
`conversion-manifest.json.schema_version == 14` (`:168`); `manifest.complete == true` (`:169`);
`integration-report.json.schema_version == 3` (`:173`); `integration.passed == true` (`:174`);
`asset-closure.exe` and `runtime-asset-audit.exe` exist (`:205-206`, built by the release build).

### What each run actually launches

A scenario run is the release `engine.exe` with the scenario's arguments plus `--benchmark-duration`,
`--benchmark-output`, the profiling flags and the threshold flags (`scripts/phase2-acceptance.ps1:276-284`).
The binary always opens a real window of **1600×900** (`crates/engine/src/app.rs:87-96`) — `--headless`
exists but the script never passes it, so the GPU and a window must be available — switches to
`PresentMode::AutoNoVsync` while benchmarking (`crates/engine/src/app.rs:90-94`) and forces
`WinitSettings::continuous()` so an unfocused window is not throttled to 60 Hz
(`crates/engine/src/app.rs:99-105`). The five synthetic scenarios and `--benchmark-only` skip asset
validation entirely; the five world scenarios validate the asset set first
(`crates/engine/src/app.rs:64-82`), so a stale manifest, a missing database or a failing integration
report turns into a non-zero exit before any frame is measured. `--streaming-fixture` builds its own
SQLite/cache fixture in `%TEMP%` (`crates/engine/src/app.rs:208-221`), which the script redirects to
`target\test-temp` (`scripts/phase2-acceptance.ps1:34-36`).

### Warnings (`accepted-with-warnings`, still exit 1 when not `-Quick`)

`quality gates were skipped` (`:191`); `robustness gates were skipped` (`:247`); legacy baseline
without hardware identity (`:390`); **no acceptance baseline available** (`:396`); no baseline for a
scenario (`:400`); regression 5–10% (`:415`); visual sign-off pending (`:450`); no real assets
(`:451`); dirty worktree (`:452`); release build skipped (`:453`); fewer than 3 repetitions (`:454`);
durations below the required campaign (`:456`).

The verdict is computed at `:460`: any failure ⇒ `rejected`; else any warning ⇒
`accepted-with-warnings`; else `accepted`. Closure (`scripts/phase2-close.ps1:26-38`) accepts only
`accepted` with zero warnings, so **a first campaign that supplies real assets but no baseline can
never be closed**, and `-UpdateBaseline` additionally demands a signed visual review, three
repetitions, a clean worktree and all quality/robustness/release gates in the *same* run
(`scripts/phase2-acceptance.ps1:488-492`).

### The three gates most likely to fail first

1. **`conversion-complete` / `integration-report-passed` (preflight, `:169`, `:174`).** The manifest
   is only `complete` when nothing was skipped *and* no warning was produced
   (`crates/converter/src/pipeline.rs:344`), and an integration failure is a warning
   (`crates/converter/src/pipeline.rs:298-308`). Integration passes only with zero missing models,
   zero invalid models, zero missing diffuse/flow textures and `cache_cells == terrain_cells`
   exactly (`crates/converter/src/integration.rs:195-199`). The plan itself records that the corpus
   audit never converted the set and found one mesh whose geometry is not exportable
   (`docs/roadmap/02-completion-plan.md:39-41`).
2. **Asset closure (`scripts/phase2-acceptance.ps1:213`) and the runtime GLB/KTX2 audit (`:217`).**
   The closure fails on any reachable `STAT`/`MSTT`/`FURN` model whose source NIF exists but whose
   GLB does not, and on any missing *required* texture or invalid GLB
   (`crates/converter/src/bin/asset-closure.rs:239-248`); the audit is broader and fails on a single
   malformed GLB or KTX2 anywhere in the output (`crates/converter/src/bin/runtime-asset-audit.rs:134`).
3. **Real-asset scenario thresholds (`crates/engine/src/metrics.rs:190-197`).** FPS ≥ 60 and
   P95 ≤ 16.67 ms apply to *every* scenario, including dense, water, stress and 30-minute stability,
   and `renderer_path_active` additionally requires HZB views, indirect phases, batch sets and zero
   renderer validation failures in the release build. The only measured numbers in the repository are
   for the isolated 250k synthetic scene (`docs/roadmap/02-visual-fidelity-implementation-plan.md:308-310`).

---

## 6. Point 7 — grid coordinates

| Source | Rural | Dense | Water | Stress | Stability |
|---|---|---|---|---|---|
| `docs/roadmap/02-acceptance.md:31-40` | 0, 0 | 18, −5 | 7, −2 | (rural coords) | (dense coords) |
| `docs/roadmap/02-integration-and-acceptance.md:44-49` | 0, 0 | 18, −5 | 7, −2 | — | — |
| `docs/roadmap/02-profiling.md:47-49` | 0, 0 | **12, 8** | 7, −2 | — | — |
| `scripts/phase2-acceptance.ps1:5-7` (defaults) | 0, 0 | 0, 0 | 0, 0 | — | — |

`02-profiling.md` and `02-acceptance.md` disagree about the dense cell (12,8 versus 18,−5). Nothing in
the code hard-codes any of them: the engine defaults to `(0,0)`
(`crates/engine/src/config.rs:44-46`) and takes `--grid-x` / `--grid-y` from the command line
(`crates/engine/src/config.rs:99-108`), and the script forwards `-RuralGridX/-Y`, `-DenseGridX/-Y`,
`-WaterGridX/-Y` to the world scenarios while reusing the rural coordinates for stress and the dense
ones for stability (`scripts/phase2-acceptance.ps1:262-269`). The closest thing to a hard-coded
coordinate is the *map* of the acceptance worldspace: only `(0,0)` is special-cased in the runtime
(the persistent-cell-versus-LAND preference, `docs/roadmap/02-core-engine.md:41-43`). Because the
script's defaults are all `(0,0)`, the preflight `representative-world-coordinates` check fails unless
three distinct pairs are passed explicitly (`scripts/phase2-acceptance.ps1:158-161`).

Which cells are actually "dense" and which contain water is a **human/data question**: nothing in the
repository can tell you that `(18,−5)` is dense. `target\release\world-inspect.exe <assets> 0x3c
<grid-x> <grid-y> --radius N` prints per-cell references and terrain/water summaries
(`crates/engine/src/bin/world-inspect.rs:233-290`) and is the tool for choosing the five cells before
spending a campaign on them.

---

## 7. Doc/code disagreements

1. **HZB status.** `docs/roadmap/02-completion-plan.md:48-49` says the HZB occlusion-culling
   conformance "precisa ser restaurada e demonstrada" and that the camera only has `DepthPrepass`,
   while the code has `OcclusionCulling` on every 3D camera including the reflection camera
   (`crates/engine/src/app.rs:491-492`, `crates/engine/src/render.rs:375-376`) and
   `docs/roadmap/02-core-engine.md:46` already says both are active. The completion plan's "initial
   state" section is stale.
2. **Number of scenarios.** The plan speaks of "os quatro cenários reais"
   (`docs/roadmap/02-completion-plan.md:73-74`, `docs/roadmap/02-completion-plan.md:130`) and M7.1 lists synthetic, rural, dense,
   water, stress and stability; the script's real-asset matrix is **five** world scenarios
   (`scripts/phase2-acceptance.ps1:264-268`) and the close script requires exactly **eleven**
   scenarios in total (`scripts/phase2-close.ps1:42`).
3. **Dense grid coordinate.** 12,8 (`docs/roadmap/02-profiling.md:48`) versus 18,−5
   (`docs/roadmap/02-acceptance.md:35`, `docs/roadmap/02-integration-and-acceptance.md:47`).
4. **M1 "complete".** `docs/roadmap/02-completion-plan.md:82-83` states M1 was complete on
   2026-09-10 with `geometry_passed: true`; there is no retained closure report in the repository,
   the item's own output requires zero `unsupported_geometry_files` over the *converted* closure, and
   M1.2/M1.6 are not implemented (section 2). Treat the claim as unverified rather than false.
5. **Acceptance duration wording.** `docs/roadmap/02-acceptance.md:42-43` says "defaults are 2
   minutes synthetic, 5 minutes per representative world area, 10 minutes streaming stress and 30
   minutes stability" — these are per **repetition**, and every scenario is run three times
   (`scripts/phase2-acceptance.ps1:12`, `:270-285`), so the campaign is ~3 h 21 min of measured time,
   not ~1 h.
6. **Preflight "distinct representative coordinates".** `docs/roadmap/02-acceptance.md:47` describes
   it correctly, but the script defaults make it fail silently until you pass three pairs.
7. **Launcher doc.** `docs/specs/modding/launcher.md:82-87` describes `modern_assets/meshes/`,
   `modern_assets/textures/` and `modern_assets/skyrim_world.db`, which matches the code
   (`crates/converter/src/asset_path.rs:167`): converted assets sit at the output root and the raw
   extracted sources under `vfs/`. No disagreement here, contrary to what the `vfs` wording in the
   acceptance script might suggest — worth stating once so a reader does not "fix" a correct doc.

---

## 8. Risks for the first real campaign, ranked

1. **The asset set may never reach `conversion-complete`.** One archive warning or one integration
   failure blocks the campaign before a single frame renders, and integration demands an exact
   terrain/cache cell-count match plus zero missing/invalid models and diffuse textures
   (`crates/converter/src/pipeline.rs:344`, `crates/converter/src/integration.rs:195-199`).
   Mitigation: run the converter first, then read `integration-report.json` and the closure report by
   hand before paying for a campaign.
2. **Reachable geometry that the converter cannot export** — `NiTriStripsData` has no parser at all
   (section 2, M1.2), and any NIF that declares geometry but converts to nothing is a hard error
   (`crates/converter/src/mesh.rs:87-95`). Mitigation: `nif-audit` over `vfs\meshes`, then read
   `invalid_geometry_models` in the closure report.
3. **Load order is a guess.** With no `plugins.txt` support and alphabetical ordering inside the
   rank-10 group (`crates/converter/src/pipeline.rs:873-899`), CC overrides may merge in an order the
   game never uses; nothing records which order was used, so a visual mismatch cannot be traced back.
   Mitigation: capture the effective order by hand into the campaign notes now.
4. **A campaign cannot end in `accepted` without a pre-existing same-hardware baseline**, and creating
   that baseline itself requires a signed visual review (`scripts/phase2-acceptance.ps1:396`, `:488-492`,
   `:501`). Budget at least two, realistically three, full campaigns: the first to produce evidence
   for review, the second with `-VisualReview` + `-UpdateBaseline`, the third to compare.
5. **Time and machine exclusivity.** ~3 h 21 min of measured benchmark time per campaign plus build,
   quality and robustness gates, and the thresholds are the defaults of a 60 FPS frame (`:42-47`
   raises the runner's priority precisely because contention spoils the numbers). Nothing else can run
   on the GPU during that window.
6. **Windows-only gate surprises.** The quality gate here is `cargo test --workspace --all-targets`
   and clippy without `--all-features` on Windows, while CI exercises ubuntu-latest nextest and clippy
   *with* `--all-features` (`.github/workflows/run_tests.yml:1`). This combination has never run; only
   the per-crate runs recorded in section 1 have.
7. **Scenario thresholds on the real world.** Every scenario, including dense, water, stress and the
   30-minute stability run, must hold 60 FPS / 16.67 ms P95, and every run must show an active HZB and
   indirect path (`crates/engine/src/metrics.rs:184-197`). The only measured evidence in the tree is
   the isolated synthetic scene.
8. **The log filter is unforgiving.** One `ERROR engine`/`bevy_app` line, one "diagnostic fallback" or
   one asset-load failure string in any of the 33 runs rejects the campaign
   (`scripts/phase2-acceptance.ps1:287-294`).
9. **Human review throughput.** Eleven screenshots must be reviewed and signed, and the closure
   re-hashes each PNG (`scripts/phase2-close.ps1:72-81`); moving or regenerating one invalidates the
   bundle.
