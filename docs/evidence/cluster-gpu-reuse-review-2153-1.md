# Exact GPU reuse review corrections (#2153.1)

Branch: `phase2/cluster-gpu-reuse`. Started at retained implementation `2bf6c5ff8`;
fast-forwarded integration `e9b2d6cf8` (newer than requested `ea12733c5`) before edits.
Later fast-forwarded `94d8722a0` at the lead's request before the final release,
preserving the new instance-inventory field and layout-scope publication contract.
Review input: `D:/Mudcrab/review/drafts/REVIEW-2153/review.md`, read completely.
No push, PR or merge back into integration is authorized or performed by this round.

## Production changes and verification boundary

- Directional cascades belong to distinct camera/physical-slice allocations.
  The ordinary allocation sums cameras rather than giving each camera slice zero.
  A bounded overflow allocation must preserve the existing per-camera/spot demand;
  see the final atlas section below.
- Only `DirectionalShadowMapInputs` views enter retained stock caster bookkeeping.
  Point/spot phases and visibility keep their ordinary behavior, without retained
  caster locks, reverse-index membership, or draw-wrapper cache locks.
- All six missing-draw sites skip that item and continue the phase. Verification
  records incompleteness without propagating a new production phase-abort error.
- Default-off `gpu-reuse-verify` owns cull/shadow/raster readbacks, replay lanes,
  actual-bind validation, audit fields, material diagnostic bindings, CLI and app
  wiring. Scene inventory diagnostics and tests have separate source modules and
  a dedicated commit. Ordinary force-recompute controls and cache windows remain.
- Production root manifest and committed lock resolve registry core_pipeline 0.19.0.
  Native raster hooks live only in `tools/gpu-reuse-verify`, selected with an explicit
  Cargo overlay plus the verification feature. Cargo cannot condition `[patch]` on
  an application feature. The overlay is never auto-loaded; no production core
  pipeline patch/exclude remains. The dev copy retains its licenses/LUT assets.
- Malformed raster thresholds log failure and disable that audit without startup
  panic; other diagnostic environment values are ignored unless its directory is set.
- Poisoned dependency state refuses reuse. Map-owner recovery withdraws owners/keys
  and disables retention for the resource lifetime while preserving the same-frame
  early-skip decision. Fresh early rendering still receives late occlusion work.
- Late producer revocation after early reuse skips late append, withdraws ownership,
  and requires a full cleared render next frame. Global mesh/image/pipeline/geometry
  revisions stay conservative and documented. The unused per-asset revision map
  and revision API were removed; no settled dependency walk was introduced.
- Large new test suites moved to adjacent test modules. Pure import-order changes in
  the dev core pipeline were restored to pristine 0.19.0 ordering.

## Intermediate source proof

The initial all-target workspace compile exposed a test extraction bug: integration
fixtures also include `cull_reuse.rs`, so uniform tests could not import render-only
helpers through its parent. They now live in `cull_uniform_tests.rs`; reusable state
machine tests remain in `cull_reuse_tests.rs`.

Independent source review confirmed feature isolation and skip restoration. PBR
review found two newly introduced edge cases before final acceptance: permanent
late-pass skipping after mutex poisoning, and summed atlas demand exceeding an
otherwise supported device budget with many spotlights. Both were corrected and independently rereviewed before the final gates below;
intermediate passes are not final evidence.

Snapshot `804030f2878d5803b51c6e81b703bc5ba32100e6c88de8697d7d23be2383d6b5`
passed strict default workspace/PBR Clippy and 56 PBR tests, including two-camera
retention and actual early/late poison regressions. These precede atlas overflow
corrections. The many-point diagnostic runs actual pre-fix bookkeeping helpers
then the production directional-only admission gate, with 30 lights, six faces,
10,000 casters and four directional views:

| CPU diagnostic | Ungated reference | Directional-only gate |
|---|---:|---:|
| Point/spot mutation locks | 3,600,360 | 0 |
| Point/spot caster membership attempts | 3,600,000 | 0 |
| All retained views | 184 | 4 |
| All retained memberships | 1,840,000 | 40,000 |
| One test-process elapsed time, ms | 412.3142 | 10.0645 |

