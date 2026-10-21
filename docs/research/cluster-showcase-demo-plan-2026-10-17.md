# Cluster showcase demo: restore the portal demo experience

Status: not ready; current action order updated 2026-10-18. Requested after the user tried the Tamriel
cluster demo. This is a VG7 integration plan; it does not start a second demo engine or publish work.

Execution started on the user's go, 2026-10-17. Existing OP6, OP4/OP15, A16/OP3, S5, converter and
H18 threads received their boundaries and requests for checked heads/integration status. OP4 and
OP15 are now claimed by the existing preset research thread; OP10 remains unclaimed. The launcher
milestone reuses `tools/demo/play-demo.ps1` with configurable engine/package and the default walking
controller. The local Tamriel shortcut now starts on the Riverwood road instead of the aerial pose.
Parser/dry-run and independent review checks passed; interactive input/GPU acceptance is outstanding.

## Current blockers and action order (2026-10-18)

The user resumed coding, builds and result-driven review cycles on 2026-10-18, replacing
this slice's earlier no-build restriction. The demo target is now stable 120 fps (8.33 ms frame
budget), at the existing Showcase quality, resolution and complete draw distance. The integrated
checkout is `D:/Mudcrab/merge/dev-integration`, branch `opt/cluster-showcase-integration`,
with known root-owned renderer WIP. Prior research remains evidence with its original scope.
No accepted executable or ready-demo claim follows from these edits. The detailed findings,
prepared fixes and evidence limits are in
[the current source audit](cluster-demo-source-audit-2026-10-18.md).

| Order | Observable outcome | Work and exit condition |
|---|---|---|
| 1 | Optional shadows cannot prevent an otherwise drawable crossing | Unify destination readiness with the actual selected draw variant. Waiting/Unavailable optional shadows use the ordinary terrain path with a visible reason; exact pipeline/material/geometry proof remains required. Cache layout refusal at the actual device/layout identity and select the ordinary variant rather than retrying an unusable variant every frame. |
| 2 | First doors respond correctly, without animation guesses or unrelated upload waits | Correct the new static-door timeout regression and audit the older quiet-frame/animation-discovery race. Reuse the existing DoorAnimation resolution facts; distinguish resolved no-clip, unresolved and failed loading. Keep fast opening for a resolved animated door, retain capture/crossing proof, and preserve ordinary timeout semantics after activation prerequisites. |
| 3 | Repeated views keep geometry stable without reload loops | Retain the reviewed scene-ID ownership correction and real empty-loader regression. Collapse the likely lint-producing nested if; check cached NoClip before repeating source qualification, while preserving ownership and edit/removal invalidation. Validate the combined bounded corrections once when builds resume. |
| 4 | Distant objects and trees are complete with clusters on and off | Implement the missing native object/tree inventory runtime adapter through existing database/assets/streaming seams. Establish actual coverage-unit/world ownership and near/far overlap rules before suppression. Partial reference mapping cannot hide a whole baked unit. |
| 5 | Optional shadow preparation behaves well while streaming and travelling | Separate CPU packet construction from asset-loading IO; bound world GPU retention by actual live/cached/in-flight view references. Filter irrelevant shadow views from the optional readiness map. Classify terminal load failures using current demand before shortening failure latency. No eviction may invalidate a drawable/report still in use. |
| 6 | Performance work targets current measured costs | After correctness and stable geometry, reuse stock startup/settled profiling. GLM's placement cache, event-driven qualification and bind-group reuse remain candidates, not promised speedups. Implement only justified bounded improvements with complete invalidation keys. |
| 7 | One usable walking demo is delivered | On a frozen tree, batch affected checks, current-image door/route/coverage/shadow/all-off inspection, justified performance acceptance and final workspace/doc gates. Activate the accepted-image launcher only after these pass. No publication or dev merge is authorized. |

### Cycle27 measured disposition and immediate fallback integration

The six-shot TEST at19045bf4/8ae118c0 exits naturally with all captures, actual optional forward encoding and260 ready terrain batches. Loaded195-210s opaque GPU median1539.325ms over ten events (one following event1458.279ms) remains unusable. Source28.09-28.54ms/terrain5.18ms confirms the prior CPU terrain reduction approximately persists; this is diagnostic inference, not guarded timing. Cheap180s opaque samples mostly precede optional drawing and cannot establish a shader gain. Door and final far captures retain geometry. Full examined inventory delays package opening in this run; repeated authority snapshots are a source-backed candidate, not a proven explanation. Current memo caches only within one shared-lock open and preserves first full evaluation.

Next observable behavior is exact rejected-reference stock fallback in the existing placed-world distant view. The shared spawn extraction and sparse exact-ID worker request are preparation only: integrate their consumer with one near/far owner, ordinary material/snow/bounds and generation handling before claiming coverage. No billboard/BTO suppression without an exact relation and per-view/pass proof. Stable120, native completeness, portal route, all-off and final launcher acceptance remain open.

### Cycles30/31: homogeneous shadow experiment rejected

The homogeneous-product candidate43a1e0a passed the retained19 actual-GPU numerical observations, but that did not establish renderer cost. Cycle30's immutable Tracy image7173733b completed14 shots naturally without logging actual optional forward encoding; its cheap ordinary-pass samples are not shadow improvement. Cycle31 reused exactly that executable, extended the repeated far pose and logged actual optional forward encoding at21:37:40.956886Z. Loaded15-second opaque bins then had roughly1.98-2.32s median GPU duration, versus the prior cycle27 loaded1.54s window. These TEST populations/windows are diagnostic, not controlled FPS acceptance, but give no reason to retain the candidate. Both production shader and numeric source pin are restored to the checked early-exit version.

Cycle31 also failed the unchanged30-second far settle deadline despite zero reported pending counts; the next screenshot did not finish writing. Only its door PNG exists. A graceful close of the exact owned process failed to finish after more than90 seconds; only that revalidated process was stopped. This is failed usability/exit evidence, never a completed route or natural exit. No specific shader mechanism is established for the close stall. Retained trace/identity/stop note: `C:/Modding/Mudcrab/builds/vg7-cycle31-tracy-runtime`; summaries: `local/vg7/cycle31-zone-summary.json` and the raw exports. The launcher remains unset.

The exact distant-source consumer now passes current fmt, all-target/all-feature engine clippy and all-target nextest2,030/2,030 (eight existing skips). It reuses the ordinary source loader, exact placing-world request, one source entity and combined terrain/distant role publication. Fourteen producer/DB/ECS regressions cover identity, transfer, withdrawal, eviction, failure preservation, one-time collision promotion and scoped readiness; portal ownership has nine transfer regressions. Whole-view queued/failed work and exact-cell destination assets now participate in residency gates, while render proof remains unchanged. Current-image route/visual checks are next. The explicit one-exterior-owner restriction remains; no simultaneous cross-world substitution or native billboard relation is invented. These checks do not establish native completeness, optional-shadow performance or120fps.

### Consolidated review disposition (Opus focused + Opus broad + GLM Flash)

All three reviews were static/read-only. They establish source findings and draft ideas, not
compiled correctness, visual acceptance or frame-time gains. The reviewer labels are advisory;
the reported door regression is a correction requirement despite the focused review's opening
"no blockers" wording. The resumed cycle has since passed engine all-target/all-feature clippy
and 310 focused tests. Current-image visual and performance acceptance remain open.

The resumed functional TESTs each completed all three door/repeated-distance shots, but the
optional-shadow off/on images contain different distant object populations. The off main-camera
handoff stayed at zero eligible instances while shadow views progressed; the on main camera
accepted clusters. These runs do not establish a shadow performance cost or complete coverage.
The next diagnostic separates pending/failed colour pipelines, missing or wrong-phase plan
representatives and relocated prepared-material entries before changing caching or readiness.
The first-door prerequisite hold is now one-way: later package work cannot restart it after
activation becomes eligible. The native inventory reader is compiled but has no runtime consumer.

The diagnostic off run (`95d21d97`, ignored `local/vg7/review-cycle3-off-gpu.log`) confirms
all three functional shots complete while colour pipelines repeatedly cycle through
queued/creating/error states, with representatives present and no main-view material-address
mismatch. Locked Bevy `ShaderCache::get` resolves every import before applying definitions;
the ordinary entry sources mention enabled-only libraries inside disabled `#ifdef` branches.
The old composition test explicitly skipped those imports and did not model the real cache.
The correction separates enabled-only entry assets, restores the ordinary sources exactly,
and makes the composition regression resolve every import. Immutable corrected-import image
`c5fc6181` completes all three functional shots with the option off: the main view reaches
29,097 eligible instances and all 260 terrain batches prove ready. Inspected distant views restore
the missing tower and mountain objects. Log: ignored `local/vg7/review-cycle4-off-gpu.log`.
Do not reinterpret the pre-fix screenshots or their settle result as complete colour coverage.

The follow-up keeps the ordinary pipeline while the optional pipeline is queued, unresolved or
failed, and wakes the cached walk when compilation or shader withdrawal changes selection. Both
test import skips are removed. Actual buffer IDs, the current prepared publication and certificate
now govern binding validity; a content append cannot revoke an unchanged binding after Queue.
Late placement records retain the append revision in their cache key, so duplicate/invalid claims
are rechecked after PrepareResources without rebuilding that binding. Final focused review passes.
Current fmt, engine all-target/all-feature clippy and 1,947 engine tests pass (eight existing skips),
including the pending/compiled/revoked selector and late-duplicate regressions. Enabled
current-image proof remains outstanding; require actual forward-encoding and same-frame-append
continuity logs, rather than interpreting a small pixel difference as optional-shader execution.

Enabled-runtime follow-up: immutable diagnostic image
`5b3deb1b2d60666dac9c12b86cc5dd5c2b3365d3e26f36433bb23c72753ff18a`
passes the three functional shots (cycle6), then the same image passes a longer 93-shot
functional soak (cycle7). Neither run logs an optional forward draw or a ready optional
pipeline. All five libraries and both entry assets load, with no missing direct imports;
the initial entry-not-loaded error retries. The state logger originally suppressed each
previously seen state, so its last printed Queued is not a final-state assertion. It now
records transitions instead. This is function/diagnostic evidence, not timing or shadow
acceptance; the ordinary path keeps geometry visible.

A bounded experiment replaces interval widening's one/eight bit-step loop with one unsigned
operation, preserving its pre/post underflow handling exactly. The sign bit cannot change
within eight steps from a finite normal, so the endpoint bits are identical for every
supported call. Independent source review and boundary/exponent equivalence fixtures cover
the change. Current fmt, engine all-target/all-feature clippy, all1,948 engine tests and
quick build pass; the pinned actual-GPU numerical test passes all16 controls on RTX3090
Vulkan (cycle12). Cycle8 later reaches the real optional pipeline and encodes a draw;
cycle9 proves encoding survives a same-frame terrain append. No controlled shader
compilation/instruction saving is established. No budgets, settings or proof rules are reduced.

### Forward execution and the next performance question

Cycle8 eventually reaches a compiled optional pipeline and encodes a draw. The same normal
image (`9b28b36f...`) in cycle9 proves both forward encoding and continuity across a
same-frame append; all three PNGs arrive and geometry remains present. Do not interpret
this as acceptable speed or positive distant-shadow image coverage. Capture delivery is
conspicuously slow once the far pose draws; stock Tracy cycle13 below locates the
dominant pass. Isolated numeric and source tests do not answer its runtime cost.

