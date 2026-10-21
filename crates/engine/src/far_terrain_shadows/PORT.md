# F1 packet and receiving port agreement (2026-10-17)

This slice now includes isolated CPU contact/numeric code and composable WGSL
receiving/traversal. No engine registration, shared shader/material/setting,
render graph, GPU allocation or database reader is added.
Checked geometry reference: `d349ba57`; CPU slice: `6906a585`, harness: `dd1d712a`.
Budget-checked base: `cb3458f0`. Root wiring below remains proposed; isolated shader
implementation/composition is not GPU execution proof or visual/performance acceptance.
Root owns `cluster/render.rs`, the certified reader and publication lifecycle.

## Integration status, 2026-10-17

The agreement above records the original isolated slice. VG7 has since adopted
these modules and added the retained certified reader, shared immutable heights,
actual joint-publication fixtures and a background source owner (test-only until
its runtime consumer is integrated). The unchanged arithmetic/receiver executes
on the engine-created RTX 3090 Vulkan device: 16 numeric readbacks pass, including
known Occluded/Clear and Unknown controls. Pinned selected-sun PBR composition and
all-off checks also pass. None of those results proves a live renderer option,
complete material-layout GPU compatibility, visual acceptance or performance.
The original source/package is unchanged; incomplete-data notices remain.
Runtime view/sun association, binding ownership and receiver integration are still
pending. See the tracked showcase plan and handoff log for the current checkpoint.
## Source contract

The reader supplies the exact world/database/core/canonical identity and VHGT
status. Two bounded proof paths may supply `PresentValid`:

1. `converter::esm::cell_cache::validate_inhouse_land` validates the authoritative
   raw subrecord, including its presence and authored byte validity.
2. Under a retained certified core/package guard, NONEMPTY `land.heightmap` bytes
   are the converter's winning LAND VHGT projection and prove positive tag presence.
   The adapter must strictly decode those bytes in the producer's f32 operation
   order and require exact bit correspondence to all 33x33 pre-world-weld cell-cache
   samples before supplying `PresentValid`. This is a bounded projected-data proof,
   not an attestation of original ESM winner authority or global source parity.

An empty projected blob conflates omitted VHGT and present-empty: both remain
`Unknown`. Missing LAND and unlisted cells also remain `Unknown` on this positive-only
path; it supplies no absence or closed-world attestations. Raw inspection can still
identify a present-empty tag as `PresentInvalid`. Neither a finite cache/canonical
grid alone nor published canonical output repairs invalid authored bytes. Missing
VHGT may have an explicitly identified rendered default only through an independent
appropriate source proof, never by interpreting an empty projection as absence.
The packer never decodes VHGT, selects winners, welds seams or infers parent worlds.

For this adapter, authenticated published `cluster_terrain_heights` supplies the
canonical post-weld model under the approved producer/output trust boundary.
Pre-weld cell-cache correspondence is the positive authored-data check, not the
canonical shadow surface. No independent weld replay is claimed. Root owns the
retained guard, cross-side decoder fixture, exact coverage/recipe association and
cached package snapshot lifecycle; the current adapter artifact is not adopted.

Canonical pages must be FINAL POST-WORLDSPACE-WELD outputs with the corresponding
recipe/content identity. `post_worldspace_weld` is an explicit trusted reader assertion,
not evidence manufactured by this packer. Canonical base geometry is the deliberate
highest-detail shadow model; it does not equal simplified cluster or stock LOD silhouettes.
Duplicated 33/32 sample dimensions, south-to-north ordering and `[a,b,c]/[b,d,c]`
split still need shared-helper agreement or versioned recipe certification before adoption.
The mathematical global f64 base grid also needs a certified relationship to the
converter's anchor-relative f32 positions; shared topology alone is insufficient.

Showcase core `complete=false, record_data_complete=false` remains open. Both flags
must be true before this packer will honor even supplied known-empty/exterior-empty
attestations; complete flags themselves NEVER create attestations. Missing directory
entries always remain UNLISTED, even for a complete source. Bounds never prove absence.
This intentionally conservative rule can reject useful locally certified absence;
supporting finer scoped completeness is outside this slice.

## Version 1 byte layout

All words are little-endian u32, signed coordinates encoded by i32 bit pattern;
float samples/bounds encoded by f32 bit pattern. Upload serialized words, never Rust
struct memory. `NONE=0xffffffff` is an absent page/root, not height zero.

