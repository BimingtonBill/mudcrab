# impl-1088-portal-shadow-view-laps (track: Portal; queue item B2 follow-up)

Base: portal head (48374f26, impl-1075 merged). Measurement only, zero cost when cycle tracking is off.

## Why
impl-1075 split render/graph_and_present. Window 2026-10-08 20:10, portal-heavy x3 (medians, steady state):
graph_and_present 9.0-10.0 ms; render/graph/views 6.2-7.9 ms with view_count 3; but the Core3d set laps inside the
views only sum to ~1.4 ms (prepass 0.4, main_pass 0.8, post_process 0.13); submit 1.6-2.5; remainder 0.35.
So ~5 ms per frame inside `render/graph/views` is unattributed. Data:
local/impl/impl-1075-portal-graph-present-laps/target/profiling/20261008-201005-*/portal-heavy/run-*/render-cycles.csv

## Work (crates/engine/src/render_timing.rs, profiling.rs, frame_budget.rs; same pattern as impl-1075)
1. Find, in Bevy 0.19.0 sources (~/.cargo/registry/src/index.crates.io-*/bevy_core_pipeline-0.19.0, bevy_pbr-0.19.0,
   bevy_render-0.19.0), every schedule `camera_driver` (bevy_core_pipeline src/schedule.rs ~133-206) runs in the
   RenderGraph `Render` set, and everything else in that set: shadow-light views (bevy_pbr src/render/light.rs ~2093),
   EarlyPostProcess, any non-Core3d schedules, per-view overhead between schedules. Cite file:line.
2. Laps, each a render-cycles.csv column summed per cycle: shadow views separately from camera views
   (`render/graph/shadow_views` ms + `render/graph/shadow_view_count`), camera views (`render/graph/camera_views`),
   `render/graph/early_post_process`, and within each kind the Core3d stages already timed where they apply.
   Plus `render/graph/views_remainder` = views - (all parts), never negative, tested like impl-1075's identity.
   Keep the impl-1075 columns unchanged (the 20:10 window's numbers stay comparable). If view_count today counts
   shadow views or not, say which and keep its meaning.
3. Note how the existing laps tell a camera view from a shadow view (a component on the view entity, e.g. the
   light's view marker in bevy_pbr), with file:line.
4. Docs: render_timing.rs module doc and frame_budget.rs for the new columns. Tests: header order, the identity,
   nothing recorded when tracking is off.
No Bevy fork/patch, no Cargo.toml change, no behaviour change.

## Scope (owned files)
crates/engine/src/render_timing.rs, crates/engine/src/profiling.rs, crates/engine/src/frame_budget.rs. Never git stash,
amend, rebase or push. Commit only these paths, subject `[deepseek] impl-1088: ...`.

## Done when
Builds must compile: run them, don't only format. Through the gate (BUILD_GATE_OWNER=portal); `df -h /c` first, below 15 GB stop:
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
If the gate holds builds (a timing window or conversion), keep coding and retry; don't stop with an unbuilt change.
About 70 steps.

## Report
Bevy structure found (file:line); new columns; how shadow vs camera views are told apart; tests; check results.
