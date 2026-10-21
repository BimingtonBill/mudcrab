# ADR-0013: Cluster LOD payloads for opaque statics, keyed by mesh content and chunk

- **Status:** Draft (for discussion on issue #203 before the converter experiment under #103)
- **Date:** 2026-10-11

## Context

Issue #203 proposes automatic distant detail for opaque static objects: the converter
builds a cluster hierarchy per mesh with meshoptimizer's `clusterlod.h` (MIT, vendored at
v1.3 behind the `meshopt-cluster` crate), and the engine selects clusters by screen-space
error and draws them with GPU-driven indirect draws. The maintainer asked that the payload,
versioning and fallback contracts be agreed before any converter experiment, and that
cluster LOD reuse the existing streaming, eligibility, validation and near/far handoff
contracts rather than add a second residency system. Terrain, small meshes, trees and
unsupported materials keep their own paths.

What exists today:

- Terrain LOD chunks are GLB files indexed by `lod_chunks` with an R-tree and one
  `lod_build` identity (ADR-0010, ADR-0011, `crates/shared/src/lod.rs`). The runtime probes
  the table in `sqlite_master`, verifies each payload's content hash, classifies failures as
  transient or terminal with bounded retries, and hands off per source cell when the full
  cell is ready (`docs/specs/engine/lod-architecture.md`).
- Converted assets are content-addressed: every GLB has a `source_hash` and `output_hash`
  in the conversion manifest (`crates/converter/src/cache.rs`).
- Issue #185 proposes per-cell input keys for derived data (hash of the winning records,
  references and asset output hashes a cell is built from), with chunks of cells as the
  exterior storage unit (`docs/research/derived-data-input-keys.md`, branch
  `phase1/room-graph`).
- Measured: `clodBuild` on 75 real meshes runs at about 1.5 s per million triangles single
  threaded and emits about 14 bytes of cluster data per input triangle, with no failures;
  55% of primitives are under 128 triangles and get a single cluster, so a size threshold is
  needed (`local/research/fun/p1-vg2-cluster-spike-2026-10-09.md`). wgpu 29.0.4 gives
  `multi_draw_indexed_indirect_count` on Vulkan and DX12 only, and Bevy 0.19 does not use
  it on DX12 (`vg4-wgpu-cluster-indirect-2026-10-11.md`).
- The `meshopt-cluster` crate (branch `phase1/vg3-clusterlod`, c344ca61) returns a
  `Hierarchy` of `Group`s (depth, bounds) and `Cluster`s (bounds with error, `refined`
  parent group, vertex count, global indices), a `LocalGeometry` of u32 vertex references
  plus u8 local triangles, and an optional `Bvh` over groups.

This ADR fixes the data contract so the engine prototype (queue VG7) and the converter
stage (VG8) can start in parallel. It does not decide whether the cluster path ships; that
is VG10, after measurement.

## Decision

### 1. Two payload kinds, one build identity

Cluster LOD data has two parts with different lifetimes, so it is stored in two kinds of
payload:

| Kind | Unit | Changes when | File |
|---|---|---|---|
| Mesh clusters | one converted GLB primitive | the source GLB's `output_hash` changes, or the builder identity changes | `clusters/meshes/<hh>/<mesh_hash>-p<N>.mcl` |
| Chunk instances | one block of exterior cells (same grid as LOD chunks) | any eligible reference, its base record or its mesh in those cells changes, the worldspace's LOD origin changes, or the builder identity changes | `clusters/instances/<worldspace>/<side>/cell_<x>_<y>.mci` |

`<mesh_hash>` is the source GLB's `output_hash` (lowercase SHA-256 as the manifest already
records it) and `<hh>` its first two hex digits. `<N>` is the glTF primitive index. A mesh
placed a thousand times has one cluster payload. Chunk instance payloads are small
(about 64 bytes per instance) and are the only part that depends on plugins.

Both kinds carry the same **builder identity** in their header and in the database:

```
cluster_builder_identity = sha256( canonical JSON of {
  "payload_format":      1,                 // this ADR's layout version
  "vertex_layout":       1,                 // section 4
  "eligibility_rules":   1,                 // section 5
  "clusterlod_source":   "<sha256 of vendor/meshoptimizer/demo/clusterlod.h>",
  "meshoptimizer":       "<sha256 of vendor/meshoptimizer/src/*.cpp, sorted>",
  "builder_config":      { every clodConfig field the wrapper sets: preset, max_triangles,
                           max_vertices, min_triangles, partition_size, partition_spatial,
                           cluster_spatial, cluster_split_factor, simplify_ratio,
                           simplify_threshold, the simplify_error_* terms, the permissive,
                           sloppy and regularize flags, optimize_bounds, attribute weights
                           and protect mask, bvh node_width },
  "triangle_threshold":  <section 5>,
  "block_side_cells":    16,
  "converter_schema":    CONVERTER_SCHEMA_VERSION
} )
```

`converter_schema` is in the identity because it names the mesh contract the source GLBs
follow (which attributes exist, how they are encoded); a retained mesh from an older producer
must not be clustered under a newer identity. The world database schema is not in it: the
cluster tables are additive and do not change with it (section 6).

