# Cluster command encoding and upload investigation, 2026-10-23

Scope: task #2130, base `085c692c`, branch `phase2/perf-encoding`. The investigation uses source and the existing `D:/Mudcrab/review/drafts/PERF-INT/capture8.log` and `capture9.log`. No engine, GPU capture, or timing run was started by this task. Source line numbers in the inventory below refer to the base commit unless an external dependency path is given.

## Command inventory

For **V > 0 eligible views with nonempty cluster draw plans and ready compute pipelines**, the cluster path adds:

- **One command encoder per frame**, shared by all views. `ClusterRenderPlugin` registers one root `run_cluster_cull` system (`crates/engine/src/cluster/render.rs:879`). Its `RenderContext` lazily creates one encoder; it does not flush between views (`vendor/bevy_render-0.19.0/src/renderer/render_context.rs:252`). Queue functions build phase items, not encoders.
- **Eight compute passes per frame**, already shared across all views, with **8V dispatch API calls**: seven direct calls and one indirect call per view (`render.rs:3218`, `:3227`, `:3254`, `:3269`, `:3283`, `:3294`). A render-origin shift adds one global pass, direct dispatch and 32-byte parameter upload (`:3133`, `:3146`, `:3159`, `:3166`). In a settled frame without rebasing, that extra pass is absent.
- **Zero dedicated cluster render passes.** Cluster draw commands join Bevy's existing forward, prepass and shadow phases (`render.rs:842`). For each view, every drawable group issues one `multi_draw_indirect_count`, or one `multi_draw_indirect` on the fixed-count path (`:3735`, `:3750`, `:3761`). These are API call counts, not a fixed number of individual GPU draws. The GPU count is data-dependent; fixed-count draws include unused zero-vertex arguments.
- A camera group draws in the main pass and, if enabled and ready, once in the prepass. The prepass ONCE guard prevents drawing the same cluster group again in the late prepass (`:3704`, `:3740`). Shadow commands lack this ONCE guard and can draw in each active early/late shadow pass (`:4048`, `:4060`; Bevy `bevy_pbr-0.19.0/src/render/light.rs:2820`).
- One count-buffer clear per eligible view, plus one full argument-buffer clear only on the fixed-count path (`render.rs:3204`). Count/argument construction and all eight stages still run when the world and camera are unchanged.

Existing stock passes own rendering: `bevy_core_pipeline-0.19.0/src/core_3d/main_opaque_pass_3d_node.rs:83,91`, `src/prepass/node.rs:190,206`, and `bevy_pbr-0.19.0/src/render/light.rs:2834,2843`. Directional cascades share their camera's shadow render context (`light.rs:2784,2793`); point/spot shadow maps are auxiliary views (`:2762`). Encoder count follows render-system invocations, not a one-to-one view rule.

The streamed-world camera already receives `DepthPrepass` independently of cluster settings (`crates/engine/src/app.rs:2896`); cluster prepass participation fills existing phases rather than enabling another camera pass. With no eligible views, the eight cull stages do not open passes; origin shifting remains separately conditional.

For 13 genuinely eligible cluster views, the formula gives **104 dispatch calls in eight passes and one encoder**. Capture8 does not establish 13 eligible cluster views. Its settled queue reports count one camera plan and four shadow plans per frame: 600 skipped camera plans and 2400 skipped shadow plans over 600 frames (`capture8.log:668`, repeated later). The reported 13/f is a shadow-visibility query invocation count (`:1828`). The camera has 11 draw groups; the latest rebuilt shadow plan has eight. The latter gauge cannot establish every shadow view's group count. Five eligible plans would imply 40 dispatches and, before this fix, five 128-byte uniform writes per frame; this is an inference, not a measured cull-view count.

## What the 1.907 ms submit system contains

The workspace patches Bevy render to its vendored implementation (`Cargo.toml:62`). `RenderContextState::queue` hands over each system's encoder in submission order (`vendor/bevy_render-0.19.0/src/renderer/render_context.rs:270`). `PendingCommandBuffers::take` finishes queued encoders (`:206,215`). Parallel finishing dispatches one task per unfinished encoder, waits, then restores their original order (`:123,159`). The zone field `encoders=24` counts that batch's unfinished encoders, not cluster encoders or visibility views.

