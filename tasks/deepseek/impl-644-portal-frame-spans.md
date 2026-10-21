# Goal
Add profile spans and churn counters to the portal's per-frame systems so a door-tour benchmark shows what the portal chain costs per frame (track: Portal; the 160 fps goal).

# Context
- Branch: the portal demo branch (Rust, Bevy 0.19). Profiler: `crates/engine/src/profiling.rs`, resource `ProfilingState` with `record_ms(name, ms)` / `record_elapsed` (read it), `increment(name, n)`, `set_gauge(name, v)`. Span names use the `area/name` style (`streaming/plan_cells`, `lod/collect`). Look at how `streaming.rs` brackets `streaming/plan_cells` and copy that pattern exactly (`Option<ResMut<ProfilingState>>` or `ResMut`, whichever streaming uses; a run without profiling must cost nothing extra).
- The analysis that chose these spans (read it): `local/agents/runs/analysis-642-post-exit-burst-spans.1/report.md`, sections "Proposed spans" items 1-6 and "Ranking".

# Scope
You own only: `crates/engine/src/portal.rs`, `crates/engine/src/transition.rs`. Change nothing else. Do not change behaviour: only timing and counters.

Add:
1. `portal/update` around the body of `update_portal` (portal.rs ~3189).
2. `portal/isolate` around `isolate_cells` (portal.rs ~2948) after its early-return guard, plus counters `portal/cells_relayered` (roots walked this frame) and `portal/nodes_relayered` (descendants walked, the `count` summed).
3. `portal/colliders` around `gate_cell_colliders` (portal.rs ~3135), plus counter `portal/colliders_gated` (colliders enabled or disabled this frame).
4. `portal/floor_probe` around `anchor_on_measured_floors` (portal.rs ~1473).
5. `portal/door_leaves` around `show_load_door_leaves` (portal.rs ~2762).
6. `portal/prestream` around `plan_door_prestream` (transition.rs ~337).
Every early `return` inside a bracketed body must still record (use a guard/closure pattern or record before each return; keep it simple and consistent).

# Done when
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings` and `cargo nextest run -p engine --all-targets` pass in your worktree (check `df -h /c` before building; below 15 GB free, stop and report).
- One unit test (portal.rs tests module) shows `isolate_cells` records `portal/isolate` and the relayer counters when a role changes (reuse the existing isolate tests' setup, e.g. near the tests that call isolate_cells).
- Never use git stash.

# Report
Files changed with line ranges; the exact span/counter names added; test names; check results; anything you could not bracket cleanly.

# Foundations check (AGENTS.md, required in your report)
State each point for your change: (1) no shortcuts - measurement only, no behaviour change, nothing hidden, capped or special-cased; (2) foundationally optimised - zero cost when profiling is off, bounded memory, scales with content; (3) flexible - no hard-coded IDs, paths or worldspaces; (4) moddable - nothing assumes vanilla data or a fixed load order, the portal stays switchable; (5) aligned with the Mudcrab goals (here: the 160 fps goal's measurement). Also answer: what shortcut does this take, and what would break for a mod or another worldspace?
