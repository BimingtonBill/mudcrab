# Cluster payload adoption into the OP6 data set

2026-10-17. OP6 owns the envelope/data capability contract; VG8 owns production and VG7 owns
cluster readers and Showcase adoption. This reserves the IDs and sets validation expectations.
The cluster writers/readers are **not integrated on OP6 yet**. VG7/VG8 have acknowledged the
shared-store/terrain-metadata split. Final mapping schemas and the checks below must be reviewed
and implemented before regeneration. No separate private-schema
package is approved by this contract.

Inputs read: OP6 extension spec and implementation; VG7's approved
`docs/research/cluster-showcase-demo-plan-2026-10-17.md`; VG8 ADR-0013, shared cluster format and
converter stage; VG7 `cluster/terrain.rs`. ADR-0013's old schema 24/7 and mode-exclusive removal
of baked content are superseded by the approved one-tree policy.

## Envelope and ownership

Use the existing `conversion-manifest.json` field exactly:
`extensions: [{id, version, owns, sha256, built_from, stats}]`. Core converter/database versions
remain main's (26/9 at this base). Do not encode cluster availability in core version numbers.

| Extension | Wrapper version | Exclusive ownership |
|---|---|---|
| `mudcrab.clusters` (`shared::extensions::CLUSTERS`) | 1 | Shared object/terrain geometry store: `cluster_build`, `cluster_meshes`, `cluster_chunks`, `cluster_chunks_spatial`, its SQLite R-tree shadow tables (`cluster_chunks_spatial_node`, `_parent`, `_rowid`), `cluster_rejects`; reserved companion tables `cluster_instance_map`, `cluster_reference_coverage`, `cluster_provenance` (if a separate header is needed); `cluster-manifest.json`; all payload files under `clusters/`. List any explicit secondary indexes too. |
| `mudcrab.cluster_terrain` (`shared::extensions::CLUSTER_TERRAIN`) | 1 | `cluster_terrain`, `cluster_terrain_heights`, `cluster_terrain_build`; `cluster-terrain-manifest.json`; any additional terrain-only file outside `clusters/` must be explicitly listed before generation. |

Shared geometry tables cannot be split by row between extensions: OP6 ownership covers whole
tables/files. Terrain `.mcl`/`.mci` geometry and atlases under `clusters/` belong to the base store;
the terrain extension owns their interpretation/height metadata. It does not also own `clusters/`.
Unrelated tools, reports and benchmark alternatives must not be placed inside this owned directory.
Confirm actual generated paths and SQLite objects from a dummy fixture before freezing these lists.
Ownership is assigned to an extension, not to a producing stage: both object and terrain stages may
populate the base store. They must finalize it jointly before publishing the base hash. Existing
terrain rows/files need not move merely because their producer is a different stage.

`cluster-report.json` currently contains timings. Put such reports outside the converted root as
run artifacts. If retained inside the data set, declare ownership and hash all bytes; there is no
"owned but excluded from the hash" exception in `mudcrab-ext-hash-v1`.
VG8 may remove machine-specific `cluster_build.probe_json` before freezing wrapper v1 and record
adapter/driver probe output beside those run artifacts. Deterministic requirements derived from
the produced payload remain in the cluster manifest; the runtime checks its own device.

The envelope starts at version 1. Existing MCL/MCI binary format version 3 and terrain-build version
1 remain separate identities; wrapper v1 must explicitly declare which it contains. A reader must
check all of them. Incompatible layout/ownership/semantics changes require a wrapper version bump;
builder configuration/source changes alter builder identity even when the binary layout is unchanged.

## Hash, provenance and dependencies

- `sha256` is the existing **`mudcrab-ext-hash-v1`** content hash, computed by
  `ExtensionHasher` and independently checked with `tools/extension_hash.py`. It covers all owned
  tables, indexes and file bytes, including payload-owned manifests. It is not a builder ID or
  a hash of an arbitrarily printed manifest. Ownership lists are sorted, relative and disjoint.
