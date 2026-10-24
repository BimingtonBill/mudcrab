# Cluster mode without stock stand-ins (#2143)

## Scope and source

The user decision of 2026-10-24 supersedes the complementary stock design. A cluster-owned primitive must never have a stock render body in the main world. Refused primitives remain stock permanently. Loading admission requires drawable clusters for all required camera, prepass, and shadow consumers. Runtime invalidation is an observable defect, never permission to restore stock.

This work starts at integration head `fffd75da07e17db4889f8a03d3fa30305fc6d6f9`, in `D:/Mudcrab/worktrees/no-stand-ins`, branch `phase2/cluster-no-stand-ins`. The shader-invalidation readiness fix `6451ae30a` is retained as `b31276f9e`; integration handoff history is preserved. The source-only queued-carrier proof from `d810b58f9` is reused independently of its stock complement. The payload-format bump `dbb5a72a2` is not taken.

All heavy Cargo commands use build-gate owner `sol-2143` and `D:/Mudcrab/builds/no-stand-ins-target`. No engine/GPU run, publication, or deletion of another task's folders is authorized.

## Ownership and readiness design

Initial source/material qualification fixes ownership for the certified package. An unsupported material feature can become permanent stock coverage. Loader failures, source edits, shader reloads, pipeline failures, and later qualification changes cannot turn an owned primitive into stock.

Loaded references retain their gameplay hierarchy and source mesh handles. Reference-specific scene templates remove owned render components before `WorldAssetRoot` instancing. Bounds validation and legacy render-proxy collision read retained source geometry independently of renderer queries. Distant partial templates select only permanently refused ordinals; their exact affine flattening and dynamic ancestry remain.

The owned-object ClassFilter application, stock restoration, joint hidden-set intersection, complementary instance-mask overlay, and BatchDrawability lifetime seam are obsolete. ClassFilter itself remains necessary for native LOD and terrain handoff. Terrain clipping and refused-source identity validation also retain their existing jobs.

Readiness combines the actual queued carrier, exact material/bind-group state, compute dependencies, compiled forward/prepass/shadow pipelines, and every applicable view. Missing views block admission. Producer-owned mutation revisions replace settled material and pipeline scans. The main-world loading gate consumes changed snapshots scoped to the package, layout, and transition generation. `cluster/owned_not_drawable` exposes failures during play; publishing failure never creates stock.

## Implementation map

- `crates/engine/src/cluster/ownership.rs:331` freezes terminal decisions per certified package. Requalification, withdrawal, and same-package reset retain those decisions. New certificate allocations replace the index and terminal facts.
- `cluster/ownership_template.rs:33` clones the complete gameplay hierarchy but grants render components only to the fixed refusal allowlist. Unknown source ordinals retain source/gameplay data without acquiring render bodies. `:128` routes ordinary loaded arming through this filter. Source/clone errors use a generated empty scene plus original-source failure accounting.
- `crates/engine/src/streaming.rs:4750` reads either live render primitives or embedded nonrender source geometry through one SystemParam. Complete source bounds, materials, and legacy render-proxy collision remain available.
- `streaming/partial_stock.rs:241` instantiates only refused distant primitives, retaining exact affine flattening and skin/morph dependencies. `:338` promotes through the same fixed allowlist. `streaming/far_stock.rs:345` observes terminal revisions and `:415` projects frozen material refusals with full static scenery reach. Generic runtime feedback cannot extend that reach.
- `cluster/handoff.rs:2809` keys readiness with exact queued proof, source/material events, shader invalidation, and mutation-owned pipeline/material/binding revisions. Camera prepass requirements come from requested camera components even when a required phase is absent (`:3682`). Batch material tables are built only on readiness misses.
- `cluster/render.rs` queues both directional and point/spot shadow pipeline families before examining light views. `cluster/shadow_warmup.rs` tracks the exact scene/material/layout plan and checks compiled IDs only after that plan or pipeline availability changes. The loading readiness key includes its proof; a later-visible local light cannot trigger the first compilation of a needed cutout variant.
- `cluster/readiness_bridge.rs:60` aggregates only changed consumer identities/revisions. Missing cameras and local shadow consumers block. Local lights without an authored world inherit only a unique dependent camera world; ambiguous worlds remain missing. `:406` publishes guarded snapshots and `cluster/owned_not_drawable`.
- `cluster/loading.rs:49` owns package/layout/generation admission and explicit teleport covers. Startup (`pacing/startup.rs:216`), first-ready (`pacing.rs:538`), load doors, and seamless admission consume it. Collision placement and camera follow progress under explicit covers; the final pose needs fresh proof. A loading timeout restores pacing but cannot grant cluster readiness.
- `app.rs:591` always requests the existing whole-package batch preload in cluster mode. `config.rs:2080` rejects disabling the required cluster prepass; parsed cluster configurations normalize preload on (`:2090`). No visibility-time stock fallback or runtime recovery cover is installed.
- Vendored mutation revisions are exposed by `vendor/bevy_render-0.19.0/src/render_resource/pipeline_cache.rs:256`, `vendor/bevy_render-0.19.0/src/erased_render_asset.rs:214`, and `vendor/bevy_pbr-0.19.0/src/material_bind_groups.rs:486`. They are scoped to their owning cache/collection/allocator and add no scans.

Removed baseline machinery includes `cluster/batch_drawability.rs` (former lifetime seam at baseline line 13), ownership hide/restore observers (`ownership.rs:420`, `:506`, `:513` at `b31276f9e`), the object ClassFilter/complement consumer (`handoff.rs:3619`, `:4116`, `:4287` at that baseline), and resident owned fallback selection/initialization in `streaming/partial_stock.rs:88`. Its respawn and resident-body telemetry are removed. Native LOD and terrain handoff still use ClassFilter; the module remains for those consumers.