A source-only critique identifies three conditional experiments, not measured fixes:

- `receiving.wgsl` walks the complete cell DDA, doing a binary directory lookup and starting
  a separate local hierarchy at every visited cell. If the GPU profile locates that work,
  derive a world-level hierarchy of cell-height bounds with validated escape indices.
  Retain current leaf triangle predicates. A known-page BVH alone cannot prove Clear:
  unknown cells and missing directory regions need explicit unbounded-height XY regions
  or a separate exact coverage traversal. Uncertain numeric box results mean visit.
- A global known maximum can exclude geometry only after positive travel when outward
  intervals prove the rising ray stays strictly above it. It cannot erase an unknown
  tail or uncertified exterior; a check at the canonical receiver itself is vacuous.
- A stackless local quadtree could remove private stack traffic if the profile identifies
  that cost. Preserve child order and budget outcomes; it does not solve world DDA scaling.

Any derived spatial data belongs to the existing source-owned background world publication,
keyed by package/height allocation identity. Sun/view changes reuse the index with current
query inputs. Upload changes need explicit version/footprint/address validation, complete
content and unchanged enclosure/Unknown rules. Do not obtain speed by reducing distance,
quality, resolution or traversal budgets. Preserve seams/corners, remote narrow occluders,
partial coverage and signed extreme-coordinate controls.

The old shots runner uses virtual `Time`, so clamped slow frames can make its logged
seconds and nominal capture deadline lag real elapsed time. VG14's current correction uses
`Time<Real>` for phase deadlines and monotonic request/completed-write timestamps for captures.
It excludes only the interval behind an explicitly observed startup hold. Late completion fails;
a timely completed write can survive late polling. PNG encoding finishes in memory before a
checked file write. Independent review caught and corrected buffered-finalization errors and
startup-release interval charging. Existing timeout limits and readiness are unchanged. All
1,962 engine tests pass, including these regressions; the correction is local commit `0fc372ab`.
Old TEST settle logs remain unsuitable as load-time evidence.

Stock Tracy capture cycle13, source `2f22b1b0`, immutable image `c816110b`, reproduces
actual optional forward encoding and append continuity. The opaque GPU pass accounts for
95.76% of exported GPU-zone time (4,812 pass events, maximum 2.674 seconds). This capture
includes startup and two poses: its average is not settled frame time, and the pass is not an
isolated measurement of only the receiving function. CPU extraction is separately expensive:
`cluster::render::handoff::extract_sources` averages 22.52ms over 3,287 calls. Window preparation
also waits substantially. Both images and the final shots log were written; root stopped only
this owned process after it stayed GPU-busy during shutdown. Exit is nonzero, trace preserved;
this is failed usability evidence, never route/shadow/performance acceptance. Trace and full
stock CSV exports: `D:/Mudcrab/perf/2026-10-18-vg7-far-forward-cycle13`.

Post-processing the stock per-event GPU export separates the far portion: the 30-second
program-time bins from 120 through 270 seconds each contain 11-12 opaque-pass events with
median durations of 2.36-2.57 seconds. This is sustained pass cost in this diagnostic capture,
not just the single maximum. Earlier incomplete-loading/different-view bins are not an
option-off baseline. Retained analysis: `gpu-events.csv` and `opaque-epochs.json` beside the trace.

The bounded GPU experiment now integrates the reviewed world partition into the existing
source-owned background upload, with a runtime-only v3 suffix and unchanged bindings/header
size. Exact retained canonical samples supply bounds; missing regions remain XY-only Unknown;
surface leaves retain the original triangle predicates and the exterior rule. Work budgets cover
the full content, and allocation/limit refusal never truncates coverage. Independent source
review found no geometric counterexample and corrected a resource-refusal diagnostic. Current
engine checks pass (1,962 tests, eight existing skips, all-target/all-feature clippy). Actual
RTX3090 Vulkan numerical execution passes all19 controls, including strengthened boundary
fixtures, Unknown after the known-height maximum, a remote narrow caster and malformed escape.
Image `119a61a116f8fb3b590e49371b5809e400f249bd2433606ab774baafd18e399c`, ignored
`local/vg7/f1-numeric-cycle15-gpu.log`. The first run's old flat-cell visit-count assertion failed
after the root correctly proved Clear; the revised test checks that pruning explicitly, and peaks
in each adjacent cell force the seam/corner tests to retain their multi-cell proof. Independent
review also caught an inexact test sun component before dispatch; changed it to a dyadic slope.
No production predicate or numeric failure was waived.

The same two-pose TEST at `ce3bee97`, immutable Tracy image `0a768f23`, completes both
captures and exits normally (cycle16). The loaded far-view 120-150s bin has 21 opaque-pass
events with median 1.663s, versus 12 events/2.491s in cycle13. This one diagnostic pair
indicates a partial improvement; it is not guarded timing acceptance, and seconds per pass
remain unusable. Resident counts match (door70,382/far69,786); all260 terrain batches become
ready. The inspected far image retains the terrain, tower and mountain objects. Shutdown
logs one failed asynchronous render-statistics download; retain that limitation alongside
the successful process exit. Trace: `D:/Mudcrab/perf/2026-10-18-vg7-world-tree-cycle16`.

The CPU follow-up is deliberately separate: repeated ancestry walks allocate a HashSet per mesh
in source discovery and terrain qualification, before output equality gates. Frame-local memoized
ancestry is a candidate with a simple invocation/generational-entity key; per-view readiness,
retirement, exact matrix multiplication order and source invalidation must stay current. Which
walk dominates extraction remains an inference until stock spans split those substeps. No CPU
cache performance is credited yet. The current bounded implementation shares a fresh nearest-root
memo between source presence and primitive qualification in one immutable extraction. Every
reference-query root remains a barrier, including unrelated/invalid roots; full generational
entity keys and invocation lifetime avoid cross-frame invalidation. Scratch cycle markers drain
alongside memo insertion, without retained-capacity clearing per mesh. Independent review and
six regressions cover baseline equivalence, shared positive/negative tails, deep-then-short walks,
reparenting, recycled entities and an actual extraction fixture. Two coarse stock tracing spans
separate presence and qualification. Exact transforms, retirement and current per-view proof
remain outside the memo. A one-per-world node-kind diagnostic will distinguish structural
Unknown branches from fully bounded branches; it does not measure fragment visits.
Corrected engine checks pass: fmt, all-target/all-feature clippy and all1,968 tests (eight existing
skips). The first run's generation fixture assumed immediate allocator reuse; explicit Bevy
same-index/different-generation construction fixes that setup without weakening its assertions.

The CPU-only cycle18 image (`a430fc87`, SHA058dddbf) proves actual optional forward drawing
and same-frame append continuity, and its runtime counter confirms all Tamriel world branches
are bounded. Door capture passes and was inspected. The far shot fails the unchanged30s readiness
window with16 pending cluster batches; process exits1 and the trace is retained. Shutdown again
logs the asynchronous statistics-buffer download warning. This is failed functional acceptance,
not a completed far-view comparison or FPS claim. The global extraction mean20.462ms includes
startup and is not its loaded cost. Stock per-event export bins120-135s have median62.320ms
extraction versus72.971ms in cycle16, but pending populations differ and these are diagnostic
observations only. Current substeps in that bin are presence20.841ms and qualification8.678ms;
the residual is not automatically attributable to either loop. Artifacts, filtered raw CPU events
and `source-epochs.json`: `C:/Modding/Mudcrab/builds/vg7-cycle18-tracy-runtime/tracy`.

Source inspection identifies an additional bounded CPU candidate: terrain identity separately
walks every Mesh3d's full ancestry, allocating a cycle set per walk even when no terrain marker
exists. Batch only within the immutable extraction, composing coverage/world/chunk summaries;
do not cache final identity alone, since descendants can supply missing markers or introduce
duplicates. Preserve same-world repeated markers, duplicate coverage/chunk refusal, all roots,
cycles, quadrant/tier checks and current entity generations. Add a coarse stock span before
crediting its cost. Independent CPU work and the reviewed stackless local shader traversal can
share one compile/validation cycle; their numerical/ECS proofs and stock CPU/GPU evidence remain
separate. No additional quality, distance or timeout change is proposed.

The cycle19 candidate: invocation-local terrain ancestry summaries replace repeated per-mesh
walks; reference memo maps use Bevy's generational entity hasher. Nine real-ECS/oracle regressions
cover ambiguity, incomplete shared ancestors, cycles, removals/reparenting and generation separation.
The local shader uses an implicit depth-four DFS in exactly the previous child/budget order,
removing its dynamically indexed private stack without changing canonical predicates or limits.
Independent source/control-flow review passes. The first combined run caught five Naga composition
failures caused by a missing explicitly typed tail return; corrected source passes a direct locked
composer negative/positive control, fmt, all-target/all-feature engine clippy, and **1,977/1,977
engine tests (eight existing skips)**. Actual RTX3090 Vulkan numerical TEST cycle19 matches **all19
Observation words exactly** against cycle15, including states, reasons and every work counter.
Image1a71e3ee, source a430fc87 plus patch89f2e46f, normalized receiver24e967dd; retained in
`C:/Modding/Mudcrab/builds/vg7-f1-numeric-proof-cycle19`, log `local/vg7/f1-numeric-cycle19-gpu.log`.
No performance gain is established by those proofs.

Current disposition after actual cycles20/21: the stackless local traversal is **rejected**.
Cycle20 captures both poses and exits0 but its optional pipeline is still Creating, so ordinary
terrain draws; its5.433ms active opaque median is NOT optional-shadow performance. Both images
were inspected: doorway/interior, terrain/tower/mountains remain visible; complete native coverage
is still unproved. The unchanged image57118cca (ed255af6) extended in cycle21 proves the optional
forward draw and same-frame append continuity (epoch185). It then fails the far30s readiness
window with16 pending batches, fails another quiet window with zero pending, captures one later
far image, then fails the60s screenshot deadline and exits1 normally. Timers are not extended.
Stock opaque GPU medians remain1.53-1.69s in later active bins, comparable to cycle16's1.58-1.66s;
there is no useful traversal speed gain. The old local stack/predicates are restored for the next
candidate. Artifacts/identities/raw CPU/GPU events remain under C:/Modding/Mudcrab/builds/
vg7-cycle20-tracy-runtime and vg7-cycle21-tracy-runtime. Statistics-buffer shutdown warning recurs.
No profiler/HUD value is FPS acceptance.

Cycle21 separates residual CPU work: later source extraction47-49ms, presence9.0-9.3ms,
qualification7.4-7.6ms, terrain identity25-26ms; selector falls to4.7-4.8ms after loading. Next
CPU candidate reuses only terrain identity results between consecutive extraction observations,
with exact generational membership from the literal ancestry query domain, tracked component
add/change/removal and missed-removal detection, and a fetch-continuity guard. Visibility, views,
materials and draw readiness remain live. A Children-only enumeration was rejected before edits:
Bevy's safe Skip relationship hooks can omit a reverse edge. Initial cache regression exposes a
wrong Allow-versus-optional-query assumption (1610 pass, one fail,376 not run); literal matching
query data corrects the domain and tests the original refusal, with a valid far sibling. Independent
source review approves the correction; compile/lint/test results belong to their later logs.