A change in the meshoptimizer sources, the vendored `clusterlod.h`, any builder setting, the
vertex layout or the eligibility rules changes the identity, and the engine refuses every
payload whose header identity differs from the `cluster_build` row (terminal failure, logged
once per payload, handling per section 7). Hashing the vendored sources, not a version
string, means a patched vendor file cannot be mistaken for the release it came from.

The identity deliberately does **not** include plugin hashes: a plugin change must rebuild
only the chunk instance payloads whose inputs changed (section 3), never the mesh clusters.
This differs from today's `lod_build` identity, which folds in every plugin hash and so
rebuilds all terrain LOD on any plugin change; #185 is moving that contract the same way.

### 2. Mesh cluster payload (`.mcl`)

A flat little-endian binary file, memory-mappable, loaded by a custom Bevy `AssetLoader`
so it inherits the asset server's readiness tracking, failure states and retry behaviour.
It is **not** a GLB: its consumer is a GPU storage buffer read by the cluster render node,
not a scene, and routing it through the glTF loader would decode it into `Mesh` assets the
cluster path never draws. (ADR-0010 chose GLB for terrain chunks because they *are* scenes.)

```
header (160 B; 132 used, the rest zero)
  magic            u8[4]    "MCL1"
  format_version   u32      3
  builder_identity u8[32]   section 1, raw bytes
  mesh_hash        u8[32]   source GLB output_hash, raw bytes
  primitive_index  u32
  flags            u32      bit 0: has vertex colours; bit 1: has tangents; others 0
  vertex_count     u32
  cluster_count    u32
  group_count      u32
  bvh_node_count   u32
  root_error       f32      largest finite cluster error (the coarsest drawable level), mesh units
  bounds           f32[4]   whole-mesh sphere: centre xyz, radius (mesh space)
  uv_min, uv_scale f32[2],f32[2]   dequantisation for the UV stream
section table (at byte 160; 10 entries of { offset u64, length u64 }, 160 B; absent sections zero)
sections, each 256-byte aligned, in this order:
  0 positions        f32[3] per vertex, mesh space (the instance transform applies at draw time)
  1 normals          snorm16[2] per vertex, octahedral encoding
  2 uvs              unorm16[2] per vertex, (uv - uv_min) / uv_scale
  3 tangents         snorm16[2] + sign in the low bit of y, octahedral (present iff flags bit 1)
  4 colours          unorm8[4] per vertex (present iff flags bit 0)
  5 clusters         ClusterRecord[cluster_count]
  6 cluster_vertices u32 per cluster vertex, concatenated per cluster, indexes the vertex streams
  7 cluster_triangles u8[3] per triangle, concatenated per cluster, local to the cluster's vertices
  8 groups           GroupRecord[group_count]
  9 bvh              BvhNode[bvh_node_count]

ClusterRecord (48 B)
  bounds           f32[4]   centre xyz, radius: clusterlod's culling bounds (its own error field is
                            not monotonic and is not stored)
  error            f32      the simplified error of the group that produced this cluster
                            (clusterlod `refined`), 0 for source geometry; monotonic up the DAG
  parent_error     f32      the simplified error of the group this cluster belongs to; +inf
                            (f32::MAX) for clusters of the terminal group
  vertex_offset    u32      into section 6, in u32 units
  triangle_offset  u32      into section 7, in triangles
  vertex_count     u8       at most max_vertices (128 in both presets, always at most 255)
  triangle_count   u8       at most max_triangles (128), hence u8
  group            u16      index into section 8, the group this cluster belongs to
  refined_group    u16      the group that produced this cluster, 0xFFFF for source geometry
  reserved         u8[10]   zero
GroupRecord (32 B)
  bounds f32[4]; error f32 (the group's simplified error, f32::MAX for the terminal group);
  depth u16; first_cluster u16; cluster_count u16; reserved u16; reserved u32
  (counts fit u16 per primitive; a primitive with more than 65,535 clusters or groups is
  rejected, reason `too_many_clusters`)
BvhNode (32 B)
  bounds f32[4]; error f32; first_child u32; child_count u16; group u16 (0xFFFF for an
  internal node); reserved u32
```

Everything the engine needs per frame is in sections 5 to 9; the vertex streams are read
only by the vertex shader. All offsets are relative to the file start, so a mapped file is
uploaded with one copy per section. Records the GPU reads (sections 5, 8, 9 and the instance
records) are multiples of 16 bytes so the same layouts serve as WGSL storage-buffer structs
without repacking.

The selection rule the runtime applies is clusterlod's (`clusterlod.h` lines 139 to 151):
draw a cluster when `project(error) <= threshold` and `project(parent_error) > threshold`,
with both errors scaled by the instance's largest scale factor and projected with the view's
distance and field of view. The builder enforces that errors only grow up the DAG; the
reader checks it when it validates a payload (terminal failure `non_monotonic_error`), and
that monotonicity is what makes the cut unique and crack-free between neighbouring groups.
`root_error` in the header is the largest finite `error` of any cluster, the error of the
coarsest drawable level.

Size estimate: VG2 measured about 14 bytes of cluster data per source triangle and did not
count vertex data. Assuming 0.6 vertices per triangle (a typical closed-mesh ratio, not
measured) and 24 bytes per vertex adds about 14 bytes, so roughly 28 bytes per triangle. For
the eligible opaque subset of vanilla that is in the order of 0.5 GB on disk; VG8 measures
it.

