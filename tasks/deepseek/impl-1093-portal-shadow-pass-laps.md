# impl-1093-portal-shadow-pass-laps (track: Portal; B2 follow-up 2). Measurement only, zero cost when cycle tracking is off.

Base: portal head (d826a6b1, impl-1075 + impl-1088 merged). Owned: crates/engine/src/render_timing.rs, crates/engine/src/profiling.rs,
crates/engine/src/frame_budget.rs. Same pattern and gating as the existing graph laps in render_timing.rs (add_graph_marks,
start_view, lap_view_*, graph_lap, CycleSides, RENDER_CYCLE_GRAPH_LAPS).

## Why
The 20:10 window left ~5 ms inside render/graph/views outside the Core3d set laps. Directional (sun) shadow cascades
are not separate views: bevy_pbr-0.19.0 src/lib.rs:424-444 adds per_view_shadow_pass::<EARLY_SHADOW_PASS / LATE_SHADOW_PASS>
and shared_shadow_pass::<..> (render/light.rs:2756-2784, all pub, re-exported by bevy::pbr) to the Core3d schedule,
ordered only after early/late_prepass_build_indirect_parameters and before early_downsample_depth /
main_build_indirect_parameters / Core3dSystems::MainPass. They are in no Core3d set, and nothing orders them against our
start_view, so today their time may land before start_view (outside every lap) or inside the main_pass lap.

## Work
1. Order start_view before all four shadow pass systems (and keep it before Core3dSystems::Prepass).
2. Add laps bracketing them in each camera run: a mark after early_prepass_build_indirect_parameters and before both
   EARLY shadow systems, a lap after both EARLY systems and before early_downsample_depth; same for LATE (after
   late_prepass_build_indirect_parameters, lap before main_build_indirect_parameters / MainPass). Sum both into one column
   render/graph/shadow_passes (ms per cycle, all camera runs). If the ordering makes the existing prepass or main_pass
   laps overlap the new one, make the stage laps exclusive of it and document it, keeping the identity
   camera stages + shadow_passes <= camera_views. Keep every existing column's name and position.
3. Tests like the existing ones: identity, never negative, header order, nothing recorded when tracking is off; a
   schedule test that the four bevy systems are ordered between our mark and lap (build the Core3d schedule's
   dependency graph or assert with the schedule's ambiguity/ordering API if feasible; else document why not).
4. Docs: render_timing.rs module doc, frame_budget.rs.
No Bevy fork/patch, no Cargo.toml change, no behaviour change.

## Done when
Builds must run, not only fmt. Through the gate (BUILD_GATE_OWNER=portal); `df -h /c` first, below 15 GB stop:
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
Commit only the owned files, subject `[deepseek] impl-1093: ...`; never stash, amend, rebase or push. About 50 steps.
## Report
Ordering chosen (file:line), new column, tests, check results.