| Buffer | Layout |
| --- | --- |
| Header | 16 words/64 bytes: version, world, directory count, page count; min east/north, max east/north; closed-exterior flag, exterior reason, lookup-miss reason; complete, record-data-complete, post-weld; two reserved zeros |
| Directory | Eight words/32 bytes per record, sorted signed `(east,north)`: east, north, page, absolute root-node index, coverage, reasons, raw VHGT status, selected surface provenance |
| Heights | 1089 words/4356 bytes per page; sample offset `page*1089 + north*33 + east`; retain every original finite f32 bit pattern |
| Nodes | Four words/16 bytes: min bits, max bits, coverage, reasons. 1365 nodes/page; levels bottom-up at offsets 0,1024,1280,1344,1360,1364; sides 32,16,8,4,2,1 |
| View/instance context | 25 vec4<u32>/400 bytes; exact flattened slots documented in `view.rs`; production uses `pack_view`, probe-only receiver slots are distinguished by word 19 |

Leaves bound the four vertices of one quad, covering both linear base triangles.
Parents take extrema; no averaging or obstacle decimation. HAS_BOUNDS bounds only known
surfaces: a skip across ALL coverage additionally requires COMPLETE. Unknown children
propagate reasons and clear COMPLETE; certified empty is a distinct state with no bounds.
These are per-page hierarchies, not a world BVH. Unknown directory gaps are represented
implicitly; the packer does not enumerate a huge rectangular hole.

Coverage bits: 0 HAS_SURFACE, 1 COMPLETE, 2 KNOWN_EMPTY, 3 HAS_BOUNDS.
Reason bits 0..12 respectively: UNLISTED, ABSENT_LAND_UNATTESTED,
SURFACE_NOT_SUPPLIED, INVALID_BYTES, STALE_BYTES, INCOMPLETE_SOURCE,
OUTSIDE_DOMAIN, POST_WELD_UNCONFIRMED, PAGE_PENDING, IDENTITY_MISMATCH,
RECEIVER_UNRESOLVED, NUMERIC_UNCERTAINTY, WORK_BUDGET.
Raw status 0 unknown, 1 no LAND, 2 absent VHGT, 3 present valid, 4 present invalid.
Selected surface 0 none, 1 canonical authored, 2 canonical rendered default.

## Corrected GPU upload layout and device-limit gate (supersedes v1 proposal)

The v1 packet above is a CPU inspection format, NOT an accepted GPU upload format.
At 11,186 directory entries and valid pages its exact sizes are:

| V1 buffer | Bytes | MiB |
| --- | ---: | ---: |
| Directory | 357,952 | 0.34136962890625 |
| Heights | 48,726,216 | 46.46894073486328 |
| Nodes | 244,302,240 | 232.98477172851562 |
| Aggregate storage | 293,386,408 | 279.79508209228516 |
| Header | 64 | |
| View/instance context | 400 | |

The node binding alone exceeds a 128 MiB storage-binding limit. Three additional
fragment storage bindings were also an unchecked proposal, not acceptance evidence.
`gpu.rs` now provides the bounded GPUv2 upload layout and pre-allocation limit refusal.
It retains every original sample and drops ONLY the 1024 redundant quad-bound leaves
per page: WGSL can derive their bounds from the four retained quad vertices. Remaining
341 nodes/page contain only min/max pairs (8 bytes). Whole-page coverage and reasons
remain in the directory; a partial/unknown page cannot use a status-free node.

| GPUv2 single storage section | Bytes at 11,186 valid cells |
| --- | ---: |
| Directory, same eight-word records | 357,952 |
| Heights, unchanged | 48,726,216 |
| Upper nodes, 341 pairs/page | 30,515,408 |
| Single storage buffer total | **79,599,576** |

This is 7,116 bytes per valid cell, 75.91207122802734 MiB total, below 128 MiB.
Header stays 64 bytes; context stays 400 bytes before device dynamic-offset padding.
All counts above assume every one of the stated 11,186 cells has a valid canonical
page; this is capacity arithmetic, not a claim that incomplete source rows are valid.
The current runtime-only GPUv3 retains those word offsets: directory starts at zero;
header word14 is the height-section offset, word15 the upper-node-section offset,
both in u32 WORDS. Header version=3. The table above describes the unchanged base
sections, before the world partition described below.
Directory root is now a compact PAIR index `(page+1)*341-1`. Upper level offsets are
0,256,320,336,340, sides16,8,4,2,1, with each bottom node bounding a 2x2 quad region.
No height quantization, simplified caster, omitted cell or sun-distance cap is added.

