# Stable cluster draw order (2026-10-20)

Branch: `exp/stable-cluster-order`, isolated T3 worktree `t3-98125ef1`, base
`b64a6655`. Source commit: `e35d4a100c8658666a90d0b9a748a2d5bd6c142c`.
The demo checkout is untouched. No push or publication was performed.

## Outcome and implementation

Replaces scheduling-dependent append slots with GPU stream compaction in source
instance order, then cluster index order, within each existing draw group. The
same per-view arguments feed forward, camera prepass, shadows and terrain.

The eight stages are instance visibility/local prefix, instance tile prefix,
stable instance scatter, cluster selection, source-ordered draw tile prefix,
draw-group tile prefix, and small/large stable cluster scatter. Each has its own
`render/cluster_*/elapsed_gpu` span, also forwarded to Tracy GPU zones by the
vendored Bevy diagnostic recorder when built with `--features tracy`.

An immutable source-order table is rebuilt with the view's draw plan. It contains
only CPU-known instance indices and group membership; visibility and final draw
counts stay on the GPU. Each uploaded instance receives a fixed scratch slice.
Scratch zero means rejected; selected records store triangle count plus one so
valid zero-triangle clusters keep their original count/argument semantics too.
Four packed instance planes separate selected counts, ranks/draw prefixes, tile
IDs and surviving source IDs. Counts and rank/rejection sentinels are rewritten
for every current instance before compaction; no previous-frame data is trusted.
The buffer retains the original 16-byte-per-instance allocation.
Workgroup scans use ping-pong storage and uniform barrier bounds. The only
remaining atomic add is a commutative workgroup count reduction, never append.
Lists of at most 64 clusters scatter in one thread each; larger lists retain
cooperative traversal, scheduled by immutable CPU-known source IDs. This split
changes scheduling only, with the same GPU-derived final offsets and selections.

Argument group bases/capacities are unchanged. `multi_draw_indirect_count` reads
the exact GPU-prefixed count. The fixed-count path still clears arguments before
selection, leaving zeroed unused entries. Scatter writes only draw slots, keeping
the appended terrain records in the visible buffer intact. All new pipelines
join existing readiness decisions before stock geometry may hand off.

## Implementation plan and acceptance

- [x] Trace all list consumers and readiness gates; preserve existing quality and ownership.
- [x] Add GPU compaction and immutable source metadata with bounded storage layouts.
- [x] Add CPU metadata boundary/capacity tests and production-WGSL integration proof.
- [x] Independent source review after each structural change (no concrete findings; execution not covered).
- [x] Execute GPU integration test and confirm deterministic equality with CPU reference.
- [x] Execute requested FLIP probes and paired GPU measurements, including warm-up.
- [x] Capture retained-stage Tracy GPU zones.
- [x] Engine fmt, all-target/all-feature clippy, nextest; workspace handoff checks.

The ignored `cluster_stable_compaction::stable_compaction_matches_cpu_repeatedly`
test runs the production WGSL on a hardware Vulkan adapter. It checks current
draw counts, each argument and visible tuple against a CPU reference across six
epochs with changing/empty visibility and both count contracts. Cases cover
0/1/63/64/65/1301/66049 instances, empty/full draw groups, cluster counts
0/1/2/63/64/65/129/255/256/257/513,
256-instance tile boundaries, multilevel prefixes, the 65535 dispatch-row boundary,
stale buffer reuse and a terrain-tail sentinel. Readback is confined to the test.

## Validation

- D: disk preflight: 89.1 GB free, 40 GB requested, 10 GB floor retained.
- All required tool prerequisites exist, including Rust, nextest, NumPy, Pillow,
  NVIDIA FLIP and Tracy capture/export tools; GPU is an RTX 3090.
- Clean base quick build passed through the gate (jobs=2, isolated target), 12m52s.
  Preserved executable: `D:/Mudcrab/review/stable-order-20261020/engine-base-b64a6655.exe`.
- A pre-edit regression assertion confirms the old shader lacks stable scatter
  and still emits with racing atomic append. This is source evidence only.
- Current `cargo fmt --all -- --check` and `git diff --check` passed.
- Engine all-target/all-feature clippy with `-D warnings` passed after the final
  binding fix (13.44s). Production-WGSL GPU integration proof passed all four tests
  on hardware Vulkan, NVIDIA RTX 3090 driver 616.92 (0.35s). The initial GPU round
  correctly rejected simultaneous storage/indirect access to the dispatch buffer;
  cull now reads the survivor count from the prefix header instead. Final GPU proof
  exercises that corrected layout; no validation suppression was added.
