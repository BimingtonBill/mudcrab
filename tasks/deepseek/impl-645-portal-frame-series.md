# Goal
Write a per-frame series of selected profile spans, keyed by the frame number the profile timeline uses, so a benchmark's slow frames can be attributed frame by frame; and add spans to the light budget and the physics step (track: Portal; the 160 fps goal).

# Context
- Branch: the portal demo branch (Rust, Bevy 0.19). Profiler: `crates/engine/src/profiling.rs` (`ProfilingState`, `record_ms`, the timeline `event(...)` with its `frame` counter, `write_bundle` writing `cpu-spans.json`, `streaming.json` etc.). Benchmark frame times: `crates/engine/src/metrics.rs` (`write_frame_times`, `frame_times_csv`, `--benchmark-frame-times`; frame times are sampled from the real clock).
- Design (read it first, follow it unless it is wrong; say where you deviate): `local/agents/runs/analysis-642-post-exit-burst-spans.1/report.md`, sections "Proposed spans" items 7-9 and "Per-frame series".
- Another coder adds spans named `portal/update`, `portal/isolate`, `portal/colliders`, `portal/floor_probe` in portal.rs at the same time; your series must include those names as columns even though this branch does not record them yet (the columns are then empty/0).

# Scope
You own only: `crates/engine/src/profiling.rs`, `crates/engine/src/metrics.rs`, `crates/engine/src/lights.rs`, `crates/engine/src/physics.rs`, `crates/engine/src/shadow_layers.rs`. Change nothing else.

Add:
1. `lights/budget` around `budget_lights` (lights.rs ~458).
2. `shadow/relayer_mark` around `reextract_relayered_meshes` (shadow_layers.rs ~141) plus gauge `shadow/relayered_meshes`.
3. `physics/step`: the rapier fixed-schedule work bracketed by two marker systems (before the rapier sync set, after its writeback; physics.rs ~266-343). If that is not possible cleanly, say why and skip it.
4. The per-frame series in `ProfilingState`: one row per frame, opened in `First` (not `Last`), with `frame` (the same counter the timeline's events carry), `real_ms`, and these columns: `portal/isolate`, `portal/colliders`, `portal/update`, `portal/floor_probe`, `streaming/plan_cells`, `streaming/frame_commit`, `lod/collect`, `scene/spawn_batch`, `lights/budget`, `physics/step`. Bounded (e.g. 100,000 rows). Written by the bundle as `frame-series.csv` beside `cpu-spans.json`. Only when profiling is on; no cost otherwise.
Keep the existing `frame,ms` CSV of `--benchmark-frame-times` unchanged (other tools read it).

# Done when
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, `cargo nextest run -p engine --all-targets` pass in your worktree (check `df -h /c` before building; below 15 GB free, stop and report).
- Unit tests: a series row records spans recorded in that frame (not the previous one), rows are keyed by the timeline frame number, the CSV header lists the columns, the bound holds.
- Never use git stash.

# Report
Files changed with line ranges; the CSV header; how frame numbers align with the timeline and with `--benchmark-frame-times` rows (state the warm-up offset); test names; check results; deviations from the design.

# Foundations check (AGENTS.md, required in your report)
State each point for your change: (1) no shortcuts - measurement only, no behaviour change, nothing hidden, capped or special-cased; (2) foundationally optimised - zero cost when profiling is off, bounded memory, scales with content; (3) flexible - no hard-coded IDs, paths or worldspaces; (4) moddable - nothing assumes vanilla data or a fixed load order, the portal stays switchable; (5) aligned with the Mudcrab goals (here: the 160 fps goal's measurement). Also answer: what shortcut does this take, and what would break for a mod or another worldspace?
