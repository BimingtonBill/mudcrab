# impl-1107-portal-camera-driver-laps (track: Portal; B2 follow-up 3). Measurement only, zero cost when cycle tracking is off.

Base: portal head. Owned: crates/engine/src/render_timing.rs, crates/engine/src/profiling.rs,
crates/engine/src/frame_budget.rs. Same pattern and gating as the existing graph laps (add_graph_marks,
graph_lap, CycleSides, RENDER_CYCLE_GRAPH_LAPS; impl-1075/1088/1093).

## Why
Window 2026-10-09 04:34 (local/review/portal-1093-window-2026-10-09/): render/graph/views 5.7 ms, camera runs 1.2 ms,
views_remainder 4.5 ms outside every camera run. Split it.

## Work
1. In the RenderGraph schedule's Render set (bevy_core_pipeline-0.19.0 src/schedule.rs: camera_driver ~123-206), add
   a mark immediately before and a lap immediately after camera_driver (it is pub; order .before(camera_driver) /
   .after(camera_driver) inside RenderGraphSystems::Render). New columns, appended last:
   render/graph/camera_driver (the whole camera_driver system, ms), and render/graph/render_set_other
   (views - camera_driver, never negative). Inside camera_driver, the per-run laps that already exist give the
   per-view schedule time; add render/graph/driver_overhead = camera_driver - (camera_views + shadow_views), never
   negative.
2. If other systems sit in the Render set before or after camera_driver (see bevy sources), bracket them as a
   group with their own column where the ordering allows it; otherwise leave them in render_set_other and say so.
3. Tests: header order (existing columns unchanged), the identities, never negative, nothing recorded when off.
4. Docs: render_timing.rs module doc, frame_budget.rs.
No Bevy fork/patch, no Cargo.toml change, no behaviour change.

## Done when
Builds must run. Through the gate (BUILD_GATE_OWNER=portal); `df -h /c` first, below 15 GB stop. If a timing hold is
set, the gate waits: keep working and retry.
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
Commit only the owned files, subject `[deepseek] impl-1107: ...`; never stash, amend, rebase or push. About 50 steps.
## Report
Ordering (file:line), new columns, tests, check results.
