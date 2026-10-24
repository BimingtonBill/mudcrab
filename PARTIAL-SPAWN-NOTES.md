# Distant flat stock spawn (#2139)

## Final renderer contract

The lead's latest #2136 integration boundary supersedes the earlier retirement
proposal. Per-consumer complement requires an exact live stock-body binding. A
positive BatchDrawability observation cannot authorize destruction: losing that
binding can suppress the replacement immediately, even before readiness drops.

Keep every owned body draw and its assets resident and extract-eligible. Refused
draws remain stock permanently. #2136 hides an owned stock copy only in consumers
where its replacement is drawable, and restores it in the same Queue on withdrawal.
There is no main-world retirement, settle timer, or respawn path. Consequently this
branch introduces no render-to-main/instantiation recovery window and never records
far_stock/fallback_respawn_after_drop. The regression requires that counter to stay
zero and verifies the same entity IDs survive true and false observations.

The original refused-only lifetime target is therefore not implemented; the saving
comes from removing static hierarchy entities, while retaining all fallback draw
entities required by the current renderer. A future body-retirement design needs a
renderer lifetime protocol that preserves replacement proof without a live body.

## Spawning and identity

Only owned-mode partly-owned references born beyond the loaded grid enter this
path. Wholly refused references and initial in-grid references use ordinary full
spawning. Promotion recreates the complete ordinary scene before gameplay/physics
adoption. Existing retained near owners stay full.

One shared render-only source template per asset/ordinal set keeps all source
meshes, exact materials, render flags, source identities and necessary ancestry.
It strips non-rendering ancestor behavior and lights. Unknown new source ordinals
remain stock, including an interior addition that leaves aggregate bounds unchanged.
Original conversion bounds remain required and are checked; computed subset bounds
do not replace the original full-source validation contract. Actual source edits,
including edits while PendingModel is still awaiting arming, restore ordinary stock.

After ordinary readiness validation and material conversion, static primitive
entities detach and lose Transform. Their GlobalTransform retains the exact
propagated affine, including shear. The local chain supports exact rebasing.
Animation and billboard branches retain necessary ancestry. Skin/morph sources
retain the ordinary full scene, preserving non-ancestor joints and referenced
weight controllers, including cached sources edited before a later instance.
A single scene avoids duplicated animation inventories or per-batch closures.
Failed loads also retain ordinary arming/readiness handling, preserving underlying
error records, failure counters and configured diagnostic fallbacks.

FlatSourceOwner and linked_spawn provide identity and recursive cleanup without
transform parenting. Qualification checks the original source handle and source
model matrix; owner and affine changes invalidate the source index. ClassFilter
and terrain/stock complement consume the resulting reference-plus-ordinal binding.
Native LOD filtering leaves role-less meshes untouched; owned mode disables native
object/tree LOD. Promotion restores ordinary hierarchy before near ownership.
PropagateOver<RenderLayers> protects detached layer output, and sync runs after
PropagateSet<RenderLayers>. Nested mesh poses are captured before any detach;
ancestors needed by retained dynamic descendants are not detached.

## Readiness seam and integration

The API remains crate::cluster::batch_drawability::BatchDrawability:
publish(&Arc<CertifiedPackage>, layout_scope, changed (u32,bool)), matches, drawable,
and take_changes. Package allocation and lazy-layout identity guard every fact;
missing rows/producer mean false. True requires nonempty consumers, all cameras,
required prepasses and applicable caster-shadow views; #2136 owns that aggregation.
This branch initializes the conservative false resource and retains the agreed API,
but does not consume observations as entity lifetime decisions. #2136 supplies the
render producer/complement. Its file was inspected read-only at dbb5a72a2; no
ownership.rs or readiness/complement code was edited here.

## Same-center structural projection

Read-only SQLite/GLB counting uses C:/Modding/Mudcrab/SkyrimConverted-main28
and derived/clusters-d4ca7334b3e2a58e/clusters.db. At worldspace 60, center
(5,-12), radius 2 it reproduces
25,525 distant look candidates. It found zero mismatches between GLB SHA-256 and
coverage, accepted coverage and instance maps, or default-scene ordinal sets.

| Population | Entities |
| --- | ---: |
| Partial placement roots (21,479 references) | 21,479 |
| Resident accepted primitives | 62,609 |
| Permanent refused primitives | 25,388 |
| Dynamic authored ancestors and synthetic roots | 164 |
| Wholly rejected full hierarchies (3,918 references) | 26,623 |
| NotEligible full hierarchies (128 references) | 2,790 |
| Shared distant cell holders | 4,795 |
| Total implemented guarded projection | **143,848** |

Original full hierarchies total 298,953; retaining the render-only selected ancestry
before flattening totals 298,687 (266 empty authored nodes disappear). The all-flat
resident floor is 143,684. Guarding 70 dynamic draws adds 154 authored ancestors and
ten synthetic roots: 21 animated draws (8 accepted, 13 refused), and 49 billboard
draws (all refused). Static flattened draws total 87,927. The exact selection uses
696 distinct GLBs, all with one scene and no skins or morphs. The earlier 699-file
estimate and 81,230 retired-body projection are superseded.

The implemented projection removes 155,105 entities versus complete stock scenes.
Counts assume successful loading and exclude retained near owners, unrelated engine
entities, cluster entities and temporary instancing hierarchy. These are structural
projections, not measured runtime entity totals or GPU draw calls. No engine run or
performance/visual acceptance is authorized here; lead owns integration/GPU checks.

## Verification

Source-identity seam is isolated in commit `08493faca`.

Gated engine all-target/all-feature Clippy with warnings denied passed. Engine
all-target nextest passed: 3,146 tests, zero failures, 13 skipped (initial 72.314 seconds; final repeated
run 66.495 seconds).
The suite includes full/partial material and affine equality after real readiness
and snow conversion, unchanged in-grid/fully-refused policy, identity extraction,
layer propagation, required-Transform non-reinsertion, nested mesh/billboard
ancestry, owner cleanup, original-bounds rejection, source edits, skin/morph
reference remapping, and actual AssetServer failure/diagnostic accounting.
True-to-false drawability observations preserve the same resident body entity IDs.

Whole-workspace gated acceptance passed:

- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo nextest run --workspace`: 5,344 passed, zero failed, 63 skipped,
  11 slow, 718.521 seconds. Cold compilation took 24m23s.
- `cargo test --workspace --doc`: 18 passed, zero failed, 15 ignored.

The final independent Codex compatibility review found no actionable issue in the
skin/morph and failed-load guards/tests or directly related resident handling.
Earlier review findings were addressed before this acceptance run. Tests ran amid
other gated builds and owned-artifact compression; their elapsed time is not an
engine performance measurement. The existing proc-macro-error2 v2.0.1 future
compatibility notice remains.

Only owned build archives/metadata/PDBs were NTFS-compressed for capacity: 2,378
files, 8,669,601,846 logical bytes stored in 3,907,118,304 bytes. All artifacts were
retained. Gate limits and the assigned target/jobs=2 were unchanged. The receipt
is D:/Mudcrab/builds/partial-spawn-target/capacity-compression.log.

Fresh exact engine fmt/Clippy/nextest checks passed before the implementation
commit; their logs are retained as final-fmt.log, final-engine-clippy.log and
final-engine-nextest.log in the same owned target.