This is bounded CPU helper evidence, not a native interior scene or quiet renderer
timing. No elapsed-time threshold is used. Raw log:
`D:/Mudcrab/builds/cluster-gpu-reuse-target/review1-default-pbr-tests.log`.
The owned one-hold capture controller independently passed 137 CPU checks; no
actual capture result follows from those synthetic subprocess fixtures.

## Final atlas, gates, footprint and GPU result

The fitting path keeps one summed array. Overflow uses a master array and retained
camera arrays sized to the original maximum per-camera cascades plus admitted
spots. No admitted light/camera is dropped. Shared spots raster once and copy
whole depth slices at the same shader indices. Pristine `camera_driver` executes
all auxiliary root schedules before camera schedules, preserving store/copy/sample
ordering. Production overflow copies are distinct from feature-only audit copies.

A follow-up source review found stale destination handles on spots that left the
admitted budget. Preparation now retires those views too, through native deferred
despawn, so they cannot retain obsolete arrays after camera removal. ECS command
application and strong-reference-release regressions cover the 252-to-248 example
and later readmission. Final current-source gates and GPU evidence follow below.

## Source footprint against pristine Bevy 0.19.0

| Crate | Default production +/- | Verification plus explicit overlay +/- | Verification delta from default +/- |
|---|---:|---:|---:|
| bevy_render | +714 / -103 | +1,819 / -105 | +1,106 / -3 |
| bevy_pbr | +2,221 / -122 | +3,776 / -122 | +1,589 / -34 |
| bevy_core_pipeline | 0 / 0 | +189 / -104 | +189 / -104 |
| Total | +2,935 / -225 | +5,784 / -331 | +2,884 / -141 |

These are physical source-line diffs after resolving only the verification and
test gates, with tests disabled. Other platform/feature predicates remain intact;
this is reproducible partial source accounting, not compiler expansion or binary
size. Comments and blank lines count. Earlier Mudcrab patches already contributed
+595/-53 production lines. Direct comparison against the parent of original #2153
is +2,361/-193, net +2,168; different diff comparisons cannot be added together.

Extracted Bevy test files contain 2,656 lines (renderer 544; PBR 2,112), with inline
test exclusions recorded separately. Listed engine verification source adds
+587/-3 relative to default; its three extracted test files contain 240 lines.
The explicit dev core-pipeline copy retains 54 files and 1,080,216 working-tree bytes, including
unchanged WGSL/LUTs; none enters the default dependency graph. Its hooks account
for the core-pipeline verification delta above.

The accounting validated 137 syntax projections with rustfmt and rechecked 386
unchanged input hashes. Full method, per-file operations, pristine archive checks,
and limitations: `D:/Mudcrab/builds/cluster-gpu-reuse-target/footprint-review1-merged/REPORT.md`
and `account.json`. No compilation or timing result is inferred from this analysis.

## Merge contract

The cull reuse key reads no `ClusterScene` field directly. It consumes GPU
allocation/generation/layout, material, selection, plan, readiness and pipeline
epochs, origin, and exact view/uniform words. A later published instance inventory
must preserve the existing layout-scope/package-identity invalidation boundary;
this round neither reads nor bypasses a future inventory Arc.


## Final frozen checks and provenance

All retained Rust gates use runtime-source fingerprint
`b46b8d1c3340c4759388487154d5ae2ae40ebf99aa25cb8a9854d448496517a5`.
The release and captures use integration base
`94d8722a0b116260e6359f1f31d25c192541a60a` plus this round's frozen local changes.
Full pre-documentation source pin:
`324f2b2ae42b90348a2c4fdbb1daa110db4f97266d46e4144dc96c3d18b264b0`.
Release executable SHA-256:
`b03c92dd1b3dbf2c7fd5ffd9062919ca1b06477eee1a838a67a5191229f26e1b`.
It was built with `cargo build --release -p engine --features tracy,renderdoc`;
`gpu-reuse-verify` is off. The loopback Tracy prerequisite is already integrated.