- `built_from` is the current core `lod_build.build_identity`, for both extensions. Cluster adoption
  requires that identity to exist; do not use an empty identity or a cluster builder ID in its place.
  Rebinding it through `refresh-extensions` cannot regenerate stale content.
- Keep `cluster_build.builder_identity`, each MCL/MCI header's builder identity, source GLB hashes,
  per-file content hashes and per-block `input_key` checks. Builder identity covers relevant vendored
  sources, configuration, eligibility and vertex layout. Plugin/load-order and source-asset changes
  invalidate derived placements/geometry at their responsible layer; reuse unchanged verified meshes.
  VG7 has deferred stored-tangent generation for the first shared-data adoption: retain current
  runtime generation and eligibility. A later stored-tangent optimization must match the runtime
  generator on the same decoded payload and change builder identity; no steady-state performance
  gain is established. New provenance/coverage mapping belongs in this adoption's reviewed builder
  transition. Companion mapping tables are owned by the base extension and must bind actual final
  instance order to dataset-scoped reference/primitive/source identity; counts alone do not prove
  complete draw-ready replacement coverage. Their exact schema remains VG7/VG8 work before freezing.
  The base manifest declares `instance_map_version: 1` and `coverage_version: 1`. Expected primitive
  identities and clustered/rejected/not-eligible status/reasons must share the same resolved source
  inputs as the mapping. An unresolved reference override must be rejected explicitly; a constant
  provenance value cannot certify it. Reserved `stable_id` semantics remain unchanged.
- The generic OP6 envelope currently has **no `requires` field**. Do not rely on an ignored JSON
  field to enforce dependencies. Record terrain's dependency in its owned
  `cluster-terrain-manifest.json` as
  `requires: [{id: "mudcrab.clusters", version: 1, sha256: "<exact base extension hash>"}]`, plus
  the SHA-256 of the actual core `cell_cache.rkyv`, the terrain-build version and terrain builder/input
  identities. VG7 must validate these before reporting terrain capability as usable.
- Publication order is acyclic: finish and validate all object and terrain geometry in the base
  store, refresh its final counts, write its manifest and compute
  its extension hash; record that hash in the terrain dependency manifest; then hash terrain metadata.
  The base manifest must not contain the dependent terrain extension's hash.
- Geometry that bakes extension-derived material behavior must record the exact material-extension
  input digest in its owned manifest and validate it. Do not make every geometry store depend on
  snow, doors or lighting merely because those extensions exist; declare only actual inputs.

Missing/stale/unsupported dependencies produce a capability reason naming the dependency and remedy.
No requested cluster path may silently substitute another path or hide failed coverage. A selected
but unusable option remains a recorded user selection; the functional development UI/CLI reports its
unavailability separately. No Skyrim UI implementation is required here.

## Regeneration and integration gates

1. VG8 confirms exact owned paths/tables and the object/terrain shared-store boundary with OP6/VG7.
   Wire specs and readers through this envelope; current OP6 reserves IDs but does not emit them.
2. Prove with fixtures: clusters off preserves core byte parity; clusters on adds only owned content;
   main loads the result; Rust/Python extension hashes agree; missing/stale/version-mismatched
   dependencies visibly disable the requested capability; refresh cannot bless stale data.
3. Keep baked core output and stock/collision data intact. The same data set must support clusters
   on/off and an engine that ignores cluster extensions. Representation ownership changes occur in
   runtime selection/readiness, not by deleting or rewriting core baked payloads during conversion.
4. Regenerate additively only after the envelope/dependency checks and exact producer/reader commits
   are agreed. Use the existing data set, reuse verified assets, disk guard and asset lock; do not
   create a new full set or delete old data as part of adoption without the separately required reason/yes.
5. Serialize integration in `D:/Mudcrab/merge/dev-integration`. Its clean working tree does not
   settle ownership of its four room-culling commits. Resolve that history with its owner/B5 before
   OP6 integration; VG7 follows the reviewed OP6 integration base. OP6 does not merge VG7.

Default options remain vanilla; Showcase selects clusters explicitly. Disabled options register no
cluster systems/passes and load no cluster payloads. Data capability and user selection remain
separate even in the functional development settings panel.