GPUv3 appends a deterministic DFS world partition after `pages * 682` compact-node
words. Each nine-word record holds inclusive signed cell min/max (four words),
canonical min/max height bits (two), a strictly forward subtree escape index,
kind and Unknown reasons. Kinds are unbounded branch (0), height-bounded branch (1),
surface cell (2), certified empty rectangle (3), Unknown rectangle (4). The suffix
must be nonempty and divisible by nine; node count follows from its exact size.
The CPU builder validates the topology; the receiver also refuses non-forward or
out-of-range escapes and invalid visited rectangles/kinds.

Every domain cell belongs to the partition, including absent directory keys.
Unknown descendants prevent aggregate height bounds. Surface bounds come from
the exact retained canonical samples. Integer cell bounds are converted relative
to the receiver, with uncertain arithmetic treated as a possible intersection.
Inclusive boxes retain shared edges/corners; surface leaves keep the original
triangle predicates. Finishing the tree does not certify the ray outside the
domain. Full-content work budgets include every world node and every local page
node; no early exhaustion is used as an optimization. Upload limits include all
appended bytes before storage allocation; a limit refusal never truncates coverage.

Construction runs in the existing source-owned background world job and is reused
across view/sun changes. No package format, database table or producer regeneration
changes. This traversal is an optimization candidate until the actual GPU controls,
visual path and same-population profile pass.

Corrected proposed exact slots for BOTH terrain forward variants:

```wgsl
// Existing cluster group 4 bindings 0..6 retain their meanings.
@group(4) @binding(7) var<uniform> f1_header: F1Header; // four vec4<u32>
@group(4) @binding(8) var<storage, read> f1_words: array<u32>;
@group(4) @binding(9) var<uniform> f1_view_instance: F1View; // 25 vec4<u32>
```

Root must derive full fragment-visible layout counts from the ACTUAL specialized
pipeline, then call `Footprint::check`/`GpuUpload::build` with the CREATED RenderDevice
limits, not the physical adapter maxima. Checks include this single storage binding
against both max_storage_buffer_binding_size and max_buffer_size, stage storage and
uniform counts (+1 and +2), uniform bytes, five bind groups and u32 word addressing.
Refuse the entire optional upload with an explicit reason before allocating its Vec;
never raise requested caps, truncate the world or call omitted casters clear. Structural
packet checks do not reauthenticate content: input must remain an unchanged output of
the certified-input CPU packet builder. Full host SnapshotIdentity is retained.

Stock needs an equivalent group/layout and draw command; cluster extends group4.
New entries are FRAGMENT-only; dynamic offsets use actual device alignment. Empty
storage uses four dummy bytes with zero counts. No new texture, sampler, camera,
shadow pass or compute pass is needed. No shader/binding exists when the option is off.
Unknown/empty records cost 32 bytes. Worlds too large for one checked binding are
explicitly unsupported by this bounded upload; a future paging design must preserve
upstream reach independently of camera residency. No paging is implemented here.

Fragment storage resource audit uses real Bevy0.19 shaders (including actual vendored
bevy_render) and checked runtime `d349ba57`, compiled by NagaOil and inspected through
Naga entrypoint global usage in `bindings_audit.rs`, imported only by the durable
test harness. Full-layout visibility is a SEPARATE gate: an unused
but FRAGMENT-visible storage entry still consumes pipeline-layout budget. In particular,
visibility_ranges@group0/binding14 is VERTEX-only in the actual Bevy layout; counting
every WGSL declaration as a fragment resource would overcount it. Conversely, bindless
extension layout tables may remain allocated even when the fragment never reads them.
The observed desktop forward baseline with fog, DFG and storage lighting is:

| Material mode | Entrypoint-live storage | Fragment-visible layout storage | Layout with GPUv2 | Layout uniforms, before/after F1 |
| --- | ---: | ---: | ---: | ---: |
| Actual stock (nonbindless) | 4 | 4 | 5 | 7 / 9 |
| Cluster nonbindless | 5 | 5 | 6 | 7 / 9 |
| Cluster bindless | 7 | 9 | 10 | 5 / 7 |

