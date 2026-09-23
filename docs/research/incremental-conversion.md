# Incremental conversion: rebuilding only the models a change affects

**Status:** research and design, 2026-09-23 (`research-151`). Nothing here is implemented, and no
file under `crates/` was changed. The design is written to be portable to upstream (see §8).

**Question asked:** when `CONVERTER_SCHEMA_VERSION` goes up, `crates/converter/src/cache.rs`
(`matches!(manifest.schema_version, 12..=18)`) drops every `.glb` entry and all 22,761 models are
rebuilt - 16.7 minutes on 2026-09-23. Would a finer-grained cache have made schema 16 take minutes
instead of seventeen?

**Method:** reading the converter crate (`cache.rs`, `pipeline.rs`, `mesh.rs`, `material.rs`,
`nif_animation.rs`, `archive/mod.rs`, `integration.rs`, `config.rs`, `main.rs`), the run logs under
`local/`, and the published tree `C:\Modding\SkyrimConverted` (schema 19, `complete: true`, 70,031
manifest entries). **No converter and no engine was run.** Counts measured on the published tree
with `rg -a -l` are listed with their commands in the appendix; timings come from the runs' own
`elapsed_ms`.

## 0. The answer

**No - not by itself.** Feature-scoped invalidation would have saved roughly **three minutes of the
seventeen**, because converting the models is only about 15-20% of what a conversion run does. The
other 80% is work the pipeline repeats on every run whatever the schema changes: re-materialising
the ~175,000 extracted source files, re-hashing and copying all 70,031 published assets, rebuilding
the world database and cell cache from scratch, and deleting the previous 68 GB output tree.

Two things have to be done to make a bump cost minutes, and they are independent:

| | models rebuilt | wall clock for a bump |
|---|---|---|
| today (schema 19, 2026-09-23) | 22,761 | 16.7 min measured |
| feature-scoped invalidation only (§4) | 839 for schema 16 | ~14 min (est.: the same 12-19 min floor, minus the ~3 min of models that no longer need rebuilding) |
| invalidation **and** the always-paid work removed (§4.8) | 839 | ~1-2 min (est.) |

So the honest headline is: *schema 16 would have rebuilt 3.7% of the models and still taken about
as long, because the run's cost is not dominated by the models.* The design in §4 has to include
both halves; taken alone, each is worth little.