| Final gate | Result | Retained target receipt/log prefix |
|---|---|---|
| Workspace default strict all-target Clippy | PASS | `review1-default-clippy-final-r4` |
| PBR default strict all-target Clippy | PASS | `review1-default-pbr-clippy-r4` |
| Workspace default all-target nextest | 5,648 passed, 65 skipped | `review1-default-tests-r4` |
| PBR default all-target nextest | 68 passed | `review1-default-pbr-tests-r4` |
| Workspace doctests | 18 passed, 15 ignored | `review1-default-docs-r4` |
| Workspace verification strict all-target/all-feature Clippy, dev overlay | PASS | `review1-verify-clippy-r4` |
| Engine + PBR verification strict all-target Clippy, dev overlay | PASS | `review1-verify-pbr-clippy-r6` |
| Workspace exact verification-feature all-target nextest, dev overlay | 5,675 passed, 65 skipped | `review1-verify-tests-r7` |
| Excluded PBR verification all-target nextest, dev overlay | 88 passed | `review1-verify-pbr-tests-r7` |
| Production release | PASS, 27m07s | `review1-release` |

Workspace fmt, scoped PBR/dev-core formatting and whitespace checks pass. The
parent independently ran all 147 capture-controller CPU checks. Current-source
PBR tests include the actual cache's independent two-camera retention, real
poisoned early/late decisions, late revocation, atlas budget/layout transitions,
retired spot-view deferred despawn/strong-reference release, and readmission.
The final source reviews found no remaining confirmed Important/Critical finding.

The excluded-PBR command selects both `-p engine -p bevy_pbr` to enable the
engine-owned forwarding feature, then uses `-E 'not package(engine)'` to run
only PBR. Nextest rejects `package(bevy_pbr)` during filter validation because
that dependency is excluded from the workspace. Its pre-compilation exit-94
receipt is retained as `review1-verify-pbr-tests-r6`; no test failure was hidden.
The earlier Cargo feature-selection errors are also retained.

An extra nextest attempt using unrelated `--all-features` failed to link optional
static DXC: `rust-lld: error: could not open 'atls.lib': no such file or directory`.
No all-feature nextest pass is claimed. The required default and exact
`engine/gpu-reuse-verify` suites pass; the renderer verification feature itself
has no backend dependency. After overlay checks, offline Cargo metadata without
the overlay restored registry core_pipeline 0.19.0. The complete source, binary,
dataset and tool pin then passed before capture and again inside finalization.
No dependency upgrade was performed.

## Quiet production recapture

Owned evidence: `D:/Mudcrab/builds/cluster-gpu-reuse-target/capture-2153-review1`.
`standing-01/campaign.json`, `review-summary.json`, finalization log, individual
launch/guard/validation/verdict files and raw Tracy exports are retained.
The accompanying repository JSON receipt records the table and file hashes.

After the explicit lead relay `2151.3 BASELINE COMPLETE` and completion of the
lead's `cluster-publish-video-flyfast` TEST, the controller held
`timing-2153-gpu` from 11:52 until finalization completed around 12:05 on
2026-10-26. One drained hold covered the discarded warm-up, all three captures,
final identity checking and every export/verdict. Its single finally block
cleared the owned hold. Fresh gate status confirmed it absent; the lead received
`WINDOW COMPLETE`. No foreign hold or process was cleared or killed.

All four guards are quiet, with no intruders. Other-CPU means are 5.4% for the
discarded warm-up and 4.3%, 4.2%, 5.4% for the retained controls. Engine and Tracy
recorder are descendants of the same guarded `_inner` process. No recorder CPU
was reclassified or threshold changed.

Each capture is 150 seconds on the same main28 dataset/package, Vulkan RTX 3090,
1600x900, same standing pose and graphics configuration. All three retain
112,278 entities, 35,713 meshes, 29,116 transform instances, 25 resident cells and
27,106 distant stock references. The inherited diagnostic launch includes
`--allow-incomplete-assets`, local lights off, `as_played=false`, and permissive
benchmark FPS/memory thresholds. These settings were unchanged across the old
and new same-binary controls; they are not complete-content FPS acceptance.
The unchanged official verdict independently reports CONTENT PASS for each run.

Values below are GPU pass attribution in milliseconds per common render frame,
from counter-covered 30-second post-ready/full-stock tails. They are not GPU
critical-path times, end-to-end frame times or profiler-free FPS.