Live common resources are (group,binding) `(0,8),(0,9),(0,10),(2,0)`; cluster adds
`(4,6)` clip records; bindless cluster also reads `(3,0),(3,10)`. The extra bindless
extension layout buffers `(3,100),(3,101)` are NOT live in this fragment, but consume
layout slots. Thus three extra buffers on bindless cluster would need 10 live/12
layout storage slots; even GPUv2's eight live buffers would still need TEN layout
slots. Nonbindless GPUv2 needs six, within eight without raised limits.

Source layout evidence: Bevy mesh_view_bindings.rs default fragment entries8/9/10;
its binding14 override is VERTEX; mesh_bindings.rs mesh storage is VERTEX_FRAGMENT;
checked cluster draw layout marks only entry6 VERTEX_FRAGMENT. Checked cluster terrain
extension enables bindless data50/binding_array101 and index_table100; stock terrain
extension has no bindless annotation. AsBindGroup's bindless entries include FRAGMENT
visibility. These are code/layout counts, not measured created-device capabilities.
The audit records exact shaderdefs and asserts full binding-pair sets, not just totals.
Current checked engine uses its own atmosphere, not a Bevy Atmosphere component,
and no OIT/clustered-decal component was found. Optional Bevy atmosphere adds one
layout storage buffer; decals add one; OIT adds three plus two uniforms. SSR/contact
settings add up to two uniforms. Such variants require their actual full-layout
count; the compute cull's portable-eight rule is not fragment evidence. Naga
Capabilities::all here permits liveness inspection, not device capability acceptance.

For the 3090 demo, refusal is not the completion plan. Use this complete-world compact
buffer plus an explicit NONBINDLESS F1 cluster-terrain material variant if the current
bindless layout has insufficient stage slots. A MaterialExtension type with the same
clip uniform/shader semantics but no bindless annotations makes ExtendedMaterial force
the base StandardMaterial nonbindless as well (Bevy extended_material.rs). This is
one targeted material type/pipeline agreement, not a global bindless disable or raised
device cap. Retain bindless F1 when the ACTUAL created-device limit and full
specialized layout permit it (the audited base layout needs at least10 fragment
storage slots); select the targeted nonbindless variant only when needed. Check
optional-resource counts, binding bytes and dynamic offsets for either path.
It preserves geometry and lighting; its draw/bind cost still needs later
functional/profile acceptance. Material-type changes must invalidate root's per-pass
readiness/masks before replacement. No material implementation/change is made here.

Port ownership budget: one shared F1 WGSL module; stock terrain and cluster terrain
forward call sites; root-owned per-view group/pipeline/draw binding; CPU composition
and numeric fixtures; one nonbindless F1 terrain extension type if needed. Existing
prepass, shadow caster queue, replacement masks and
portal readiness stay with root and unchanged by F1. No global Bevy shadow-return
patch: the earlier plan's suggested generic patch would affect unrelated materials.
If direct-sun insertion needs a broader Bevy lighting fork or another runtime owner,
stop at that concrete seam for a new agreement. This is a scope budget, not an
unmeasured development-time or frame-time estimate.

## View, origin and lifetime

Context retains exact SnapshotIdentity, view ID/generation, sun ID/revision and
toward-sun direction, view origin, actual instance origin and origin revision. Direction
is Creation `[east,north,up]`, not a normalized/quantized proxy. Renderer input is
`[east,up,-north]` with horizontal instance-local offsets and world height. Horizontal
integer origins are never folded into large absolute f32 positions. The caller must
certify any vertical translation, rotation or scale; this slice does not invent it.

`pack_view` carries the production context. Fragment receivers must be derived from
actual fragment positions with the same origin mapping; `pack_receiver` is a flagged
single-ray CPU/probe packet, never a draw-wide receiver. Exact input conversion is
necessary but does not certify subsequent floating-point shader arithmetic.

Portal/destination views bind their own world and sun. Match full snapshot identity
on the host before publishing buffers; world alone or a truncated hash is insufficient.
Any world/database/cache/recipe/view/sun/origin/instance transform revision invalidates
dependent results. Resource/residency or pipeline failure invalidates F1 availability;
never keep an old result as current. Publication must use root's checked per-view
generation/readiness lifecycle, with no changes to replacement or caster masks here.

