# Cluster object LOD parity, 2026-10-18

Implementation base: `210ab8e5`, branch `opt/cluster-showcase-integration`, checkout
`D:/Mudcrab/merge/dev-integration`. Final gates and the authorized missing-data
representation mode pass below. Producer-dependent replacement and validation
limits remain explicit; this is not a per-reference BTO identity certificate.

Local source commits: `27beaa13` settings/fingerprints, `087355cf` grid-bound
demand, `a75b7892` native representation integration, `ecb9b96f` extraction
retention. Every runtime-r8 Rust source hash still matches after this split;
there is no remaining uncommitted Rust source. Nothing was pushed or published.

## Revised representation decision

The lead revised the original per-object BTO suppression requirement after the
input audit: vanilla BTO geometry carries no reference identity. Suppression must
be exact at block/level scope, requiring every possible contributor (record VWD
flag plus a LOD model at that level) to have certified drawable replacement in
the same view/pass. Otherwise native BTO draws and the corresponding contributor
clusters do not. Noncontributors may use certified clusters beyond the grid.
Missing contributor data requires native retention and counted, logged far
cluster suppression. Missing tree relation/runtime proof likewise retains native
billboards and suppresses far tree clusters. This decision is in the task thread.

## Input audit

Read the full task brief, distant-fallback-loader.md F1-F6 and demo-toggle plan,
prior measurements and newest handoff entries. The checkout's ignored local plan
and research indexes are absent. `tools/skyrim_rec.py index` and `sheet STAT`
were read; their own stale-sheet notice is retained as a limitation. The live
schema and current exporter independently establish the statements below.

Read-only SQLite and all 1,078 object GLB JSON chunks were inspected in
`D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified`:

- `lod_block`: 744 level-4, 248 level-8, 86 level-16 object blocks.
- `lod_tree_instance`: 43,333 rows; columns worldspace_id, block_x, block_y,
  tree_index, pos_x, pos_y, pos_z, rotation, scale. No reference or ordinal.
- `lod_tree_reference` and `lod_tree_reference_binding` are absent. Manifest
  lists inventory v1, but no `mudcrab.lod_tree_references` extension.
- Both tree reader commits `23d2aa32`, `beaa0b2d` are Git objects, not ancestors
  of this HEAD. Source was read with `git show`; their branch was not merged.
- `statics.flags` preserves `record.flags` (esm/exporter.rs, generic world-model
  arm). `statics.model_path` is the full `MODL` model, not a per-level LOD path.
  Actual `statics` columns are id, editor_id, model_path, flags, bounds_min_x/y/z,
  bounds_max_x/y/z and bounds_valid.
  No normalized STAT `MNAM`/level-0..3 model-path columns or table exist.
  Raw archived `records.data` is not an implemented/validated LOD-path consumer.
  xEdit's [TES5 definitions](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas#L9751)
  distinguish base STAT Has Distant LOD (header bit 15) from REFR Visible When
  Distant (also bit 15). STAT MNAM contains four fixed-width path fields. The
  runtime conservatively retains native LOD for flagged/unknown bases while the
  per-level paths are unavailable; a clear base flag is data proving a
  noncontributor under the lead's rule, so certified far clusters may draw it.
  Live STAT-reference/base-flag combinations (base bit, reference bit, rows):
  (0,0,140154), (0,1,564), (1,0,8485), (1,1,12271). These different flags must
  not be substituted for each other.
- Across all BTO GLBs: 2,392 meshes; primitive counts 8,449/384/152 at levels
  4/8/16. Native primitive extras contain cell `lodSegment`, not reference IDs.
  Material extras identify source shape/shader blocks, not placed references.
  No position/nearest-neighbour join or entire-cell cluster ownership is valid
  evidence of per-reference BTO replacement.
