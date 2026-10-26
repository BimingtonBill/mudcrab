# Exact cluster and cascade GPU reuse

Task #2153, 2026-10-25. Base: `0ef2bde23e04f27fecb26ac939f72bbfca2d6fc6`.
Implementation follows changes 1 and 3 of the local GPU-2152 plan. Its estimates
are hypotheses, not measured savings. The complete-content baseline has 112,278
entities, 35,713 mesh entities and 27,106 distant stock references on main28.

## Ownership and invariants

GPU selection outputs belong to one retained view and its physical allocations.
Input validity is separate from permission to draw in the current render frame.
An exact retained result must acquire the current frame's draw permission, so
early/late prepass duplicate suppression continues to reset every frame.
Recomputation clears counts and runs all eight stages. Reuse does neither.
Only an entirely encoded result can be retained. A changed input invalidates the
previous result before encoding; an interrupted encode cannot certify it.

Existing selection and material-readiness revisions own source, material,
visibility and runtime-state changes. A per-view selection revision advances
when the exact selection dependency proof changes, even if its resulting mask
happens to compare equal. Queue-plan application owns source-order and batch
table writes. Append/layout and resource identity belong to ClusterGpu.
Uniform comparisons use exact bit patterns with only the shader-unused epoch
normalized. Full projection/pose/viewport inputs are also compared so a change
still invalidates when an explicit clip matrix leaves the derived shader
uniform unchanged. Origin offset is also an explicit input. No instance, material,
mask, or source-order scan is added to settled checking.

Required selection invalidations: view/frustum/projection and viewport; object
and terrain error settings; origin shift; package identity; append/layout;
source-order plan; instance transforms; visibility masks; readiness; material
pipeline state; runtime reference state from #2147; and buffer replacement.
The immutable cluster transform contract uses source-change suppression for
runtime pose changes rather than silently drawing the old owned geometry.

Shadow reuse must retain the exact map and array slice, skip both clear and
render, and include stock casters. Required dependencies are cascade matrices,
light settings, caster transforms/visibility/geometry, alpha textures, animated
material state and resource reallocation. Unknown dependencies must not be
certified as unchanged. There is no timer, frame cap, distance/quality change,
or camera freeze in this design.

## Upstream check

**In 0.20: no exact shadow-map content cache found.** Search and source inspection
performed before implementation on 2026-10-25. Both the `v0.20.0` light renderer
and main still obtain a depth attachment and render the shadow phase on each
invocation. Main was independently inspected at
`fd9e7609109758a2d9af401345f100be9442e9f6`. Retained render phases and pipeline
specialization caches are different from retaining a valid depth map. No matching
implementation PR was found by this bounded search. Issue #12590 lists static
shadow-view caching as an opportunity; it is not an implementation PR.

Sources:

- https://github.com/bevyengine/bevy/blob/main/crates/bevy_pbr/src/render/light.rs
- https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_pbr/src/render/light.rs
- https://github.com/bevyengine/bevy/issues/12590

The 0.19 trace identified two additional ownership requirements. Directional
cascade slices were reused between cameras, and TextureCache is a transient pool.
The review correction allocates disjoint ranges for each active camera: array
size is the sum of their enabled cascades when that plus the original admitted
spot demand fits the device layer limit. Shader offsets and attachments share the
same camera range base; spot slices follow all directional ranges. If the sum
would exceed that limit, each camera receives a retained physical atlas with local
cascade indices and a common spot offset after the largest per-camera range.
Shared spots still raster once; whole depth slices copy to the extra camera
atlases on the same native encoder before camera sampling. This preserves the
original admitted shadow content and per-camera layer budget. Physical slice-owner
validation remains a second residency check; cameras do not evict each other.
Topology/extent/usage changes invalidate ownership and refresh unchanged spot
attachments. Removed camera atlas keys are pruned.
Matching descriptors or unchanged view entities therefore do not prove map
residency. Retention must own the allocation or track every slice overwrite.
Stock caster invalidation also needs mesh/deformation and material/image content
writers; cluster handoff revisions alone are insufficient. Existing renderer
scans are not made into new cache-check scans. The settled cache decision itself
must use mutation-owned facts. Unknown custom shader dependencies cannot be
certified as unchanged.

