# Cluster path by ownership, chosen at launch

Status: proposal, 2026-10-19, reviewed (see Review). Branch: `opt/cluster-showcase-integration`.

## Summary

The clusters-on demo spends a large part of each frame deciding, per object and per view, which
of three representations draws it: the stock model, the baked object LOD, or the cluster.
ADR-0013 section 7 chose a different design ("Ownership, not handoff"); the implementation
drifted into a runtime handoff after the one-set option policy of 2026-10-16 moved the choice
from conversion to runtime. This proposal returns to ownership and moves every choice that does
not change during play out of the frame:

1. The representation is chosen **once, at launch** (vanilla or cluster), never in game.
   Switching is a restart. Each mode installs only its own systems.
2. In cluster mode the cluster path **owns** every eligible static primitive at every distance.
   Nothing hides or swaps it per frame.
3. Facts that are static are **decided at conversion**: ownership, a defined far-visibility rule
   per instance, the placed world and cell of each instance.
4. What a view draws is decided **on the GPU per cell and per view domain**, from a small table.
   The CPU does not walk instances per frame.
5. Readiness proofs shrink from per instance to **per owned block and batch**; the crossing
   proof itself stays.

Arbitration is not the whole gap. Removing all of it leaves about 12.6 ms a frame by the table
below; render encoding and submission (`render_system` 4.9 ms, command submission 2.8 ms) are
the next target and need exclusive attribution first.

## Why (measured)

Tracy, clusters-on tour bench, all batches preloaded (`518e56cf`,
`D:/Mudcrab/perf/2026-10-19-preload-par`), per-frame means after the startup load:

| Where | ms / frame | What it does |
|---|---:|---|
| Extract (blocks main and render threads) | 11.0 | |
| of which `extract_sources` | 8.2 | which stock objects are loaded, visible, qualified to hand over |
| of which `native_lod::extract` | 2.1 | walks every entity under every LOD root and resident cell |
| Render thread | 18.6 | |
| of which `select_instances` | 5.4 | per view: instance mask, stock overlay, material checks |
| of which `render_system` + command submission | 7.7 | CPU encoding, submission, presentation (spans may nest) |
| of which `native_lod::filter` | 1.3 | baked LOD vs near/cluster geometry, per view, per visible entity |
| Main schedule | 10.8 | incl. visibility and transform passes over all stock entities |
| **Frame** | **~30** | extract + max(main, render) |

The cluster prototype (clusters only) ran its render thread in about 7.1 ms (~143 fps at the
`lod-far` pose). That is a comparison point, not proof that arbitration explains the whole
difference. Two further facts:

- A clusters-on run runs both far systems, baked object LOD and clusters, and the per-view
  arbitration between them. The suppression rule (`native_lod.rs:47`) keeps native far LOD for
  tree-LOD sources and for Visible-When-Distant references whose base has, or may have, distant
  LOD, because the package lacks per-level BTO paths and the tree relation (`handoff.rs:2142`).
  It prevents overlap; it does not draw the same placement twice.
- Content arriving mid-play made crossings fail: a batch arriving each frame voided the
  destination proof every frame (fixed by preloading, `92612307`). The proofs also guard view,
  pass and pipeline readiness, which preloading does not replace.

## Design

### 1. Launch modes

- **Vanilla** (default, Vanilla High): stock models in the loaded grid, baked object LOD and tree
  billboards beyond. No cluster system is installed.
- **Cluster**: the ownership rules below. Systems that only arbitrate between representations
  are not installed.

The mode is a launch option, as `--cluster-package` is today, but nothing changes it during a run
and nothing in the frame asks which mode is on. "Clusters off = SkyrimSE geometry/LOD parity"
stays exactly as it is.

### 2. Ownership in cluster mode

- An eligible primitive of an eligible reference (ADR-0013 section 5) is drawn by clusters from
  the far plane to the player's feet, and by nothing else.
- The reference itself still spawns: root identity, door and portal anchors, colliders, scripts,
  lights, and the rejected primitives of the same model.