- The split version's four GPU integration tests passed (0.33s), using explicit
  production binding layouts. Automatic test layouts had omitted the large
  scatter's unused uniform, so the test now checks the actual host contract too.
  Fresh workspace all-target/all-feature clippy passed (31.80s). Full scene input:
  448,799 instances, including 3,721 cooperative-scatter instances (>64 clusters).
- Clippy initially queued behind `vg7-timing` (`vg7-lead`, regression ladder
  lod-far, advertised end 2026-10-20 02:24 local). A baseline TEST attempt was
  refused with exit 4. The owner released the hold about 01:12; builds resumed.
  No hold was taken, cleared or bypassed by this task.
- Supplied FLIP baseline: valley 889 changing pixels / 885 in 2+ pairs;
  pines 3369 / 801.
- Fresh warmed base FLIP: valley **909 changing / 905 in 2+ pairs**; pines
  **3425 / 3397**. A complete preceding base shot run was discarded as warm-up.
- First seven-stage warmed stable-order FLIP: valley **0 / 0**, pines **0 / 0**, at the
  supplied script's FLIP >0.1 threshold across seven consecutive pairs each.
  No one-off above-threshold changes. First post-build stable launch was discarded
  as warm-up; both complete shot runs exited zero, with all 16 shots and no settle
  timeout. Pine foliage/terrain were visually inspected. The final shutdown-only
  statistics-buffer mapping warning also appears in the base; no shader error.
- First seven-stage GPU cost missed the target: total two culls plus five added
  stages mean **2.048150 ms** versus old **1.035952 ms**; added stages alone
  **1.047833 ms**, including **0.790200 ms** cooperative scatter. That run's
  benchmark verdict passed. This measurement triggered the bounded small/large
  scatter split, measured below.
- Split version (`split-gpu.json`): total **1.510828 ms**, added passes
  **0.509998 ms**, still above target. Four-item cooperative scatter and fewer
  instance writes (`final-gpu.json`, an intermediate iteration) lowered total to
  **1.438131 ms**, added passes **0.449874 ms**. Its warmed `probe` FLIP was again
  **0 / 0** for both poses. Folding the draw-tile scan made that stage slower
  (0.165612 vs 0.122324 ms), so it was reverted to one record per lane. Packed
  instance planes address the suspected strided-access cost. `packed-gpu.json`
  and `probe-packed` measure the retained version; files named `final-*` are
  earlier iterations. The measurements support a gain, not a causal isolation
  of memory-access cost.
- Packed version: warmed `probe-packed` FLIP is **0 / 0** for both poses, with
  no settle timeout or one-off above-threshold pixel. `packed-gpu.json` passes:
  total cull/compaction mean **1.237560 ms**, versus base **1.035952 ms**
  (**+0.201608 ms**). The six added stages alone total **0.335614 ms**, so the
  strict added-pass 0.3 ms target is missed by **0.035614 ms** in this TEST run.
  This is the retained implementation; no lower setting or cap was introduced.
  Draw-tile prefix has median 0.061440 ms but mean 0.142131 ms, with occasional
  0.42 ms samples; do not substitute the sum of medians for a total-frame median.
  All 1,931 GPU samples retain the exact prepass/opaque geometry counts recorded
  below.
- A pre-window hardware snapshot during packed warm-up/scoring setup reports
  11,632 / 24,576 MiB GPU memory, 1,980 MHz graphics clock and 68 C. This single
  snapshot does not establish clock stability throughout the run.
- Additional allocation is `4 * max_cluster_slots + order_capacity +
  prefix_capacity` bytes per view; at this package's 448,799 instances,
  6,879,646 slots and 10,164 batches, **30.096 MiB per view**, or **150.479 MiB**
  for the camera and four shadow views. This is allocation arithmetic, not a
  measured VRAM delta. The packed instance work buffer stays at its original size.
- Expanded 257/513-cluster cases caught stale local-array tail values in the
  four-item cooperative scatter. Every element is explicitly cleared per chunk;
  the resulting numeric GPU test passed before any scene run with that shader.
  The packed-plane GPU test passed all four tests (0.46s), including all fixtures.