The engine publishes current-frame cluster dependency certificates after cull
encoding or exact reuse. The output revision namespace is shared across view
allocations, so recreation cannot alias an old map certificate. Terrain clip
record writes advance the shadow-content revision independently of selection
compute. Wrapped shadow draw commands retain Skip/Failure behavior and report
incomplete rendering to the map cache. Empty cluster content is explicit.

Each registered external shadow command also publishes its own current-frame
certificate. An aggregate cluster certificate cannot authorize an unrelated
non-mesh command. Draw registry insertion, replacement, clearing and resource
recreation advance a mutation-owned revision. Concrete producer classification
runs only when that revision or its declarations change; settled checks compare
the cached revision, unknown count and the two cluster command certificates.
Unknown registered commands refuse reuse even when their current phase is empty.
This conservative refusal preserves custom rendering without a population scan.
Draw storage is read-only outside the registry's structural mutation methods;
ordinary command preparation and draw access do not advance its revision.

The optional API draw-count diagnostic preserves the shadow tracking wrapper
when it replaces a leaf mesh command. Its distinct counted producer types
conservatively refuse retained shadow-map reuse. Count captures remain separate
from the ordinary-cache GPU timing experiment.

Light and shadow-view keys use fixed typed arrays: 121 light/camera words and
56 shadow-view words. Key construction, copying and comparison do not allocate
variable-length input vectors. Every word has an independent invalidation test.
Morph extraction reads the mesh-value and referenced-weight ECS change ticks.
Only a changed producer or a newly observed visible morph inspects weight bytes;
settled morphs perform a constant-time membership lookup in the existing upload
loop. Removal, missing references and visibility loss withdraw their morph facts.

An already initialized empty material closure refreshes frame freshness without
advancing content identity. A populated-to-empty transition removes its reverse
dependencies and advances the revision. This makes the GPU-absent, stock-only
path reusable while retaining immediate invalidation on content transitions.

Ruling: stock shadow phase batching and its existing visibility producers may
continue to run while a valid map is retained. This task removes repeated
cluster selection and shadow map raster/clear work. It does not claim to retain
unrelated transient stock indirect allocations or remove all existing CPU
visibility scans. No new population scans are permitted for cache checking.
If retaining the map without removing that separate work misses the measured
GPU budget, that remaining cost must be attributed before further changes.

## Texture writer contract

Lead decision, 2026-10-26: **Every texture write that can affect a shadow caster
must mutate `Assets<Image>` or reacquire `RenderAssets<GpuImage>::get_mut` before
that frame's shadow-cache decisions.** The mutable prepared-asset access advances
the image-content revision, even when the GPU texture identity stays unchanged.
The write must precede shadow sampling. Keeping a cloned GPU handle after one
mutable access does not authorize future unstamped writes.

For example, a mod updating a CPU-backed RGBA8 alpha texture can use the ordinary
asset path; extraction and successful preparation publish the new content:

```rust
if let Some(image) = images.get_mut(&alpha_texture_handle)
    && let Some(texels) = image.data.as_mut()
{
    for rgba in texels.chunks_exact_mut(4) {
        rgba[3] = new_alpha;
    }
}
```

A render-world producer instead reacquires `gpu_images.get_mut(image_id)` for
each update, then writes through that returned image before the shadow pass.
It must not perform later writes through a retained raw alias without another
mutable access. Arbitrary raw queue/encoder writes are outside this cooperative
contract and are not automatically intercepted. The cache does not claim to
enforce that boundary against code using escaped GPU handles. This is an
explicit producer requirement for mod authors, not a texture-trust certificate.
Ordinary loaded `COPY_DST` images remain eligible. Render/storage-written bound
textures and opaque directly created material bind groups refuse reuse.