`bevy_core_pipeline-0.19.0/src/schedule.rs:228` calls `pending.take()` at line 232, then `queue.submit` at line 238. The nested `queue_submit` span starts at line 236, after finishing. Therefore the system's **1.907 ms includes 1.058 ms parallel finish** (`capture8.log:1829,1836`). The remaining approximately **0.849 ms includes submission and surrounding work**. It is not a measured backend duration. These costs also sit inside the enclosing `render_system`/RenderGraph scope, so they must not be added to that inclusive total.

With wgpu 29.0.4, finishing replays recorded commands and performs validation, tracking and native command encoding (`wgpu-core-29.0.4/src/command/mod.rs:1033,1069,1096,1233`). More cluster commands can therefore increase finish time in existing stock encoders. Submission performs per-command-buffer and resource checks, initialization and transitions (`src/device/queue.rs:1231,1295,1830,1865`; `src/command/memory_init.rs:168`), and lifetime maintenance that can retire earlier staging buffers (`queue.rs:1465,1475`). These mechanisms are plausible contributors, without measured shares.

`write_buffer` immediately allocates a staging buffer and copies/flushes its payload (`wgpu-core-29.0.4/src/device/queue.rs:545-550`), then records barriers and a copy (`:689-730`). Copies aggregate in one pending-writes encoder, while staging allocations remain separate. Submission prepends that encoder (`:1419`). Redundant per-view writes add staging churn, not one submitted buffer per view.

Capture8 confirms Vulkan with indirect count enabled (`capture8.log:6,12`). The count path reaches native Vulkan `cmd_draw_indirect_count` (`wgpu-core-29.0.4/src/command/render.rs:2948`; `wgpu-hal-29.0.4/src/vulkan/command.rs:1221`). Fixed-count per-slot emulation is a separate conditional path and is not a proven explanation here. Vulkan HAL batches command buffers into one native submission (`wgpu-hal-29.0.4/src/vulkan/mod.rs:1266,1292,1343`).

Bevy creates and submits another screenshot/readback encoder after RenderGraph (`vendor/bevy_render-0.19.0/src/renderer/mod.rs:83-100`). That encoder is outside the 24-encoder batch and is not cluster-specific.

The exact 1.14 ms release clusters-on/off difference remains unattributed. Missing evidence: matching nested queue-submit timings, encoder owner inventories, active cull views, dispatch/draw counts and upload counts. Tracy values are diagnostic rankings, not release timing acceptance.

Capture9 arrived during verification. Its settled table reports render_system 3.757 ms, submit_pending_command_buffers 2.385 ms and parallel finish 1.287 ms with 24 encoders (`capture9.log:1470,1476,1482`). The approximately 1.098 ms arithmetic remainder has the same attribution limit. Its queue report again counts one camera and four shadow plans (`:1387`), while unchanged selections total 3000 reused over 600 frames (`:1393`). This is another profiler observation, not a matched before/after comparison of this fix or a new release baseline. It supplies no nested queue-submit split that resolves the remaining cause.

## Settled work and upload guards

- Mesh payload/metadata writes occur only for ready unappended blocks and meshes not already pooled (`render.rs:1237,1253,1262`). Instance, absolute-position and mesh-uniform block uploads occur only for nonempty appended instances (`:1369`).
- Material-slot mesh uniforms change only when the resolved slot changes (`:2556`). Origin-shift parameters are written only when the applied origin differs (`:3065,3133`).
- Source-order uploads use shared identity or exact word equality (`:1134,1143`). Batch tables compare exact bytes (`:1152,1153`).
- Selection masks skip unchanged domains/selections and compare exact bytes before upload (`crates/engine/src/cluster/handoff.rs:3993,4000,4023,4068`). Terrain clips compare exact bytes (`:2657,2662`).
- Before this task, **cull uniforms unconditionally uploaded 128 bytes per eligible view per frame** (`render.rs:3196,3203`). Their epoch changes every run, but production `cluster_cull.wgsl` never reads that member. CPU epochs independently protect current-frame arguments and prepass uniqueness.
- Indirect args, counts, instance lists, prefixes, selected slots and dispatch args are rebuilt on the GPU every frame. They are not settled per-frame CPU uploads. Moving lights/camera, projection, viewport or settings can still change shader inputs in an otherwise static world. Stock TAA alone does not dirty these parameters: Bevy jitters a local copy of the projection for its view uniforms, while the cluster builder reads the unmodified ExtractedView (`vendor/bevy_render-0.19.0/src/view/mod.rs:1041-1058`). A custom camera that changes ExtractedView matrices does require a fresh upload.