**One correction to the premise, found while checking it.** The "16.7 minutes" (`converted 22761,
reused 220208 ... in 1001228 ms`, `local/conversion-s19-publish.out.log`) is the last of three
attempts that day, and it ran with `--resume-staging` over a staging directory
(`SkyrimConverted.staging-26344-...`, whose name stamps it as created around midnight and which the
day's stderr logs show in use at 08:18 and 09:02 as well) - so it did not pay for extracting the
sources, and it need not have paid for the models either. The schema-19 reconversion as a whole
spanned 15:23:15 to 16:09 - **about 46 minutes of wall clock** - and the 16.7 minutes is one
segment of that, dominated by the always-paid work. This matters below, because it is exactly the
run whose stage breakdown I would otherwise have read as "the cost of rebuilding 22,761 models".

## 1. Where the time goes

### 1.1 What the logs have, and what they lack

- **No timestamps.** `ProgressEvent` (`crates/converter/src/progress.rs`) carries stage, counts,
  file and message; `main.rs` prints `"{stage} {percent}% {message}"` (main.rs:62). Nothing in a
  conversion log is time-stamped, so **per-stage durations cannot be recovered from them**.
- What the logs do give: one line per asset (`Meshes 42% Converted asset`), the stage sequence, and
  a final summary line with the run's total (`Converted ..., reused ..., skipped ... in N ms`).
- `--report-json` gives the same totals machine-readably (`PipelineReport.elapsed_ms`,
  `converted`, `cache_hits`, `skipped`, `artifacts`), plus a 73,500-line artifact list.
- The only timestamps anywhere are in two small staging-size monitors written by hand during the
  schema-19 attempts (`local/conversion-s19.staging.log`,
  `local/conversion-s19.staging.attempt1.log`): a `du` of the staging directory every minute.

Because of that, the decomposition in §1.3/§1.4 is: **measured byte volumes and measured run
totals, with the components' time shares inferred from the monitor's measured write rate.** Each
inference is labelled.

### 1.2 The measured runs

| run | date | log / report | converted | reused | elapsed |
|---|---|---|---|---|---|
| full fresh conversion (all 242,969 assets, textures re-encoded) | 2026-09-22 | `local/conversion-162905/report.json` | 242,969 | 0 | **18,405,060 ms (5.11 h)** |
| same, first campaign (incomplete: 1 skipped) | 2026-09-21 | `local/conversion-run1/report.json` | 242,833 | 0 | 19,800,267 ms (5.50 h) |
| **no-op** (nothing changed, everything reused) | 2026-09-22 17:10 | `local/conversion/report.json` | 0 | 242,969 | **738,327 ms (12.3 min)** |
| no-op (second sample) | 2026-09-22 01:47 | `local/conversion-110211/report.json` | 0 | 242,969 | **1,141,318 ms (19.0 min)** |
| schema 19 bump, **resumed staging** (see §0) | 2026-09-23 | `local/conversion-s19-publish.out.log` | 22,761 | 220,208 | 1,001,228 ms (16.7 min) |

Sample size is small and the machine was busy on both days (other engines and workers), so the
12.3 / 19.0 minute spread between two identical no-op runs is real variance, not precision.

**Reading the summary line.** `converted` and `reused` are two *different* quantities added into the
same sentence: the archive ingestion counts one per extracted/restored **file inside a BSA**
(`pipeline.rs:234-237`), and the asset batches count one per **asset**. On a no-op run
`reused 242,969` = 195,722 archive file entries restored into `vfs/` + 47,247 cached assets (32,945
`dds` + 14,302 `pex`); on the schema-19 run `converted 22,761` is the 22,761 `nif` converted to
`.glb`. The model
count is not "22761 of 242969" - the real input census is in the manifest's `inputs_by_kind`:
**`nif` 22,761, `dds` 32,945, `pex` 14,302 = 70,008 assets**. Anyone reading the summary as
"222,000 assets reused" is overstating the run by 3x; the log line should probably say so.

### 1.3 The byte budget of a run

Measured on the published tree today (all `du`, sizes rounded):

| what | size | files |
|---|---|---|
| `textures/` (KTX2, the bulk of the output) | 19 GB | 32,945 |
| `meshes/` (GLB) | 1.3 GB | 22,761 |
| `scripts/` (Luau) | 121 MB | 14,302 |
| `skyrim_world.db` | 1.26 GB | 1 |
| `cell_cache.rkyv` | 489 MB | 1 |
| `conversion-manifest.json` (70,031 entries) | 62.5 MB | 1 |
| `vfs/` (extracted sources, **converter input, published anyway**) | 25 GB | 179,076 inodes |
| `.ingestion-cache/sha256/` (**a second copy of every extracted file**) | 22 GB | 156,844 blobs |
| **published output total** | **68 GB** | 408,101 inodes |

What one run does to those bytes, from the code (§1.4):

| operation | reads | writes |
|---|---|---|
| hash each archive (Skyrim's `Data` is 15 GB), verify each ingestion blob (`--no-verify-cache` off) | ~37 GB | - |
| copy previous ingestion blobs into the new staging cache | 22 GB | 22 GB |
| copy every blob into `vfs/` again | 25 GB | 25 GB |
| hash each source asset | 25 GB | - |
| hash each published output on cache hit | 20 GB | - |
| copy each cache-hit output into the new staging tree (GLB + KTX2 + Luau) | 20 GB | 20 GB |
| read + SHA-256 every published KTX2 in validation | 19 GB | - |
| read/parse every GLB three times (semantics, pruning, sRGB aliases) | 4 GB | - |
| read/parse one GLB per static in integration (17,022 parses) | ~1 GB | - |
| rebuild `skyrim_world.db` + `cell_cache.rkyv` from the plugins | - | 1.75 GB |
| publish: rename, then **delete the previous 68 GB output tree** | - | (68 GB freed) |
| **total per run** | **~170 GB** | **~137 GB + ~400,000 file deletes** |

The staging monitor measured the machine sustaining **30-66 MB/s** of staging growth during the
schema-19 window (5,917 MB at 15:30:37 → 28,022 at 15:36:09 → 41,232 at 15:39:46 → 49,961 at
15:44:54). At that rate, restoring `vfs/` + `.ingestion-cache` alone - 47 GB of the writes above -
is 12-26 minutes. **That single item is, on the evidence available, the largest cost in the
pipeline, and it is paid on every run, including the 12.3-minute no-op one.**

### 1.4 The always-paid work, by code

Nothing here depends on which models the cache invalidates:

1. **Archive ingestion restore** (`archive/mod.rs:45-88`, `restore_cached_files:173-205`).
   Every run: hash each archive, hash **every blob** in the previous `.ingestion-cache` when
   `verify_cache` is on (default), `copy_if_missing` every blob into the new staging cache, and
   `fs::copy` every blob into `vfs/`. Copies, not hardlinks - `copy_file` is `fs::copy`
   (`archive/mod.rs:235`), although the sRGB alias pass next to it already demonstrates
   `fs::hard_link` (`pipeline.rs:934`).
2. **The extracted sources are published.** `publish_directory` (`pipeline.rs:1114`) renames
   staging over the output, and staging contains `vfs/` and `.ingestion-cache/`. So 47 GB of
   converter scratch is part of the 68 GB published tree, is deleted with it at the next publish,
   and is what the "disk for a conversion is ~67 GB" note in `AGENTS.md` is really about.
3. **Per-asset cache verification** (`pipeline.rs:549-573`): on a cache hit the published file is
   hashed in full (`hash_file(&old)`) before being copied into staging. 20 GB of SHA-256 per run,
   unconditionally - `--no-verify-cache` does not reach this path (it is only read by the archive
   extractor, `pipeline.rs:206`).
4. **Three full GLB passes** after the mesh stage: `collect_texture_semantics`
   (`pipeline.rs:805`) walks and JSON-parses every GLB; `prune_dangling_texture_uris`
   (`mesh.rs:225-295`) reads and JSON-parses every GLB; `publish_srgb_texture_aliases`
   (`pipeline.rs:902`) does it a third time. 22,761 parses x 3.
5. **The world database is rebuilt every run** (`pipeline.rs:255-274`): `EsmParser::convert_plugins`
   writes `skyrim_world.db` (1.26 GB) and `write_cell_cache` writes `cell_cache.rkyv` (489 MB),
   then `validate_database` reads it back. No cache, no version check.
6. **Integration re-parses one GLB per static** (`integration.rs:87-118`): 17,022 `glb_bounds`
   calls for 17,022 statics with models, even though far fewer distinct models are involved
   (repeated parses are not memoised).
7. **Validation reads everything again** (`pipeline.rs:953-1000`): `fs::read` on every artifact,
   including all 19 GB of KTX2, each re-hashed inside `inspect_ktx2`.
8. **Publish deletes the old tree** (`pipeline.rs:1128-1130`): `fs::remove_dir_all(backup)` over
   ~405,000 files, synchronously, before the run is allowed to report complete.
9. **The manifest is one 62.5 MB JSON document**, deserialised at start (`ConversionManifest::load`)
   and rewritten in full at the end (`save`), with `to_vec_pretty`.

### 1.5 What converting the models actually costs

Three independent estimates agree that the mesh stage is small:

- The schema-19 no-op-vs-bump A/B: 738 s (nothing rebuilt) versus 1,001 s (successful run). Done
  carefully the comparison is weak - different days, different load, and the 1,001 s run resumed a
  populated staging directory - but it brackets the mesh work at **0-4.4 minutes**.
- The crashed attempt (`local/conversion-s19.out.log`, 15:30-15:48) printed all 22,761
  `Meshes ... Converted asset` events. The monitor shows staging reaching ~50 GB (≈ `vfs` 25 +
  cache 22 + `meshes` 1.3) at 15:44:54, so extraction finished around then and the mesh stage ran
  from roughly 15:45 to the crash at 15:48: **~3 minutes for all 22,761 models**.
- 22,761 models writing 1.3 GB is ~57 KB per model; ~8 ms per model wall clock across
  `available_parallelism()` threads is consistent with a NIF parse plus a GLB export per model.

**Call it 3 minutes of a 17-minute run, or ~18%.** A texture re-encode is the opposite extreme: the
full campaign converted the same 70,008 assets in 5.1 hours, and `dds -> ktx2` (32,945 encodes) is
almost all of it - so a schema change that invalidated textures would cost hours, and the
`.glb`-only migration arm is doing real work already.

### 1.6 Could a material-only change avoid NIF parsing altogether?

Yes - see §5, the machinery already exists. But §1.5 is the reason it does not answer the question
that was asked: a material change that avoids every NIF parse still pays the ~80% floor, and the
floor is 12-19 minutes. The cheap material path is worth having (it turns a 3-minute stage into a
~1-minute one), but **the floor is the first thing to fix**.

## 2. The premise table, checked

The brief's table ("models it should have needed") against the code and the published tree:

| schema | what changed (commit) | brief's guess | measured on the tree | verdict |
|---|---|---|---|---|
| 15 | vertex alpha is not blend, blend factors (`material.rs`), editor markers dropped (`mesh.rs`) | materials: most | 12,313 NIFs have a `NiAlphaProperty`; 1,337 GLBs carry `COLOR_0`; 1,012 NIFs name an `EditorMarker` shape | two features, not one; **"most" is too strong** - the alpha half is 12,313/22,761 (54%), the marker half 1,012 (4.4%) |
| 16 | door `Open`/`Close` clips | door NIFs only (~768, 171 animated) | **1,313 NIFs declare a `NiControllerManager`; 839 published GLBs carry an `animations` array** | **wrong scope**: the trigger is `NiControllerManager` (`mesh.rs:150`), not "is a door". Doors are 171 of the 839; chests, levers, mill wheels and magic FX are the rest |
| 17 | animation scale channels as VEC3 | animated NIFs only | the same 839 | **right** |
| 18 | glossiness -> roughness | materials: nearly all | 18,283 GLBs have geometry *and* materials | right, but capped at 18,283 - 4,478 GLBs (20%) are empty scenes with no materials at all |
| 19 | specular mask + env-cube colour space | materials with a normal map: nearly all | 17,155 GLBs have a `normalTexture`; **17,062 carry `KHR_materials_specular`/`specularTexture`** | right: 75% of the tree, so a 25% saving at best |

Two more facts the design leans on:

- **4,478 of 22,761 GLBs have no `meshes`/`materials` at all** - they are the `empty_scene_glb`
  placeholders written for deferred dynamic meshes (`/actors/`, `/magic/`, `/effects/`,
  `mesh.rs:490-522`) and marker-only models. Their content depends on nothing but the rules that
  decide emptiness, so no material or animation change can affect them.
- **Animation is rarer than the brief's table implies, and `NiControllerManager` is the exact
  predicate.** The raw-string count over the NIFs (1,313) is a proxy for
  `diagnostics.block_types.contains_key("NiControllerManager")`, which is the condition in
  `convert_nif_to_glb` itself; the 839 is the number that actually ended up with clips - the other
  474 have sequences whose channels do not resolve (the 1,288 "clip ... has no usable channels"
  warnings in `local/conversion-0923.err.log` are the visible part of that population).

## 3. What each output depends on

Everything a `.glb` carries comes from one of five converter features, and for each one the inputs
that can change it are known **at conversion time, for free**, from the parse the converter already
does:

| feature | produced by | can only differ when | free input signal |
|---|---|---|---|
| **geometry** (nodes, primitives, transforms, bounds offsets) | `nif_to_model` / `nif_to_static_model`, `shared::coordinates::CREATION_TO_RUNTIME_ROTATION`, marker drop | any NIF (transform convention) or a NIF with marker shapes | block types; `drop_editor_marker_geometry` count; `has_skeleton()` |
| **materials** | `build_nif_material_contract` + `publish_gltf_materials` | a NIF with shape materials | `NifShapeMaterial` contract: `alpha_property_block`, `shader_flags_1` (model-space normals), slots, glossiness, emissive |
| **texture bindings** (URIs, sRGB aliases, prune) | `rewrite_materials_and_texture_uris`, `prune_dangling_texture_uris` | a NIF with texture slots, or a missing texture on disk | the contract's `textures: Vec<NifTextureSlot>` + which slots resolve |
| **animation** (clips appended) | `append_nif_animations` (`nif_animation.rs`) | `diagnostics.block_types` has `NiControllerManager` | that key - the exact condition at `mesh.rs:150` |
| **texture encoding** (KTX2, not the GLB) | `TextureEncoding::from_semantics` over the semantics collected *from the GLBs* | a texture's semantic set changes | already handled: the encoding is part of the texture's `source_hash` (`pipeline.rs:521-523`) |

An empty-scene GLB depends only on the emptiness rules - geometry, in this table.

## 4. The design

Three levels of version, where today there is one:

1. **Format version** - today's `CONVERTER_SCHEMA_VERSION`. Kept whole-cache, bumped only when a
   change cannot be expressed as a feature (glTF container layout, a `project-wormhole-nif`
   upgrade, anything the author does not want to reason about). A format bump drops every `.glb`
   entry exactly as today, and is the escape hatch that keeps this design safe.
2. **Publication features** - a small monotonic counter per feature above. Bumped by the change
   that touches them; that is what today's single number becomes.
3. **Input signature** - a per-entry record of which inputs the model *has*, so a feature bump can
   be narrowed to the entries that can be affected.

### 4.1 What an entry records

`CacheEntry` (`cache.rs:13-19`) gains two fields; nothing is removed, so a manifest written by an
older converter still loads (and, having no `publication`, is rebuilt - see §4.4).

```jsonc
"meshes/architecture/whiterun/wrhouse01.glb": {
  "source_hash": "<nif sha256>:<skeleton sha256>",   // as today
  "output": "meshes/architecture/whiterun/wrhouse01.glb",
  "output_size": 57412,
  "output_hash": "…",                                // as today, after pruning (§4.9)
  "publication": {                                   // the versions this file was built at
    "geometry": 3, "materials": 7, "texture_bindings": 4, "animation": 2
  },
  "inputs": {                                        // measured from the parse that built it
    "geometry": true,            // false = empty scene
    "materials": 6,              // published material count (0 = none)
    "controller_manager": false, // can carry clips at all
    "clips": 0,                  // clips actually written
    "editor_markers": 0,         // marker shapes dropped
    "alpha_property": true,      // NiAlphaProperty present
    "vertex_colors": false,      // COLOR_0 in the output
    "normal_map": true,          // a Normal slot resolved
    "environment_cube": false,   // an EnvironmentCube slot resolved
    "model_space_normals": false,// SLSF1_MODEL_SPACE_NORMALS
    "skinned": false,
    "shapes": 6, "primitives": 6 // shape blocks -> mesh index mapping is stable
  }
}
```

All of it is computed during `convert_nif_to_glb`; none of it needs a second parse. Keep the keys
snake_case and the struct `#[serde(default)]`, so an old entry simply lacks flags.

### 4.2 The feature table

```rust
// crates/converter/src/publication.rs  (upstream: converter/src/publication.rs)
pub enum Feature { Geometry, Materials, TextureBindings, Animation }

pub const PUBLICATION: &[(Feature, u32)] = &[
    (Feature::Geometry, 3),
    (Feature::Materials, 7),
    (Feature::TextureBindings, 4),
    (Feature::Animation, 2),
];
```

with one doc comment per feature naming the code it covers and the bump history, replacing the
prose paragraph currently inside `ConversionManifest::load` (`cache.rs:68-78`). The database and
cell cache get their own feature in the same table (or stay under `WORLD_DATABASE_SCHEMA_VERSION`,
which already exists and is already shared with the engine - see §4.6).

### 4.3 The predicates

The one thing that must be written down per feature is **which inputs it can affect**:

```rust
fn affects(feature: Feature, inputs: &InputSignature) -> bool {
    match feature {
        // Every non-empty model is re-exported by a transform or export change.
        Feature::Geometry         => true,
        // Only a model with published materials can differ.
        Feature::Materials        => inputs.materials > 0,
        // Only a model whose texture slots or their resolution can differ.
        Feature::TextureBindings  => inputs.materials > 0,
        // No controller manager, no clips: `mesh.rs:150` never calls the exporter.
        Feature::Animation        => inputs.controller_manager,
    }
}
```

Rules that make this safe rather than clever:

- **A feature belongs to one asset kind.** `Feature::owner_kind()` is `Mesh` for the four above,
  `Texture` for the KTX2 encoding feature, `Script` for the script ABI (which stays in
  `configuration_hash`). An entry of another kind is never touched by that feature's bump - without
  this rule, the first GLB-feature bump would drop all 32,945 textures and 14,302 scripts, whose
  input signature has no `materials` flag and would therefore answer "affected".
- **Fail closed.** A feature version present in `PUBLICATION` but absent from an entry's
  `publication` map means "built by something older than this mechanism": affected. A missing input
  flag means affected. `InputSignature` fields are `Option`, and `None` answers `true` to every
  predicate. A predicate table with no arm for a feature answers `true`.
- **A predicate may only read a flag that the same parse that produced the output also produced.**
  Then the flag cannot be stale relative to the bytes it guards: they were written together.
- **Predicates are hypotheses, and there is a procedure to falsify them** (§7.3). That is what
  turns "geometry always" and "animation only with a controller manager" from an assumption into a
  checked claim.
- Keep them coarse where the data does not support finer. `Feature::Materials => inputs.materials >
  0` already excludes the 4,478 empty scenes; splitting materials further (base, specular, alpha)
  is a real gain for schemas 15 and 19 and is the next refinement, not the first one.

### 4.4 The migration

`ConversionManifest::load` keeps its shape; the `12..=18` arm is replaced by:

```rust
// format version: unchanged contract, everything is rebuilt
if manifest.schema_version != CONVERTER_FORMAT_VERSION {
    return Ok(Self { schema_version: CONVERTER_FORMAT_VERSION, ..Default::default() });
}
// feature versions: rebuild only what the predicates say can differ.
// An entry is kept when the feature does not own its asset kind, or it already
// carries the wanted version, or the predicate says its inputs cannot differ.
if manifest.publication != CURRENT_PUBLICATION {
    for (feature, wanted) in CURRENT_PUBLICATION {
        if manifest.publication.get(feature) == Some(wanted) {
            continue;
        }
        let before = manifest.entries.len();
        manifest.entries.retain(|_, entry| {
            entry.kind != feature.owner_kind()
                || entry.publication.get(feature) == Some(wanted)
                || !affects(feature, &entry.inputs)
        });
        log!("{feature:?} {old}->{wanted}: {} of {before} entries invalidated",
             before - manifest.entries.len());
    }
    manifest.publication = CURRENT_PUBLICATION;
    manifest.complete = false;
}
```

Two properties worth stating explicitly:

- **The log line is part of the design.** Today the operator learns nothing about *why* 22,761
  models are being rebuilt; a run should print `Animation 1->2: 839 of 70,031 entries invalidated,
  69,192 reused`.
- `entries` holds textures and scripts too, whose features are their own (`textures` bump on an
  encoding change - already carried in the hash, §3) and `scripts` on the script ABI (already in
  `configuration_hash`). Those keep behaving as they do now.

### 4.5 Worked examples

| change | format | feature bumped | entries rebuilt |
|---|---|---|---|
| no-op run | - | - | 0 |
| door `Open`/`Close` clips (schema 16's converter half) | no | `animation` | 839 |
| scale channels as VEC3 (17) | no | `animation` | 839 |
| roughness mapping (18) | no | `materials` | 18,283 |
| specular mask (19) | no | `materials` | 17,062 (with a `normal_map` predicate arm; without one, 18,283) |
| vertex alpha / blend factors (15) | no | `materials` | 12,313 |
| editor markers dropped (15) | no | `geometry` | 22,761 (all - `geometry` is coarse on purpose) |
| glTF container layout | **yes** | - | 22,761 |

Item 6 shows the predicate granularity doing the work: `geometry` is deliberately all-or-nothing
because a transform-convention change really does touch every model, and a marker-only change riding
the same counter would drag all 22,761 with it. If that matters, split `Geometry` into `Transform`
(all) and `ShapeSet` (`inputs.editor_markers > 0` - 1,012) - two counters, both honest.

### 4.6 The engine side

The engine refuses stale assets with one private constant, `converter_schema_version()` in
`crates/engine/src/app.rs` (hand-synced with `CONVERTER_SCHEMA_VERSION`), compared against the
manifest's `schema_version`. With feature versions, the check becomes: the manifest carries
`"publication": {...}` for the run, and the engine requires the features it depends on to be **at
least** the versions its code needs (the portal engine needs `animation >= 2` for door clips, the
renderer needs `materials >= 7` for the gloss fix). A feature the engine does not name cannot block
startup.

The shared table belongs in `crates/shared/` beside `WORLD_DATABASE_SCHEMA_VERSION` - which is
already the plan on both tracks (`docs/design/portal-plugin.md` H6/item 5 of
`docs/pr/two-tracks.md`, branch `phase2/shared-converter-schema`), so this design should ride that
PR rather than open a second refactor of the same constant.

### 4.7 The world database and the cell cache

`skyrim_world.db` and `cell_cache.rkyv` are rebuilt from the plugins on every run, and the DB is
then enriched with bounds read from the GLBs (`integration.rs`). Give the pair a `database` feature
with an input signature of "(plugin hashes, `WORLD_DATABASE_SCHEMA_VERSION`, converter DB feature
version)", and reuse the published pair when none of those changed - it saves writing 1.75 GB and
reading it back, and above all it saves the ESM parse.

The bounds enrichment still has to re-run for models that *were* rebuilt, but that can be scoped:
the manifest knows which GLBs were rebuilt in this run, so the `UPDATE statics SET bounds...` pass
can touch only the statics whose `model_path` is one of them. The full 17,022-parse pass is only
needed when the DB is rebuilt or when a geometry feature changed.

### 4.8 Removing the always-paid work (the other half)

Independent of the versioning, and in rough order of value:

1. **Keep `vfs/` and `.ingestion-cache/` out of the published tree.** They are converter scratch:
   nothing but the converter reads them, and every run copies them into a fresh staging directory
   and then deletes them with the old output. Move them to a stable sibling (`<output>.cache/`)
   that staging links from or that a resumed run points at. Saves ~47 GB of writes and ~47 GB of
   reads per run, and takes the "conversion needs ~67 GB" requirement down to the size of the
   actual output (~20 GB).
2. **Hardlink instead of copy** wherever the byte-identical file is already on the volume:
   blob -> `vfs`, and published output -> staging (`fs::hard_link`, or `fs::copy` as the fallback,
   exactly as `publish_srgb_texture_aliases` already does at `pipeline.rs:934`). Same volume
   (staging is a sibling of the output by construction, `config.rs:81-86`), so link creation is
   metadata-only. Saves ~45 GB of writes per run. *Caveat:* a hardlinked staging file that the
   converter then overwrites in place is fine (`write_glb_atomic` writes a new file and renames);
   what must never happen is an in-place `fs::write` that would edit the published file through the
   link. `rebuild_glb_with_document` + `write_glb_atomic` (`mesh.rs:283-285`) already write and
   rename, so this is a review point, not a blocker.
3. **Sample the verification.** `hash_file` on every cache hit (20 GB of SHA-256) and on every
   ingestion blob (22 GB) exists to catch bitrot. Hash a random sample (say 1% plus every entry
   whose size or mtime changed) and make `--verify-all` the opt-in for a paranoid run. This changes
   the guarantee from "every byte is re-verified" to "bitrot is caught with probability p", which
   should be a deliberate, documented decision - the entries' `output_hash` is what makes an
   unsampled file *checkable* later.
4. **Cache the three GLB passes.** `collect_texture_semantics`, the prune pass and the sRGB alias
   pass all parse all 22,761 GLBs every run. The first two can be recorded per entry (§4.1: the
   texture slots are already in the input signature) and the alias list derived from the same
   record; the prune pass still has to run when the texture tree changed.
5. **Memoise `glb_bounds` by path** in integration (17,022 parses for far fewer distinct models).
6. **Delete the old output tree after reporting, or with a lower-priority pass.** 68 GB /
   405,000 files of `remove_dir_all` currently sits inside the timed run, before "complete" is
   printed.
7. **Split the manifest.** 62.5 MB of pretty JSON is parsed and rewritten in full per run; per-kind
   files (meshes/textures/scripts) would make an incremental rewrite possible. (Also `to_vec` rather
   than `to_vec_pretty` - roughly a third of those bytes are indentation.)

A materials-only change with all of the above plus §5 would touch ~1.3 GB of GLBs and almost nothing
else. That is the "minutes" answer, and it comes mostly from §4.8, not from §4.1-4.6.

### 4.9 A bug found in passing (upstream, low priority today)

There is also a real correctness wrinkle in the prune path: the
`CacheEntry` for a GLB records `output_size`/`output_hash` *at conversion time*
(`pipeline.rs:639-659`), but `prune_dangling_texture_uris` rewrites the same file in staging
afterwards (`pipeline.rs:310`) and nothing updates the entry. On the next run the cache-hit check
compares `hash_file(published glb)` against the pre-prune hash, decides the entry is stale, and
re-converts that model - which is then pruned again. It is invisible today (the current data prunes
0 references, `pruned_texture_references` is empty), but it would become a recurring rebuild of
every pruned model under this design, and it is wrong in principle: the manifest should record what
was published. Fix: re-hash after the prune pass, once, for the files it touched.

## 5. The cheap path for material changes

**Is it sound?** Yes, and it is closer than it looks: the converter already rewrites a published
GLB's JSON chunk without touching its binary chunk, for the prune pass
(`glb_json_from_bytes` -> mutate -> `rebuild_glb_with_document`, `mesh.rs:259-295`), and the
material contract is already `Serialize + Deserialize` (`NifShapeMaterial`, `material.rs:249-263`).
A materials-only bump could therefore be served by *re-publishing materials from cached inputs*:

1. The converter writes, per converted NIF, a small sidecar holding the `NifShapeMaterial` contract
   plus the exported shape-block -> primitive mapping (`exported_shape_blocks`, `mesh.rs:127`).
2. On a `materials`-only invalidation, for each affected entry: read the published GLB, parse its
   JSON, re-run `publish_gltf_materials` from the sidecar, rewrite `materials` / `images` /
   `textures` and the primitives' `material` indices, and rebuild the container.
3. No NIF is parsed, no geometry is exported, no animation data is touched.

**Cost.** 1.3 GB of GLB read + 1.3 GB rewritten, with JSON parsing - well under a minute at the
rates measured here (the pure model stage is ~3 minutes). Storage: the contract is on the order of
a few hundred bytes to a few KB per model (59,268 materials over 22,761 models), so a few tens of
MB for 22,761 sidecars. The JSON of a GLB is a few percent of the file (59,268 materials against
1.3 GB), so this rewrites the same file size but skips all the work that produced it.

**What it needs:**

- The sidecar must be part of the cache contract: hashed, versioned, invalidated when the material
  inputs change (a NIF change already changes `source_hash`), and stored outside the published tree
  if it is to survive a publish (or inside it, if being published is acceptable - it is dead weight
  for the engine).
- It is already almost usable as-is: `publish_gltf_materials(document, contract,
  exported_shape_blocks, glb_output_path)` (`material.rs:314-319`) takes the parsed JSON document,
  the contract and the shape mapping, and builds its own `TextureRegistry` internally - i.e. it is
  already a function of exactly the three things a sidecar would hold. What has to be established is
  that it depends on nothing else outside them.
- The shape-block -> primitive mapping must be stored, because the number of materials can change
  and every primitive's `material` index has to be recomputed consistently.

**What could go wrong:**

1. **Image/texture index remapping.** Exactly the bug impl-081 hit in `prune_document_images`: a
   material's `normalTexture` index left pointing at a removed image. Any rewrite must remap every
   index it touches, including extension textures (`KHR_materials_specular`). This is the top risk.
2. **Silent material/primitive mismatch.** If `publish_gltf_materials` emits a different number of
   materials than the GLB has, primitives must be remapped, not merely overwritten; a mismatch
   produces a model that loads with the wrong material on a shape - the class of failure that is
   hardest to notice on screen.
3. **Animations and extras.** The JSON chunk being rewritten also holds `animations`, `extensionsUsed`
   and `extensionsRequired`; a naive "replace the materials key" must not drop or reorder them
   (`KHR_materials_specular` must appear in `extensionsUsed`).
4. **Textures that were pruned must not resurrect**, and URIs whose texture has since disappeared
   must still be pruned - so the prune pass still has to run after this path.
5. **Determinism.** If the fast path produces different bytes than a full conversion, then the
   verification of §7.4 compares two legitimate encodings and cries wolf.
6. **It cannot serve a change that also alters texture *encoding*** (a semantic change moves bytes
   in the KTX2, not the GLB) - that must bump `textures` and re-encode.

**Recommendation:** land §4 first (it is the smaller change and it cuts the *set*), then the
material fast path (it cuts the *cost per model*). Between them, they change the shape of a schema
bump from "17 minutes, all models" to "seconds of work plus whatever the floor costs".

## 6. What it would have saved

Model counts are measured (appendix); minutes are estimated from the ~8 ms/model stage cost derived
in §1.5, and rounded. "Floor" is the 12.3-19 minute measured always-paid cost (§1.4); the last
column assumes §4.8 is done too, which brings the floor to roughly 1-2 minutes.

| schema | feature(s) bumped | models rebuilt today | under the design | min today | min: invalidation only | min: invalidation + §4.8 |
|---|---|---|---|---|---|---|
| 15 | `materials` + `geometry`(markers) | 22,761 | 12,313 + 1,012 (≈13,000) | 16.7 | ~15.5 | ~2 |
| 16 | `animation` | 22,761 | **839** | 16.7 | **~13.8** | ~1.5 |
| 17 | `animation` | 22,761 | **839** | 16.7 | **~13.8** | ~1.5 |
| 18 | `materials` | 22,761 | 18,283 | 16.7 | ~16.1 | ~2 |
| 19 | `materials` (specular) | 22,761 | 17,062 | 16.7 | ~16.0 | ~2 |
| no-op | - | 0 | 0 | 12-19 | 12-19 | ~1 |

**Read the two middle columns together, because that is the finding:** the best the invalidation
alone can do is ~3 minutes (schemas 16 and 17, where it avoids 21,922 of 22,761 model rebuilds),
and for the material schemas - the ones that keep coming back, 15, 18 and 19 - it saves 0.5-2
minutes, against a run that costs 12-19 minutes whatever you do. Feature-scoped invalidation is only
worth building *with* §4.8; §4.8 alone is worth more than §4 alone. If only one of the two gets
built, build §4.8.

## 7. Risk: how the design cannot ship a stale model

The failure mode to fear is not a crash - it is a run that reuses a model it should have rebuilt,
prints `complete: true`, and hands the engine a wrong-looking asset that nobody notices for a week.

1. **Fail closed, everywhere.** Missing feature version, missing input flag, missing predicate arm,
   unknown feature name, unparseable entry -> rebuild. An old manifest (no `publication`) rebuilds
   everything, which is today's behaviour, so the migration cannot be *more* permissive than today
   at the moment it is introduced.
2. **Inputs are recorded by the parse that produced the bytes**, so a flag cannot be stale relative
   to the output it describes.
3. **Predicate falsification sampling - the load-bearing guarantee.** After a feature bump, the
   converter rebuilds in full a random sample of the entries the predicate *said were unaffected*
   and byte-compares the result with the published file. If any differ, the predicate is wrong:
   the run fails loudly, the feature falls back to "affected" for every entry, and the next run
   rebuilds that feature wholesale. This is the same discipline the project already applies to
   tuning tasks ("a tuning task must also pass on examples the worker never saw",
   `AGENTS.md`) - the sample is the entry's own holdout, generated by the same run, so it is
   affordable on every bump (a few hundred NIF conversions, seconds). Default sample: a few hundred
   entries plus every entry of a kind the affected feature has never been exercised on, so a new
   predicate is cheap to trust.
4. **Determinism is a precondition, not an assumption.** Byte comparison only proves anything if a
   full conversion of an unchanged input is byte-identical. The converter very likely is (GLB JSON
   is produced by `serde_json` with `BTreeMap`-ordered maps and no timestamps; `empty_scene_glb`
   pads deterministically), but **this was not established here and must be**: a test that converts
   a handful of real NIFs twice and compares hashes, plus the falsification sampling itself, which
   will scream immediately if it is not true.
5. **Record the converter that produced the run.** Add the converter's version/commit to the
   manifest (a build-script constant), and refuse to reuse entries from a run whose producer is
   unknown. `configuration_hash` covers configuration; it does not cover code.
6. **Keep the whole-cache escape hatch.** A format bump still drops every GLB. Any change the
   author is not certain about bumps the format, not a feature - the design only has to be right
   where it is claimed to be right.
7. **Make it visible.** Print the per-feature invalidation counts (§4.4) and a summary of what the
   run reused versus rebuilt by feature. A cache that is quietly wrong is only dangerous while it
   is quiet.
8. **Two hazards that exist today and get worse under a mixed-age tree:**
   - `--resume-staging` trusts every file already in the staging directory
     (`pipeline.rs:575-605`: non-empty file -> reuse, whatever wrote it). The 2026-09-23 schema-19
     work reused one staging directory across three attempts and a whole day - and a resumed
     staging directory holding schema-18 GLBs would have been published as schema-19 output. A
     per-entry `publication` record makes this checkable (compare the staging file's feature
     versions, not just its existence); the same fix makes resume safe.
   - The manifest records `output_hash` before the prune pass rewrites the file (§4.9), so pruned
     models rebuild for ever.

## 8. Two tracks

- **The mechanism is converter-only and belongs upstream.** `docs/design/portal-performance.md`
  already assigns "convert only what changed" to the Phase 2 track, precisely because it speeds up
  every conversion and the converter is shared. Nothing in §4 depends on portal-only converter code:
  it is `cache.rs` + `pipeline.rs` + a new `publication.rs`, plus (if §5 is built) a sidecar writer
  in `mesh.rs`/`material.rs`. Upstream's layout is flat (`converter/src/`, `engine/src/`), so file
  paths differ but the shape does not.
- **Sequence it after the shared-constant PR.** `docs/pr/two-tracks.md` item 5,
  `phase2/shared-converter-schema` (impl-502, checks passing, held), moves
  `CONVERTER_SCHEMA_VERSION` into `crates/shared` beside `WORLD_DATABASE_SCHEMA_VERSION`. The
  feature table and the engine's required-feature list belong in exactly that file. Landing this
  design before that PR would collide with it.
- **Each bump still updates the Compatibility section** upstream expects (`02-core-engine.md`,
  "Compatibility": today DB 3 / converter manifest 14 / cell cache 3), and a `phase2/*` PR that
  bumps a schema does so in the same commit.
- **Portal-only parts: none in the mechanism.** The portal track's contribution to this story is the
  pressure (`schema 19`), and schema 19 itself is a portal-side change to shared converter code
  that, per the one-way flow rule, would be ported to a `phase2/*` branch before it is offered
  upstream. If §4.8 changes where `vfs/` and `.ingestion-cache/` live, that is a behaviour change
  for every user of the converter and belongs in the same upstream PR.
- **What to land in what order, upstream-shaped:** (1) `fix(converter)`: re-hash pruned GLBs (§4.9,
  no bump); (2) `perf(converter)`: the §4.8 floor removals - cache outside the published tree,
  hardlink instead of copy, sampled verification (no bump, biggest win); (3) `feat(converter):
  publication features` - §4.1-4.6 behind the existing schema, with the falsification sampler;
  (4) `perf(converter)`: the material fast path (§5). Steps 1-2 pay off immediately with no
  semantic risk; step 3 is the part that needs the verification to be trustworthy.

## 9. What I could not establish

- **Per-stage timings.** No log line is time-stamped, so the §1.3 shares are inferred from byte
  volumes and from the one measured write rate. The asset-event counts in the logs are exact
  (70,008 events = the 70,008 assets; `local/conversion-162905/stdout.log` is 37 MB for the same
  70,008 events only because that run's libraries printed 301,673 `WARNING` lines that later runs do
  not), and the run totals are the run's own summary and `elapsed_ms`. What cannot be recovered is
  *when* any stage ran.
- **Whether the successful schema-19 run converted the models itself or found them already in the
  resumed staging directory.** The evidence for "found them" is the staging directory's age (stamped
  around midnight, referenced by the day's stderr logs at 08:18 and 09:02), the monitor's timeline,
  and the 22,761 `Meshes` events the *crashed* attempt printed. Under either reading every published
  GLB was built with schema-19 code during that day, and §1.5's conclusion - the mesh stage is a few
  minutes against a 12-19 minute floor - does not change, which is why I did not chase it further.
  It does mean the 16.7 minutes should not be quoted as "the price of rebuilding 22,761 models".
- **The exact count of non-door animated NIFs at clip level.** 1,313 NIFs declare a controller
  manager and 839 publish clips - both measured - but which are doors (171 of 768 door NIFs per
  `docs/pr/upstream-pr-plan.md:296`) was not re-derived here.
- **Whether the no-op runs used `--no-verify-cache`.** If they did, the floor without verification
  is lower than 12.3 minutes and §4.8 item 3 is smaller than I claim.
- **The converter's byte-determinism** (§7.4). Not tested; it is a prerequisite for the whole
  verification strategy.
- **Which disk this runs on and its sustained rate.** `Get-PhysicalDisk` was not available in this
  session; the 30-66 MB/s figure comes from the monitor's own `du` samples.

## Appendix: how the measured numbers were produced

Read-only, on the published schema-19 tree; no converter or engine was run.

```bash
# asset census (from C:/Modding/SkyrimConverted/conversion-manifest.json: inputs_by_kind)
#   dds 32945, nif 22761, pex 14302  -> 70,008 assets; 70,031 manifest entries
rg -c '"source_hash"' C:/Modding/SkyrimConverted/conversion-manifest.json

# sizes
du -sh C:/Modding/SkyrimConverted/{meshes,textures,vfs,.ingestion-cache}
du --inodes -s C:/Modding/SkyrimConverted/vfs            # 179,076 files+dirs
du --inodes -s C:/Modding/SkyrimConverted/.ingestion-cache  # 156,844 blobs

# feature populations (glb: -a is required, the GLB header contains NUL bytes)
rg -a -l "NiControllerManager" C:/Modding/SkyrimConverted/vfs/meshes | wc -l   # 1313
rg -a -l '"animations"'      C:/Modding/SkyrimConverted/meshes | wc -l         #  839
rg -a -l "EditorMarker"      C:/Modding/SkyrimConverted/vfs/meshes | wc -l     # 1012
rg -a -l "NiAlphaProperty"   C:/Modding/SkyrimConverted/vfs/meshes | wc -l     # 12313
rg -a -l 'COLOR_0'           C:/Modding/SkyrimConverted/meshes | wc -l         # 1337
rg -a -l '"primitives"'      C:/Modding/SkyrimConverted/meshes | wc -l         # 18283
rg -a -l '"normalTexture"'   C:/Modding/SkyrimConverted/meshes | wc -l         # 17155
rg -a -l 'specularTexture'   C:/Modding/SkyrimConverted/meshes | wc -l         # 17062
rg -a -l '"alphaMode":"BLEND"' C:/Modding/SkyrimConverted/meshes | wc -l       #  2067

# run totals: local/*/report.json ("elapsed_ms", "converted", "cache_hits")
# staging-size timeline: local/conversion-s19.staging.log, local/conversion-s19.staging.attempt1.log
```

The `NiControllerManager` and `NiAlphaProperty` counts are raw-string counts over the NIFs (the
block-type name is stored as a string in the header), i.e. a proxy for - not a re-implementation of
- the conditions in `mesh.rs` and `material.rs`. `EditorMarker` is a shape-name substring.