### 3. Chunk instance payload (`.mci`) and keys

Chunk instances reuse the LOD chunk grid: `ChunkKey { worldspace_id, tier, anchor }` with
the per-worldspace `LodOrigin` from the `lodsettings` sidecar (ADR-0011, GEOM-02). The
cluster path has no discrete tiers (detail is continuous), so one block size is used and
the `tier` column records it. The block side is 16 cells (`LodTier::Tier16`); the only other
legal values are 4 and 8, because `LodTier::from_side_cells` and the shared query path accept
nothing else, and the choice is part of the builder identity. A worldspace without a valid
LOD origin gets no cluster instances, as it gets no terrain LOD. Payload paths follow
`chunk_payload_path`'s spelling: lowercase 8-digit hex worldspace id, decimal anchors, so
`clusters/instances/<worldspace hex>/<side>/cell_<ax>_<ay>.mci`.

```
header (96 B; 88 used, the rest zero)
  magic "MCI1"; format_version u32; builder_identity u8[32]
  worldspace_id u32; tier u32; anchor_x i32; anchor_y i32
  instance_count u32; mesh_count u32
  bounds_min f32[3], bounds_max f32[3]   world space, Creation units, conservative over every instance
mesh table      { mesh_hash u8[32], primitive_index u32, reserved u32[3] }[mesh_count]
                (48 B each, at byte 96; sorted by (mesh_hash bytes, primitive_index) ascending, no duplicates)
instances       InstanceRecord[instance_count]   (256-byte aligned after the mesh table; sorted by
                (cell_y, cell_x, mesh, stable_id) so identical inputs give identical bytes)
InstanceRecord (96 B)
  transform      f32[12]  3x4 row-major, mesh space to world space (Creation units, engine axes)
  mesh           u32      index into the mesh table
  cell_x, cell_y i32      the source cell in absolute world grid coordinates (as `source_cells`
                          in `lod_chunks`, not relative to the LOD origin), for the cell loader's ownership match
  stable_id      u8[16]   #185 stable reference identity, so overrides and saves can address the instance; required for a shipped `cluster` package; zero only in test fixtures until #185 lands
  flags          u32      bit 0: the instance is placed in a child worldspace of the block's (source_worldspace_id); others 0
  base_form_id   u32      the base object's id in the same package's world database (`statics.id`), 0 if unknown
  snow_material  u32      the base object's snow material (`statics.material_object`, a `MATO` form id), 0 = none
  source_worldspace_id u32 the child worldspace the object is placed in (`WhiterunWorld`, say), 0 when it is placed in the
                          block's own worldspace; non-zero iff flags bit 0
  reserved       u8[4]    zero; a reader rejects a block with a non-zero byte here
```

Instance payloads hold no plugin names or load-order slots. `base_form_id` and `snow_material`
are the ids the package's own world database uses (`statics`, `matos`), so they are valid for
that package only and are part of the block's input key: a conversion that renumbers them
rewrites the block. Snow belongs to the base object, not to the reference, so the engine reads
the per-object material through these two fields exactly as the stock path does
(`snow.rs`: DNAM maximum angle plus the `MATO` on the STAT); a snow-covered reference is an
ordinary cluster instance and no rule excludes it. `snow_material` is 0 when the database was
converted before schema 16, which adds `statics.material_object`; `base_form_id` still lets the
engine look the object up itself. The `stable_id` field takes #185's identity once that
contract is agreed; until then it is zero and the engine addresses instances only by chunk and
index. Format version 3 (version 2 grew the instance record from 80 to 96 bytes; version 3 adds
`source_worldspace_id` and the flag; both payload kinds carry the version); nothing outside
prototype outputs has used versions 1 or 2.

Child worldspaces. A worldspace without its own LOD origin, such as `WhiterunWorld` (the city)
or `WhiterunDragonsreachWorld`, has no blocks. When its coordinates are its parent's (checked
for `WhiterunWorld` by comparing reference heights per cell against Tamriel's; not true of every
child worldspace, which is why the stage takes an explicit list), the converter places its
eligible references into the parent's blocks with the parent's origin and block grid, sets
`source_worldspace_id` to the child and flags bit 0. Ownership: `cell_x`/`cell_y` are the cell
of the reference's position as before; no parent cell loads a child's references, so nothing
hides these instances while the player is in the parent worldspace, which is what a distant view
wants. While the player is inside the child worldspace the engine hides the instances whose
`source_worldspace_id` equals it, because the real objects are loaded there.

**Database rows** (optional tables, section 6):

