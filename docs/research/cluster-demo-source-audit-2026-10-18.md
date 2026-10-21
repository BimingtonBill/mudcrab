# Cluster demo source audit, 2026-10-18

The user's six-item no-build work slice: trace missing distant objects, audit the first-door
gate, interpret existing performance evidence, review the latest loader patch, revise the
completion order, and prepare bounded fixes/regressions. Source base: `3b5e392e` plus known
root-owned WIP in `opt/cluster-showcase-integration`. No build, test execution, engine launch,
benchmark, regeneration or publication belongs to this slice. Prepared Rust is unvalidated.

## Distant objects: confirmed runtime gap

The current engine consumes compiled terrain `lod_chunks` in `world/database.rs` and
`streaming/lod.rs`. Its scene validator requires source-cell/terrain-quadrant metadata.
There is no engine query/render consumer of native object `lod_block` or `lod_tree_*`
inventory. This is a missing implementation, not a working renderer disabled by a setting.
The converter's native inventory writer is `crates/converter/src/portal_lod.rs`; its schema is in
`crates/converter/src/esm/exporter.rs`. These are distinct from converted terrain chunks.

The current certified package manifest declares 1,078 native object blocks and 43,333 tree
instances. These are inventory counts, not proof that the runtime draws them. Its cluster
manifest reports 37,866 object instances with a 1,024-triangle eligibility threshold and
exclusions including 7,301 below-threshold and 980 unsupported-material entries. Those
exclusions are not necessarily reference counts. Partial cluster geometry cannot replace
complete distant coverage. Visible clustered mountains do not establish object-LOD parity.

Cluster admission also depends on actual source templates, usable tangents and matching
placed-world/material/binding proofs (`cluster/handoff.rs`). Those checks protect the handoff;
weakening them would not create the missing fallback renderer.

The implementation boundary is the existing database/asset/streaming path:

1. Add bounded object-block and tree-inventory reads through the existing world database
   job mechanism. Carry actual world/grid/level, path, bounds and billboard metadata. Keep
   terrain validation separate; native object GLBs are not terrain-quadrant scenes.
2. Reuse the existing converted GLB/material loader, view ownership, origin rebasing and
   residency/failure reporting. Verify native block coordinates and billboard transforms
   against the producer recipe before implementing transforms; no guessed offsets.
3. Define selection and handoff at the unit the fallback can actually draw. Native object
   blocks do not provide a certified per-reference replacement relation. A few mapped
   references cannot authorize hiding a whole baked unit. Child-world storage placement
   does not authorize parent-view substitution. Missing relation/coverage proof remains
   an explicit blocker to selective suppression, not an inferred complete state.
4. Verify mixed clustered/rejected/unknown content, stock-only and cluster-on views,
   near/far transitions, child-world ownership, withdrawal of material/pipeline proof,
   and tree overlap. Require complete rendering without double representations before
   accepting the adapter. Do not globally remove stock billboards to conceal overlap.

This slice establishes the responsible seam and required regression cases; it does not
pretend that a new object/tree renderer is a small completed patch. No dataset change is
currently justified by the inventory counts alone.

## Follow-up review disposition

The two Opus reviews and GLM Flash critique are consolidated in the current section of
[the demo plan](cluster-showcase-demo-plan-2026-10-17.md). They supersede the earlier positive
static verdict as final acceptance: Opus identified a no-clip first-door timeout regression
in the prepared split, a likely lint issue, and an optional-shadow draw/readiness mismatch.
These are pending corrections, not tested fixes. GLM's caches are unapplied hypotheses.

## First door: separate opening from photographing

In R4, the destination was resident at 15:34:49.842Z and the clip resolved at 15:34:51.330Z.
The shots runner waited for package certification and all 340 blocks, reporting readiness
at 15:36:31.398Z. It requested opening at 15:36:31.399Z; the doorway was open/rendering at
15:36:32.159Z. These timestamps identify a scripted prerequisite delay, not measured player
input latency or a benchmark. Source: ignored `local/vg7/f1-runtime-r4-off-gpu.log`.

Prepared in `shots.rs`: `Phase::Door` uses a separate automation clock that rejects a failed
requested package but does not wait for certification or unrelated uploads. Existing door
facts still require its actual animation player/graph, or the existing quiet-static proof.
An unresolved door is never forced open. Opening remains the ordinary `OpenDoor` message.