The bounded writer audit covers all direct texture-write/copy call sites in
`crates/engine` and `vendor`, including initial uploads. Its reproducible grep
receipt is `docs/evidence/cluster-gpu-texture-writers-2153.txt`. The table records
why each current owner satisfies the contract or cannot authorize map reuse.

| Writer | Shadow dependency handling | Proof |
|---|---|---|
| `material_animation.rs` standard material and effect palette updates | Both write through `Assets::get_mut`; prepared-material deltas update only dependent cascades | Writer grep; animation tests; relevant/unrelated material and late-closure revision tests |
| Terrain clip material/coverage and terrain source assets | Asset mutations publish prepared-material/image revisions; per-view CPU clip writes advance `shadow_content_revision` | `coverage_is_idempotent_and_reports_only_real_changes`; rebased view-record tests; handoff revision publication grep |
| `bevy_render/texture/gpu_image.rs` queue upload, initial data creation and copy-on-resize | Successful asset preparation inserts the resulting `GpuImage` and advances image revision, including same-ID updates/retries | `same_identity_content_replacement_and_mutation_advance_revision`; `successful_retry_publishes_content_revision_in_its_own_frame` |
| `bevy_render/mesh/morph.rs` morph displacement image creation | Owned by mesh preparation; prepared mesh/allocator content revisions invalidate geometry | Writer/mesh preparation grep; independent geometry revision gate tests |
| PBR dummy white and renderer fallback image initialization | Immutable initial data; GPU resource recovery reconstructs cache ownership | Startup-only writer grep; resource/allocation lifecycle gate tests |
| PBR SSAO noise and medium density/scattering LUT creation | SSAO resource initialization and prepared-medium LUT creation; these are not stock shadow alpha inputs | Writer and stock prepass shader grep |
| `water_ssr.rs` sky-ramp queue write and opaque-copy encoder write | `WaterMaterial` is undeclared, so it cannot authorize retained maps; its view transmission texture is not a stock shadow alpha input | Writer grep; unknown-material refusal tests |
| `portal.rs` portal depth copy | `PortalMaterial` and `PortalDepthMaterial` are undeclared; these producers cannot authorize retained maps | Writer/declaration grep; unknown-material refusal tests |
| Core prepass/deferred depth copies and PBR transmission copy | Native pass/view attachments, not stock shadow alpha bindings; render/storage-bound custom material textures refuse reuse | Writer/stock prepass shader grep; GPU-written texture refusal test |
| Renderer native raster audit scratch copies | Opt-in verification attachments; no material texture content is replaced | Writer grep; native audit admission/comparator tests; native standing and moving-tail audits passed |
| `retail_curve_probe` example and `RenderDevice::create_texture_with_data` wrapper | Standalone probe has no engine shadow cache; wrapper delegates to the initialization owners listed above | Complete call-site grep receipt |

These proofs cover current owners and the stated cooperative extension contract.
They do not establish universal coverage of arbitrary raw GPU writes.

The opt-in verification mode is `--verify-shadow-reuse <DIR>`, with an optional
`--verify-shadow-reuse-request <FILE>` sentinel (default: `DIR/REQUEST`). Create
the sentinel after settlement; the first observation selects one actual render
frame. The mode pairs the native normal-policy map A with forced-render B,
including early/late draw completion. A can be a retained map or a map that
normal policy already renders. Fresh nonoccluded maps use the captured original
clear descriptor twice; A is copied only after normal native completion. Fresh
occluded maps and unsupported attachment load operations remain inconclusive.
It is compiled only with the default-off `gpu-reuse-verify` Cargo feature.
Without the feature there is no audit module, resource, texture-binding snapshot,
audit copy usage flag, replay loop, CLI switch, or actual-bind thread-local check.
The production atlas overflow path independently uses texture copy capabilities
to distribute shared spot results; those copies are required rendering work,
not verification readback or replay.
Native camera raster parity additionally requires the explicit development-only
Cargo overlay in `tools/gpu-reuse-verify/README.md`; production core_pipeline
resolves the pristine registry package. Cargo cannot make `[patch]` conditional
on a feature, so that overlay is intentionally separate and never auto-loaded.