```sql
CREATE TABLE IF NOT EXISTS cluster_build (
    id INTEGER PRIMARY KEY CHECK (id = 1), builder_identity TEXT NOT NULL,
    triangle_threshold INTEGER NOT NULL, block_side_cells INTEGER NOT NULL,
    lod_mode TEXT NOT NULL CHECK (lod_mode = 'cluster')
);
CREATE TABLE IF NOT EXISTS cluster_meshes (
    mesh_hash TEXT NOT NULL, primitive_index INTEGER NOT NULL,
    payload_path TEXT NOT NULL, content_hash TEXT NOT NULL,
    source_triangles INTEGER NOT NULL, cluster_count INTEGER NOT NULL,
    vertex_count INTEGER NOT NULL, payload_bytes INTEGER NOT NULL,
    material_key TEXT NOT NULL,
    model_path TEXT NOT NULL DEFAULT '',  -- converted model the primitive came from
    material_index INTEGER,               -- the primitive's glTF material slot in that model; NULL = none
    alpha_mode TEXT NOT NULL DEFAULT 'opaque',  -- 'opaque' or 'mask' (cutout); blended primitives are rejected
    alpha_cutoff REAL,                    -- the material's alphaCutoff for 'mask' (glTF default 0.5), else NULL
    double_sided INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mesh_hash, primitive_index)
);
CREATE TABLE IF NOT EXISTS cluster_chunks (
    worldspace_id INTEGER NOT NULL, tier INTEGER NOT NULL,
    anchor_x INTEGER NOT NULL, anchor_y INTEGER NOT NULL,
    payload_path TEXT NOT NULL, content_hash TEXT NOT NULL, input_key TEXT NOT NULL,
    bounds_min_x REAL NOT NULL, bounds_min_y REAL NOT NULL, bounds_min_z REAL NOT NULL,
    bounds_max_x REAL NOT NULL, bounds_max_y REAL NOT NULL, bounds_max_z REAL NOT NULL,
    instance_count INTEGER NOT NULL, source_cells TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (worldspace_id, tier, anchor_x, anchor_y)
);
CREATE VIRTUAL TABLE IF NOT EXISTS cluster_chunks_spatial USING rtree(
    id, minX, maxX, minY, maxY, +worldspace_id, +tier, +anchor_x, +anchor_y
);
CREATE TABLE IF NOT EXISTS cluster_rejects (
    mesh_hash TEXT NOT NULL, primitive_index INTEGER NOT NULL,
    reason TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (mesh_hash, primitive_index)
);
```

- `content_hash` is the SHA-256 of the payload file, verified before use as `lod_chunks`
  does today.
- `input_key` is the #185 key for the block: the hash of the worldspace's `LodOrigin` (an
  origin change moves cells between blocks) and its member cells' input keys (records,
  references, base records and the `output_hash` of every mesh they use). A
  conversion recomputes the keys (measured at about 1 ms per interior and microseconds per
  exterior cell) and rewrites only the blocks whose key changed; a plugin reorder that changes
  no winner rewrites nothing. Until #185 lands, the converter fills `input_key` with the
  hash of the block's instance payload and rebuilds every block each conversion, which is
  correct but slow. The column exists from the start so the engine and tools never change.
- `material_key` is the SHA-256 of the primitive's glTF material object as canonical JSON
  (sorted keys, no whitespace) with every image reference replaced by the converted
  texture's asset key (for example `textures/.../x.ktx2`), so the engine can group clusters
  by material and build the material's bind group without opening the source GLB. Whether
  the engine reuses its existing material assets for that key is VG7's call.
- `model_path` and `material_index` name the stock material directly: `model_path` is the
  converted model (for example `meshes/architecture/x.glb`) the primitive was built from and
  `material_index` is its glTF `materials` slot in that model (alpha mode, cutoff and double-sided
  state are also stored as explicit columns; `material_key` hashes the whole material, so a change
  to any of them changes the key) (`NULL` when the primitive names
  no material, which glTF draws with its default). `(model_path, material_index)` finds the
  converted material the stock path loads for the same model; `<N>` in the payload path is
  not the glTF primitive index, so the engine reads both columns, never derives them. Several
  models with byte-identical content share one `mesh_hash`; the row names the first of them
  in path order, and the material and textures are the same for all of them because they are
  part of the hashed bytes. These two columns were added after the first prototype; the
  payload formats (`.mcl`, `.mci`) are unchanged and the builder identity changes with the
  converter sources, so an older output is rebuilt, not read with missing columns.
- `cluster_rejects` records why a primitive got no clusters (section 7), for the inspector
  and for mod authors; it is data, not a log line.

Publication follows the LOD compiler: payloads are written to staging, every referenced
payload is validated, the tables are written, and the whole output is swapped in one step.
A conversion whose plugin inputs changed mid-run refuses to publish, as `lod_build` does.

### 4. Vertex data layout