`Phase::Settle` retains certification, initial upload and existing view settle requirements.
Crossing retains its independent exact destination/generation/render-proof authorization.
The ordinary door timeout is unchanged and now runs during background package work; a slow
door can therefore fail sooner rather than receive an invisible timeout extension. This
patch does not claim reduced background loading cost. Opus subsequently identified the
no-clip timeout case as a regression to correct; earlier static approval was insufficient. The separate demo-tour startup gate
is unchanged. Narrowing capture to view-specific startup demand is future work, requiring
an authoritative demand/readiness seam rather than ignoring pending counts.

Prepared regressions cover Pending/Ready door-clock progress while capture stays withheld,
Failed-package termination, unchanged stock timeout, actual DoorWorld graph/player eligibility
and withdrawal during pending work, and retained capture residency gating. Independent static
review passed the patch and fixtures. These tests have not run. The existing actual door-facts predicates remain unchanged.

## Existing performance evidence and its limits

| Evidence | Supported conclusion | Does not establish |
|---|---|---|
| R1-R4 package log: 5.05-6.21 s load; 34.93-39.76 s tangent preparation | Substantial startup work occurs; tangent work logs include multiple worker threads | Isolated critical-path contribution or settled fps |
| R4: 3,010,083,424 bytes cluster allocation; R1 F1: 79,599,576-byte upload | Allocated/uploaded sizes from these runs | Frame cost, memory bottleneck or shadow overhead |
| R3: 93 Scene Modified events over 93 paths; R4: 52 over nine paths, repeatedly including three empty models | Source replacement/reinstancing is real repeated work; prioritize its known cause | Exact CPU/GPU savings of the pending correction |
| R1 on/off qualification populations: 603 vs 439 bindless batches | Those runs differ in rendered work | A valid price for optional distant shadows |
| Historical 2026-10-16 Tracy prototype: finish/submit account for most excess render time | A useful hypothesis for the later integrated stock-span capture | Current integrated render-thread bottleneck or accepted fps |

The custom 600-frame prepare aggregates include startup/append work. Portal diagnostic
samples also include loading/pipeline creation. Neither is a settled performance verdict.
The `runtime-r1/off-repeat-profile` directory contains only OWNER, not a profiling bundle.
Historical notes: `cluster-render-thread-tracy-2026-10-16.md` in the prototype's ignored
research files; retained TEST captures are under `D:/Mudcrab/perf/2026-10-16-151104-lodfar-baked-long`
and `2026-10-16-151314-lodfar-cluster-long`.

After reload and coverage correctness, reuse the existing Tracy capture/report tools on
the next planned capable image. Separate startup from settled windows; match image, dataset,
pose, actual drawn populations and graphics. Read stock Render schedule, command-buffer
finish/submit and GPU forward/prepass/shadow spans. No new timers or stats-only build is needed.

Two source-backed candidates remain hypotheses: cache exact F1 placement metadata across
unchanged layout/placement/origin/target offset; update template/batch qualification inputs
on actual asset/handle/certificate changes while retaining pending retries and invalidation.
Do not implement these ahead of evidence that their current cost matters.

## Loader patch review and next validation boundary

R3 identified a real mountain source reload. Avoiding whole-file glTF reload when the actual
nonempty completed source proves no animation stabilized the R4 repeat pose with F1 off.
That image is `d88997104bf988c9231564b7ecf1739514bd51bf38287bda5a3468b9ddfde747`.
This is visual stability evidence for that pose, not global completeness or performance.

R4 exposed a second loop: empty dummy/trigger sources have no nonempty static proof, yet
overbroad invalidation evicted their legacy root-derived NoClip answer. The pending correction
associates scene IDs only with actual source-proved answers and invalidates only those owned
IDs. The real-loader empty-scene regression exercises same-pass cache reuse, natural root
collection and repeated genuine Modified events beyond twice the fallback window. Independent
static review found no blocker. The correction and new regression remain uncompiled/unrun.

Prior R4-tree clippy and 90 focused tests passed; they do not validate this newer correction
or the shots patch. R15's earlier Alftand/Riverwood route proofs remain applicable only to
that exact earlier image. All-off parity, current portal continuity, complete distant objects,
final shadow appearance and settled performance remain acceptance gates.

Next, when runtime work resumes: run affected-engine fmt/clippy and focused loader/shots
regressions once; inspect one retained current image for first-door response and repeat-pose
stability. Finish the native distant-object adapter before claiming restored presentation.
Then batch the integrated route/all-off/shadow inspection and justified stock profiling,
followed by required final workspace/doc checks on the frozen handoff tree. Keep failure
logs; do not repeat earlier broad checks merely because a turn or provider changed.
