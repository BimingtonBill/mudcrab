# impl-1125-portal-flush-lap-and-skip-test (track: Portal; queue B2d, measurement only)

Base: portal head (includes impl-1107 laps and impl-1121 render_skip.rs).

## Why
analysis-1123 (report: local/agents/runs/analysis-1123-portal-driver-overhead-flush.1/report.md) found that each
camera's Core3d run costs ~1.4 ms that no lap sees (`render/graph/driver_overhead`), the same for the quarter-size
reflection as for the main view: 3-view cycles 4.3-4.8 ms, 2-view cycles 3.0-3.2 ms in the same runs. Candidates:
(a) the executor's end-of-run deferred flush (every RenderContext encoder's finish(), bevy_render-0.19.0
render_context.rs:106-126, run at multi_threaded.rs:293-303 because Core3d has no ApplyDeferred), or (b) per-run
executor/schedule bookkeeping. This job only MEASURES which; it changes no rendering.

## Do
1. **Flush lap.** In the cycle-tracking path only (same switch as the other Core3d laps, render_timing.rs
   `core3d_marks()` ~539-599): make the end-of-run flush lap-visible by adding Bevy's `ApplyDeferred` system at the
   end of Core3d (after the last existing lap, after upscaling and ui_pass) followed by a lap, so a new column
   `render/graph/flush` holds that time per frame (summed over views like the other view laps). If the final
   auto-flush must be disabled for the lap to see it (executor `set_apply_final_deferred(false)`,
   bevy_ecs-0.19.0 multi_threaded.rs:318-320 / Schedule::set_executor schedule.rs:544), do that ONLY when cycle
   tracking is on, and prove in a test that commands still run in the same order (one ApplyDeferred at the very
   end is equivalent to the final flush). Default runs (no cycle tracking) must be byte-for-byte the same schedule.
   `driver_overhead` stays = camera_driver - (camera_views + shadow_views); add `driver_overhead_unflushed` =
   driver_overhead - flush.
2. **Encoder counter.** A column `render/graph/command_buffers`: `PendingCommandBuffers` length read after
   camera_driver and before submit (render_timing.rs's lap_camera_driver slot ~524-526; bevy_render
   render_context.rs:37-60). Count only; no behaviour change.
3. **Skip-join test (from the impl-1121 review).** A unit test for crates/engine/src/render_skip.rs
   `extract_skipped_views`: a main-world camera with a RenderEntity, marked in SkippedViews, ends up in
   SkippedRenderViews as that render entity; an unmarked or entity-less camera does not; and the plugin registers
   `drop_skipped_views` in RenderSystems::Render before render_system (assert via the schedule graph if practical,
   otherwise a headless App with RenderApp sub-app driving extract then render). No production change unless the
   test finds a bug (then report it, fix minimal).
4. Docs: the column list in render_timing.rs and the history note (as 1107 did); one line in the module docs saying
   flush is measurement-only.

## Constraints
- Measurement only: no change to what is drawn or when; default (non-cycle) schedule unchanged.
- Foundations: no shortcuts, nothing scene-specific; columns per frame, summed over views.
- Bevy stays 0.19.0; no vendoring, no [patch].

## Scope (owned files)
crates/engine/src/render_timing.rs, crates/engine/src/render_skip.rs (tests only, unless a bug), and the profiling
CSV writer if the columns are declared elsewhere (find it; say which file). Never stash, amend, rebase or push.
Commit only these paths, subject `[deepseek] impl-1125: ...`.

## Done when
Builds through the gate (BUILD_GATE_OWNER=portal); `df -h /c` and `df -h /d` first, below 15 GB stop and report.
If a timing hold is set the gate waits: keep working and retry.
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 100 steps; if longer, commit what passes and report what is left.

## Report
Design (file:line), how the flush lap works and why default runs are unchanged, new columns, tests, check results.