- An engine nextest round passed 2,135 tests / nine ignored after restoring exact
  committed LF license bytes matching locked Bevy. `core.autocrlf=true` had produced
  CRLF in this checkout and failed the existing byte-pinned license test. No test
  or license content was changed. The retained packed layout then passed the whole
  workspace: **3,931 passed / 51 ignored**, 325.488s (two slow tests), including
  all engine tests. Workspace all-target/all-feature clippy with `-D warnings`
  passed on the retained source (17.23s). Workspace doctests passed, with four
  existing ignored examples. Tests used the quick profile to reuse this worktree's
  optimized dependency graph; no tests or failure thresholds were disabled.
- Base and first seven-stage benchmark fingerprints match (Vulkan RTX 3090,
  driver 616.92, 1600x900, schema 9, cache 3, same graphics). The JSON producer
  reports commit `unknown` and labels this optimized build `release`; actual
  invocations are preserved quick-profile command logs and named executable
  copies. Do not use those JSON fields to infer source provenance. Early prepass
  and main opaque each retain exactly 16,702,752 vertex invocations and 5,287,892
  clipper primitives per measured frame. No geometry was dropped for the fix.
- Fresh base GPU JSON (`baseline-gpu.json`), 1600x900 valley pose, 16,000 discarded
  frames then 2,000 measured frames: cluster cull mean 0.796641 ms / median
  0.664576 ms; instance cull mean 0.239310 ms / median 0.234496 ms. These are TEST
  observations during CPU compilation, without a quiet GUARD, therefore inference.
  The report is **passed=false** because one startup streaming commit exceeded its
  existing budget (36.486 ms); runtime invariants and renderer path otherwise pass.
  Measured vertex/primitive counts are constant across the scored frames. No
  threshold was relaxed. This is GPU attribution evidence, not FPS acceptance.
- Historical `runs/ladder/R4-demo.json`: cluster cull mean 0.755518 ms,
  median 0.670720 ms; instance cull mean 0.227824 ms, median 0.214016 ms.
  These are reference numbers, not a paired comparison or proof of the new cost.

## Retained-stage GPU attribution and Tracy

Times are milliseconds. Default quick-build JSON means are the paired measurement;
Tracy means use the final 2,000 complete eight-zone sequences in the separate
instrumented capture. They are not interchangeable runs.

| GPU zone | Default quick mean | Tracy mean |
|---|---:|---:|
| `cluster_instance_cull` | 0.265646 | 0.269152 |
| `cluster_instance_prefix` | 0.031640 | 0.032953 |
| `cluster_instance_scatter` | 0.031778 | 0.032441 |
| `cluster_cull` | 0.636300 | 0.689144 |
| `cluster_draw_tile_prefix` | 0.142131 | 0.076640 |
| `cluster_draw_group_prefix` | 0.011546 | 0.015581 |
| `cluster_draw_scatter` | 0.063577 | 0.102575 |
| `cluster_draw_large_scatter` | 0.054943 | 0.070263 |
| All eight stages | **1.237560** | **1.288749** |
| Six added stages | **0.335614** | **0.330452** |

`cargo build -p engine --profile quick --features tracy` passed through the build
gate (7m46s). The first complete capture/shot launch was discarded as warm-up.
The measured repository `tools/perf/perf_capture.py run tracy` launch used the
gate's TEST wrapper, the same package/settings/valley pose, 16,000 warm-up frames
and 2,000 benchmark frames. It exited zero; `tracy.json` passes, with a fingerprint
identical to the baseline. Capture metadata identifies source commit `e35d4a10`.
Tracy capture/export 0.14.1 successfully read the client's 0.14.0 capture.

`tracy-capture/capture.tracy` contains 17,515 complete active cluster-frame
sequences, with no incomplete events. `analyze_tracy.py` exports raw GPU events
and scores the final 2,000 complete sequences, excluding startup. Its
`tracy-capture/steady-gpu.json` records the capture SHA-256 and window. Added-stage
per-frame sums have median **0.275456 ms**, p95 **0.517120 ms**, mean **0.330452 ms**;
all-stage sums have median **1.207808 ms**, p95 **1.447936 ms**. These percentiles
are computed from frame sums, not sums of individual-zone percentiles. The
engine's concurrent `tracy.json` has 1,998 delayed timestamp samples, so this
independently selected Tracy window is close to, rather than exactly identical
to, that JSON window. No baseline Tracy capture was taken; before/after cost
comes from the default quick JSON pair. Both retained measurements slightly
miss the requested 0.3 ms added-pass aim. They are TEST attribution observations,
without a quiet GUARD, and do not establish stable 160 fps acceptance.