## Exact reduction and proof boundary

The implementation keeps the last uploaded cull uniform with each `ViewGpu` allocation. At the upload boundary it normalizes the unused ABI epoch, compares all remaining bytes exactly, and writes only changed data. A new allocation always uploads. CPU cull epochs, clear commands, dispatches, shaders and draw order remain unchanged. A reporting-window counter records actual cull uniform uploads.

The predicted saving is **one staging allocation/copy/write and 128 payload bytes per unchanged eligible view per frame**. It saves no render passes, compute passes, dispatches, draw calls or cluster encoder. This is a mechanism-based inference; no millisecond saving has been measured.

CPU regressions cover first upload, repeats, changed epoch, every other uniform byte, allocation-cache reset, NaN payloads and signed zero. A Naga semantic check verifies the production shader does not read epoch or load the whole uniform. This does not certify arbitrary externally replaced shaders that start reading epoch. The validation receipts below cover compilation and CPU behavior; they do not establish runtime visual or performance acceptance.

Riskier GPU-dependent ideas remain only in the uncommitted worktree-root `ENCODING-NOTES.md`: count-clear removal, exact settled cull reuse, stage fusion, render bundles, empty readback suppression and late shadow draw deduplication. None is implemented or accepted here. Performance, visual equality and portal-heavy runtime acceptance remain unverified.

## Validation receipt

The retained code passed the required engine checks through the build gate and unique target `D:/Mudcrab/builds/perf-encoding-target`, with jobs=2:

- `cargo fmt --all -- --check`: exit 0 (`worker-fmt-check.log`).
- `cargo clippy -p engine --all-targets --all-features -- -D warnings`: exit 0 (`worker-clippy.log`).
- `cargo nextest run -p engine --all-targets`: exit 0; **3091 passed, 13 skipped**, 54.918 seconds test execution (`final-nextest.log`).
- Regression proof: one pass/two expected failures before the fix (`worker-red-lib.log`); all five focused upload/semantic/layout tests passed after the fix (`worker-green-contract.log`).
- Additional `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0 (`workspace-clippy.log`).

Broader `cargo nextest run --workspace` compilation was stopped twice for capacity before tests executed. The stops are recorded in `workspace-nextest-capacity-stop.log` and `workspace-nextest-capacity-stop-2.log`; exit 101 reflects deliberate termination of owned compiler processes, not a Rust diagnostic or test assertion. The second stop left 10.53 GiB free on D:. The sequential `cargo test --workspace --doc` command consequently never started. Whole-workspace test/doc acceptance is **unverified**; this bounded result supplies the task's required engine acceptance, not an integration handoff or merge gate.

Two independent Codex reviews found no actionable blocker. The final review notes the additional immutable view scan as unmeasured CPU work. The unchanged source SHA-256 values are recorded in `worker-result.log`; no source changed after the passing checks. Cargo still reports the existing `proc-macro-error2 v2.0.1` future-incompatibility notice.

The first full-suite attempt never started because the build gate timed out on low disk. Reversible compression of owned build caches enabled the successful retry. Final linking briefly reached **9.82 GiB free on D:**, below the project's 10 GiB hard floor; the stop audit found compilation had already finished. Capacity was recovered by further compression. No files were deleted and no gate floor/configuration/hold was changed. Detailed receipts are in `capacity-recovery.log` under the owned target. This resource incident is separate from renderer correctness or performance evidence.
