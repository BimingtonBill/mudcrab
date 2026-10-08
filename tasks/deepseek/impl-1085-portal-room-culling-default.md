# impl-1085-portal-room-culling-default (track: Portal; queue item B5)

Base: portal head at launch. Evidence: local/agents/runs/analysis-1084-portal-room-culling-demo.1/report.md (read it first;
it maps the code with file:line), local/review/188-room-culling-image-test.md.

## Goal
Room culling (crates/engine/src/room_culling.rs) is opt-in (`--room-culling`, config.rs ~191/308/335/943, default false) and
no demo, tour or perf run has ever passed it. Make it on by default wherever the world database has the room tables, with an
explicit off switch, and make its effect measurable.

## Work
1. Config: `room_culling` becomes a tri-state or bool defaulting to on; `--room-culling` keeps working, add
   `--room-culling=off` (and `=on`), help text at ~335. A DB without the tables already disables it safely
   (`room_tables_present`, room_culling.rs ~603-612, ~774-777, ~882-890): keep that, and log once at info that it is off
   because the tables are absent.
2. Measurement: the number of Mesh3d entities culled per frame (`culled_last_frame`, ~758/~1024, today read only by tests)
   reaches the profile bundle: a per-frame counter in ProfilingState's frame series (profiling.rs; follow how other per-frame
   counters are written there) and a profiling span around `cull_room_views` (like render.rs ~1074, `record_elapsed`,
   profiling.rs ~333). Zero cost when profiling is off.
3. Tests: default is on; `=off` turns it off; a DB without tables leaves it off; the counter lands in the frame series.
   Update any existing test that assumed the default off.
4. Docs: the module doc and README/docs entry for the flag, if one exists (grep `room-culling` in docs/).
Do not change the culling logic itself, the cameras it applies to (main StreamingCamera only; the portal and reflection
cameras keep full lists by design), or any launcher script.

Foundations check: data-driven (on only where the converter wrote rooms, per cell), switchable, no scene knowledge; the
image check on portal-rooms data follows as a separate job before the demo relies on it.

## Scope (owned files)
crates/engine/src/room_culling.rs, crates/engine/src/config.rs, crates/engine/src/profiling.rs, crates/engine/src/app.rs
(registration only), docs/**/*.md lines about the flag. Never git stash, amend, rebase or push. Commit only these paths,
subject prefix `[deepseek] impl-1085:`.

## Done when
Through the build gate (BUILD_GATE_OWNER=portal); check `df -h /c` before each build, below 15 GB stop and report:
`cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 80 steps.

## Report
Changes with file:line, new column/span names, test names, check results.
