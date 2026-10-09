# Goal
Find what owns the ~140 ms main-world Update on the frame after a door crossing, and make impl-1077's unload gauges report real counts. This is measurement only, and it decides where the next "cheaper despawn" work (A4) aims. Track: Phase 2.

# Context
- The A1 window read: local/research/engine-foundations/analysis-1103-window-read-2026-10-09.md, §1 and §4. In short, a heavy crossing (434 or 417 cells unloaded) costs:
  - frame c+1: main-world `Update` 140-190 ms while the render thread is idle. No span covers it: `streaming/space_change_unload_apply` is only 3.5-4.3 ms and `space_change_unload` 0.1-0.2 ms;
  - frame c+2: PreUpdate `assets/handle_drop` 150-200 ms, in parallel with the render world's removal (`extract_commands_and_assets`, 160-220 ms).
- impl-1077's spans are now in portal: crates/engine/src/streaming.rs (`stamp_space_unload_flush` ~1493, `record_space_unload_flush` ~1500, the gauges ~1463-1467) and profiling.rs. How to read them: local/agents/runs/review-1080-p2-crossing-spans.1/report.md.
- The gauges `streaming/space_change_unload_roots` and `_entities` are last-value gauges, so the run report reads 0. They must report the crossing's real counts: the max over the run, or a per-frame column in FRAME_SERIES_COLUMNS. Pick one and say why.
- Evidence to re-read if needed: D:/Mudcrab/evidence/p2-window-2026-10-09/cap-0/runs/ (frame-series.csv, cpu-spans.json, render-cycles.csv).

# What to build
1. **Find the 140 ms.** Read the Update schedule on portal's head and list every system that runs on the frame after a crossing and can touch 400+ despawned cells. Candidates:
   - Bevy's own despawn work: hierarchy, observers and `On<Remove>` hooks;
   - our `On<Remove>` / `RemovedComponents` readers;
   - `despawn_cells` / `despawn_subtree`;
   - door_crossing.rs, portal.rs, room_culling.rs and water systems reacting to the space change;
   - an automatic sync point that applies the despawn commands later than the 1077 bracket.
   Use the frame-series and span names to narrow it. Say which systems are plausible and why.
2. **Spans.** Add profiling-gated spans (the 1077 pattern, zero cost when profiling is off) around the candidates, so the next window attributes the 140 ms to a named owner. Cover the whole Update gap: add a bracket around Update's command application if needed. Say how much of Update the new spans cover in a unit test with a synthetic despawn of N cells, and whether wall-clock brackets can include concurrent systems (review-1080 discusses this).
3. **The gauges** report real counts, as above.
4. **Tests:**
   - a synthetic crossing with N mock cells records the new spans and a nonzero root and entity count in the run report;
   - profiling off adds no systems.

# Scope
Owned files: crates/engine/src/streaming.rs, crates/engine/src/profiling.rs, crates/engine/src/door_crossing.rs, crates/engine/src/portal.rs, crates/engine/src/app.rs
- Spans and gauges only, with no behaviour change. portal.rs and door_crossing.rs: span brackets only.
- No lod.rs. No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2. Run `df -h /c` first; stop if C: has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
Measurement only. No behaviour change in ordinary runs. Nothing hard-coded.

# Report
The candidate list with reasons; the spans added (name, file:line, what each brackets); the gauge fix; the test names; the commands and their Summary lines; what the next window should read.