- Converter `mesh/lod_segments.rs` explicitly omits tags for shapes with at most
  one nonempty segment (known N6 source limitation). The port must report its
  effect rather than claim that an untagged primitive is a proven empty cell.
  The audited payloads contain 430 level-4 singleton shapes without these tags;
  they stay visible rather than being assigned invented cell identities.

Audit script/log and exploratory patch are retained under the owned ignored
`local/vg7/object-lod-20261018/`. A read-only DeepSeek source audit independently
confirmed the native identity limitation; its actual package access was denied,
so the live SQLite/GLB claims above are the implementation agent's direct checks.

## Validation and runtime results

Final immutable r8 quick executable SHA256:
`964aeb90765ad9f7455a887895bf8825bb57140bf9f82a9538d012b0d96cdc5e`.
Both modes use it with showcase settings, seamless doors, the certified dataset,
default stream radius 2, and optional far-terrain shadows off. The first three
cycle34 poses are unchanged; 20 additional captures repeat the same far pose
with the ordinary quiet-frame count and deadlines. No timing hold is used.

Clusters-off r8 exits 0 and saves all 23 captures. Canonical settles are
15.22/0.35/0.52 s. The final 120-call interval is
06:12:01.041928..06:12:02.875743 UTC: native extraction 2.2415 ms/call,
timestamp-derived cadence 15.2818 ms/call. Cluster source extraction is not
installed, and optional far-stock demand/queue is absent (zero). Current native
queued-draw proof reports pending=0, awaiting=0, visible=1518, drawn_twice=0 and
drawn_by_nothing=0. No nonzero overlap/hole report occurs in the run.

Clusters-on r8 exits 0 and saves all 23 captures. Canonical settles are
0.40/8.53/0.24 s after the existing separate certification/upload waits.
Certification 131265.6519 ms comprises envelope 18254.8956,
source-authority 3385.438, mapping 50312.7762, mapped-index 3456.4234,
examined-index 55475.6109 and coverage/index 380.2495 ms. Main-thread loading is
8042.0776 ms. This run overlaps another thread's heavy build through the ordinary
gate; the longer certification is a shared-PC observation, not an isolated
regression measurement or hidden shot-settling cost.

On far demand is 1,864 references, all 1,864 resident; queued/pending/failed cells=0,
responses=0 and spawned references=0. The final 120-call interval is
06:14:54.568634..06:14:58.170687 UTC: source extraction 3.6097 ms/call,
non-terrain 2.9603 ms, terrain identity/copy 0.6495 ms (identity query 0.6389 ms),
timestamp-derived cadence 30.0171 ms/call. Native extraction separately costs
2.6830 ms/call. The preceding interval gives source 3.5659 ms and cadence 26.6542 ms.
Qualification refreshes stay 449 across the last 360 calls, reusing all 360;
that cache window begins about 2.2 s before the far shot finishes settling.
The measured final 120-call interval is entirely post-certification and
post-far-ready and adds no hot/identity/other/structural invalidation. This is
distinct from the pre-certification r7 warm interval.
Current native proof reports pending=0, awaiting=0, visible=1481 and both counters=0.
Every overlap/hole report in both final logs is 0; no shot times out, no ERROR or
panic occurs. Both runs emit a render-statistics async-map warning during teardown
after the captures, and exit 0 naturally.

Personally inspected both r8 modes' door/far originals and final repeat:
lit fireplace/pot through the doorway, Whiterun walls/tall keep and billboard
trees remain visible. Mountains/terrain differ with the cluster extension;
no missing keep or billboard group is apparent in the inspected views. This is
a same-pose visual check and the original fixture comparison below, not a claim
of pixel equality between extensions or against an independent branch binary.

The exact original screenshots and all counter logs remain in the owned root:
`off-r8-extended/` and `on-r8-extended/`, with immutable executable identity in
`runtime-r8-identity.json`, plus `object-lod-{off,on}-r8-extended.{log,json}`.