Positions stay `f32` in mesh space: an instance transform applies at draw time, and
quantising positions would move vertices differently per mesh and reopen cracks the builder
locked. Normals and tangents are octahedral `snorm16x2` (4 bytes each): the unit vector is
projected onto the octahedron (`v / (|x|+|y|+|z|)`), the lower hemisphere is folded with
`(1-|y|)·sign(x), (1-|x|)·sign(y)`, and each component is rounded to `snorm16` (Cigolle et
al. 2014, "oct" encoding); the tangent's handedness is the least significant bit of its
second component, set after rounding. UVs are `unorm16x2` against per-primitive `uv_min`
and `uv_scale` (`uv = uv_min + q / 65535 · uv_scale`), so tiled UVs far outside 0..1 keep
full precision. Vertex colours are `unorm8x4`, present only when the source primitive has
`COLOR_0` (Skyrim's `SLSF2_Vertex_Colors` meshes). Streams are separate sections, not
interleaved, so optional streams cost nothing when absent and the vertex shader reads only
what the material needs. 24 bytes per vertex without colours, 28 with.

The vertex set is the subset of source vertices any cluster at any level references, in the
builder's spatially sorted order. meshoptimizer's simplifier collapses onto existing vertices
and creates none, so no level needs vertices the source lacks. Attribute-aware simplification
(normals, UVs) is on; its weights are builder settings and in the identity.

### 5. Eligibility

Eligibility is decided by **one shared classifier**, used by the baked object chunks (#106)
and the cluster stage alike, so a reference is never in both paths and never in neither by
accident. The classifier runs per reference and per primitive and records the first failing
rule as the reason.

Reference rules (the same as #106 Phase 2, in the same order): the base record is a `STAT`
or another fixed static type the baked path accepts; the reference is in an exterior cell of a
worldspace with a valid LOD origin; not initially disabled; no enable parent; not movable,
animated, or a large reference whose policy is unresolved; a resolved base model. Trees
(`TREE` records and the baked billboard path), terrain, water, sky, doors, actors and
anything with scripts that can move it stay on their current paths.

Primitive rules: indexed triangles; `alphaMode` `OPAQUE`; `shaderFamily` `lighting`
(not `effect`); not skinned (no `JOINTS_0`, no skin on the node); not a non-rendering
stand-in (`extras.openSkyrim.nonRenderingExclusion`); material features inside the
supported set (base colour, normal, specular, vertex colour, double-sided; parallax,
environment map, glow and other `OPEN_SKYRIM_material` features are reject reasons until
VG7 adds them); and at least **`triangle_threshold`** triangles. The threshold is a
converter setting (default proposal 1024; VG2 found primitives under 128 triangles are a
single cluster and the plan's estimate is that under about 2,000 the hierarchy buys
nothing; VG5's comparison against plain LOD chains sets the final default) and part of the
builder identity.

A model whose primitives split between eligible and rejected is allowed: the eligible
primitives draw through clusters at every distance, the rejected ones through the baked or stock
path. Both parts are owned by exactly one path each, so the model is never half-missing or doubled.

The classifier's eligible set is what the engine may draw through clusters. It does not change
the baked object chunks: the one-set option policy (2026-10-16) keeps every baked table and file
as `baked` mode builds it, and supersedes the exclusion set of earlier drafts. Which of the two
draws a primitive is decided at runtime from the option and the GPU, not at conversion.

### 6. Optional tables, no schema bump

The five tables are additive and probed at runtime with
`SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE ...)`, exactly as `lod_chunks` is probed
today. `WORLD_DATABASE_SCHEMA_VERSION` stays 7 and `CONVERTER_SCHEMA_VERSION` stays 24: a
package without cluster tables is a complete, valid package, and a package with them is
accepted by an engine that does not know them. This matches the policy proposed in #113
(additive changes outside major releases). The converter writes the tables only in `cluster` conversion mode
(below). A cluster-related schema change
later (a new column) is additive too; a layout change in the payload is a
`payload_format` bump inside the builder identity, which rebuilds the payloads and leaves
the database schema alone.

The `cluster-manifest.json` beside `lod-manifest.json` repeats the builder identity, the
format version, the mesh and block counts and the reject totals. It is derived from the final
store (tables and files) by the base publisher after the last stage that adds geometry, the
terrain stage included, so it describes the whole store.

The reference mapping (`shared::cluster_mapping`, `instance_map_version` 1 and `coverage_version` 1,
declared in the manifest) belongs to the same store and is hashed with it. `cluster_provenance`
binds it to the core `built_from` and the builder identity; `cluster_instance_map` maps every
record of every listed `.mci`, by the file's content hash and the record's final ordinal taken from
the writer's real sort order, to its dataset reference id, placing worldspace, cell, mesh and
primitive and an input digest; `cluster_reference_coverage` lists, for every reference the
converter looked at, either one row per primitive of its model (`clustered`, or `rejected` with
the reason) or one whole-reference `not_eligible` row (a classifier rule, or an unresolved
override such as `XMSP`, `XALP` or `XEMI`). A reference with no row is unknown, never complete.
Synthetic instances such as terrain blocks are not placed references and have no rows. The MCI
format and `stable_id` are unchanged. `validate_mapping` checks all of it against the tables and
the files; the runtime treats a failure as "mapping off" for the package. A package-level check,
`validate_cluster_build_contract` in the shared crate beside `validate_lod_build_contract`,
runs at startup when the setting is on and the tables exist: the manifest's identity equals
the `cluster_build` row and is a canonical SHA-256, the manifest's counts equal the table
row counts, and the format version is one the engine reads. A failed check turns the cluster
path off for that session with one error naming the reason; it never fails the package,
because the tables are optional.

**Conversion mode: one path per package, chosen before conversion.** The goal is that a PC
that can run clusters never pays for baked object LODs, and a PC that cannot gets the
classic baked LODs of Skyrim SE with DynDOLOD. The converter takes `--lod-mode
auto|cluster|baked`.

- `auto` runs a **capability probe** before any LOD work starts: it asks the GPU adapter
  of the converting machine, through the same wgpu version the engine uses, for what the
  cluster path requires (`INDIRECT_FIRST_INSTANCE`, storage-buffer sizes and binding counts
  at least those VG7 declares, and a backend that is Vulkan, DX12 or Metal). All required
  items present means `cluster` mode; anything missing means `baked` mode, with the missing
  item in the conversion report. A headless or adapter-less machine gets `baked`.
- `cluster` mode builds the cluster tables and payloads and leaves every baked table and file
  as `baked` mode builds it (see section 5).
- `baked` mode builds exactly what the converter builds today and writes no cluster table.
- The probe record (backend, adapter name, features and limits as probed) describes the PC that
  converted, not the dataset, so it is not stored in the package: the conversion writes it as
  `cluster-probe.json`, with `cluster-report.json` (timings, memory), into the run-artifact
  folder the caller chooses, outside the converted root. The runtime probes its own adapter.
  `cluster_build` keeps only the mode. Forcing a mode overrides the probe, for CI and for
  packages converted on one PC and played on another.
- Baked chunks and their input keys are the same in every mode; `cluster` mode only adds.
- Until VG10 decides, the default of `--lod-mode` is `baked`; `auto` and `cluster` are
  opt-in. After a go, the default becomes `auto`.

### 7. Runtime: setting, selection, ownership, failures

**Setting.** A graphics setting `cluster_lod` (`auto` | `off`; CLI `--cluster-lod`, default
`auto`; the converter side is `--lod-mode`, section 6) controls the path. On a package without cluster tables (`baked` mode, or any package
made before this ADR) nothing happens: the tables are never read, no cluster asset is
loaded, no render node runs, and the frame is identical to today's; VG7 measures that with
the existing bench scenes. On a `cluster` mode package, `auto` runs the path when the
engine's adapter passes the same check the converter's probe used, and `off` forces it off.
In `cluster` mode packages, the eligible objects exist only as clusters, so when the path is
off, fails its startup check or the adapter lacks a required feature, those objects are
absent. The engine logs one error that says so and names the fix (reconvert with
`--lod-mode baked`); it never silently draws something else. `cluster_lod_error_pixels` (default 1.0) is the
screen-space error threshold. There is no distance cap and no instance budget: the far
plane is the largest configured LOD reach, as today.

**Streaming.** Cluster chunks are queried with the same R-tree, generation, centre and
radius as LOD chunks (`LodStreaming`'s query path, parameterised by table), out to the
largest configured reach; blocks leave when they fall outside the unload radius. A block
loads its instance payload and the mesh cluster payloads its mesh table names (shared
across blocks by handle; a mesh payload is unloaded when no resident block references it).
A block is **Ready only when its instance payload and every mesh payload in its table are
validated and resident**; until then it draws nothing for its eligible objects (the rest of
the model stays on its stock path). A terminal mesh failure makes every block that names that mesh Failed (logged once
per mesh, counted once per block), so a block never half-draws. Loads go through the asset
server under the existing per-frame commit and upload budgets.

**Ownership, not handoff.** The cluster path is one system at every distance. An eligible
primitive of an eligible reference is drawn by clusters from the far plane to the player's
feet, and by nothing else: the cell loader does not spawn a renderable for it, and the baked
object chunks (#106) built in `cluster` mode do not contain it. There is therefore no
near/far swap for these objects, no overlap to hide and no per-cell hide bit to flip; the
cluster error threshold alone decides how much detail each part gets. Everything else about
a loaded cell is unchanged and stays with the stock path: colliders, physics and navmesh
data, scripts, lights, the rejected primitives of the same model, trees, terrain and
anything not in an exterior cell.

This needs two things:

- **The cell loader must know which references the cluster path owns.** The authority is the
  instance block: a reference is owned if the block covering its cell lists it. That match
  needs a stable key per reference. It cannot be a FormID or a path baked into the payload
  (section 3), so the `stable_id` field of #185 stops being optional for `cluster` mode: until
  #185 supplies it, VG8 and VG7 key the fixture by (cell, mesh, transform) and the real
  package is not allowed to ship (open question 2).
- **A near cell is not ready until its cluster block is.** A cell inside the full-detail
  radius counts as not ready to present until the cluster block that owns its eligible
  objects is Ready, the same way it already waits for its own models. Otherwise a failed or
  late block leaves a hole in the world next to the player, which is far worse than at
  distance. A terminal failure of a near block is shown to the player once, with the key and
  reason; it never silently draws another form.

A cluster block that is missing, failed or not yet loaded at distance shows nothing for its
eligible objects, for the same duration a missing baked chunk shows nothing today. That is
the cost of having no baked copy; the safety net is the conversion mode above, chosen once
per package, not a per-frame fallback.

**Material parity.** Because the cluster shader now draws these objects close up, the
supported material set of section 5 is defined as exactly what the stock path's shading
does for them: base colour, normal, specular, vertex colour, double-sided, shadow casting
and receiving, fog and the same lighting model. A feature the cluster shader does not
reproduce is a reject reason, so those primitives stay on the stock path. VG7's acceptance
is an image diff of the same scene through both paths at the near range; a difference above
a stated tolerance fails the step.

**Failures.** `.mcl` and `.mci` loads reuse `LodChunkFailureClass`: transient (I/O, busy)
retries at most three times after 1, 2 and 4 seconds within the generation; terminal
(missing payload, hash mismatch, header or builder identity mismatch, section bounds out of
file, non-monotonic errors) fails closed, is counted in `cluster/chunks_failed` and
`cluster/meshes_failed`, logged once with key and reason, and shown in the unrecovered
gauge. A failed mesh payload fails the blocks that name it (above). Converter rejects
(section 5) are not failures: a rejected primitive is not in the eligible set, so only the
baked chunk or the stock path draws it, and the reason is in `cluster_rejects` and in the
conversion report's totals by reason.

**Per frame, the engine reads:** the instance buffer (96 B per instance), the per-instance
visibility bits, each resident mesh's `ClusterRecord`s (48 B per cluster) and, when a BVH
is used, its nodes. The cull pass is per view (main camera, shadow cascades, and secondary
views unless the view policy of #187 excludes them): instance frustum and visibility test,
then cluster selection by the rule in section 2, writing survivors to a compacted list;
a second pass writes one `DrawIndexedIndirectArgs` per survivor with the cluster id in
`first_instance`. The draw uses `multi_draw_indexed_indirect_count` where the adapter has
`MULTI_DRAW_INDIRECT_COUNT` (Vulkan and DX12 in wgpu 29.0.4; Bevy 0.19 avoids it on DX12).
Without it (Metal, GL, WebGPU, and DX12 until that changes) the engine issues
`multi_draw_indexed_indirect` with a fixed count equal to the argument buffer's capacity,
and the compaction pass zeroes the `index_count` of every unused slot, so no readback and no
frame of latency; the cost of walking empty slots is unmeasured and VG7 measures it on the
far-view scenes. `INDIRECT_FIRST_INSTANCE` is required for the path to enable at all; without
it the setting stays off with one logged reason. The vertex shader fetches the cluster's local
triangle, its u32 vertex reference and the vertex streams from storage buffers; there is no
index buffer.

**GPU residency.** The unit of upload is one mesh payload section (256-byte aligned), placed
in pooled storage buffers (64 MiB each, first-fit with a free list) with a per-mesh
descriptor table giving section base offsets; the cluster record stores offsets relative
to its mesh, and the descriptor adds the base. A mesh is wholly resident or absent in this
version; paging clusters within a mesh is deferred until a measurement shows the far-ring
resident set does not fit (estimate: the 16-cell blocks within the reach hold a few hundred
MB at most, to be measured by VG7). Uploads count against `max_upload_mib_per_frame`.

**Room culling and secondary views.** The cluster path draws exterior statics only in this
version, so interior room culling (#188) does not meet it. The per-instance visibility bits
are the hook for later: a room-visibility pass can clear bits for instances in hidden rooms
without touching the payload. Portal and reflection views run the same cull pass per view or
skip the node entirely under #187's view policy; the choice is a setting, measured by VG7.

### 8. Interface for VG7 (engine prototype) and VG8 (converter stage)

Task ids follow `local/plans/task-queue.md` (VG7 engine prototype, VG8 converter stage); the
older `virtualized-geometry-task-order.md` numbered them one lower. Both start from this
section and the fixture, not from each other's code.

- **Shared contract** (`crates/shared/src/cluster.rs`, proposed; the shared crate is a
  gated change and goes through the user): the header and record structs above with
  `#[repr(C)]` layouts and `const` offsets, `read_mcl`/`read_mci` that validate magic,
  version, section bounds and monotonic errors and return borrowed sections, and
  `write_mcl`/`write_mci`. The DDL strings for the five tables. `BuilderIdentityInputs` and
  `builder_identity()`.
- **Fixture:** a tiny deterministic `.mcl` and `.mci` pair generated from `dummy-content`
  meshes (one eligible primitive of a few thousand triangles, one instance block with three
  instances in two cells), committed as test data with its expected hashes. VG7 renders the
  fixture; VG8's first acceptance test is byte-equality with it.
- **VG8 owns:** `crates/converter/src/cluster/` (classifier adapter, builder call through
  `meshopt-cluster`, payload writers, tables, manifest, report), the `--lod-mode` flag with
  its capability probe, and the reject
  reasons. Acceptance: fixture equality; a rebuild of identical inputs produces
  identical payload hashes; changing one mesh changes one `.mcl` and only the blocks that
  place it; the eligible opaque set of a converted vanilla build is counted and timed; and,
  for the maintainer's comparison, the same eligible set run through meshoptimizer's plain
  LOD chain (VG5's method) at matched error, reporting time, bytes and draw counts side by
  side.
- **VG7 owns:** `crates/engine/src/cluster/` (asset loaders, streaming queries against the
  tables, GPU pools, the cull and draw render node, the setting, metrics) and the small
  hook in the cell loader that skips owned primitives and waits on the cluster block, reading the
  fixture and then VG8's output. Acceptance: a package without cluster tables is frame-identical to main;
  an image diff of the fixture against the same meshes drawn by the stock path; the far-view
  scenes `lod-far` and `lod-flythrough` with cost per view reported.
- **Neither changes:** `lod_chunks`, the terrain path, the eligibility rules outside the
  shared classifier, or `WORLD_DATABASE_SCHEMA_VERSION`.

**Prototype proof (what the demo must show).** Before any issue or PR is opened upstream,
the two modes run on the same input and the same machine and report these side by side:

1. **Conversion time, per stage.** The wall-clock of the LOD stage in `baked` mode and in
   `cluster` mode, split into: the capability probe, the baked object chunk build, the
   cluster build, and the rest of the LOD work. Each is run cold (no cache) and warm (cache
   from an earlier run), and once more after changing one plugin, to show what a load order
   change costs in each mode. The headline number is total conversion time of the whole
   package, not only the LOD stage. This tests the claim that `cluster` mode saves time:
   it holds only if the baked object bake it skips costs more than the cluster build it
   adds, and the report must say plainly if it does not.
2. **Bytes on disk** of each mode's LOD output, and peak memory of the converter.
3. **Image difference** against full detail for each mode at matched screen error, near and far.
4. **Frame time** on the `lod-far` and `lod-flythrough` scenes, per view.
5. **Detail pops**: the same flythrough captured for both modes, side by side.

The numbers are written to one report file next to the fixture, so the comparison can be
rerun by anyone with the same data.

## Consequences

- Mesh clusters are built once per unique mesh content and survive any plugin, load-order
  or profile change; only small instance blocks follow the plugins. A mod that adds a dense
  rock gets cluster LOD, at every distance, from its first conversion with no extra tool.
- Any builder, meshoptimizer or layout change rebuilds every payload, by identity, and an
  old payload can never be read as a new one. Mixed-generation packages fail closed per
  payload with the reason logged; the eligible objects of a failed payload are absent, not
swapped for something else.
- The database schema does not change; packages without the tables stay valid; engines that
  do not know the tables ignore them. The cost is five more probed tables and one more
  manifest file.
- The engine gains a second residency unit (mesh payloads shared across blocks) inside the
  existing streaming generation and budget machinery, not beside it. A reference is owned by exactly
  one path (clusters or stock), so it is never drawn twice and never swapped at a distance.
- Positions stay `f32` and every level references source vertices: no new cracks, and the
  image diff against the stock path is a fair test. The payload is about twice the size of
  cluster records alone (about 28 bytes per source triangle).
- The far-view frame cost of cull passes per view is not yet measured; VG7 measures it
  before VG9 integrates anything, and the converter's `--lod-mode` default stays `baked`
  until VG10.
- A `cluster` package saves the baked object LOD bake time and disk for every eligible
  object, and its eligible objects have no baked copy: it needs a GPU that passes the probe.
  A `baked` package is today's behaviour, and nothing else changes for it.

## Open questions (marked for discussion)

1. **Which upstream issue defines the project schema this must follow (queue E9)?** The
   record-schema series #156 to #160 is closed, and #202 is the streaming plan. If no world
   database schema proposal is open, this ADR proposes the additive-table rule of section 6
   under #113 and asks the maintainer to confirm it on #203.
2. **#185 decisions** this ADR depends on: the stable ID format and length (the `stable_id`
   field is sized at 16 bytes; a longer identity needs a side table), where `record_hash`
   comes from, and the exterior unit (this ADR assumes a block of cells aligned with the LOD
   grid, the note's recommendation).
2a. **Near-range ownership.** Clusters draw at every distance (section 7), so the cell loader
   must skip the eligible primitives it would spawn today, and a near cell waits for its
   cluster block. That touches the cell and streaming code owned elsewhere (not only
   `crates/engine/src/cluster/`), and needs the stable per-reference key of question 2. VG7
   should size that change in its first week and report it before building more; it is the
   largest cost this ADR commits to. Interiors stay on the stock path in this version.
2b. **Material parity near the camera.** Parallax, glow, environment maps and similar
   features are rejected today. If a large share of eligible meshes carry them, the cluster
   shader grows (VG7) or the share of owned objects shrinks; the conversion report counts
   rejects by reason so the number is known before that choice.
3. **Baked object chunks.** Settled by the one-set option policy (2026-10-16): baked output
   stays complete in every mode, the cluster payloads are additive, and no exclusion set is
   handed to the baked builder (#106 is Chef's).
3a. **Package moved to another PC.** The probe runs on the converting machine. A `cluster`
   package played on a PC whose adapter fails the check keeps its baked objects, which are
   always present; the runtime probes its own adapter and picks the path.
4. **Triangle threshold default** (1024 proposed) waits for VG5's matched-error comparison
   of plain LOD chains against clusters.
5. **Material set** supported by the cluster vertex and fragment shaders in VG7, and how
   textures bind (texture arrays per material family versus bindless; Metal has no partially
   bound arrays).
6. **Block side** (16 cells; 4 or 8 are the only alternatives the shared tier type allows) and
   whether interiors ever join (not in this version).
7. **Builder threading and caching inside one conversion**: per-mesh payloads are an obvious
   cache by `mesh_hash`; whether the cache lives in the ingestion cache or the staging area is
   VG8's call within the existing cache contract.

## Foundations check

- **No shortcuts:** the same source vertices and materials are drawn; the cut is by measured
  screen error, not distance caps; an image diff and the bench scenes gate every step.
- **Right layer, measured:** the hierarchy is built offline once per mesh content; the engine
  reads flat GPU tables; VG2 timed the builder and VG7 times the frame before integration.
- **Data-driven:** no FormIDs, plugin names, paths or magic distances in any payload or key;
  thresholds and block size are converter settings carried in the identity.
- **Moddable:** derived data keyed on content, rebuilt only where inputs changed, stable
  identities reserved for #185, overrides and saves untouched, the stock path always drawable.
- **Aligned and licence-clean:** meshoptimizer and `clusterlod.h` are MIT and vendored with
  their licence; no GPL source is read or copied.