| Production control | Eight compute stages | Directional shadows | All recorded GPU passes | Tail frames |
|---|---:|---:|---:|---:|
| Cull + shadow recomputation forced | 0.892260 | 1.130528 | 7.225837 | 3,185 |
| Cull reuse, shadows forced | 0 | 1.136332 | 6.341410 | 3,223 |
| Combined normal reuse | 0 | 0.621172 | 5.802041 | 3,119 |

The observed saving is **1.423796 ms**, versus the previous sequence's
**1.428357 ms**: retained within 0.004561 ms (0.32%). Normal GPU pass sum is
0.213796 ms below the previous 6.015837 ms. This is one-sequence inference;
integration changes and ordinary timing variation prevent exclusive attribution
to this fix round. The data supports unchanged GPU saving, not a new speedup claim.

Both reuse runs have all eight compute counters exactly zero. The normal run's
per-view windows certify zero early/late renders for cascades 0, 1 and 2, with
stable identity/labels, full frame coverage and every refusal/audit counter zero.
Cascade 3 remains fresh: its final window has `rendered=300`,
`unknown_material=300`, `reused=0`. No declaration or eligibility exemption was
added. Normal `forced=0` and `audit_forced=0` throughout the selected windows.
The final aggregate window records 900 reuse decisions and 300 renders.

All three evidence validators pass with empty error lists. Official PERF and
OVERALL remain FAIL; diagnostic post-ready FPS is 106.30, 107.02 and 103.56.
TRAVERSAL is N/A. All retained runs have zero post-ready >33ms hitches, no
streaming failures and zero undrawn static/runtime-refused geometry. Engine exit
1 retains 32 startup commit-budget violations; no threshold was relaxed here.
The same nonfatal `SymInitialize` code 87 appears at startup. Transient asset
warnings and startup queue-full recovery are retained; there are no warning/error
lines after the explicit world-ready inventory message, and no GPU validation
error. Unrelated `settled_work.clean=false` remains visible.

## Remaining limits and integration details

- No new native byte-parity, moving-camera or portal-heavy acceptance run was
  requested or performed in this fix round. The previous proof remains historical
  evidence; current feature behavior has compile/test proof, not a new GPU replay.
- Native overflow-atlas allocation/copy validation, reclamation and camera/light
  budget transitions remain untested on a live GPU. CPU layout/cache/ECS fixtures
  and source review cover them. The many-point result above measures actual CPU
  bookkeeping helpers, not a native many-light interior renderer or GPU timing.
- Global geometry/mesh/image/pipeline revisions conservatively redraw unrelated
  cascades. Unknown dependencies remain refused. Texture writers must follow the
  documented cooperative Bevy asset mutation contract; arbitrary escaped raw GPU
  writes are not tracked automatically.
- The key does not read a whole `ClusterScene` or its new inventory Arc. It consumes
  effective origin and allocation/layout/material/selection/plan/readiness/pipeline
  epochs plus exact view/uniform words. GPU construction/update producers read
  `origin_offset`, `meshes`, `instances`, `blocks`, `terrain_blocks` and
  `certificate`. #2151.2's `inventory` field/fixtures and existing
  `LazyBatches.layout_scope` publication boundary were preserved when merging
  `94d8722a0`; inventory metadata alone is not a GPU selection input.
- Sustained profiler-free FPS, Metal and broader moddable-world behavior remain
  unproved by these TEST captures. No quality/content shortcut, push, PR or Plus
  merge was performed. No owned evidence folder expired or was deleted.


## Local commit split

- `1b21a3b9e546d75c9996916ee31cf7fcd67e7739`: production ownership/bookkeeping
  corrections, feature isolation, explicit dev overlay, test extraction and
  developer instructions.
- `1bab551555109856e9d4a9eb3c6ff1638f5b6ec2`: exactly the four SceneState/profiling
  inventory extraction files, separate from the renderer change.

Both carry `Co-Authored-By: GPT Sol <noreply@openai.com>`. Evidence/design/handoff
are committed afterward. Runtime source remains identical to all final gates and
the release; only documentation changed after capture. Nothing was pushed.