These are cumulative-counter deltas over 120 calls after readiness, with cadence
from the matching timestamps. They are shared-PC TEST inferences, not isolated
FPS or a causal GPU/portal-cost comparison. The requested profile-output
directories contain no emitted bundle; the retained CPU logs and JSON command
records are the evidence. Overlay FPS is ignored.

### Bounded-demand precursor

Pre-extraction-cache r6 uses the same immutable quick-profile executable for both
states: SHA256 `c08620f7ae006e910da97506e9bf6acc56f559422e6dc247f02a82ac4c382f06`.
Both gated TEST runs exit0 and capture the three unchanged cycle34 poses.
Cluster-off settles in 13.50/0.27/0.15s; cluster-on in 0.52/8.41/0.56s after
the existing separate certification/upload waits. No deadline/reach/budget was
changed. Native inventory hashing takes 13.282s off and 12.708s on (owned files).
Cluster certification takes about107s; precise phases remain in the on-r6 log.

Far stock on: 1,864 desired references, all already resident, no sparse spawns,
queued/pending/failed cells0. Off: the optional far-stock system is absent.
All periodic native structural overlap/hole counters are0 in these runs.
The final on120-call interval (04:54:31.380245..04:54:44.793244 UTC) measures
source extraction33.295ms =27.004ms non-terrain +6.291ms terrain identity/copy;
timestamp-derived cadence111.775ms/call. This is a shared-PC loading TEST
inference, not settled FPS or a release performance claim. It is an improvement
over the prior89-99ms source extraction, but still requires the incremental
qualification/map-copy work below. Cluster-off does not schedule cluster source
extraction. Its native extraction costs2.245ms in the final120-call interval,
with timestamp-derived cadence14.638ms/call. Pose/loading windows differ and do
not isolate cluster GPU or portal costs.

Personally inspected r6 on/off door and far screenshots, including enlarged city
crops and the on repeat. Both show the same lit fireplace/pot through the open
door, Whiterun's walls and tall central keep, distant billboard trees and native
object detail. Cluster terrain changes mountain geometry as expected. Overlay
FPS is not used as evidence. These are visual checks, not pixel parity with an
independently run plus-n2 binary. This precursor is superseded by final r8 above.

## Implementation and review milestones

- Ported the native selector, BTO payload/hash/bounds validation and unload pacing,
  tree atlas/type geometry and merging, settings/INI/fingerprint and readiness
  hooks from `plus-n2-object-lod` at `3a46745c`, retaining the current index.
- Removed old FullDetailReady and both main-world cell hide systems. Native
  coarse/fine and near handoff use per-render-view StockDrawable payload/material
  and enabled forward/prepass pipeline proof. Current actual queued membership
  is checked after preparation for readiness. Ancestor stand-ins stay resident.
- Native BTO/BTT roots publish together with terrain/far stock through the existing
  combined role owner. Pinned roots stay hidden. No second crossing planner or
  implicit old tree-plugin installation was added.
- Ordinary exact-source demand now uses stream_radius, with position-cell index
  ranges bounded to the loaded grid. No terrain-reach full-model fallback remains.
- Source contributor metadata distinguishes trees and available base header flags;
  missing per-level paths and missing tree relation/runtime proof are explicit
  counted/logged suppression reasons. This is the lead-authorized conservative
  path, not the unavailable complete-contributor replacement mode.
- Review #412 found the paced-despawn wakeup bug; fixed by invalidating the plan
  when the pacer removes a block. Main/render selection history is cleared on a
  world change. Stub visibility metrics were removed in favor of render decisions.
- Own follow-up found held hidden cell roots could vacuously qualify an empty
  visible set. Root explicit/inherited visibility and per-view layer eligibility
  now gate near coverage before primitive draw proof. Regression fixtures cover
  hidden roots and nested native/cluster visibility-filter restoration.