- Ownership is matched by reference plus source occurrence ordinal (`SourcePrimitive`; the
  certificate's `(reference, primitive)` lookup), never by mesh handle or model path. Record keys
  are package-local (`reader.rs:87`); stable reference and occurrence identity are kept separate.
- Trees, terrain, water, doors, actors, movable or scripted objects and interiors stay on their
  current paths. Trees join when `TREE` clustering (`592d8032`) is on.

### 3. Static facts decided at conversion

- **Far-visibility rule.** A defined, versioned rule covering reference flags, usable per-level
  assets, the tree relation, distances and missing data, written per instance. Whether an
  instance's cell is inside a given view's grid stays runtime state.
- **Remainder object LOD.** Baked object LOD for cluster mode leaves out owned primitives. Under
  the one-set policy the package carries both sets: full for vanilla, remainder for cluster.
  This is real producer work: native BTO primitives carry cell segments, not reference
  identities (`docs/evidence/cluster-object-lod-parity-2026-10-18.md`).
- **Placed world and cell of each instance**, which need not be the instance's storage block
  (`handoff.rs:2145`).

### 4. GPU selection per cell and view domain

Each view has its own world and grid (main, reflection, portal destination, shadow consumers;
`package.rs:992`). Per frame the CPU uploads a small table per view domain: which resident cells
are inside that view's grid. The cull pass reads each instance's static bits (owned, far rule,
placed cell) and decides on the GPU. A grid move changes a few hundred entries once per cell
crossing. No per-view CPU instance mask, no stock visible set, no per-instance material
comparison; the drawability checks that the overlay makes today (template, tangents, source
material, renderer refusals; `handoff.rs:2369`, `2828`, `source_fallback_bridge.rs:22`) move into
qualification at load.

### 5. Readiness at block granularity

`--cluster-preload-batches` loads every batch behind the startup cover, and the cover waits for
certification, every block and every batch (`518e56cf`). The crossing proof stays: a portal,
landing or reflection view used for the first time can still need pipelines and view resources
(`destination.rs:452`, `transition.rs:1577`). Its granularity shrinks to owned blocks and
batches. Stock interiors and rejected primitives keep today's per-view proof.

### 6. Portals and secondary views

A portal into an exterior is another view in the same cluster cull pass. A portal into an
interior still needs the interior's stock statics: interiors are not clustered
(`handoff.rs:1331`).

### 7. Extract discipline

Main-world systems keep incremental state current when things change; extract copies small
finished results. No extract system walks entities, batches or instances per frame.

## Plan, fastest evidence first

Order: cost isolation, near parity, primitive ownership, remainder-LOD provenance, GPU cell
policy, proof simplification.

**Phase 0 - bounded experiment (1-3 days), measured after each step.**

1. Baseline with counters first: binary and package identity, preload done, selected / hidden /
   rejected instances, far suppressions, queued groups, active views and passes; exclusive spans
   for extract, selection, queueing, render encoding, submission and presentation. Same
   settings, timing hold, warm-up discarded, closed and open doors, percentiles and means.
2. Hide owned primitives once at instance spawn: ownership on the reference in
   `spawn_reference` (`reference_spawn.rs:140`), acted on at `WorldInstanceReady`
   (`streaming.rs:4124`). Keep the hierarchy, transforms and collision inputs (the legacy
   collider proxy reads render meshes, `streaming.rs:5472`). A small near-parity image set at
   once: split models, snow, normal maps, shadows, rebasing. About half a day to a day.
3. Stop object-block demand through the existing `lod_objects` decision (`lod_objects.rs:773`),
   keeping the shared inventory (tree billboards read it, `lod_trees.rs:343`) and terrain. A few
   hours. The far picture is knowingly incomplete: an optimistic cost sample, not parity.
4. Replace the object overlay in stages: first bypass only object binding / material arbitration
   and its mask rebuild, keeping qualification, view domains, terrain clipping, tree suppression
   and the crossing proof; then split installation so removed work stops running.
5. Two decisions follow the measurements: does removing coordination save what the table
   predicts, and what remaining work (render encoding and submission first) stands between the
   result and 8.33 ms with margin and smooth portals. An incomplete experiment missing 8 ms does
   not reject ownership; deleting far geometry and passing does not accept it.

**Phase 1 - converter.** Far-visibility rule; remainder object LOD with real provenance; stable
reference and occurrence identity.

**Phase 2 - engine ownership.** True non-instantiation of owned meshes (several days: filtered
scene instantiation, bounds validation, collision preservation); GPU per-cell, per-view-domain
selection.

**Phase 3 - parity gate.** ADR-0013's near-range image diff (section 7, "Material parity")
before ownership is broadened.

**Phase 4 - proofs and cleanup.** Block and batch level proofs; the handoff layer leaves cluster
mode.

## Risks

- **Near visual parity.** Clusters draw owned objects up close: vertex colours, snow, materials
  and shadows must match the stock path (no invented visuals). Phase 3 gates it.
- **Split models.** Owned and rejected primitives of one model must line up: same reference and
  mesh-local transforms, snow treatment, origin rebasing.
- **Memory.** All batches resident: about 10.9 GB engine RAM on the showcase.
- **Mods.** Cluster package and remainder LOD are derived data and rebuild when the load order
  changes (ADR-0013).
- **Interiors.** Exterior statics only; interiors keep the stock path.

## Open questions

1. Which systems besides the legacy collider proxy and loader validation read owned mesh
   entities? Phase 2 must answer before removing them.
2. Does the showcase package carry enough data for the far rule, or does Phase 1 come first for
   any far-picture comparison?
3. The one-set policy was the user's decision. This keeps it (both LOD sets in one package) and
   changes only when the choice is made.

## Progress, 2026-10-19 (`--cluster-own-statics`)

Post-ready fps at `lod-far` (full demo flags: showcase graphics, cluster terrain, preload,
startup cover; object and tree LOD off), one run each, so inference:

| Step | Commit | fps |
|---|---|---:|
| Start of the day | `ce7978ef` | 62.8 |
| Selection independent of the visible set; source change tracking skipped | `56ca77b7` | 61.9 |
| Cluster terrain owns the far terrain it covers (the LOD pass hides those stock patches) | `963d1b25` | 94.5 |
| A view's batch readiness reused while its plan inputs are unchanged | `6dcdb6ee` | 114.1 |
| The cluster draws what it owns at every distance (no far withholding, no baked object or tree LOD) | `b7a5da81` | 91.0 |

The 114.1 drew 282k fewer far instances: the far rule withheld trees and object LOD contributors for
a native far LOD that was switched off. 91.0 draws the full far world and is the figure to compare
with the prototype's 143-149 (same AA/SSAO/shadow settings; cluster terrain is extra). Tracy then
showed the GPU at 8.4 ms a frame against the prototype's 4.3: the camera prepass drew every cluster
group twice (Bevy's early and late prepass under occlusion culling), 4.2 ms together. The tour with
this mode passes 8 of 8 crossings; closed doors mean 11.8 ms, p95 12.5 ms.

## Review

GPT-6.1-Sol, 2026-10-19, read-only against `518e56cf` plus the working tree: **proceed with
changes.** Folded in above: removing arbitration alone does not establish 120 fps (about 12.6 ms
remains); keep the crossing proof; the first draft's far diagnosis was partly wrong (suppression
prevents overlap) and the far rule needs a data contract; `native_lod` cannot go wholesale while
tree billboards depend on its inventory; hide-at-spawn is the Phase 0 hook and
non-instantiation is multi-day; near parity gates broad ownership; record keys are
package-local; portals into interiors keep stock statics; each view needs its own grid domain.

## Foundations check

1. No shortcuts: removes per-frame arbitration at its root; no caps, no lowered settings.
2. Right layer: static facts at conversion, launch-time mode, GPU per-cell selection; measured
   before (table) and after each Phase 0 step.
3. Data-driven: ownership and far rule come from converted records, no baked IDs.
4. Moddable: derived data with recorded inputs; rebuilds on load-order change.
5. Matches ADR-0013 section 7 and the 160 fps goal; the vanilla parity mode is untouched.