Next GPU candidate retains the original local stack and all existing structural/numeric/budget
checks. Runtime-only GPUv4 uses the surface node's unused ninth word to carry its exact directory
ROW, found/validated once in the background. The shader checks index bounds and exact signed cell
key, then uses the original page/root/coverage/reason validator. This removes a repeated binary
search for every visited cell; receiver lookup, traversal order and canonical predicates stay.
Unknown leaves still carry reasons. CPU packet, MCL/MCI, dataset schema/extension hashes and source
data are unchanged. Independent review approves writer/consumer and the new non-page-row positive,
wrong-row and legacy-version GPU fixtures. Fast locked composer passes; full current checks and
all19 earlier GPU result words plus the new controls are required before keeping it. Any speed
benefit is unmeasured. This does not by itself establish a path to120fps.

The selector audit identifies live readiness construction, cache-miss placement scans and nested
terrain-cell membership as candidates. Existing completed cycle16 raw events separate phases:
120-135s selector median61.533ms (17 calls), then135-150s4.769ms (14),150-165s4.742ms (14).
Thus failed cycle18's52-55ms active window does not establish a steady selector cost. Readiness
must remain live per view/pass; neither sources.version nor gpu.generation proves it unchanged.
Before another optimization, profile the retained terrain batch plus stackless traversal on the
same route and inspect actual settle/geometry/CPU/GPU results. No timeout or quality changes.

Earlier diagnostic question: does current source extraction improve? Reuse the existing stock
capture and exact two poses. A small read-only helper reuses the production world-tree builder
under the shared AssetLock: Tamriel's11,186 cells all have positive projections/canonical rows,
matching the retained certified-reader count. Domain[-57,-43]..[61,50] is exactly119×94 cells;
its22,371 nodes comprise11,185 bounded branches and11,186 surface leaves, with zero Unknown
or unbounded branches. This is structure-only evidence, not fresh content certification or
fragment visits. It rules out the proposed second coverage walk as a useful optimization for
this hot world; keep that experiment unimplemented. Source/output: ignored
`local/vg7/world-tree-structure`.

Do not infer
the residual GPU bottleneck from the opaque pass alone: world traversal, local traversal,
interval arithmetic and register pressure remain competing explanations. A reviewed possible
experiment separates known-geometry bounds from metadata-only XY coverage, but no such change
is implemented or accepted. It would have to preserve remote occluders beyond Unknown, numeric uncertainty,
outside-domain/seam rules and full-content cumulative budgets across both walks.

### Resumed-cycle boundaries: distant replacement

The current dataset includes clustered trees; the old N2 assumption that trees are excluded does
not apply. Reuse N2's native renderer, adapting its demand and layer ownership to the current main
and portal views. Do not enable native billboards over clustered trees without an exact relation.
Object BTO units have no certified per-reference relation, so a partial cluster map cannot suppress
a whole baked unit. Rejected object primitives need the existing complementary stock-source path,
with sparse placed-world/reference demand and shared spawning, rather than a second guessed model
or position join. Near/far ownership and promotion must retain one source and its actual draw proof.

The current source audit locates the bounded seams: N2's native modules exist at3a46745c but are
absent here; adapt their planners to current `LodDemandViews`, and aggregate native plus terrain
roots in the single `sync_lod_roles` owner. Legacy cell-ready/global tree hiding is insufficient.
`CertifiedPackage::records_in_placed_cell` and `unclustered_primitives` already index partial
references, but wholly rejected references are omitted because the reference map is seeded only
from emitted instances. Retain authoritative rejected-reference demand at package open, then
visit affected cells rather than scanning the package per frame. Reference creation is currently
inline in `spawn_cell`; extract that path and add sparse requests through the existing database
worker. Preserve exact placed identity, pose, authored model/snow/bounds/readiness and one-source
promotion. No adapter or sparse fallback is implemented by this source audit.

The current `portal_lod.rs` writer sorts BTT paths and preserves parser group/instance order. It
inserts native rows before reference resolution. The parsed record has a reference FormID, not a
base FormID; neither group nor cross-file reference uniqueness is enforced today. The additive
tree-relation contract is acknowledged at OP6's `7ae481df`; shared code `23d2aa32` plus the
comment-only `b1a1c987` is not yet adopted. Its independent review confirms the binding, status,
multiset, ordinal and domain checks. Root source review of `beaa0b2d` accepts the added validated
row interface, nonempty unknown reasons, core-first type/world classification and conservative
known non-TREE collision policy. Component adoption remains separate from runtime proof:

- Key companion rows by logical block and continuous emitted ordinal across repeated type groups.
  Tree type is copied data, not part of that key. Independently validate
  native totality, multiplicities and copied-value tuples; never join by rowid, SELECT order or
  approximate position. Runtime draws the validated keyed companion rows.
- Bind semantic core build identity and the final `mudcrab.lod_inventory` hash. Original BTT digest
  inventory and parser completeness are producer attestations, unless those inputs are explicitly
  published and owned. Runtime checks the core/native facts; it does not invent external-file
  verification. Unsupported parser versions refuse qualification.
- Recheck resolved and disjoint identities against core and the validated cluster map. Unknown
  native identity or opaque parser scope disqualifies tree replacement for the whole placed-world,
  level-4 domain, with a visible reason. An unresolved billboard staying drawn alone does not prove
  it cannot overlap an eligible clustered tree.
- A qualified complete tree may retire its whole-tree billboard only with current per-view/pass
  proof that every required cluster primitive is drawable; loading its original stock scene is
  unnecessary. Partial coverage needs current complementary stock-plus-cluster proof before the
  billboard retires. Until either proof, the billboard draws alone. No-native references still
  obey domain qualification and the existing per-primitive policy.

No tree schema, real-data regeneration or runtime suppression is accepted merely by this design.
Existing core and native LOD payloads remain intact.

The current terrain LOD planner explicitly has one exterior owner: an active exterior uses the
main camera; an active interior may use the rendered/prewarmed portal's exterior destination.
An exact vanished crossing demand can retain hidden residency but supplies no draw/readiness
proof. Native reuse must preserve this contract and aggregate root roles with terrain under one
owner; a second independent `replace_lod_roles` call would erase terrain's classifications.
Simultaneous exterior-world demands need a separate explicit planner decision. Generic destination
readiness does not prove full-detail cell content, source primitive completeness or a native relation.

Use each validated keyed row and its exact payload so native rendering never reconstructs
identity from unordered stock rows or ambiguous equal tuples. Known same-world non-TREE native
collisions retain the billboard alone. An unqualified tree domain must not disable unrelated
object clusters. Unknown non-TREE collisions remain an explicit producer-attestation limit;
their absence is not inferred. A missing relation remains explicit unqualified tree coverage,
never evidence of no billboards.

**Correctness first.** The broad review's far-shadow gate finding is accepted from source:
`cluster/destination.rs` rejects optional Waiting/Unavailable although the encoder selects an
ordinary terrain pipeline. Fix the disagreement rather than broadly marking views ready.
The layout-refusal finding is also accepted: persist the refusal, preserve the diagnostic,
and route both draw and readiness through the supported ordinary variant. Regressions must
cover Ready, Waiting, Unavailable, layout refusal and transitions back to Ready, including
an exterior portal and selected sun absent from that view's prepared lights. Expected result:
ordinary terrain can prove ready; a stale specialized draw still fails closed.

**Door/reload corrections.** The first-door patch currently charges its 30-second deadline
while an unanimated door's quiet-state predicate is held by cluster work. Fix this without
putting animated doors back behind all-world upload. Inspect the existing `DoorAnimation`
component and `swings()` API before adding any state. Do not infer "no animation" from an absent
player or 120 quiet frames; verify how loader failures/timeouts populate resolution facts.
Use prerequisite waiting only where genuinely unresolved, with explicit reason and existing
failure handling, rather than changing deadlines. Regressions: resolved animated first door,
resolved no-clip first door, unresolved animation and graph withdrawal, over-30-second package
work, package failure, unchanged ordinary timeout, and capture still withheld. The existing
actual-query test is useful but does not replace a CPU `run_shots` phase test.

The scoped invalidation/empty-model correction is supported by both earlier reviews and Opus.
Keep its real-loader regression. Resolve the probable collapsible-if lint before the next
compile. Avoid qualifying an already cached NoClip answer again; test that proof-owned source
edits/removal still revoke, and legacy root-derived empty answers do not acquire that ownership.
Correct the short converter paths in the audit and name helper tests for their actual scope.

**Streaming/resource work.** CPU-heavy packet/upload-word construction currently runs in
`IoTaskPool`, which also serves assets. The shared pool is a source fact; contention and savings
are unmeasured. Keep database reads/locks and CPU work correctly separated if moving packet
construction to `AsyncComputeTaskPool`. Bound GPU world tables using active, cached and in-flight
owners; do not simply drop everything absent from this frame's main camera. Keep view/bind-group
leases valid through rendering. Add a camera/view-domain filter only after confirming all actual
portal/proof camera entities are retained. Terminal cluster failures must still fail visibly;
fail earlier only after fresh view/demand identity rules out a stale previous-pose failure.
A moving sun's per-frame nonce churn is a future conditional risk, not an observed regression.

**GLM drafts, not yet adopted.**

- Placement qualification cache: promising source-backed repeated work. Key by exact GPU/placement
  identity, epoch, origin and target-offset bits; include all claims in duplicate rejection.
  Cache qualification only, retaining per-view fresh/active-record stamping checks.
- Template/normal-map vectors: event-driven rebuilding is a candidate. Prove handling of changed
  handles, certificate, scope, Added/Modified/Removed events and pending dependency retries. Do not
  cache pipeline readiness behind an asset-only version; the worker explicitly declined that.
- Bind-group reuse: resource-identity reuse may avoid work on append. Retain clip invalidation
  until proven that appended claims cannot alter existing placement/duplicate certification.
  An unchanged Prepared nonce alone is not that proof. Combine with the placement cache only if
  its invalidation and clip-encoding dependencies remain exact.

No optimization earns priority solely from a worker's "high value" label. Establish current
stock-phase cost before scheduling these ahead of missing presentation or correctness work.
GLM's full unapplied drafts are retained in the ignored local worker report at
`C:/Modding/Mudcrab/mudcrab/local/agents/runs/research-vg7-glm-flash-code-critique-r1-20261018.1/report.md`.

### Execution and validation grouping

The current cycle implements and reviews the bounded source corrections while a reuse audit
locates the existing N2 native-object/tree implementation. Do not recreate an existing renderer.
Root owns
shared readiness, queue/pipeline contracts and integration. A bounded implementation may be
delegated against explicit owned files after the seam is fixed; independent review follows.
Do not launch additional overlapping broad reviews by default.

In the resumed validation cycle, compile/check the combined engine correction slice once and
run affected regressions, correcting all failures before another cycle. Inspect one retained
current executable for first-door response, repeated-pose stability and optional-shadow fallback.
Then finish/inspect distant object/tree integration and the complete portal/walking route.
Only after drawable populations are stable compare startup and settled performance with the
existing stock tools under the required quiet timing hold. Keep the user's stable 120 fps target and Showcase
quality, draw distance and failure criteria unchanged. Final workspace/doc checks and all-off
acceptance apply to the actual frozen handoff tree; old R15/component checks are retained as
historical evidence, never relabelled as proof for new edits.