- Read-only review #412.1 confirmed that installing the cluster RequiredHandoff
  owner in clusters-off mode would activate a different portal policy; native
  readiness uses the existing per-view queue proof instead. Fixed its STAT
  Has Tree LOD classification finding, sparse fine-inventory audit finding and
  world-wide failed-tree-type readiness hold. Demanded blocks still fail visibly
  if their named type cannot be merged, including a wholly failed catalog.
  Did not adopt its suggestion to release unknown/tree clusters when native
  inventory is absent: the lead explicitly requires conservative suppression
  without qualified contribution/relation data. Such inputs cannot establish
  exact far replacement, and are reported as missing contributor/proof data.
- Focused streaming nextest at the pre-review-fix revision: 229 passed, two
  held-interior fixtures failed because their fake manifest used `abcd` as a
  SHA-256. Updated them to a valid 64-digit digest; production hash validation
  stays unchanged. The final workspace run passes both fixtures.
- Diagnostic r3/r4/r5 clusters-off runs honestly held the door's 30-second
  deadline. r5 isolated all 273 rejected BTO primitives (35 blocks) to absent
  material bind groups *before Queue*, while selected draws passed post-prepare
  proof. Bevy's bindless allocator invalidates a slab on material allocation/
  updates and normally rebuilds it later in PrepareBindGroups. Native selection
  now invokes the same preparation before selection; the ordinary later pass
  remains safe for unchanged slabs and normal buffer-upload ordering is retained.
  No predicate/deadline was relaxed. r6 validates the fix in both states.
- Extraction-cache review found two invalidation holes before retention: bare
  global lifecycle observers do not establish Bevy's archetype observer flags,
  and a temporary scene/certificate mismatch cleared bindings without forcing a
  future rebuild. Replaced them with component-scoped observers covering proof
  components and registered default disablers; uncovered newly registered
  disablers use the original uncached/query-domain path. A mismatch invalidates
  qualification. Independent follow-up finds no blocking production issue;
  the final workspace regression gate passes these cases.

## Incremental extraction scope

The first cache revision (r7) passed both canonical runs and extended repeated
captures, but its post-certification 120-call interval still refreshed
qualification 92 times: source extraction21.627ms, including terrain0.404ms,
cadence40.503ms/call. This justified the subsequent value-based hot-fact
invalidation. The pre-certification warm interval was much faster, but is not
used as the post-ready result. r7's retained extended logs distinguish the two.

The expensive source qualification maps are retained while their inputs are
unchanged. Certificate, origin, asset invalidation, source identity/reparenting,
watched proof-component mutations, structural changes and extraction-fetch gaps
force a fresh qualification. Camera/world ownership, light dependencies,
material/template qualification and actual render-view/pass draw proof remain
current-frame work. This is a cache of authoritative CPU source facts, never
cached GPU draw readiness. A changed source can still trigger a complete bounded
qualification rebuild; it is not a per-reference delta updater.

Terrain identity returns a shared immutable snapshot. The extractor converts
Entity keys into MainEntity keys only when that snapshot changes, and avoids
repeating equal-map comparisons/copies. With observed structural inputs, warm
identity queries avoid enumerating all main-world nodes for membership. The
existing exact membership-scan fallback remains available without the tracker
or with an uncovered disabler. Bevy's default-filter domain is fixed when each
query is initialized; the fallback preserves that query's original semantics.

Hot proof inputs compare their values, not only Bevy Changed ticks. Root/cell
transforms remain literal. A model node outside its authored baseline remains
unqualified as it animates; returning to the baseline invalidates qualification.
Identity snapshots reset across an extraction-fetch gap, so a change missed by
that query's tick window cannot leave an old equality baseline. Independent
source review found no blocking issue in this refinement; it did not run builds.

## Scope and remaining producer work

The missing-data branch of the lead's rule is implemented. Complete-contributor
block replacement is unavailable until the converter publishes validated
per-level STAT MNAM model paths. Tree cluster replacement remains disabled beyond
the grid until the reference relation and runtime proof are present. Neither
absence is inferred from geometry. Known non-tree bases with Has Distant LOD
clear can still use certified far clusters; flagged/unknown bases and trees keep
native representation, with separate suppression counts per view/pass.

