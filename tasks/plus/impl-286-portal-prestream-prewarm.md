# impl-286: prestream pre-warm — spend the prestream wait on the destination's first-visit burst

Track: **Portal**. Base: the portal branch (the branch impl-1058 was cut from, portal lineage at
99d8a737 or its current head). This is fix candidate 3 of
`local/research/portal-crossing-frames-2026-10-08.md` (§6). Fix 1 (measurement) landed as
impl-285; fix 4 (spawn-time layer stamps) landed as impl-1058. This is the remaining big wall.

## Evidence the change answers (all measured, door-tour baseline 2026-10-08, run-1 f894-class)

Each catastrophic door crossing stalls ~330-360 ms wall on the materialization frame, while its
tracked in-schedule spans sum to ~2-33 ms. The burst has three parts, all landing at the
crossing instead of during the approach:

1. **Asset finalize**: `main_schedule/PreUpdate` worst 175.3/184.2/153.5 ms across runs
   (aggregate — inference, not frame-pinned): bevy_asset's PreUpdate event flush finalizing the
   interior's asset loads in one go (streaming.json: 8220 meshes / 10604 materials / 40730
   images validated over the run).
2. **Pipeline specialization**: 44 pipelines created over 17 frames at the crossing
   (render_pipelines block; render-thread mean 28.05 ms on frames with pipeline activity vs
   10.50 without). impl-285's per-cycle render phases (`render-cycles.csv`) now show exactly
   which phase carried it.
3. **Render-side upload/extract burst**: `render/extract_commands_and_assets` worst 158-188 ms
   across runs; `wait_for_render_thread` worst 61.4-64.7.

The wall time to spend exists: `transition::plan_door_prestream` (transition.rs:338) spawns the
destination cells during the approach (doors within `DOOR_PRESTREAM_RADIUS`), and the tour
already waits in `Phase::Prestream` (demo_tour.rs:1475, 1492) before pressing on.

## Why the burst still waits for the crossing (hypothesis — Stage A verifies, do not build on it)

A pre-streamed destination is born on `DESTINATION_LAYER` (portal.rs, layer 2), and no camera
renders that layer until the portal picks a door — `portal_shows_through` (portal.rs:1919) is
true only while a door is opening, open or closing. During the approach the door is still
closed, so the destination's meshes and materials are never extracted by any view: pipelines
never specialize, the `RenderAssetBytesPerFrame`-paced uploads never run for them, and asset
finalization piles up until the crossing's frame. Stage A's job is to prove or correct each
step of that chain with file:line citations from bevy_render 0.19.1.

## Stage A — mechanism research (read-only, no engine run, no code change)

Answer, each with file:line from the vendored sources
(`C:/Users/Brannock/.cargo/registry/src/*/bevy_render-0.19.1/`, bevy_asset, bevy_app) or the
engine:

1. **What makes a pipeline specialize** in bevy_render 0.19.1's `PipelineCache`: which
   extraction/prepare step first requires it (mesh + material + view?), and which condition
   fails for the pre-streamed destination during the approach (no view whose render layers
   match layer 2?). Cite the code path.
2. **What the PreUpdate asset flush does** on a frame where many loads finish: which events
   enqueue what, where the per-asset cost sits, and whether load completion can be finalized
   earlier (polling, a warm touch, or an explicit wait) — or whether it is unavoidable once
   the assets are requested.
3. **The upload path**: which assets `RenderAssetBytesPerFrame` paces, and whether an asset no
   view references is uploaded at all before its first render. Cite where the budget applies.
4. **Pre-warm options, ranked for this engine** — expected gain, cost, risks, and the smallest
   experiment that proves each:
   (a) a warm-up view rendering `DESTINATION_LAYER` behind the prestream wait;
   (b) forcing specialization without a view (hidden instances or direct queueing);
   (c) a persistent `wgpu::PipelineCache` to disk (does wgpu 26 on DX12/Vulkan Windows support
   it, and does Bevy expose it?);
   (d) reducing variants (converter-side normalisation — likely a separate converter task;
   note it, do not do it here).

**Report**: status, verdict, summary; one section per question with citations (mark anything
unverified as UNVERIFIED); then the recommended pre-warm with expected gain and the exact
frames it should move. Under 1,200 words. File it in `local/research/`, add the INDEX row.
research-654 (`tasks/deepseek/research-654-p2-pipeline-prewarm.md`, Phase 2's, untracked) asks
the same Bevy questions — if its answers exist by the time Stage A starts, fold them in and
say so instead of redoing them.

## Stage B — implementation (only after Stage A's recommendation)

- Implement the recommended pre-warm so the destination's specialization, uploads and asset
  finalization happen **behind the prestream wait** (the tour's `Phase::Prestream` and, for
  walked runs, whatever window `plan_door_prestream` already provides) instead of on the
  crossing frame. A walked run approaching a door prestreams too — the fix must work there,
  not only in the tour; if Stage A's mechanism proves tour-only is the only honest scope, say
  so in the handoff and label it, but do not silently narrow it.
- **Budget the wait**: the pre-warm may not turn the approach into its own stall — name the
  per-frame budget from Stage A's expected cost and keep uploads inside
  `RenderAssetBytesPerFrame` pacing (no budget bypass, no raising it: that would be hiding the
  load, not moving it).
- **Generic**: any worldspace, any door, no scene or route knowledge, nothing vanilla-specific.
- Owned files: what Stage A names — expected `transition.rs`, `portal.rs`, and only if the
  warm-up view needs wiring `render.rs`/`app.rs`/`demo_tour.rs`. Touch nothing in
  `streaming.rs` (impl-1058's file) so the branches stay reviewable separately.
- **No timing run**: the AFTER numbers come from the booked re-profile window together with
  impl-1058's merge — one window prices both. Your acceptance is the checks below plus a
  before/after *count* of pipelines-specializing and upload bytes on the crossing frame from
  an unbooked smoke run, if the machine is quiet and the gate free; otherwise leave it to the
  window.

## Tests

- The pre-warm is observable: a test that after the prestream wait the destination's materials
  have specialized (or the warm-up view ran), deterministic, no GPU required where possible.
- A run without the portal is byte-for-byte unaffected (the no-portal path spawns nothing new).
- Existing transition/portal/tour tests stay green.

## Rules

- Heavy cargo only through the gate:
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- <cmd>`.
- Never `git stash` (shared by all worktrees). Path-only commits, no amend/rebase/push.
  Commit messages upstream style (`perf(engine): ...` / `feat(engine): ...`), your co-author
  line last.
- Check free disk before builds (`df -h /c /d`; below 10 GB on either, stop and report).

## Done when

`cargo fmt --all -- --check`, gated `cargo clippy --workspace --all-targets --all-features
-- -D warnings`, gated `cargo nextest run -p engine --all-targets` all pass; Stage A filed +
INDEX row; Stage B committed on its own branch from the portal lineage; handoff entry at the
top of `docs/handoff/LOG.md`. Hand to Upstream Dev for review before any merge into portal.

## Foundations check (answer in the handoff)

- **No shortcuts**: nothing hides or delays loading — the wait already exists and the work
  moves into it; uploads keep the pacing budget; nothing knows the scene or route.
- **Foundationally optimised**: the burst moves off the crossing frame into idle approach
  time — the right layer; measured by the booked re-profile (before numbers are the note's).
- **Flexible**: any worldspace/door/load order; no FormIDs or paths in logic.
- **Moddable**: no vanilla-only assumptions; derived work knows its inputs.
- **Aligned**: the catastrophic crossings are the last big wall to the 160 fps portal goal.