Reuse the affected-crate target for the first fmt/clippy/focused-regression and image cycle.
Full workspace checks remain final integration handoff gates; repeat them only for relevant
changed inputs or failures. Every actual failure receives a disposition from its retained log.
Report measured startup, settled and traversal results separately; averages alone do not prove
stable 120 fps. No quality reduction, object omission, reduced portal resolution or hidden
loading is authorized as a performance fix.

## Earlier route checkpoint (2026-10-17, R15)

R15 passes all four Alftand seamless walking crossings into Blackreach and all eight Riverwood
crossings, each on the first approach, with no step-back retry or source-intent cancellation.
Both final walking checks stay grounded (600 horizontal units in Blackreach, 465 in Riverwood).
All 77 Alftand and 135 Riverwood retained crossing images pass integrity checks. Inspection of
the immediate before/swap frames at Blackreach and the final Riverwood return shows continuous
geometry and lighting, with no blackout. This is functional/visual evidence, not timing acceptance
or a claim about every material, worldspace or travel route.

The fixed mechanism has three parts. Candidate invalidation compares old/new inputs and follows
placed-reference worlds, including children stored in parent blocks. The source history contains
only certified replacement candidates; unrelated stock references cannot trigger unknown-owner
global revocation. The crossing consumes the current same-door gate's exact fresh report after
an explicitly ordered report-combination step, without waiting an extra frame for the later
portal proof cache. Stale generations, render revocations, foreign/replaced views and orphaned
proofs still fail. Certificate/global material/scene changes and unresolved ownership remain
conservative. Ordinary same-door views retain their existing ownership rule.

The combiner reads residency/assets from the previous streaming frame, matching its labelled
previous-render-frame proof. The seamless consumer follows it in Update; the load-screen consumer
already follows Update in PostUpdate. Request rebasing remains with the request owner. Block
arrival retains preallocated resources; future eviction/reallocation must explicitly revoke
reports or retain the old drawable resources through consumption. No range, timeout, streaming
radius, collider or graphics quality was relaxed.

R11/R12/R14 failures remain recorded. R13 was interrupted before its tour when the user paused
for gaming and supplies no route verdict. R14 passed three crossings but exposed the uncertified
stock-history leak and missed Ready-consumption window. Per-attempt capture retention preserves
failed approaches. Exact terrain-identity/visibility and request-coverage diagnostics distinguish
real destination changes; source-world respawns do not prove destination respawns.

Frozen-source checks pass: fmt, workspace all-target/all-feature clippy, workspace all-target
nextest 3553/3553 (48 existing skips), workspace docs (four existing ignored examples), and the
engine build. R15 image SHA256 is
`A0FFA981B2E4D7F7E0E6C4EE4DC8ECEFE8B659DD63FCF4B22E46329FD3C3780D`.
Ignored logs are `local/vg7/r15-*` and `crossing-proof-r15-*-gpu.log`; images are under
`C:/Modding/Mudcrab/builds/vg7-showcase-evidence-r15` to preserve D: headroom.

The demo is still not complete. Distant-shadow mathematical modules and the certified reader
are now adopted as isolated components. The real joint-publication fixtures pass, and the
unchanged receiver WGSL passes actual RTX 3090/Vulkan known-shadow, clear and unknown controls.
The local selected-sun lighting hook passes CPU composition and all-off resource checks, but
has no runtime registration or actual PBR lighting acceptance yet. Background ownership tests
pass for exact source identity, cached errors and retained package locks; real-package reader
acceptance is running read-only. Compact shadow command tables are CPU serialization only.
Renderer shadow integration, all-off checks, performance acceptance and final launcher activation
remain. Sparse absent-cell request/retry diagnostics remain VG12; Alftand reports 28 exhausted
missing-row requests, without shader/GPU validation errors. Dataset incompleteness and its explicit
development option remain unchanged. No dev merge or publication has occurred.

## Earlier route checkpoint (2026-10-17, R10)

The integrated cluster-on Showcase now passes Riverwood's two seamless walking crossings and
final quiet settle, and the full Alftand route passes four crossings into Blackreach plus its
final grounded walk. Neither run uses the former step-back retry. The pending crossing keeps an
actual destination proof view alive at the current prospective landing pose; destination sunlight
and its shadow domain are established before the Required request. Exact destination-world and
source-intent checks prevent stale or abandoned crossings. The authored ACTI support subset keeps
the Alftand platform grounded through its final door opening. Stock behavior remains separately
covered by the existing controls and policy-off regressions.

These are functional route results, not complete visual continuity or timing acceptance. The
saved images pass file/dimension/non-black checks and inspected arrivals show the expected worlds.
The old capture predicate omitted pre-swap frames once the player moved behind the door plane.
A reviewed capture-only correction includes both nearby sides of the plane within the same bounded
ring; continuous before/after images are the next visual gate. No crossing input, timeout or
readiness condition is changed by that correction. Full workspace validation passes on this
source: 3538 tests, workspace clippy and doc checks. Continuous-image acceptance is running.

Sparse exterior-neighbor requests also emit misleading missing-row errors. A read-only audit
found 31 absent coordinates and verified both required landing neighborhoods; it found no
required-cell readiness bypass. This separate request/inventory issue is queued as VG12.

Distant shadows and render-thread optimization remain incomplete. S5's isolated compact inventory
and shader-binding budget are reviewed; the canonical-surface shadow model and its receiving
implementation are underway. No far-shadow renderer or performance acceptance is claimed. The
current certified dataset retains its explicit incompleteness notices. No dev merge or publication
has occurred, and the final local launcher is not activated yet.

## Earlier data and per-view integration work (2026-10-17)

The integration working tree now contains complementary per-view object selection, explicit
terrain-instance identity, independent forward/depth terrain coverage, exact live source and
material eligibility, and same-frame destination handoff publication. Focused source/handoff
checks pass; these changes still need renderer acceptance. PL1's reviewed seamless consumer
through `f7b6d672` is adopted locally, including fresh-report crossing authorization and the
completed-prewarm lifecycle correction. The normal stock-only path keeps its existing policy.

A separate `derive-clusters` command stages extensions over an unchanged current core into a
new output directory. It preserves existing record warnings and incompleteness, foreign extension
metadata, non-owned database rows/schema and core files. It does not repair or certify global
core completeness. Terrain DDS inputs are checked against the current core's recorded source
recipe; temporary inputs are not published. Independent preservation/publication reviews pass;
the normal-pipeline manifest ordering correction and targeted checks pass. Local converter
head `e2e82e15` completed the first derivation, which was rejected because its authority
missed reserved runtime texture aliases and produced no object instances. The rejected copy was
removed after retaining evidence. The corrected shared recipe at `d24e8de6` verifies both canonical
and alias bytes against the canonical manifest hash. Independent review, workspace clippy and
1526 shared/converter tests pass; a read-only sample verifies 152 real alias/source pairs.

The corrected derivation completed at the same new output path, leaving the source untouched:
37866 mapped object records in 80 chunks, 260 terrain blocks and 1230 mesh payloads. Its base and
terrain extensions pass full seven-entry Rust/Python integrity checks. The runtime certificate
validates 35732 mapped references; coverage remains per primitive and rejected/unknown references
stay stock. This is development data over an explicitly incomplete core. No completeness flag is
changed; `-AllowIncompleteAssets` prints the existing limitation and passes the engine's explicit
option.

Synthetic object/stock GPU images agree in visible geometry/material placement, including normal
maps and origin shifting. The corrected real package also reaches the GPU: terrain forward,
prepass and shadow pipelines compile, and per-view object/terrain replacement becomes eligible.
This establishes pipeline operation, not final visual or portal acceptance.

Stock-only Riverwood walking controls pass two crossings in each door mode. Initial automation
now waits for package certification and uploads before starting its existing settle window; it
never captures an unsettled view. The integrated renderer checkpoint passes 3477 workspace tests
and workspace doc checks; the subsequent startup fix passes 1708 engine tests. These are CPU
checks, not final demo acceptance.

The corrected cluster-on seamless run completes both walking crossings with 42 valid non-black
crossing frames and visible room continuity. Its final exterior settle still fails with two stock
LOD chunks pending. The load-screen run enters the house, but its return fails closed and restores
the player: changing source facts continually revoke the current destination key. Per-field
logging now separates eligibility/composition changes from stock terrain arrivals.

A confirmed loading bug is fixed locally: material demand now retains each instance's certified
placed world and uses active main/reflection or the actual portal destination, rather than testing
all Camera3d coordinates against all worlds. The original Riverwood run unnecessarily requested
Sovngarde-only models; actual mapping rows establish that ownership. Request reach, priority and
concurrency are unchanged. World-scoping and actual empty-store loading regressions pass.

The invalidation correction at `d349ba57` separates selection changes from candidate changes.
Only far additions preserving every prior identity and previously visible stock terrain source
avoid revocation. Withdrawals, retiering that hides old stock, identity changes and existing
object/material/certificate invalidations still revoke immediately. Independent review,
workspace clippy and 1723 engine tests pass. The real cluster-on load-screen route now passes
both crossings and final settling. Portal-heavy and far-terrain shots also pass readiness;
the actual saved main-window views were inspected. These are functional checks, not timing results.

The actual interior portal view now requests stock distant terrain with exact world, layer and
collision ownership; main-view promotion reuses the cache and query generation. Same-destination
fog reach also refreshes when the first distant horizon arrives. Workspace clippy and 43 focused
checks pass. The next real Riverwood run still fails final settling: a brief portal-view withdrawal
while crossing proof is pending retires the ring, then requests it again. An independently reviewed
in-flight crossing pin is now applied locally: it keeps residency for the exact request, door,
destination, source space and camera incarnations, while contributing no rendered membership or
readiness. Main-view priority, reach and budgets are unchanged. Terminal failure cancellation and
fresh failure-latch validation are applied with it; runtime acceptance remains outstanding.
Simultaneous main/portal views of different exteriors remain a separate design limitation.

The Alftand current-hierarchy auto-close fix passes actual arrival-door checks, and the tour
uses the player's production doorway-centre targeting. The bounded ACTI authored-body extension
is now applied and checked: supported static bodies require explicit v2 ownership and raw
motion/purpose guards; unsupported children stay excluded with visible reasons. It adds no ACTI
render proxy, legacy inference or model/record-ID exception. Four actual-spawn/collision fixtures
pass. In the next real route the player remains grounded at the authored platform height while
the final door opens, then reaches CrossDoor; the earlier image never reached that request.
The transfer still fails after the proof camera disappears. Runtime collider-hit identity remains
unobserved, so no broader physics correctness claim follows.

The reviewed pending-view correction is now applied. A still-valid ordinary view of the same door
keeps its quad and clipped projection. Once that view is lost, the exact pending crossing owns a
live view at its current prospective landing, using the same landing recipe as the actual swap.
It isolates the destination geometry and lighting, discards stale doorway bounds, and repairs
both Bevy's cached render projection and culling frustum. Full-landing and clipped views have
distinct readiness keys; old Ready reports cannot authorize the new mode. Nine focused lifecycle,
projection and pose regressions pass, with engine clippy clean before the last two refinements.