The overlap/hole counters audit native block/cell selection and retained
stand-ins. They do not claim a per-reference BTO contribution proof that the
dataset cannot supply. Missing finest native inventory is not invented as a
hole; absent/failed demanded payloads remain pending/failed separately. Known N6
untagged singleton geometry stays visible, preserving the port's behavior.

This branch's existing demand protocol elects one exterior owner (main exterior,
otherwise an exterior portal). The port consumes that protocol without adding a
second crossing planner. Concurrent distinct exterior-view ownership is not
established by these runs. Native loader readiness also conservatively waits for
demanded ring payloads, while actual queued-draw proof is view scoped.

The port has not been pixel-compared against an independently run
plus-n2-object-lod executable. A source fixture audit against `3a46745c` verifies
that all 18 original object-LOD tests and all six original tree-LOD tests retain
identical test bodies (whitespace normalized), with none missing. These exercise
the original selection/stand-in, segment, billboard geometry and placement
expectations; final workspace nextest passes them with the reconciled index.
Same-pose on/off images additionally establish visible native objects and
billboards, not a stronger independent pixel-parity certificate.
The next launch-option task should cover {clusters on/off} × {load screen/seamless
doors}, preserving this missing-data policy and separating certification time
from shot readiness and post-ready extraction.

## Rejected diagnostic runs

- Initial on diagnostic used the old pre-Queue material proof and did not reach
  ready. It was stopped through its own window's CloseMainWindow; its log is
  retained, not treated as a natural successful run.
- off-r2 exited0 and saved images before native inventory validation finished.
  Inspection exposed terrain-only far views: a later metrics pass erased the
  opening pending count. The loader now owns persistent inventory_expected
  state, and its regression calls the real metrics publisher repeatedly.
  off-r2 is explicitly rejected acceptance evidence.
- off-r3/r4/r5 truthfully fail the door deadline, while far poses pass. They
  distinguish raw-primitive over-wait from the eventual material-bind-group
  ordering diagnosis. The corrected r6 runs above supersede them.
- Engine all-target/all-feature quick clippy at the pre-held-root revision passes
  with -D warnings (`clippy5.log`). It is not the final-tree acceptance gate.
  Development checks that found/fixed syntax, type and clippy errors are retained;
  aborted default-profile and lib-only checks are not passing checks.

## Final gates

All commands use the mandated C: target and serialized compiler jobs through
the shared build gate, without a timing hold. Raw command, timestamps, exit code
and logs are under the owned evidence root.

- `cargo fmt --all -- --check`: passes (`fmt-final`, exit0).
- `cargo clippy --profile quick --workspace --all-targets --all-features -- -D warnings`:
  passes (`workspace-clippy-final3`, exit0). This includes the touched engine
  crate. The dependency proc-macro-error2 future-incompatibility notice is not a
  project warning and is retained in the log.
- `cargo nextest run --cargo-profile quick --workspace --all-targets`: passes
  (`workspace-nextest-final`, gate exit0), 3,869 passed, 50 existing skips.
  Two tests are slow; the last streaming fixture passes in 147.686 s. The wrapper
  subsequently fails to print the Unicode nextest separator to the CP1252
  console, after preserving the gate's exit0 in JSON and the full passing log.
  This is an output-wrapper failure, not a passing wrapper invocation; the
  wrapper now explicitly uses UTF-8. No passing cargo check was repeated for it.
- `cargo test --profile quick --workspace --doc`: passes
  (`workspace-doc-final`, exit0), four existing ignored examples, no failures.
- `cargo build --profile quick -p engine`: passes
  (`runtime-build-final`, exit0), the source of immutable runtime r8.

A final independent read-only evidence audit verified the raw measurements,
captures, gate results and source/binary hashes. It found no material mismatch;
the 360-call reuse window and post-ready 120-call timing window are scoped
separately above. That reviewer ran no builds or edits.