Each compared map links a full caster/material/sampled-texture candidate manifest
and a bounded runtime diagnostic. Stock candidates identify main entities and
material assets; actual prepared texture views are joined to image assets by
physical texture identity. Cluster command carriers and their material closure
are explicitly not individual source-reference identities. A map-wide mismatch
cannot isolate its responsible member, so diagnostics report the candidate set,
never an invented culprit. Missing dependencies, incomplete commands, failed
copies or readbacks cannot pass. Unknown material inputs remain refused for
reuse even when a fresh native pair passes; diagnostic type registration grants
no eligibility. Every observed allocation must have a complete equal pair for
whole-map coverage. Native standing and moving-tail audits passed; the
verification ledger below states their boundaries.

Joint uploads now compare their exact matrix bits: numerical equality previously
missed changed signed-zero bytes and repeatedly invalidated identical NaN
payloads. A focused harness extracts the production slot-update helper and its
tests and compiles them against the actual cached Bevy math crate. It observed
two expected RED failures, then three GREEN tests after the bitwise comparison.
Logs are `joint-helper-red.log` and `joint-helper-green.log` in the owned target.
The final PBR and workspace Rust gates below include this helper.

## Verification ledger

Final identical-code checks pass: workspace strict all-target/all-feature
Clippy; 5,583 workspace nextest tests (65 skipped); 68 PBR tests; 18 doctests
(15 ignored); fmt and whitespace. The R4 CPU controller suite passes 123 tests.
The two physical Vulkan cull fixtures pass. All 892 Rust/WGSL/Cargo files still
match the frozen source used for the final tests and release executable.

Three quiet captures from that one executable record GPU pass sums of
**7.444194 ms** with both controls forced, **6.553069 ms** with cull reuse and
shadows forced, and **6.015837 ms** with both exact reuse paths. Each uses a
counter-certified 30-second post-ready tail, unchanged settings/pose/package,
and 112,278 entities / 35,713 meshes. The observed 1.428357 ms difference is
one-sequence inference, not FPS acceptance. Eight settled dispatch counters are
zero. Cascades 0–2 have full per-view zero-render certificates; standing cascade
3 remains fresh because WaterMaterial cannot certify its dependencies.

Native standing frame 1234 matches 39 raw pairs (1,571,992,744 bytes per lane),
covering five cull views, four maps and ten depth/normal/motion/HDR attachments.
The approved supplementary moving normal/forced runs finish 40,500 units,
retain five visible moving screenshots each and pass the official content
verdict. The normal tail verifier at frame 5587 matches all four reused maps,
including a populated map with 12,733 non-clear texels. No mismatch occurred.

The old native route starts on CPU readiness while startup's mandatory cluster
cover can still be pending. Its first moving images were black and are excluded
from visual acceptance. This belongs to traversal readiness; integration fix
`666d7ceca` is tracked by #2151. The lead approved the existing settle-20 option
for a supplementary pair and required actual cover-release-before-Moving proof.
No cover, content, quality setting or readiness rule was disabled.

Detailed pass values, raw receipt hashes, RED/GREEN history, accepted and rejected
run identities, screenshot review, counter coverage and limits are recorded in
[the evidence report](../evidence/cluster-gpu-reuse-2153.md),
[the JSON receipt](../evidence/cluster-gpu-reuse-2153.json), and
[the invalidation ledger](../evidence/cluster-gpu-reuse-invalidations-2153.md).
Legacy performance verdicts remain FAIL; sustained unprofiled FPS, portal-heavy
integration, Metal and final temporal-postprocess byte equality remain unproved.

## Integration and ClusterScene dependencies