## Receiver self-intersection and uncertainty

Actual simplified fragments can be BELOW the canonical model. Starting a ray there
can hit its own overlying terrain and falsely blacken everything. Neither an arbitrary
normal/vertical bias nor skipping a finite interval is allowed: either can skip a
nearby narrow obstacle, especially at low sun.

The earlier actual-rendered-contact API in `view.rs` retains its explicit relationship
gate. Merely sampling a matching height cannot mint ExactCanonicalContact. That API
alone cannot finish simplified receiving. Root subsequently accepted the distinct
canonical-surface visibility-field model below; the new query implements that model
with context word15=2, without falsely changing the actual-contact enum.

A later bounded error contract could classify rays robustly over a certified receiver
position interval; it must return Unknown where the visibility varies across that
interval. No such error envelope, projection policy or generic proof framework is
implemented here. The canonical shadow model remains valid even when its receiver
relationship is unresolved; caster silhouette equality is a separate claim.

Accepted shadow-model decision: F1 is the canonical surface's sun-visibility field,
transported to the rendered terrain
by a certified world/cell footprint mapping. For an actual fragment, preserve its
rendered position for all lighting/depth/normal calculations; use its certified
east/north footprint to locate the post-weld canonical base triangle, and evaluate
F1 at that triangle's canonical contact. This is an EXPLICIT canonical-receiver
model, not a ray proof at the actual simplified height and not an arbitrary height
bias. It changes the question being answered; visual acceptance remains required.

Required data: exact snapshot and per-view sun/origins; owning primitive's source
world/cells; compatible XY coordinates; canonical cell/quad/half and barycentric
contact (including seam/boundary ownership). Carry contact symbolically or with a
certified arithmetic enclosure so rounded interpolation cannot start below its own
surface. Exclude only the exact zero contact; no finite interval/obstacle skip.
Missing source footprint, a gap, coordinate uncertainty or ambiguous visibility
remains Unknown with reason. A rendered triangle spanning a missing canonical cell
does not invent coverage. This allows ordinary simplified fragments over certified
canonical footprints to receive the explicit shadow model, without asserting their
drawn silhouette equals the canonical caster surface. `receiving.wgsl` implements
this distinct model; the actual-contact enum is unchanged. The visual receiving bar
is NOT passed by source/composition tests.

If the receiver must instead mean visibility at the ACTUAL drawn height, the next
contract must supply an exact rendered-surface contact and compatible caster model,
or a certified rendered/canonical error envelope with visibility stable over it.
Merely asserting ExactCanonicalContact for simplified fragments is not that contract.

## Traversal/lighting obligations before GPU adoption

Port the same canonical base triangles with conservative arithmetic. Page extrema
are acceleration bounds, not an occlusion hit. GPU f32 cannot copy the CPU exact-dyadic
predicates naively: ambiguous comparisons, under/overflow, and nonzero subnormal
samples/bounds must return NUMERIC_UNCERTAINTY or use a separately certified conservative
method. Preserve low-sun direction; no elevation clamp. A traversal budget may bound
work but exhaustion is WORK_BUDGET Unknown, never Clear. Returning Occluded requires a
real positive-ray hit witness; traversal must keep searching known regions despite
earlier gaps. Clear requires whole-ray coverage, not merely reaching DB bounds.
The isolated query now traverses directory cells with a closed-boundary supercover
and the compact per-page hierarchy; sparse gaps cannot be jumped as certified empty.
This slice supplies no world-level acceleration tree.

One shared result function feeds stock and cluster terrain AFTER their existing clip/
discard decisions. Apply F1 only to the selected sun's direct lighting beyond the
last cascade for the first receiving slice; preserve other lights, ambient, fog,
tonemapping and existing cascade visibility. Do not multiply final shaded colour.
Unknown contributes neutral F1 visibility while remaining explicitly Unknown in
diagnostic output/counters; this is incomplete lighting, not a successful Clear.
Expose the reason bits and coverage in the opt-in overlay/acceptance evidence.
No extra filtering, fades, CSM quality reductions or performance claims are in this slice.