## Structural entity projection

The read-only projection uses `C:/Modding/Mudcrab/SkyrimConverted-main28` and `derived/clusters-d4ca7334b3e2a58e/clusters.db`, worldspace 60, center `(5,-12)`, radius 2. The target-local `projection.py` and `projection.json` retain the executable recipe and receipt.

The recipe reproduces #2139's 143,848 resident-body projection exactly. Removing owned bodies gives **81,230 entities**:

| Population | Entities |
| --- | ---: |
| Partial placement roots | 21,479 |
| Permanent refused primitives | 25,388 |
| Wholly refused full hierarchies | 29,413 |
| Shared distant cell holders | 4,795 |
| Dynamic authored ancestors | 145 |
| Synthetic roots for dynamic branches | 10 |
| Total | **81,230** |

The all-flat refused floor is 81,075. The retained 62 dynamic refused draws require the additional 155 ancestors/roots. The projection removes 62,618 entities from the resident-body result: 62,609 owned draws and nine no-longer-needed ancestors. Its full-hierarchy reference total is 298,953 including the same holders. The source/coverage comparison covers 25,525 references and 696 GLBs, with zero SHA-256 or default-scene ordinal mismatches.

These are structural projections under successful loading, excluding near owners, unrelated engine entities, cluster carriers, and temporary instancing hierarchies. They are not runtime entity counts, draw-call counts, timing, visual acceptance, or FPS evidence.

## Baked LOD dependency findings

The following references describe the pre-change integration source at `b31276f9e`. This task reports these dependencies; it does not implement launcher/converter behavior or remove native terrain ownership.

- Owned mode already suppresses baked object block drawing at `crates/engine/src/streaming/lod_objects.rs:777` and tree billboards at `crates/engine/src/streaming/lod_trees.rs:376`.
- Native inventory still opens at `lod_objects.rs:597`, reads the conversion manifest and native tables, and hashes owned billboard files at `:625`, `:656`, and `:667`. The object planner polls it before ownership suppression at `:734`. Inventory failure can therefore unnecessarily block cluster startup.
- Native index reads include `lod_grid`, object `lod_block`, tree types, and tree block locations in `crates/engine/src/world/object_lod.rs:491`, `:544`, `:666`, and `:704`.
- Terrain LOD remains active: database queries at `crates/engine/src/streaming/lod.rs:954`, compiled chunk GLB loading/hashing at `:1317`.
- Cluster terrain separately reads overlay metadata, canonical heights, and atlas files in `crates/engine/src/cluster/terrain.rs:6` and `:271`. Near terrain intentionally shares canonical heights (`:14`); this shared source-height dependency must not be confused with baked terrain LOD.

## Launcher and converter toggle findings

No launcher cluster checkbox was found. Play supplies only `--assets` at `crates/launcher/src/handlers.rs:167`. Launcher conversion builds default `PipelineConfig` at `crates/launcher/src/conversion/state.rs:346`; converter default LOD mode remains Baked at `crates/converter/src/cluster/mod.rs:60`.

The converter already accepts `--lod-mode auto|cluster|baked` and `--cluster-terrain` at `crates/converter/src/main.rs:1300` and `:1316`. Full conversion publishes the base and derives its overlay afterward (`crates/converter/src/pipeline.rs:582`). Derivation currently uses `include_trees: false` (`crates/converter/src/cluster/mod.rs:403`). Engine `--clusters` enables static ownership (`crates/engine/src/config.rs:2075` in the integration source).

## Verification status

The final engine all-target/all-feature Clippy run passed with warnings denied (`engine-clippy4.log`, exit 0). Engine all-target nextest passed **3,211 tests**, with **13 skipped**, in 54.734 seconds (`engine-nextest2.log`, exit 0). Seven vendored renderer epoch tests passed (`vendor-green.log`, exit 0); temporary PBR workspace membership and its temporary lock resolution were restored byte-for-byte afterward. Final fmt and staged whitespace checks passed. Independent review round 3 found no further actionable issue in the reviewed ownership/admission paths.

The later shadow-prewarm correction has observed red and green: seven focused tests passed (`shadow-warmup-green.log`, exit 0). A fresh independent review found no actionable issue in the final change, including queue ordering and shared specialization. Both shadow families now prepare before any corresponding light views exist. The final engine checks above include this correction. Whole-workspace strict Clippy passed (`workspace-clippy1.log`, exit 0). Workspace nextest passed **5,414 tests**, with **63 skipped** and **3 slow**, in 481.169 seconds (`workspace-nextest1.log`, exit 0). Workspace doctests passed **18 tests**, with **15 ignored** (`workspace-doc1.log`, exit 0). The previously slow streaming fixture completed successfully. No standalone engine/GPU acceptance run or runtime performance claim is made.

The final 38-file source/manifests snapshot is `793494d01710dc00607f93201850360c5609a7ed7a528f8d68e937d72944749c`; verification confirmed it stayed unchanged through every final gate. The target-local `verification.json` summarizes all eight required/supplemental receipts, each exit 0.

## Remaining integration risks

The engine deliberately loads the complete certified package and material set before play. This removes visibility-time data/pipeline discovery at the cost of startup time and resident memory; neither cost has GPU/runtime measurements in this task. Real renderer admission, later-visible point/spot shadows, and shader withdrawal still need the parent's permitted GPU acceptance run before integration into Plus.

The launcher checkbox and conversion behavior remain separate work. Existing native object/tree inventory reads and baked terrain LOD dependencies still require the converted base described above. An overlay selected with the engine's current cluster flags is not yet a self-contained conversion that omits every baked LOD product.
