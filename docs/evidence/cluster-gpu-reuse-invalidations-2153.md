# #2153 invalidation evidence ledger

Finalized 2026-10-26. See `cluster-gpu-reuse-2153.md` and its hash-linked JSON
for executed source, native, hardware and quiet-capture results.

## Proof boundaries

The CPU gate tests establish that changed input identities withdraw a cached
result in the same call. The physical Vulkan fixtures execute production WGSL
and compare complete retained/forced output allocations after individual input
mutations. Producer-routing tests establish selected stock/material lifecycle
transitions. These are different proofs: a synthetic revision mutation does not
prove that every possible live writer publishes that revision correctly.
Source tracing and the independent producer review cover that connection;
the completed native standing and moving-tail audits add the bounded live proof described below.

## Cluster dependencies

| Dependency | Responsible fact | Existing proof |
|---|---|---|
| View, frustum, projection, viewport, invert culling | Exact uploaded cull bytes plus full `CullInputs` view values | CPU test mutates every matrix/viewport word, optional matrix identity and invert flag; Vulkan fixture changes view and plane |
| Object and terrain error settings | `CullParams` byte comparison | Vulkan fixture changes both thresholds independently |
| Origin shift | Explicit three origin words; existing shift producer | CPU and Vulkan fixtures mutate each origin word; actual origin-shift dispatch is outside the synthetic retained-output fixture |
| Package identity | Allocation-local gate and process-unique `ClusterGpu::identity` | GPU identity mutation and fresh-view/allocation fixtures; package loading connection source traced |
| Append/layout generations | Layout and instance-count/uniform inputs | Vulkan append before planning, subsequent plan update, and layout-revision mutations |
| Source-order plan | `ViewGpu::plan_epoch` | Vulkan changes real group membership/order and the plan revision |
| Instance transforms | Existing source-change suppression/selection proof; GPU identity/origin for supported GPU writes | Vulkan fixture changes actual instance matrix and its revision; runtime immutable-package ownership connection source traced |
| Visibility masks | `ViewGpu::selection_revision` after exact selection proof changes | Vulkan changes actual mask and selection revision |
| Readiness | Existing `ViewGpu::readiness_revision`, material/selection facts | Independent readiness-revision mutation; existing handoff/readiness tests remain in workspace suite |
| Material pipeline state | `ClusterGpu::generation`, pipeline readiness revision, selection proof | Independent material/pipeline revision mutations; actual material and shader producers source traced |
| Runtime reference state (#2147) | Existing runtime suppression and selection-input facts | Existing runtime/handoff suite plus synthetic mask/selection mutation; no new live enable-change capture yet |
| Buffer replacement and view recreation | New allocation-local `CullReuse`, fresh namespace revision | Both physical Vulkan fixtures; unchanged uniforms cannot certify new allocations |

Four production-module CPU tests passed. The two ignored hardware fixtures were
explicitly executed through `build_gate.py test` on RTX 3090/Vulkan/616.92 and
passed. Both count modes compare full arguments, counts, visible records,
instance lists and indirect dispatch words. Exact settled fixture frames encode
zero clears and zero calls to all eight dispatch stages. Live renderer counters and native full-scene output equality have now passed
within the frame/route boundaries in the final report.

## Shadow dependencies

| Dependency | Responsible fact | Existing proof |
|---|---|---|
| Physical map, array slice and ownership | Retained texture identity, slice, last writer; resource recovery reset | Allocation/slice, cross-camera overwrite, removed/recreated-view tests |
| Cascade matrices and view state | Fixed 121-word light/camera key plus 56-word shadow-view key | All 177 words independently mutated in same-frame cache-decision tests |
| Light settings and camera/light layers | Fixed light words and exact layer values | Every word plus independent camera/light layer tests |
| Caster transforms, visibility, geometry and membership | Existing mesh extraction/queue deltas and reverse caster index; mesh allocator/prepared asset revisions | Affected-view routing, removal and independent revision tests; source wiring review; full live entrant/visibility exercise pending |
| Skinning | Existing joint producer compares exact upload matrix bits and marks affected views | Production-helper RED/GREEN covers signed zero, identical NaN payloads, ordinary changes and settlement; fresh PBR suite passed all three cases; no native animated-skin parity capture yet |
| Morphs and referenced weights | ECS value/reference change ticks, exact changed weight bits, removal/reappearance | Morph mutation, settled tick gate, missing/reappearing facts and removal tests |
| Alpha textures | Successful prepared-image content revisions and per-write mutable access under the accepted cooperative writer contract; actual binding usage refuses render/storage-written textures | Same-ID asset replacement/mutation/retry tests and texture refusal test; complete engine/vendor write/copy grep inventory in design note; arbitrary escaped raw writes are explicitly outside contract; live texture-edit capture pending |
| Animated material state | Prepared-material changed-ID drain and material-to-cascade/external-view indexes | Relevant/unrelated material routing and late external material closure tests; declarations fail closed for unknown material types |
| Resource reallocation | Physical texture identity, allocator/prepared geometry revisions and recovery reset | Allocation/slice, recreated-view, independent revision and incomplete recovery behavior tests |
| Custom shadow commands | Tracked registry structural revision, concrete producer classification and fresh typed external certificates | Fourteen producer regressions cover insertion/removal/change/Skip and unknown producer refusal |
| Pending or incomplete work | Fresh cluster/material certificates, pending stock queue refusal and wrapped Skip/Failure | No commit after incomplete early/late draws; readiness retry, late completion and refusal invalidation tests |
| Empty content | Initialized empty closure preserves content revision; transitions rebuild reverse dependencies | Repeated empty frames and populated-to-empty transition regression |

Settled shadow checks copy/compare fixed-size view keys and revision snapshots.
Material changes visit only affected reverse-index entries. Existing stock
visibility, batching and preprocessing remain active; this implementation does
not claim persistent stock indirect allocations or removal of Bevy's existing
visibility scans. It retains exact map contents and cluster caster results.

## Completed native proof and remaining acceptance work

- Native standing frame 1234: 39 exact complete pairs, five cull views, four shadow
  maps and ten camera attachments before temporal postprocessing.
- Quiet complete-content standing: all eight stages zero; cascades 0–2 zero
  renders with full per-view coverage. Refused cascade 3 remains fresh.
- Supplementary normal/forced movement:full 40,500-unit routes, five visible moving
  PNGs each, changing view dispatches in all eight stages, official CONTENT PASS.
- Native normal tail frame 5587: four reused-map pairs match, including populated
  cascade 3 with 12,733 non-clear texels. Strict all-observed coverage passes.

The initial route/cover gap belongs to #2151's traversal readiness fix 666d7ceca.
The approved settle-20 workaround additionally requires actual cover release
before movement; old black moving images do not count as visual evidence.
These proofs do not mutate every possible live writer. Table entries explicitly
separate CPU/GPU input mutation, producer routing and source tracing from live
native exercises. Unprofiled end-to-end FPS, portal-heavy integration and Metal
remain separate acceptance scopes. Final presented/TAA color is not independently
paired; native raw HDR/depth/normal/motion values are exactly equal.