CPU acceptance here: exact encoding/lookup, narrow-peak preservation, shallow-sun
intersection using unpacked samples, unknown-child propagation, incomplete-source
attestation refusal, raw-VHGT status guard, post-weld gate, negative/far origins,
per-view/sun identity separation, explicit precision/contact uncertainty. GPU numeric,
visual and cost acceptance remain separate future gates. No GPU run requested now.

## Implemented isolated query and exact root wiring seam

New source files are `contact.rs`, `arithmetic.rs/.wgsl`, `triangle.rs`,
`receiving.rs/.wgsl`, and test-only `wgsl_tests.rs`. They are imported ONLY by the
existing durable integration harness; no runtime module/plugin is registered.
Existing source edits are that harness and three test-helper visibility qualifiers.

Host call, before publishing a draw's resources:

```rust
let context_words = receiving::prepare_canonical_view(&world_upload, &view_instance)?;
```

This compares FULL SnapshotIdentity, checks GPUv2/canonical topology and the supported
binary-power cell scale, packs the actual view/sun identities and revisions, both
origins and origin revision, and tags the context as canonical-field model2. It does
not mint actual rendered contact. Bind the immutable world upload and 400-byte context
in one checked root publication epoch. Reuse `Footprint::check` with ACTUAL created
device/full-layout limits. Root retains all reader/invalidation/readiness ownership.

Register arithmetic then receiving with import paths
`far_terrain_shadows::arithmetic` and `far_terrain_shadows::receiving`. Proposed group4
bindings remain7 header64B,8 read-only words,9 context400B. No new sampler, writable
storage, texture or pass. Nonbindless composition is checked here; bindless adoption
requires the actual created-device/full-specialized-layout proof described above.

Per fragment, after the existing terrain clip/discard and only when eligible beyond
the selected sun's ACTUAL final cascade bound in THIS view:

```wgsl
let query = f1_receive_at_origin(instance_local_renderer, primitive_world,
    actual_instance_cell_origin, actual_view_id_words, actual_sun_id_words,
    max_cells, max_nodes);
let far_visibility = f1_visibility(query);
```

`instance_local_renderer` is interpolated anchor-relative `[east,up,-north]`, with
horizontal origin exactly `actual_instance_cell_origin: vec2<i32>`; rendered up is ignored
by the canonical query. It is NOT arbitrary `in.world_position` or a large global
position. Root must obtain compatible local XY from the actual stock/cluster vertex
source/transform (e.g. an anchor-relative varying) and preserve its world/instance
identity. Supply the certified per-instance cell anchor from the actual terrain
record/flat varying, matched to that instance's interpolated local XY; do not
interpolate integer origins across instances. Several origins can share the same
400-byte view context and draw group without splitting draws by anchor. Uncertified
rotations/scales remain unsupported. The existing `f1_receive(local, world, view,
sun, max_cells, max_nodes)` remains a wrapper using context words12/13, valid for
callers whose local coordinates match that fixed origin. Only the new entrypoint
overrides the mapping origin; no binding, uniform, packet or numeric acceptance
change. Full world/sun/snapshot/epoch and instance-origin association checks remain
host-owned. This does not change source geometry.

Expected view/sun IDs must come from the actual retained view/light record, not be
read back from F1's own uniform to make the guard tautological. Full generation/sun/
origin revisions are host publication keys; WGSL identity guards supplement them.
Direction toward sun is Creation `[east,north,up]`; map renderer direction as
`[render.x,-render.z,render.y]`, preserving that view's chosen light. No normalization,
sun-elevation clamp, parent-world inference or main-camera residency filter occurs.

Smallest DIRECT-SUN insertion: in the terrain-only F1 lighting variant, keep Bevy
`pbr_functions::apply_pbr_lighting` directional-light loop behavior and change only
the selected sun's existing `shadow` before `direct_light += light_contrib * shadow`
(Bevy0.19 pbr_functions.wgsl:631-636). Preserve the existing shadow-receiver/light flags:

```wgsl
if i == selected_sun_index && beyond_last_cascade_for_this_view &&
    mesh_receives_shadows && selected_sun_shadows_enabled {
    shadow = min(shadow, far_visibility);
}
```