The allocation-local cull key does not borrow or compare a whole `ClusterScene`.
Its direct values are GPU identity, append/layout epoch, material generation,
selection revision, plan epoch, readiness and pipeline revisions, effective origin,
full view/projection/optional clip matrices, viewport and inverted-culling state.
The exact uploaded shader parameters additionally cover error thresholds,
frustum planes, instance counts and projection kind.

The effective origin reads `ClusterScene::origin_offset`. GPU construction and
append read `meshes`, `instances` and `blocks`; `terrain_blocks` supplies retained
terrain metadata. `certificate` supplies package authority and the existing
source/selection certificate match. The source path already observes
`LazyBatches.layout_scope`; a scope change refreshes source facts and any changed
selection dependencies advance the published selection revision. The proposed
`Option<Arc<InstanceInventory>>` is not read by this implementation, and adding
that metadata alone does not change GPU selection inputs. Its producer must keep
the agreed layout-scope contract when final instance order changes.

For #2151.2, merge the new field into the struct and every scene fixture constructor,
including this branch's fixture in `render.rs`. `package.rs` is untouched here.
The only `handoff.rs` changes are three revision publications in two hunks:
terrain-clip content, selection dependencies and shadow content. #2145.2's
render-side CPU work overlaps `render.rs` and may overlap nearby source/terrain
code in `handoff.rs`; preserve those narrow publications when resolving hunks.
`readiness_bridge.rs`, streaming and `far_stock.rs` are untouched here. The
existing #2147 runtime-state revision remains in the selection dependency stamp.

The Tracy loopback prerequisite is local commit `1571b2fbe`, cherry-picked from
`8189a981e`. No push, PR or Plus merge is performed by this task.


## Review correction boundaries (#2153.1)

Stock caster dependency indexes exist only for views carrying
`DirectionalShadowMapInputs`. Point/spot faces keep their ordinary rendering,
visibility and pipeline specialization but never call the retained-map caster
begin/set/remove/refusal bookkeeping. The existing mutation loops still update
actual directional dependents, without scanning caster populations at reuse time.

Prepared geometry, mesh, image and pipeline readiness counters in each map key
remain global and conservative. Any such producer mutation can redraw unrelated
cascades. This is a performance limitation, not a claim of per-caster precision;
narrowing would require resource-specific producer indexes with the same complete
writer contract. No new settled dependency walk was introduced to narrow them.
Material changes already use the targeted material-to-cascade index. The unused
per-asset declaration revision map/API was removed: custom shadow types either
make the existing static declaration or remain fail-closed.

Poisoned dependency locks discard partial state and refuse certificates. Map
owner recovery withdraws keys/owners, permanently disables retention until
resource recreation, and preserves only the frame-local early-skip decision.
Fresh early passes still execute real late occlusion work. Poisoning does not
propagate an expected startup/runtime panic through every later cascade. Late revocation of early reuse skips late
append too, withdraws ownership, and requires a cleared full render next frame.
Production missing draw lookup skips that item and continues the phase; only a
verification build records the native audit failure. Ordinary command Skip/Failure
still reaches the shadow tracking wrapper and cannot certify completed work.


The final #2153.1 correction also retires unadmitted/shadow-disabled spot views
through the existing per-light deferred-despawn loop. Their destination handles
must not retain a pruned camera atlas. The 256-layer/two-camera/252-spot boundary
and budget reduction/readmission are covered by CPU/ECS regressions; live GPU
overflow transitions remain unproved. Production overflow depth copies preserve
shared spot content and are separate from feature-only audit copies.

The feature-off production recapture retains the saving: GPU pass sum
7.225837 ms forced versus 5.802041 ms reused (1.423796 ms), with all eight settled
compute stages and cascades 0-2 at zero work. Cascade 3 stays fresh. This is one
quiet profiler TEST sequence, not FPS acceptance. Final gates, exact settings,
source footprints, file hashes and limitations are in
[the review-round evidence](../evidence/cluster-gpu-reuse-review-2153-1.md).