## Reproduction and provenance

All Cargo work used `CARGO_BUILD_JOBS=2` and
`CARGO_TARGET_DIR=D:/Mudcrab/builds/stable-order-target`, through
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run --label <label> -- ...`.
Retained checks:

```text
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
cargo nextest run -p engine --all-targets --cargo-profile quick
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --cargo-profile quick
cargo test --workspace --doc --profile quick
cargo build -p engine --profile quick
cargo build -p engine --profile quick --features tracy
```

The four-test integration executable was additionally run through `build_gate.py
test --label <label> -- <test.exe> --include-ignored
--nocapture`, so the hardware-only ignored test was actually executed.
`run_scene.py` preserves the full supplied scene command, gate TEST invocation
and benchmark additions. `packed-warmup/` is discarded; `probe-packed/` is the
retained 16-shot probe. `flicker_flip.py probe-packed valley 8` and `pines 8`
produced the retained zero/zero results; exact outputs are in
`packed-valley-flip.txt` and `packed-pines-flip.txt`.

`provenance.json` records SHA-256 hashes of the four source files and preserved
executables. It verifies the complete retained production WGSL is embedded in
both `engine-stable-packed.exe` and `engine-stable-tracy.exe`, and that current
source matches commit `e35d4a10` after line-ending normalization. A Rust comment
clarification after the default build has no runtime effect. The retained source
was reviewed independently after the packed-plane change; all required checks
and GPU proof completed before committing it. Default probe and benchmark logs
are preserved alongside the executable copies.

Artifacts: `D:/Mudcrab/review/stable-order-20261020`; target:
`D:/Mudcrab/builds/stable-order-target`. Both have ownership records.

## Foundations review

No depth bias, compare-mode change, quality reduction, cap, loading concealment or
runtime draw-list CPU readback. Existing timestamp/statistics readbacks are
unchanged; test-only readback proves ordering. Ordering is based on generic
runtime instance/cluster indices, not
Skyrim identities. Draw-group changes and streamed layout changes rebuild derived
metadata; masks and overrides retain their existing per-view authority. Data and
content licences are unchanged. Requested FLIP acceptance is met. The added-pass
cost aim is slightly missed, and quiet FPS/integration acceptance remains outside
these TEST observations. Owned build/evidence folders are retained until merge;
only this task's generated `tools/perf/__pycache__` bytecode was removed.

## Round 2 implementation and verification plan

The requested demo head `0da28a2b` is merged as `223f0045`. Keep the eight-stage
structure and use the mesh-specific terrain threshold in both LOD comparisons.

- [ ] Pin the order plan's uploaded-instance high-watermark in header word 2.
  Reject a PrepareResources append until Queue includes it in a subsequent plan;
  test a growing `instance_count` with enabled, never-ordered small/large meshes.
- [ ] Share the explicit pipeline binding table and typed CullParams upload ABI
  between production and the hardware test, retaining the eight-storage limit.
- [ ] Expand hardware proof for parent/refined LOD rejection/selection, terrain
  thresholds, old append shader visible-set equivalence, and independent views
  that share the immutable instance buffer. Run the named hardware gate.
- [ ] Run fmt, all-target/all-feature clippy, engine/workspace nextest and docs.
- [ ] Build the retained engine with the existing target and two Cargo jobs.
  Discard its first launch, rerun the two eight-shot FLIP views, inspect output,
  then score the cull/compaction GPU stages under a drained timing hold/guard.
- [ ] Independently review the final diff; record each finding, commands, results,
  limitations and local commit IDs. Do not push or open a PR.

The bounded test implementation runs separately from the shader/host changes.
Findings 8/9/11 are optional performance/ordering follow-ups, retained only if
cheap and supported by measurements; they must not expand this correctness fix.

### Round 2 implementation status (04:09 local, verification blocked)

- Finding 1: header word 2 now snapshots Queue's uploaded-instance count.
  `cull_instances` rejects ids at or above that watermark before batch/mask/mesh
  reads, while resetting counts/ranks for every current instance. The selected
  list and both scatter paths therefore exclude a same-frame append. Header
  changes participate in the normal order upload and layout-epoch invalidation.
  Hardware regression: `stable_order_ignores_instances_appended_after_planning`,
  with nine dirty-buffer epochs (including an empty order with 64 uploaded ids),
  changing N, enabled appended small/large meshes,
  group-slice poison checks and both argument contracts. Written; not yet run.
- Finding 2: `python tools/cluster_stable_gpu_gate.py` is the named hardware gate.
  It builds the exact integration binary through the shared build gate, requires
  all five named ignored GPU tests to exist, then runs `--include-ignored
  --nocapture --test-threads=1` through the gate's TEST wrapper. Missing hardware,
  software adapters, missing tests, build errors or test failures fail the gate.
- Finding 3: `gpu_contract.rs` supplies production/test STAGES and typed, Pod
  CullParams. Host buffer sizing and test buffer sizing use the type's size.
  `cull_uniform_matches_production_wgsl_layout` compares every field name/offset
  and struct size against production Naga output. The bytemuck derive feature is
  explicitly enabled, without a dependency upgrade.
- Finding 4: `stable_lod_selection_follows_projected_error_rules` covers fourteen
  hand-specified cases, refined parent acceptance/rejection, frustum rejection,
  zero triangles, exact threshold boundaries, object 8px vs terrain 2px,
  perspective/orthographic projections, instance-size rejection and disabled
  batches. Both count contracts are exercised. Written; not yet run.
- Finding 5: `stable_visible_set_matches_frozen_atomic_cull` runs the exact
  `0da28a2b` atomic-append shader fixture on the GPU and compares per-group visible
  tuples plus vertex arguments modulo order (LOD scenes and 1301-instance scene).
  Fixture bytes independently match `git show` exactly. Written; not yet run.
- Finding 6: `stable_views_share_instances_with_independent_scratch` interleaves
  three views over the SAME instance/mesh/cluster/group GPU buffers with different
  mappings, masks, projections and thresholds. Per-view buffers stay independent;
  shared buffers must remain unchanged. Written; not yet run.
- Findings 8/9/11 deferred: range-merge/order caching/tail uploads, survivor-only
  scatter, and scene-identity ordering need separate performance/identity work.
  Source order remains stable within a run, based on GPU arrival index.
- Finding 12: shared STAGES documents that draw scatter binds exactly eight
  storage buffers; production's existing capability check continues to require 8.

Initial engine binary all-feature clippy passed (3m03s), and a normal quick engine
build passed (3m17s), jobs=2, existing owned target. These checks preceded the
final test expansion and do not establish final all-target correctness. Native
rustfmt checks of all touched Rust files, Python syntax/help and diff checks pass.
No test or shader validation failure was suppressed.

The first new executable launch was discarded as warm-up: exit 0, all 16 shots,
no settle timeout, both thresholds 1px and the supplied dataset/1600x900 settings.
Valley and pines were visually inspected. The only warning at shutdown is the
previously recorded render-statistics async-map warning; no shader/validation
error occurred. Preserved executable `engine-r2.exe` SHA256:
`7d2e080c5cb920b9f77ed9f2f36bdc50e010873704de1e6c476485903008514d`.
The executable contains the complete updated WGSL. Later Rust formatting/
explicit already-unified derive feature do not change shader behavior, but a
final source build and discarded launch must precede retained measurements.

At 04:06 local, `vg7-lead` set `ulvenwald-convert` for the Ulvenwald base/cluster
conversion until 07:06. This blocks all subsequent Cargo and GPU jobs. No hold
was cleared or bypassed. The contract nextest never started Cargo; its verified waiting gate PID 12992
was stopped so no unattended build remains queued.
The final clippy, nextest, named GPU gate, retained FLIP and new GPU attribution
are pending: no round-2 zero-FLIP or performance acceptance is claimed.

After the hold clears, use the existing session target and two jobs:

```powershell
$env:CARGO_TARGET_DIR = 'D:/Mudcrab/builds/stable-order-target'
$env:CARGO_BUILD_JOBS = '2'
$env:BUILD_GATE_OWNER = 'stable-order-r2'
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run --label stable-order-r2-fmt -- cargo fmt --all -- --check
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run --label stable-order-r2-clippy -- cargo clippy --workspace --all-targets --all-features -- -D warnings
python tools/cluster_stable_gpu_gate.py
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run --label stable-order-r2-tests -- cargo nextest run --workspace --all-targets --cargo-profile quick
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run --label stable-order-r2-docs -- cargo test --workspace --doc --profile quick
```

Before claiming the append regression works, run it once with the shader guard
removed (expected failure), restore the guard and run the named gate (expected
pass). Never keep the guard-removal edit. Rebuild the exact retained source and
use `D:/Mudcrab/review/stable-order-20261020/r2_run.py` for warm-up/probe/GPU.
For GPU timing, drain an exclusive timing hold first and run its GUARD mode;
`r2_analyze.py` scores equal-count GPU sample windows and sums means, not
percentiles. Mechanism: one extra immutable high-watermark read in instance cull;
prediction: selection work unchanged after loading, with negligible change to
round-1 cull/compaction cost. Scene: same valley pose, 16000 warm-up and 2000
measured frames. The 0.3ms aim applies to the six added passes.

Both independent source reviews found no concrete blocker. The test review checked
legacy ABI/stage bindings, the LOD arithmetic, packed-plane strides, shared buffers
and boundary capacities by reading source; it did not compile or execute them.
Its cheap suggestions for an empty-plan append and poisoning list/prefix/selection/
dispatch/count scratch are incorporated. The gate now requires all five named
GPU tests (the reviewer saw an earlier one-name version). Additional nonblocking
test extensions remain: scaled/offset/radius LOD fixtures, many identical reruns,
and sharing the unchanged access flags and dispatch-dimension helper. No change
was made to the existing projected-error or largest-scale formulas.

The final blocked checkpoint records source/test changes uncommitted because
the required pre-commit Rust checks cannot run under the conversion hold. Only
the demo merge and documentation checkpoint are committed. No own active engine,
Cargo job, queued build, timing hold or worker remains. Resume after the named
conversion hold is released; its advertised end time does not clear it.


## Round 3 verification (2026-10-20)

Resumed in the same isolated worktree/branch after `ulvenwald-convert` cleared.
All seven source/test/gate hashes in `r2-provenance.json` matched the inherited
round-2 files before changes. The frozen fixture independently matched
`git show 0da28a2b:crates/engine/src/cluster/shaders/cluster_cull.wgsl` byte for byte.
Local only; no push or PR.

The initial all-target engine clippy found four test-only lint errors: one
redundant closure and three manual non-multiple checks. Replaced the closure
with its existing predicate and used `is_multiple_of` (also for the identical
shared-view mask). No production code, fixture expectations, thresholds or
failure handling changed from round 2.

Fresh checks (all Cargo through the build gate, jobs=2, existing owned target
`D:/Mudcrab/builds/stable-order-target`):

- `cargo fmt --all -- --check`: exit 0 (`r3-fmt.log`).
- `cargo clippy -p engine --all-targets --all-features -- -D warnings`:
  `Finished dev profile [optimized + debuginfo] target(s) in 6.00s`, exit 0
  (`r3-clippy-fixed.log`). Cargo reports the existing future-incompatibility
  notice for dependency `proc-macro-error2 v2.0.1`; no denied lint remains.
- `cargo nextest run -p engine --all-targets --cargo-profile quick`:
  `Summary [42.634s] 2140 tests run: 2140 passed, 13 skipped`, exit 0
  (`r3-engine-tests.log`). Hardware-only tests are run explicitly below.


The append negative control removed only `min(params.instance_count,
source_order[2])` in the instance guard, compiled through the gate, then ran
`stable_order_ignores_instances_appended_after_planning --exact --ignored
--nocapture --test-threads=1` through TEST on the Vulkan RTX 3090. It failed at
step 0 (`ordered 0 of 64`): unused argument slot 1 contained `[0, 1, 0, 1]`
instead of `[0, 0, 0, 0]`. The real test result was `FAILED. 0 passed; 1 failed`.
This proves an unplanned instance reaches draw scatter without the guard.

The artifact runner initially expected the later survivor-count assertion;
`assert_exact` correctly failed first on the stray argument. The runner's
own overly specific assertion also failed, after its `finally` restored the
production shader. The diagnostic check was corrected to accept that earlier
regression failure; `r3-negative-confirmed.txt` independently checks the original
failure log and exact restored shader SHA256 against round-2 provenance. No
production/test assertion was weakened and no failed GPU proof was relabelled
as a positive run. Original logs remain `r3-negative-test.log` / `r3-negative.log`.
The complete retained-source hardware gate is the separate positive run below.


`python tools/cluster_stable_gpu_gate.py` rebuilt the restored source through
the gate (`Finished quick profile [optimized] target(s) in 2m 34s`) and required
all five ignored hardware proofs before launching the exact integration binary
through TEST with `--include-ignored --nocapture --test-threads=1`. Result:
`test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;
finished in 14.53s`, exit 0 (`r3-gpu-gate.log`). Adapter: discrete NVIDIA GeForce
RTX 3090, Vulkan, driver 616.92. All five named GPU tests report `ok`:

- `stable_compaction_matches_cpu_repeatedly`
- `stable_order_ignores_instances_appended_after_planning`
- `stable_lod_selection_follows_projected_error_rules`
- `stable_visible_set_matches_frozen_atomic_cull`
- `stable_views_share_instances_with_independent_scratch`

This executes empty/full and 64/256/65,535 boundary scenarios, dirty append
small/large paths, both argument contracts and terrain-tail preservation,
parent/refined/terrain LOD, the real frozen old-shader differential, and three
interleaved views over identical immutable instance/mesh/cluster/group buffers.
Shared ABI/layout checks also pass. Source-order indices still describe arrival
order within a run; these proofs do not establish persistent scene-identity order.


The final post-restoration format check also exited 0 (`r3-final-fmt.log`).
Workspace all-target/all-feature clippy passed with `-D warnings`:
`Finished dev profile [optimized + debuginfo] target(s) in 2m 39s`, exit 0
(`r3-workspace-clippy.log`). The existing dependency future-incompatibility
notice is unchanged. Both earlier independent source reviews remain applicable:
production files retain their reviewed hashes, and test expectations and the
shared contract are unchanged; round 3 changes only the equivalent lint
expressions described above.


Deferred review follow-ups (unchanged scope):

- **8:** Measure range merging, cached order plans and tail-only metadata uploads
  before changing the upload/rebuild path.
- **9:** Measure survivor-only indirect scatter against the current all-instance
  scatter, preserving stable order and zero/fixed-count semantics.
- **11:** Define ordering from stable scene identity across loads/runs; current
  GPU arrival/source indices guarantee repeatability within one run only.


Workspace nextest completed successfully:
`Summary [394.976s] 3943 tests run: 3943 passed (7 slow), 55 skipped`, exit 0
(`r3-workspace-tests.log`). Command: `cargo nextest run --workspace --all-targets
--cargo-profile quick`. The slow interior/exterior streaming fixture passed in
160.585s (engine-only run: 41.809s). Concurrent converter CPU/disk load remained
present; no timeout, skip or failure policy was changed. Workspace all-targets
also includes targets beyond the historical default-workspace command, so use
these fresh totals rather than comparing only the pass count with round 1.

`cargo test --workspace --doc --profile quick` exited 0 (`r3-docs.log`): all
doctest groups report `test result: ok`, with the existing four ignored examples.


Final retained build: `cargo build -p engine --profile quick` exited 0,
`Finished quick profile [optimized] target(s) in 5m 25s` (`r3-build.log`).
Copied the binary to `engine-r3.exe`; SHA256:
`51f0b33fe89f79b25c76f5c38b46a1f5e9bf911cba4abc77038b4e752cf75adf`.
`r3-provenance.json` records every retained source/fixture/gate hash and verifies
the complete current production WGSL is embedded in that executable. No Cargo
job from this task overlaps the following scene runs.

The 05:01 pre-run gate snapshot had no holds but did have the Ulvenwald converter
PID 33804 and another owner's gated `cargo build -p converter --release`
(`preview`, PID 36260). The Ulvenwald exit marker was absent. Scene runs use the
TEST wrapper while this load persists. Under the user's round-3 instruction,
GPU attribution taken before the exit marker exists is explicitly under load;
it is not a quiet GUARD or stable-160-fps result. No other owner's hold is
cleared, bypassed or reclassified.


### Round 3 retained visual proof

The first `engine-r3.exe` launch (`r3-warmup/`) was discarded: exit 0,
all 16 shots, no settle timeout. The retained `r3-probe/` launch also exited 0,
all 16 shots at 1600x900, all `timed_out=no`, resident meshes 67,001. Both logs
confirm `cluster_error_pixels=1` and `cluster_terrain_error_pixels=1` under the
same supplied package/showcase settings. Valley and pine foliage/terrain were
visually inspected in both warm-up and retained runs. No shader/validation error;
the previously recorded shutdown-only render-statistics async-map warning
appears again.

Fresh NVIDIA FLIP, eight shots / seven consecutive pairs per pose:

- Valley: `pixels changing in any pair: 0 (0.000%), in 2+ pairs: 0`.
- Pines: `pixels changing in any pair: 0 (0.000%), in 2+ pairs: 0`.

Every pair has zero pixels above FLIP 0.1 and above 0.3. The requested **0/0 at
both views is retained**. Exact outputs: `r3-valley-flip.txt` and
`r3-pines-flip.txt`; heat maps/overlays remain alongside the retained frames.
Mean FLIP is not bitwise zero (valley 0.00002-0.00003, pines 0.00026-0.00027).

The separate GPU run uses the same preserved executable and supplied scene,
16,000 warm-up / 2,000 measured frames. `shots.rs` holds the first valley pose
for a benchmark run; it does not cycle the sixteen still poses while scoring.
`r3_run.py r3-gpu --under-load` goes through TEST, and `r3-load-context.json`
records the absent Ulvenwald exit marker at launch plus start/end/exit status.
`r3-pre-gpu-gate-status.txt` records external build/converter state. Stage means
are summed only after checking that every stage has the same nonzero sample
count; no sum of percentiles is substituted for a frame percentile.


### Round 3 GPU attribution result

**TEST under load; single-run inference.** Run 05:07:47-05:11:50 local;
Ulvenwald exit marker absent at both ends, converter still active. All build
slots were free in the immediately preceding gate snapshot; no own Cargo or
FLIP analysis overlapped the run. This is the user's explicitly permitted
under-load option, without an exclusive timing hold/GUARD. Engine window labels
say TIMING because benchmark flags are set; the actual launch wrapper and
classification are TEST. No quiet-run claim is made.

Engine exit 0, benchmark `passed=true`, 16,000 warm-up / 2,000 scored frames.
All eight GPU zones have **1,952 matching nonzero samples** (asynchronous GPU
readback does not yield a sample for every CPU frame). Stage mean milliseconds:

- `cluster_instance_cull`: **0.265181 ms**.
- `cluster_instance_prefix`: **0.032389 ms**.
- `cluster_instance_scatter`: **0.031689 ms**.
- `cluster_cull`: **0.652526 ms**.
- `cluster_draw_tile_prefix`: **0.076528 ms**.
- `cluster_draw_group_prefix`: **0.011379 ms**.
- `cluster_draw_scatter`: **0.065032 ms**.
- `cluster_draw_large_scatter`: **0.056300 ms**.

All eight cull/compaction stages sum to **1.191024 ms**.
The six added passes sum to **0.273316 ms**, below the
0.3 ms aim by **0.026684 ms in this observation**.
No claim of a quiet improvement over round 1 or stable-160-fps acceptance follows
from this disturbed single run; it does not replace the historical under-load
round-1 measurements. No setting, geometry budget or failure threshold changed.

GPU geometry statistics are constant across the scored samples: early prepass
and main opaque each retain exactly **16,702,752 vertex invocations** and
**5,287,892 clipper primitives**, matching the earlier retained geometry counts.
Scene: 176,521 entities, 67,001 mesh entities, 100 terrain patches, 10 water
surfaces. Renderer: Vulkan RTX 3090 / driver 616.92, 1600x900, schema 9/cache 3,
AutoNoVsync; object/terrain thresholds both 1 px. The older retained fingerprint
lacked the two cluster-threshold keys; other fingerprint fields match. JSON
producer fields `commit=unknown`, `dirty_worktree=false`, `build_profile=release`
do not describe this uncommitted quick build accurately; the build logs and
`r3-provenance.json` identify the actual executable and source.

No ERROR/shader/validation message occurred. Existing source-mesh missing-UV
(tangent generation) warnings and the shutdown statistics-buffer async-map
warning remain visible; they are not suppressed. Exact raw report and stage
summary: `r3-gpu.json`, `r3-summary.json`, `r3-analysis.log`, `r3-gpu.log`.

All requested round-3 verification is complete on the retained source. Prior
round-2 unchecked boxes describe the blocked checkpoint; the fresh results in
this section supersede their pending status. The independently reviewed
production source is unchanged; only equivalent test lint expressions were
fixed. Round-2 source/tests/gate and this evidence are committed locally with
the session handoff. Findings 8/9/11 remain the follow-ups listed above. No push,
PR, integration merge, lowered setting or silent fallback.