Root maps actual sun entity identity to this view's light-array index; never hardcode
light0. Keep original cascades/contact shadows inside their reach. Point/spot/other
directional lights, indirect/ambient, fog and post-lighting stay unchanged. Terrain is
opaque/nontransmissive for this first seam; transmitted-light support would require
its own receiver policy, not reuse a different offset silently. Do NOT multiply final
`apply_pbr_lighting` output: that would darken unrelated terms. No global Bevy shader
patch is implemented here; root owns the terrain-only lighting hook/variant agreement.

The real-stock/cluster composition fixtures insert a DIAGNOSTIC query/output use
only to keep resources live. Their placeholder input is not a production coordinate
mapping and their tiny output perturbation is not direct-sun integration. They prove
composition/resource/layout compatibility, not visual correctness.

## Implemented traversal, numeric limits and diagnostics

`F1Result` returns state0 Unknown,1 Clear,2 Occluded; ORed reasons; cells, nodes,
triangles, unknown_cells counters. `unknown_cells` counts unresolved visits (including
possible supercover duplicates), not unique world cells. A known positive hit wins
over earlier gaps and retains their diagnostic bits. Visibility is0 only for Occluded;
Unknown contributes neutral1 while remaining visibly unresolved in root diagnostics.
Root can expose reasons/counters through its overlay/readback path without an F1
writable-storage binding. This slice does not install that UI/readback system.

New reason bits13 COPLANAR and14 BAD_PACKET complement the existing coverage/numeric/
work reasons. Header/mapping/world/view/sun/topology mismatch, malformed offsets,
unconfirmed post-weld source and unsupported arithmetic do not turn into Clear.
No finite ray distance certifies Clear. Max-cell/max-node work budgets are caller
inputs; exhaustion or counter overflow returns WORK_BUDGET Unknown, not a quality cut.
Outside inventory remains unknown unless the reader's complete-source exterior
attestation permits it. Incomplete showcase data can therefore produce many Unknown
unoccluded rays; valid known upstream mountains can still prove Occluded.

Cell traversal supercovers uncertain corner order, side cells touched at positive t,
and both strips of a parallel grid-edge ray. Uncertainty spanning another boundary
returns Numeric Unknown. Nodes use compact min/max bounds; invalid broadphase math
means MAYBE, never skipped geometry. Leaves derive both original base triangles from
four unmodified samples. No averaged height/triangle/cell omission is introduced.

Canonical contact is represented by cell/quad/half, fractions and source samples.
Only proven same3D-contact triangles are excluded after canonical-plane noncoplanarity
is established. Shared edges require equal canonical endpoint heights; shared vertices
require equal height. A discontinuous seam neighbour is still a distinct obstacle.
Coplanar/near-parallel ambiguity is Unknown, independent of rendered normal-map NdotL.

Arithmetic uses small f32 endpoint enclosures, mirrored by `arithmetic.rs` and the
triangle predicate in `triangle.rs`. Accuracy basis: the official
[WGSL floating-point rules](https://www.w3.org/TR/WGSL/#floating-point-accuracy).
Binary operations widen outward; division widens eight representable steps. Every
nonzero operand endpoint must be in `[2^-60,2^60]` BEFORE performing arithmetic;
this keeps operation results finite and normal without relying on post-overflow
classification under WGSL finite-math rules. Exact zero/point identities can bypass
an operation safely. Unsupported inputs/operations are Numeric Unknown. This is a
numeric domain, never a height bias or obstacle threshold.

Cell spacing currently must be an exactly represented binary power (normal reciprocal
step), otherwise host preparation refuses it. XY/direction scaling changes exponent
bits, so it cannot silently flush a nonzero component. Integer significand splitting
preserves exact cell/local XY or refuses an unrepresentable negative remainder.
Relative cell offsets beyond524287 are Numeric Unknown before sample-index conversion.
These limits are explicit; there is no claim of universal floating-point/GPU proof.

CPU fixtures cover contact interior/diagonal/edge/vertex, seam height disagreement,
negative/extreme origins, nonzero scaling underflow, guarded arithmetic, positive
nearby obstacles and low-sun narrow ridges against the exact oracle on hand-chosen
dyadic contacts. They do NOT execute WGSL numerically. NagaOil/Naga compose and
validate the real isolated modules with checked stock and nonbindless cluster shaders,
asserting header64/context400 and one added read-only storage binding (live totals5/6).
Actual GPU execution, compiler behavior, model acceptance at cascade/LOD transitions,
ridge/narrow-obstacle images and measured pass cost remain root's future gates.