Interactive policy-gated walking requests now retain source-space intent separately from the
landing pose. Leaving the original source space or moving beyond the existing interaction reach
from the acceptance eye cancels the request before residency/readiness checks. Repeated trigger
events cannot move that anchor. Coordinate comparison promotes to double precision before origin
offsets, preserving small movements in large worlds; scripted and policy-off behavior remain
unchanged. This follow-up and same-door continuity passed independent review. Combined workspace
clippy and all 1762 engine tests pass (six existing skips), followed by a successful binary build.
One older load-hold fixture was corrected to approach within retained intent instead of jumping
800 units; its original residency and fresh-proof assertions remain, with stronger pending checks.
There is no player freeze or timeout authorization.

Further readiness review found that an ordinary speculative warm view could prove an empty
shadow domain before its eventual daylight sun was enabled. The applied correction lights every
RequiredHandoff warm view with its actual destination before requesting proof, and tracks exact
layer-matching directional-light identities in request composition. Stock-only warming remains
unchanged. Independent review passes; CPU checks and actual routes on this final light slice are
pending. A reviewed companion success guard also requires the current resolved destination world,
so a same-door retarget cannot consume a still-valid Ready report for its previous world.

The pending-proof gap was source/log confirmed in both failed routes: prewarm refilled the
pending door's requested cells, then rejected it at/behind the doorway plane. Riverwood recovered
only through the tour's old step-back retry; Alftand remained in its source world and fell before
transfer. PL1 reviewed the corrected view and cancellation contracts. Their acceptance requires
first-attempt walking continuity, not eventual recovery through the tour's retry.

The r9 Riverwood residency pin is exercised in the renderer: Portal -> Pinned -> Portal -> Main
preserves generation 4 through the gap. Both crossings occur, but the run fails final settling
with 34 pending LOD chunks and all other pending/failure counts zero. Those chunks continue
completing. The concurrent gated build and paging are recorded; the wall time is not attributed
solely to the code. All 26 retained frames have valid dimensions and are non-black, but the return
ring contains only post-swap frames after the retry, so this is not complete continuity acceptance.

Remaining acceptance: corrected seamless Riverwood and full Alftand walking, moving terrain and
shadow inspection, final validation and the local launcher. No new performance result, dev merge
or publication is claimed. Historical checkpoints below record their earlier state and are
superseded by this active section.

## Earlier execution checkpoints (historical, 2026-10-17)

These checkpoints record their state when written. The active integration section above takes precedence; the outcome and acceptance bar remain.

- Serialized integration is `opt/cluster-showcase-integration` in `D:/Mudcrab/merge/dev-integration`.
  `dev` remains `ca281332`. The prior shared history is archived; the four B5 room fixes were retained.
- Foundation `c43e6008` combines checked OP6 shared-data work and reviewed producer `a96c515e`.
  Workspace fmt/clippy and 3141 tests passed. Terrain checkpoint `d5c8858d` adds snow-source adoption,
  geometry and locked boundaries, joint object/terrain finalization and OP6 envelope composition.
  Converter/shared/meshopt-cluster fmt/clippy and 1484 tests passed. These are CPU/publication proofs.
- The engine assembly checkpoint includes the prototype render modules, checked PL1 bridge
  `6c67091b`, OP4 presets/panel `1a24873e`, B5 discriminating tests, source primitive tags and package
  envelope precheck. Workspace all-target/all-feature clippy passes. Engine/shared nextest passed 1652 tests;
  the later per-view visibility helper passed seven focused tests;
  final workspace/doc and GPU checks have not run on this assembly.
- Object view ownership is the certified `placed_worldspace_id`. The logical block world/tier/anchor
  identifies storage only: pulled-in child references remain child-world placements. The producer
  confirms that `child_worldspaces` declares examination/storage scope, with no cross-world
  substitution relation. Parent LOD/VWD references remain distinct identities. Cross-world
  substitution requires an authored relation and a schema contract; no pose/model heuristic may infer it.
- Approximate model/position stock hiding and rounded-position proxy exclusion were removed.
  Exact source tags survive real reflected scene cloning in CPU tests. They do not themselves
  authorize replacement: the source authority, payload state and per-view pipeline/queue proof must.
- Shared authority `2498f886` is committed in reader checkpoint `48ccbc7b` after final
  source review: feature-independent raw material keys, strict malformed dependency projection,
  deterministic case-alias refusal, preserved canonical extras and declared-scope authority.
  Terrain identity coverage and existing schema fixtures were retained. Touched-crate clippy and
  all-target nextest pass (3113 tests, 43 skipped), including real producer-to-reader fixtures.
  No regeneration is authorized.
- The certified reader/cache, producer-to-reader fixtures and background certificate loader are
  committed at `48ccbc7b`. The retained guard authorizes data use, never a view drawing it. The
  visibility helper is in the assembly but not scheduled yet. These are not renderer acceptance evidence. Remaining implementation
  includes complementary per-view instance selection, terrain clip
  state per view, invalidation/revocation and destination fact 5. Stock stays until that proof.
- Actual AssetServer GLTF loading and WorldAsset instancing now prove source ordinal-to-geometry
  association for repeated/nested nodes. Tangent preparation exposes diagnostic failure status,
  and requested certificate failure exits nonzero. Engine fmt/clippy and all-target nextest pass
  (1630 tests, 6 skipped). Tangent eligibility and per-view replacement consumers remain unwired.
- The launcher reuses the walking controller, explicit Showcase preset and a single assets root.
  It exposes seamless/load-screen doors and Fly; only parser/dry-run checks pass so far. The global
  defaults remain Vanilla High and load-screen doors. UI remains functional development UI.
- No real data regeneration, timing campaign, dev adoption, push, PR or publication has occurred.
  Existing performance observations are retained; do not claim a new steady-state gain from these
  correctness changes. The terrain/render-thread/distant-shadow completion bar still gates PR split.

## Outcome

One interactive Mudcrab demo on the shared `dev` engine: the old portal demo's controls, doors and
visual presentation, with clustered objects and terrain selected through the same options system.
The user can walk Riverwood, enter and leave its houses, fly across Tamriel, and change supported
settings without changing executables or datasets. Cluster on/off comparisons use that same build.
Vanilla High remains the normal default; the demo explicitly selects Showcase and clusters.

UI scope clarified by the user, 2026-10-17: settings and controls need only be functional for
development use now. Reuse the existing graphics panel, configuration files and CLI where they
fit; do not build or style a Skyrim settings screen. The eventual integration into Mudcrab's
ports of Skyrim's UI belongs to other contributors. Keep option IDs, values, capability reasons
and persistence separate from presentation so that integration can reuse them later. A finished
Skyrim UI is not a dependency or acceptance gate for this demo.

The current branch is a prototype input, not the future demo's permanent product branch. Integrate
its completed changes into `dev` using the project's integration checkout after review and checks.
Subsequent demo adoption belongs on a bounded `opt/` topic based on `dev`. Do not merge `dev` back
into this phase branch or create another long-lived combination build. No PR split or publication
before the existing VG7 terrain, render-thread and distant-shadow completion bar.

## What the inspection established

- The current local launcher selects a high start above Riverwood, `--terrain-radius 2`, clusters
  and cluster terrain. It does not pass `--walk`, `--demo riverwood`, or a graphics preset.
- The existing `PlayerPlugin`, selected by `--walk`, already implements mouse look, click to grab,
  Escape to release, WASD, Shift run, Space jump, E door interaction, and F flight with mouse look.
  The old `tools/demo/play-demo.ps1` selects that path by default. Reuse it; do not write a controller.
  This identifies a launcher mismatch; an interactive check must establish whether input has any
  additional fault.
- The engine already contains graphics settings/panel, lighting, sky, fog, bloom, exposure, snow,
  water, animations and portal support. Their presence in source does not establish that the new
  demo activates them correctly or that their cluster variants match.
- Stage-1 cluster streaming hides stock object copies before replacements are drawable. Startup
  holes remain. The streaming design also records a portal destination render-layer gap: a portal
  view can miss clusters while ownership has hidden its stock copies.
- The approved one-tree policy is newer than the old two-tracks documentation. It calls for one
  settings inventory, zero registration/load cost when options are off, explicit GPU/data capability
  reasons, and one dataset whose extra payloads are named, versioned manifest extensions.
- Migration is incomplete. Queue owners: OP3 door modes (done), OP4 vanilla presets (queued), OP6
  shared dataset/extensions (active), S5 shadows (active), A16 door-loading hitches (active), H18
  parity tool (done). Reuse their results; do not build competing schemas or settings menus.

## Implementation order and gates

### 1. Restore an honest interactive entry point

Extend the existing demo launcher with an explicit cluster selection and configurable binary/data
locations. Start on the existing Riverwood walking route, on supported ground, rather than at an
aerial profiling pose. Keep an explicit fly-start choice and reuse existing start definitions.
Print the actual controls and selected preset/options. Do not advertise F or mouse look unless the
selected controller supports them. Preserve a separate scripted capture/benchmark mode.

Check on the present prototype first: click/grab, mouse yaw/pitch, focus loss/reacquisition, Escape,
walking, run, jump, walls, terrain contact, flight and returning to walking. A passing controller
check is the first usable milestone, not proof that portal or graphics integration is complete.

### 2. Compose the shared engine and extension-backed data

Inventory the exact old demo settings against the resolved settings in the shared engine. Classify
each missing item as a launcher omission, incomplete option migration, unavailable data extension,
or a rendering defect. Record the source of the old look before changing it.

Integrate cluster renderer/terrain changes into `dev` without replacing current door, water,
lighting or settings work. Coordinate the integration checkout with its owner. Take OP6's manifest
and loader contract for object clusters, terrain clusters, LOD inventory and door/lighting data;
validate builder identity, hashes and dependencies. Stage missing extensions additively when needed,
instead of rebuilding a parallel private-schema dataset. Keep existing datasets until validation
passes; deletion remains a separate user decision.

Use the shared inventory and preset work. Expose clusters with GPU/extension checks and the correct
restart/reconversion requirement. Unsupported or missing data gets a clear reason; it must not
silently fall back. Optional plugins, passes and payload loading are absent when disabled.

Gate: one binary and one compatible dataset can run the parity/default configuration and the
Showcase configuration with clusters on or off. OP6 is an integration dependency, not assumed done.

### 3. Restore the visual presentation through Showcase

Restore and verify the old demo's effective choices for:

- Atmosphere, sky, weather/time, fog, directional and local lighting, indoor lighting and snow.
- Water appearance and the selected reflection mode, with their current options preserved.
- HDR, tonemapping, bloom, exposure, anti-aliasing, AO and contact/cascade shadows.
- Door animation, model idle animation, interaction prompts and the existing graphics panel.

Showcase should compose those existing choices explicitly. Do not equate the capture preset with
the interactive preset: capture hides prompts/HUD and rejects timing modes. DLAA, if selected,
must have verified build/runtime support and a visible availability reason. Do not add speculative
Water/Flora/Seasons features just because they appear in the long-term roadmap.

Gate: inspect matched exterior, water, snowy, interior and doorway views against the recorded old
demo settings. Inspect moving views too: temporal AA, exposure, reflections and shadows can fail
while a held picture looks acceptable. Investigate any difference; a small pixel diff alone is
not a pass.

### 4. Make movement and portal views correct with clusters

Fix object ownership at the draw-ready boundary: retain the existing representation until its
replacement geometry, material and required passes can draw. Preserve collision independently of
render representation. Coordinate tree identity/coverage with the converter; do not suppress whole
cells or billboards on partial coverage.

