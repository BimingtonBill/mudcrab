# impl-1075-portal-graph-present-laps (track: Portal; R2 of the 160 fps front)

START ONLY AFTER impl-196 / impl-1072 / impl-1073 are merged into portal (base = portal head at launch).

## Goal
Split `render/graph_and_present` (8.85 ms p50, 57% of the portal-heavy render thread, only 0.94 ms measured inside)
into laps, so the next timing window can decide between "encoding scales with views" and "submit/present blocking".
Measurement only: no behaviour change, zero cost when profiling is off.
Evidence and reasoning: local/research/portal-heavy-render-thread-2026-10-08.md sections 2 and 4 (R2) and
"Measurement note for the next worker". Same rules as impl-285 (tasks/deepseek/impl-285-*.md, its handoff in
docs/handoff/LOG.md): wall laps from `Instant`, keyed to the cycle, written into the render-cycles series.

## Context
- crates/engine/src/render_timing.rs: `GRAPH_AND_PRESENT` (~96), `lap()` (~562, ~682), phase columns
  (`CycleSides::graph_and_present_ms` ~274, filing ~703), tests ~734, ~1066. crates/engine/src/profiling.rs writes
  render-cycles.csv (header built from the same constants). crates/engine/src/frame_budget.rs ~31-36 documents that
  submit/present waits land unsplit in graph_and_present.
- Bevy 0.19.1 (`~/.cargo/registry/src/index.crates.io-*/bevy_render-0.19.1/src/renderer/mod.rs` ~69-130):
  `render_system` runs the main render schedule (span `main_render_schedule` under the `trace` feature), then builds an
  encoder for screenshots/readbacks, `render_queue.submit(...)`, then presents each window (`present_frames`).
  Find in the 0.19.1 source how the graph/schedule is organised (system sets or nodes for GPU preprocessing,
  per-view main passes, post-processing; where command buffers are encoded and where they are submitted) and cite
  file:line in the report.

## Work
1. Pick the least invasive way to lap inside it, with no fork or patch of Bevy: e.g. our own systems ordered
   between Bevy's render sub-schedule sets, or a tracing layer that times Bevy's own spans by name (only installed
   on profiling runs), or wrapping at our plugin boundary. Report the options you found and why you chose one.
2. Laps, each its own column in render-cycles.csv and summed per cycle: GPU-driven preprocessing; main passes
   (per view if the structure allows it cheaply; otherwise all views together plus a per-view count column);
   post-processing; `queue.submit`; `present`; and a remainder so the parts plus remainder equal graph_and_present
   (test it, as impl-285's identity test does). A part you cannot split is reported as such, not guessed.
3. Update frame_budget.rs's doc (~31-36) and the render_timing.rs module doc for the new columns; tests for the
   header order, the sum identity, and zero work when tracking is off.

Foundations check: measurement only; no caps, settings or scene knowledge; gated on profiling runs.

## Scope (owned files)
crates/engine/src/render_timing.rs, crates/engine/src/profiling.rs, crates/engine/src/frame_budget.rs, and
crates/engine/src/app.rs or render.rs only for plugin/system registration (a few lines). Nothing else. Never git
stash, amend, rebase or push. Commit only these paths, subject prefix `[deepseek] impl-1075:`.

## Done when
Through the build gate (BUILD_GATE_OWNER=portal); check `df -h /c` before each build, below 15 GB stop and report:
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 80 steps; if longer, commit what passes and report what is left.

## Report
Bevy 0.19.1 structure found (file:line); method chosen and why; new columns; test names; check results; what a
timing window should run to read it (scene portal-heavy, which columns to compare).