Make visibility, worldspace identity and residency account for main, portal destination, reflection
and shadow views as required. Portal coverage must not borrow a readiness result from another view.
Keep terrain clip masks correct across world changes and origin rebases. Destination preparation
must include the required cluster resources; release/restoration must not leave holes or duplicates.
Do not cure loading hitches with a fade, delayed interaction, smaller reach or hidden loading.

Gate: Riverwood house entries/exits in both directions, an interior/exterior transition and a
worldspace transition, including immediate return, rapid camera turns, cold arrival and revisits.
Use the existing routes and crossing instrumentation. Record missing/duplicate representation and
collider readiness explicitly; keep failures visible. Coordinate crossing hitches with A16.

The initial ownership audit found these implementation requirements (no readiness fix implemented
yet): bind each candidate to its actual instance and primitive, not a modelwide primitive set crossed
with all positions; register retained instances after proxy filtering and scene ordering; preserve
and restore stock visibility owned by the handoff; scope eligibility to scene generation and view.
Tangents prepared and buffers appended are not proof of drawable materials/pipelines. Commit the
stock/cluster decision consistently within a render frame; a previous-frame readiness flag alone
can leave a hole on invalidation. Global stock Visibility cannot express different main/portal
choices. Exact reference/world/primitive provenance must follow the converter contract; positional
matching alone cannot establish ownership for colocated or differently transformed references.

Converter facts at object-stage `592d8032` / terrain-stage `37bbd4fb`: each instance is one
reference/clustered-primitive pair, but the classifier's reference ID is dropped and stable_id is
zero. Coverage counts/rejects do not identify complete per-reference replacement. Tangent sections
are supported but absent from sampled packages because source GLBs have no tangent attribute.
VG7 directs tangent derivation into cluster conversion using stock-compatible MikkTSpace and
packing; prove shading equivalence across quantization/simplification, seams and handedness before
switching. Coordinate the changed builder identity with OP6 and regenerate only once agreed.

Before a binary format bump, converter and OP6 should evaluate a versioned companion mapping keyed
by immutable MCI content hash plus final record ordinal. It must come from actual writer ordering
and identify the dataset-scoped reference, world/cell and actual primitive with resolved source/model
provenance. Counts alone do not prove coverage: validate exact expected and clustered primitive
identities plus partial/rejected state. Reference FormID is valid only within matching dataset/build
provenance, not a global stable identity. Do not repurpose the reserved stable_id field. A format
bump remains possible if the direct-record design proves simpler with an explicit compatibility
plan. Neither mapping nor format change is implemented; OP6 contract acknowledgment precedes both.

### 5. Complete shadows and performance acceptance

S5 owns shadow feature/options work; VG7 owns cluster integration and equivalent geometry/material
semantics. Default shadow behaviour must stay unchanged. Optional LOD bias/distance reductions are
quality choices, not a way to pass the unchanged-quality performance target. Finish distant-shadow
coverage and inspect near contact, cutouts, terrain, cascades, portals and moving transitions.

S5 coordination: `shadow_presets.rs` supplies cited shadow fields; OP4 selects presets and owns the
functional menu. S5 adds neutral/off shadow-bias and distance-limit settings on its dev-based branch
now and wires the renderer only after cluster adoption. VG7 will supply the exact reviewed adoption
head and checks; the current prototype head is not an adoption approval. Preserve generation,
streamed-layout, material/resource and incomplete-pipeline retry invalidation. Per-view cull uniforms
are rewritten every frame; uniform-only shadow bias must not alter the main camera or require a
material-plan rebuild. Any cached membership/pipeline state affected by options needs an option
revision and relevant cascade identity. Directional orthographic culling currently omits near/far
depth clipping intentionally, retaining upstream casters. A conservative optimization must include
caster bounds, receiver-to-light extrusion and cascade blend overlap; receiver-box-only or radial
camera-distance rejection is not a correctness-preserving replacement. Any finite limit that cannot
prove that bound is a labelled quality option, not a VG7 unchanged-quality performance win.

Use existing measurements first. Add counters that separate readiness, visibility, queue work and
actual draw work before a targeted experiment. After functional/visual gates pass, run the project's
guarded release travel, door and held-out routes at native resolution and explicit Showcase settings.
Report loading separately from settled rendering and report p50/p99/p99.9, worst frames and guard
verdicts. Retain the stable 160 fps target (6.25 ms) and existing reach/fidelity. Do not repeat
unchanged test suites or performance runs without a new question.

## Delivery and completion

Deliver incrementally: (1) controls and walking entry point, (2) shared-build Showcase visuals,
(3) correct clustered portal traversal, (4) measured final showcase. Each milestone gets an exact
launch command, resolved settings and known gaps. The first milestone must not be labelled complete
demo parity while later gates are open.

Before integration, run relevant Rust checks and the required workspace checks at the handoff/merge
boundary, plus all-options-off parity and a portal-heavy GPU check. Builds and GPU/timing windows go
through the gate. No feature implementation, builds, conversion or new timing runs were performed
to produce this plan. Nothing is pushed or published.

## Destination-readiness agreement with PL1 (2026-10-17)

VG7 accepts PL1's generic bridge seam and first terrain/static main-camera slice, with these
changes sent to its owner against `local/plans/portal-destination-readiness-contract.md`:

- Fact 5 is a render-world decision made after preparation in current-frame queue/planning, with
  stock still extracted as standby. Select stock or its exact cluster replacement consistently
  for the relevant view/pass family. Prior-frame main-world Visibility is not that decision.
- A prior-frame ready report plus one extra wait frame does not close the race. The alternative
  requires resources pinned through the consuming frame, or current-frame render selection of
  standby stock on late invalidation. Do not draw both representations as the handoff mechanism.
- Generic keys/protocol can live in swap_bridge.rs. Use actual view identity/incarnation, world,
  generation and an invalidation API. PL1 owns destination/crossing/role increments; VG7 owns
  cluster eligibility increments/revocation. Compare render-frame stamps within the render
  clock; record the extracted request generation rather than comparing main/render frame counts.
- A warm view cannot prove another destination view ready without equivalent view/pass signature.
- Cluster eligibility needs compute/cull, resident geometry/buffers, material/image preparation,
  allocator bindings, actual current queued membership and all relevant enabled colour,
  depth/normal/motion and shadow variants. Do not require unrelated cached pipelines. Terrain
  discard, alpha masks and snow remain part of the required pass semantics.
- Extract reports facts evaluated in the previous render frame; it does not predict later queue
  readiness in its current frame. The render decision is subsequently reported through the bridge.
- Failed cells are explicit failure, not successful drawable readiness, unless required coverage is
  proven by another retained representation. A stock-only first slice cannot silently certify the
  cluster-on destination before VG7 supplies fact 5. No new seamless loading hold/fade is added.

No bridge, crossing, portal or cluster implementation is changed by this agreement. PL1 supplies
its exact generic API and checked head before VG7 consumes it; each thread retains its file boundary.

## Accepted OP6 ownership contract (VG7 acknowledgment, 2026-10-17)

- `mudcrab.clusters` v1 owns all shared object/terrain geometry: whole `cluster_build`,
  `cluster_meshes`, `cluster_chunks`, `cluster_chunks_spatial`, `cluster_rejects` tables and R-tree
  shadow tables, every file under `clusters/`, and `cluster-manifest.json`.
- `mudcrab.cluster_terrain` v1 owns only `cluster_terrain`, `cluster_terrain_heights`,
  `cluster_terrain_build` and `cluster-terrain-manifest.json`. No shared-table row ownership.
- MCL/MCI payload format stays v3. Extension wrapper v1 is independent of that binary version.
- Both extension entries use the current core `lod_build.build_identity` as `built_from`.
  Cluster builder/header identity, source GLB hash and per-block input_key validation remain
  separate requirements, never substitutes for core provenance.
- The terrain manifest declares `requires` with base extension ID/version/final content hash,
  exact `cell_cache.rkyv` SHA-256 and terrain builder/input identities. The terrain extension
  hashes that manifest. VG7 validates these payload-level dependencies and reports capability
  reasons; generic ExtensionEntry gets no new requires field.
- Terrain geometry writes must pass through the base publisher's ownership. After all geometry
  writes, finalize/re-hash the base extension; then write terrain's dependency on that final hash,
  hash its entry, and publish consistent metadata/manifests under the shared dataset lock. Keep
  the terrain manifest outside the base-owned clusters/ scope to avoid a recursive dependency.
  Later base changes invalidate terrain until dependency validation/publication succeeds again.
- Baked core files/tables remain intact. Cluster conversion no longer excludes baked data under
  the old mode-exclusive ADR interpretation.
- Reference mapping/coverage additions still require declared table/file ownership and contract
  agreement; this acknowledgment does not introduce them implicitly.

VG7 sent this acknowledgment to OP6 and requested VG8's explicit ownership/path acknowledgment.
Regeneration remains held until that acknowledgment and OP6's final checked code head are supplied.

VG8 has confirmed the existing shared write paths fit this whole-store ownership. VG7 read OP6's
full proposed `docs/specs/converters/cluster-extension-adoption.md` and acknowledged its design,
including reserved base-owned companion mapping/coverage/provenance tables. Exact companion schema
and producer/reader implementation checks are still outstanding; design acknowledgment is not
regeneration authorization. Timing/probe reports should be outside the converted root as run
artifacts; any report retained inside must be fully owned and hashed, with no hash-exclusion
exception. All stages finish the shared store before its final counts/manifest/hash; only then is
terrain's dependency manifest/hash produced. Await reviewed OP6 commit, exact producer/reader
commits and fixture agreement before additive regeneration.

## Companion coverage schema work authorized by OP6 (2026-10-17)

OP6 explicitly reserves base-owned wrapper-v1 `cluster_instance_map`,
`cluster_reference_coverage`, and optional `cluster_provenance`, with base manifest fields
`instance_map_version: 1` / `coverage_version: 1`. VG8 may implement schema and dummy fixtures
now; send the reviewed exact schema to VG7/OP6 before real regeneration. MCL/MCI remains v3 and
stable_id is unchanged. Bind immutable MCI content hash and actual final writer ordinal to dataset
reference/world/cell, mesh/primitive and resolved model/material/override input digest; provenance
includes core built_from and builder ID. Require exact expected primitive identities and explicit
clustered/rejected/not-eligible statuses/reasons. Reject unresolved per-reference overrides
explicitly; constant provenance cannot certify resolution. Synthetic terrain instance treatment
must be explicit in the schema, not guessed from a zero reference ID.

The new reader must require validated mapping, coverage and provenance before suppressing stock.
Partial coverage permits handoff only for each proven primitive; suppress a tree billboard only
when the complete valid replacement is draw-ready for the relevant view. Missing/stale/unsupported
coverage reports a reason and never causes stock to be hidden. This does not authorize silent
substitution for a requested unusable cluster mode. These are runtime implementation requirements,
not a claim that the present prototype already enforces them. Base finalization hashes the new
tables and terrain depends on that final hash. Reader implementation awaits the reviewed schema;
real regeneration still awaits agreed checked producer/reader commits.

## Schema review corrections: requirements before freezing (2026-10-17)

These supersede the earlier hash-plus-ordinal-only key and emitted-table coverage descriptions.
Direction is accepted; no concrete schema is frozen and no regeneration is authorized.

1. Validate exact object-record/mapping/clustered-coverage joins and bijections, final ordinals,
   actual transforms and inputs; counts are sanity only. Declare object versus synthetic-terrain
   domains. Runtime draw readiness remains per view and pass, never inferred from rows.
2. Bind logical chunk identity (world/tier/anchor or stable key), content hash and final ordinal,
   validating context against row/header. A hash-only alternative needs an explicit uniqueness
   proof. Include coverage world/cell identity or prove reference IDs unique within the build;
   distinguish source cell from placement cell. Define primitive-piece multiplicity explicitly.
3. Enumerate expected identities independently before eligibility/build success, from resolved
   source-model content identity and source primitive index; rejected primitives may lack payload
   hashes. Explicit known/unknown reference scope is mutually exclusive with primitive rows.
   Unknown/failed source sets cannot become Complete or an empty known set.
4. Version canonical input provenance: winning reference/base, model/material/external texture
   content hashes, actual flags/transform/snow and resolved overrides. GLB hash/path alone does not
   cover texture bytes. Validate supported digest version/algorithm and hex identities; header
   versions CHECK=1 and classifier enumeration/scope must be explicit, not constant certification.
5. Check XMSP/XALP/XEMI as parsed subrecord tags, not raw substring matches. Malformed/unparsed data
   is unresolved. This is a minimum guard, not an exhaustive behavior-changing tag list; preserve
   O7 XTNM/O8 layout caveats without inventing semantics.
6. Base MODS alternate textures require N21 resolution of winning TXST/material inputs, or validated
   source detection and explicit ineligibility. If this database cannot prove absence, provenance
   stays unknown and blocks replacement for that domain. A classifier_scope note is not proof.
7. Constrain nonempty rejection/ineligibility reasons, valid hashes and ranged ordinal/primitive
   indices; clustered rows cannot use a whole-reference sentinel. Preserve all three reserved
   base-owned table names and explicit indexes, including cluster_provenance.
8. Jointly review exact DDL/statuses, resolver/producer and reader APIs, with corruption, omitted
   source primitive/piece, duplicate hash, equal-sort-key and override fixtures before freezing.

Unknown or not-eligible coverage retains stock with a visible reason and does not enable a
corresponding cluster draw that would duplicate it. First adoption retains runtime tangents,
MCI v3 and unchanged stable_id semantics. Requirements sent to VG8 and acknowledged to OP6.

Publisher review status: `b95af1df` closes the original manifest-last hashing, payload
row/hash/header and mapping-version declaration findings. Additional integration checks remain:
exact spatial-index row/key/bounds agreement (respecting SQLite outward float rounding), terrain
instance/atlas referential validation, and propagating sidecar metadata I/O errors other than
NotFound. A structurally valid mapping without authoritative inputs is not producer/runtime
coverage acceptance. Package publication still requires staging/recovery and the exclusive writer
context. OP6's checked lock/core-identity/hasher APIs already exist; no new OP6 shape is a blocker.
VG8 is implementing corrections on its own branch; wait for its exact reviewed diff/head, not its
uncommitted intermediate API, before terrain/reader adoption.

Certified finalization must read the current nonempty core identity with OP6's
`shared::extensions::core_build_identity` and require equality with mapping provenance and envelope
built_from. `validate_mapping(..., None)` is not an acceptable dataset-publication bypass; arbitrary
provenance in a fixture without lod_build is not acceptance evidence. Add missing/empty/stale core
identity failure fixtures and preserve the previous manifest. Object chunk checks do not validate
terrain instances/atlases semantically; the terrain finalizer must validate their references and
correct builder/input recipe separately before consistent publication.

## V2 reader decisions: alternate textures and hash representation (2026-10-17)

Unresolved base MODS references are not eligible for cluster replacement until N21's winning
alternate texture/material inputs are resolved and validated. The reported 24,640/104,786 raw-tag
substring count is only an upper bound, not eligibility proof. Parse real subrecords; unparsed or
unprovable absence remains unknown scope. Proven unaffected references can proceed. Retain stock
for unresolved references with a visible reason and do not enable duplicate cluster replacements.

First adoption keeps canonical lowercase TEXT hex hashes with strict hex/length constraints,
matching existing cluster tables. This supersedes the earlier VG7 BLOB request after reconciling
OP6's review: estimated storage savings alone do not demonstrate a runtime gain that justifies
another representation transition. Absent identities must remain explicit in scope/status
semantics; empty/zero values never certify coverage. Extension hashing stays mudcrab-ext-hash-v1.
This representation decision is not concrete schema/producer/reader acceptance.

Review update at `651bce49`: the publisher now reads actual nonempty core identity before the
mapping-table branch and passes it to authoritative validation. That specific bypass finding is
closed. The returned base ExtensionEntry owns root cluster-manifest.json plus clusters/ and hashes
candidate manifest bytes before replacement. Shared extension code matches checked OP6 cdc2e551.
Runtime consumes validated stored eligibility after ownership hashes, core and dependency freshness
checks; archived-subrecord parsing stays converter-side. Shared reason/tag constants can support
reader diagnostics. Production authority/population, staged package publication and reader/terrain
semantic fixtures remain required before adoption or regeneration.

## Publisher implementation split (agreed with VG8, 2026-10-17)

VG8 will supply `cluster::publish` on `phase1/cluster-converter-stage` after OP6 provides its final
checked head, lock definition and hash API. VG7 then integrates that checked commit into its owned
terrain branch, replaces direct asset writes/cluster_meshes inserts with publisher calls, finalizes
the completed shared store and emits the terrain dependency manifest. Neither thread edits the
other's branch; integration/rebase waits for clean owned state and the agreed base.

Finalization and consistent base/terrain/envelope publication require the dataset's EXCLUSIVE
writer AssetLock for the entire operation. A shared reader lock is only for read-only validation,
not publication. Use OP6 checked implementation `cdc2e5516f3cd1cb21281a0036d674daa7ee1c28`
(docs-only tip `513a7e9b71927d38fa135f2987d5437a68704e9e`); the old review-plan head is superseded.
Reserved mapping tables/indexes authorize schema design and joint review, not an unreviewed frozen
schema or regeneration. No arbitrary extra owned objects are implied.

Immutable content-addressed writes retain no-overwrite/equal-bytes idempotency. Mutable manifests
need atomic replacement under the dataset lock, not the immutable helper's no-overwrite policy.
Finalization recounts the complete object+terrain store and hashes a committed consistent database
and file view using OP6's ExtensionHasher. It must expose the final base extension hash and envelope
ownership/stats data needed by terrain; hashing only the JSON manifest is insufficient. Do not
publish an envelope claiming either new dependency pair until both manifests/hashes are complete.
Read/write locking, failure and rerun fixtures follow OP6's reviewed contract; no regeneration yet.

## VG7 next execution plan (2026-10-17, after producer review)

This is the current execution order and supersedes older pending-head/ownership notes below.
Producer source findings are closed through `a96c515e`; OP6 checked implementation is `cdc2e551`
(plus its reviewed add_file_digest follow-up already present in the producer). PL1 checked code
is `6c67091b`, with docs-only follow-up `657a74e3`. B5 reviewed engine port is `a3ce2388`.
These are inputs, not already integrated features. No new baseline/timing campaign is required.

1. **Establish the serialized integration base.** Preserve the four stranded dev-integration
   commits with named local references and their patch-equivalence mapping before reusing that
   checkout. Confirm clean owned state and record the serializer. Adopt B5's reviewed engine fixes
   without importing its overlapping old converter branch wholesale; add the discriminating
   non-finite bounds/rotation regressions at the responsible engine layer. Bring in the reviewed
   shared-data and parity/determinism inputs in a recorded local stack. Resolve conflicts against
   current source, not by keeping whichever branch compiles. Completion: recoverable old history,
   explicit input heads, compatible core/extension APIs and no concurrent checkout owner.

2. **Adapt owned terrain conversion to the publisher.** On the clean owned terrain worktree,
   adopt the reviewed producer API, replace private asset writes and direct shared mesh inserts,
   retain the existing relative-header/absolute-cell coordinate convention, and provide core-backed
   LOD origins. Validate atlas decoding/layout and the actual terrain builder/input recipe. Finish
   both geometry stages, finalize the whole base once, then emit terrain metadata/dependency
   manifest using final base hash and core cell-cache hash. OP6 owns generic envelope registration;
   coordinate that bounded edit rather than duplicate it. Completion: synthetic object+terrain
   package and one base/terrain/envelope publication under a continuously held exclusive lock,
   including failure-before-publication and replacement/recovery preservation checks. No real regen.

3. **Implement the certified engine reader.** Consume the agreed wrapper-v1/payload-v3 contract,
   verify ownership hashes, core identity, terrain dependencies and mapping structure, and expose
   explicit capability reasons. Consume producer-validated eligibility rather than duplicate
   archived-record parsing. Unknown/absent/stale coverage retains stock and cannot enable its
   cluster replacement. Completion: valid, stale-core, changed-base, missing-map, partial primitive,
   conflicting-material and corrupt-terrain fixtures, with no changes to baked core assets when off.

4. **Replace eager hiding with per-view draw-ready ownership.** Build candidates by placed
   reference and primitive, not model/position inference. In the render world decide stock versus
   eligible cluster from actual residency, buffers, materials, queued passes and pipelines for that
   view. Retain stock until the handoff is proven; make ownership reversible on unload, invalidation
   and option changes. A tree billboard needs complete applicable reference coverage and readiness
   before suppression. Completion: no holes or duplicate representations across startup, partial
   coverage, incremental loading, unload/reload, world/layer changes and camera movement.

5. **Publish PL1 handoff facts and invalidations.** Use its exact DestinationKey/render-frame
   protocol. Publish in PublishHandoff/PhaseSort after cluster planning; PL1 evaluates after
   PrepareBindGroups and before Render. Candidate changes bump generation/revoke readiness, and
   facts never transfer between views. Keep crossing logic owned by PL1. Completion: current-frame
   fact tests plus stock/cluster destination transitions and failure cases; a quiet-PC portal-heavy
   GPU route confirms the actual renderer path.

6. **Assemble the shared walking demo and unblock shadows.** Point the existing launcher at the
   integrated engine/dataset; preserve walk/mouse look, ground start and functional development
   controls. Combine OP4 selections with capability reasons without changing saved user choices.
   Give S5 the exact reviewed cluster head and invalidation/off-cascade-caster rules; S5 owns far
   shadows and any default-preserving shadow tuning. Completion: inspect the walking/door route,
   terrain seams/collision, trees, world/view visibility and shadow behaviour against the old-demo
   feature checklist. Missing distant shadows remain an explicit failure, not a settings workaround.

7. **Validate the assembled change once, then measure remaining cost.** Use focused meaningful
   checks while editing and required workspace checks at the integration handoff; retain prior
   producer/PL1 evidence instead of rerunning their unchanged topic trees. Run all-off parity and
   relevant GPU acceptance on the assembled build. Only after correctness, use existing stock
   stage counters and a guarded settled workload to answer specific remaining performance questions.
   Separate load/readiness from steady rendering; retain fidelity/reach. Completion is clustered
   terrain working, safe replacement/portals working, render-thread cost accounted for and distant
   shadows working. Only then prepare PR splits; publication still needs the user's per-item yes.

First implementation checkpoint: serialized base and synthetic joint object/terrain publication.
Next checkpoint: certified reader and reversible stock/cluster ownership. Final checkpoint: the
integrated walking demo and its correctness/performance evidence. Real regeneration stays held
until producer/reader/dependency agreement and fixtures are complete; prepare one exact rebuild
scope and output/recovery plan before seeking any required authorization.

## Project-wide ownership and dependency order

Proposed division, checked against existing thread handoffs on 2026-10-17. This records the work
allocation; it does not send instructions to other threads, claim their tasks, or start new ones.
A completed agent turn or a queue row marked done is not proof that its branch is integrated.

Integration serialization is owned by VG7 (this thread). On 2026-10-17 the integration checkout
was clean at `e739c90e`, with a generic shared OWNER; neither `01bb2d1b` nor `e739c90e` is an
ancestor of `dev` at `ca281332`. That does not establish whether equivalent fixes were ported.
VG7 requested B5's reviewed replacement head and closure decision before any reset/reuse. No
checkout mutation or merge has started; feature owners route integration requests through VG7.
OP6's final checked code commit and exact cluster object/terrain extension boundaries remain
pending. Reported core converter 26 / DB 9, `mudcrab-ext-hash-v1` and
`built_from = lod_build.build_identity` are recorded inputs, not an acknowledgment of the still
pending cluster extension contract. Data regeneration stays held until that contract is agreed.

| Order | Owner/thread | Deliverable and boundary | Dependency / completion evidence |
|---|---|---|---|
| 0 | VG7 (this thread), with B5 and the current dev integration owner | Maintain this adoption checklist and serialize shared integration. B5 resolves the four room-culling commits already in the integration checkout before it is reused. | Known integration base, exact branch heads and no unowned changes; no concurrent merges in the checkout. |
| 1a | H18 parity tool | Deliver the existing reference builder, comparison tool and deterministic/noise-floor rules to dev. Feature owners run the tool; H18 does not test every feature for them. | Tool available on the integration base; failures distinguish input mismatch, disturbance and actual rendering differences. |
| 1b | OP6 shared dataset | Finish manifest extension/version/hash validation, refresh preservation, dependency notices and the shared dataset. Own data capability, not user selections or menu widgets. | Both engines load the shared core; refresh and invalidation regressions pass. Keep visual all-off failures separate from data compatibility. |
| 1c | VG7 converter/VG8, coordinated with OP6 | Adopt OP6's agreed manifest contract for cluster object and terrain payloads; finish identity/coverage and tangent payload work where required. No second manifest design. | Cluster extension validity and additive/off output proven; engine consumes the same contract. Identity-changing exports happen once after the contract is fixed. |
| 1d | VG7 | Restore the existing walking controller, ground-level demo start and accurate launcher instructions. | Can proceed now on the prototype; mouse/focus/walk/fly/collision check. Final launcher later targets the shared dev build. |
| 2 | VG7 plus the current dev integration owner | Integrate reviewed cluster and baked-representation support into dev using the existing branches as inputs. Preserve ongoing door/water/lighting work. | After integration checkout resolution and compatible H18/OP6 inputs; relevant checks and parity at the merge boundary. This makes the shared renderer available to S5 and the showcase. |
| 3a | OP4 research thread, explicitly claimed implementation work | Own Vanilla preset evidence and the shared Showcase composition. Start cited INI/preset inventory now; do not wait for image-space conversion to research ordinary settings. | Explicit implementation claim is still needed. N23 blocks the authored image-space consumer, not the entire preset table. S5 supplies shadow fields. |
| 3b | Shared settings implementation owner (currently missing) | Reuse the existing settings inventory/configuration paths, persistence and a functional development panel for capability reasons; no Skyrim UI work. Combine OP6 ExtensionNotices and GPU support with selected options, without overwriting user choices. | Give the functional development wiring a bounded queue row/owner before editing shared settings files. It may be assigned to OP4 if accepted; reuse the current panel and defer Skyrim UI integration to its contributors. |
| 3c | Existing N23 converter and lighting/image-space owners; OP10 needs a claim | N23 supplies authored IMGS/VOLI data; the rendering owner consumes it. OP10 owns atmosphere/sky option registration and zero off-cost. OP1/W1 and OP5 keep water/reflection and snow work. | These run in parallel with cluster correctness. Preserve the old Showcase choices while completing evidence-based vanilla defaults; do not block the demo on unrelated new effects. |
| 3d | A16/OP3 portal loading and door-contract thread | Land door-mode registration, destination preparation, collision/crossing contract and crossing-hitch fixes. | OP3's latest handoff says unmerged despite the queue label. Resolve its settle failure with the responsible LOD/streaming owner, do not waive the gate. |
| 3e | VG7 with converter coverage input | Fix draw-ready replacement ownership, tree/billboard coverage, cluster world/layer visibility and readiness for secondary views. | Cluster code integrated; agree destination readiness handoff with A16. Prove no startup/arrival holes, duplicate representations or missing portal destination clusters. |
| 3f | S5 shadows | Own shadow options, relevant per-cascade evidence, default-preserving cascade tuning and far terrain shadows. VG7 reviews cluster-path integration; OP4 owns preset composition. | Baseline/option-only work can proceed now. Final cluster implementation uses the shared renderer; no simultaneous edits to render.rs. Optional quality reductions do not satisfy unchanged-quality performance acceptance. |
| 4 | VG7 showcase owner (this thread) | Assemble the final launcher/preset against one dev binary and dataset; inspect the full old-demo feature checklist and walking/portal routes. | Inputs above are integrated and version-compatible, not merely finished on topic branches. Regressions go to their owning threads; VG7 fixes cluster-specific issues. |
| 5 | VG7, using existing parity/performance tools and feature owners | Perform functional/visual acceptance first, then guarded release travel, doors and held-out performance acceptance. | All-options-off parity, portal-heavy GPU correctness and required Rust gates; unchanged Showcase fidelity/reach, no hiding of loading, honest frame-time and readiness reports. |
| 6 | H19 housekeeping; VG7 and each feature owner | Retire superseded combination builds/data only after the shared demo passes and the user authorizes deletion. Feature owners split publication units only when their completion bars pass. | Keep rollback evidence and the known-working launcher until replacement is accepted. Publishing still requires the user's per-item yes. |

Critical path: integration checkout resolution + H18 + OP6 contract -> cluster adoption in dev ->
cluster/portal readiness + Showcase/settings/data integration + S5 -> one interactive showcase ->
functional/visual acceptance -> final performance acceptance. Controls, preset research, data work,
door fixes and shadow research can progress in parallel before their integration dependencies land.

Do not wait for every parity research topic in the project to finish before restoring the old demo
experience. Do require every enabled feature to declare its actual behaviour and dependencies.
Likewise, do not treat a working launcher as proof of the complete 160 fps/no-holes bar.

References: current `dev` policy `docs/process/one-tree-options-policy.md`; local policy migration
plan `local/plans/one-tree-options-policy.md`; `docs/demo/README.md`; `tools/demo/play-demo.ps1`;
`crates/engine/src/player.rs`; `docs/process/capture-settings.md`; and this branch's
`cluster-streaming-design-2026-10-16.md`.

Current corrected candidate checks (cycle23): fmt, engine all-target/all-feature clippy and
all1,988 engine tests pass (eight existing skips). The first ECS run caught a wrong Allow/query
domain premise; the snapshot now uses identical optional-component query data, retaining the
old refusal for disabled terrain markers and a positive far sibling. Actual GPU TEST passes23
controls: first19 observations match cycle15 exactly, plus direct row1/page0 shadow, wrong-row,
out-of-range-row and legacy-version controls. Numeric image23b17297, source ed255af6 plus
patch86e445ac, retained under C:/Modding/Mudcrab/builds/vg7-f1-numeric-proof-cycle23.
No candidate performance saving or usable-demo acceptance follows until the next live profile.

Cycle24 actual profile (source1edb48d0, Tracy imageb68b243b) confirms the optional
forward draw, all260 terrain batches ready, and the original door/far captures
(resident70,382/69,786; settle0.51s/5.45s). Inspected original and enabled later
far images retain terrain, tower, mountains and trees; no complete native
coverage claim. Loaded135-240s diagnostic bins put terrain identity at4.74-5.04ms
versus roughly25ms before the result cache, and total source extraction at
27.79-28.91ms versus47-49ms. Presence/qualification remain around9ms/7.6ms.
This is profiler inference, not guarded frame-time acceptance.

GPU directory links show no useful gain: actual optional opaque medians remain
1.55-1.75s, comparable to previous cycles. Drop that runtime-onlyv4 experiment
rather than retain its extra protocol/validation complexity. Keep the CPU cache.
The ignored homogeneous-box draft checks a division-free conservative miss proof;
a smaller per-axis early-exit variant is being compared before selecting the next
experiment. No production triangle predicate, reach or budget may be weakened.

Cycle24 is a bounded diagnostic, not completed26-shot acceptance: after the original
two captures plus five extra captures and over100s of optional execution, root
requested graceful closure of only the immutable owned window. An in-flight sixth
extra screenshot then times out and window extraction includes an approximately79s
close/drain stall; the engine wrapper exits0. Preserve this limitation and the
repeated statistics-buffer warning; do not credit process exit as route success.
Artifacts, source/image identity, close note and raw exports remain at
C:/Modding/Mudcrab/builds/vg7-cycle24-tracy-runtime. No timing hold or dataset change.

Next bounded candidate restores the exact GPUv3 host protocol and old local stack,
then adds only per-axis `near > far || far <= 0` rejection after the unchanged
interval divisions. Further slab intersections can only narrow the positive-time
interval, so an already proved miss needs no later arithmetic. Independent source
review and300,020 exact common-grid rational controls retain all104,247 exact
hits; the control corpus uses fewer division-helper calls (not a runtime gain).
Some later numeric refusals are skipped after an earlier proof of miss; arbitrary
Unknown/counter/finite-budget identity is not assumed. Canonical predicates,
coverage, reach and production budgets remain unchanged. Fast locked composition
passes; current source pin08e54d65. Combined checks and actual GPU proof are pending.

To shorten later shader-only experiments, the ignored numerical harness now has an
explicit candidate-source mode: supply BOTH `MUDCRAB_F1_TEST_RECEIVER_PATH` (absolute
UTF-8 source file) and `MUDCRAB_F1_TEST_RECEIVER_SHA256` (64 lowercase hex, normalized
LF source), or neither for literal-pinned compiled production. Bad input fails
without fallback; arithmetic stays immutable and pinned. The same Composer,
validator, host buffers and original19 controls execute. Candidate logs name the
path/hash and compiled runtime GPU version; this is numerical candidate evidence,
never current production/renderer acceptance. Host protocol changes require a
fresh executable. Five loader regressions and independent source review cover this
test-only seam; compiled/default/positive/negative GPU checks remain pending.

Cycle26 checks now pass: fmt, engine all-target/all-feature clippy and all1,995
engine tests (eight existing skips). Actual default pinned GPU execution matches
all19 cycle15 Observation words exactly. The explicit external raw baseline also
matches19; a deliberate always-Clear external candidate fails exactly the first
known-shadow assertion (left1/right2, exit101). This expected negative confirms
the candidate is executed; it does not waive a production failure. Numeric
image29f7122e and source cfba5711+patch8f222912 are retained under
C:/Modding/Mudcrab/builds/vg7-f1-numeric-proof-cycle26. Next is the smaller six-shot
live profile; no early-box speed or walking-demo acceptance is claimed yet.
