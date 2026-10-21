# Goal
Add profiler spans that measure three costs on a door-crossing frame. Today nothing measures them, so their owners are guesses. This is measurement only: no behaviour change, and zero cost when profiling is off. Track: Phase 2 (engine profiling). The spans get read in the shared timing window together with the impl-1059 and impl-1060 A/Bs.

# Context
- Why: local/research/engine-foundations/analysis-1074-despawn-cost.md (§2-§4) and local/research/portal-crossing-owner-reprofile-2026-10-08.md (§2-§4).
  - The 204.5 ms `main_schedule/Update` worst on a crossing has an unmeasured despawn share: the immediate unload path in `plan_cells` has no span.
  - The 297.5 ms `main_schedule/PreUpdate` worst is attributed either to bevy_asset's load-finalize drain or to the handle-drop cascade after the despawn. Nothing tells the two apart.
  - The 303.7 ms render-thread `extract_commands_and_assets` worst may be dominated by render-world entity removal.
- The project's profiler: crates/engine/src/profiling.rs (`ProfilingState`, `record_elapsed`, gauges, events, the spans written to cpu-spans.json and the per-frame series). Existing span example: `streaming/cell_despawn` around streaming.rs:1543, and `streaming/plan_cells` (`profiler.record_elapsed("streaming/plan_cells", ...)`). Follow those patterns and names exactly, so the existing reports and render-cycles.csv pick the new spans up.
- Bevy 0.19.1 comes from the cargo registry (C:/Users/Brannock/.cargo/registry/src); it is NOT vendored, and there is no [patch]. Do not patch Bevy. Measure around Bevy's systems from our side: ordered marker systems before and after them in the same schedule, or Bevy's own system sets.
  - bevy_asset registers `handle_internal_asset_events` in PreUpdate. Its src/lib.rs (~419-435) and src/server/mod.rs (~2056-2108) show what it does.
  - Handle-drop processing happens in `Assets<A>::track_assets` (per asset type) and the asset events systems. Find where they run.

# What to build
(a) **Immediate unload span.** In `plan_cells`, wrap the immediate unload of space-change cells (the `paces_its_unload == false` branch that calls `try_despawn` on roots; streaming.rs ~1305 and ~1419-1430) in its own span `streaming/space_change_unload`. Also record the number of roots, and the total entities in their subtrees as a gauge or event, but only when profiling is on, because counting a subtree costs time.
  Note: `try_despawn` through Commands is deferred, so the real despawn work happens when the commands are applied. Also span the command application that carries it, if you can isolate it (for example an `ApplyDeferred` point or an exclusive flush right after `plan_cells`). Say plainly in the report what each span does and does not include.
(b) **PreUpdate asset split.** Two spans:
  - `assets/load_finalize`: around bevy_asset's `handle_internal_asset_events`;
  - `assets/handle_drop`: around the `track_assets` / handle-drop systems for the asset types that matter (Mesh, StandardMaterial and our ExtendedMaterial types, Image, Gltf, WorldAsset or Scene).
  Use marker systems ordered `.before()` / `.after()` those systems in PreUpdate. Where a Bevy system cannot be ordered against, say so and measure the closest bracket you can. Also record the per-frame count of asset events drained, if it is reachable cheaply.
(c) **Render-world removal span (only if cheap):** in the render app, around the part of `extract_commands_and_assets`, or the sync of removed entities, that removes render-world entities. If this needs more than marker systems, skip it and explain why.
- Every new span compiles to nothing, or returns early, when profiling is off. Follow how the existing spans gate themselves.

# Scope
Owned files: crates/engine/src/profiling.rs, crates/engine/src/streaming.rs, crates/engine/src/app.rs, crates/engine/src/render.rs
- Measurement code and its tests only. No behaviour change. No new dependencies. No git commands.
- impl-1059 and impl-1060 edit streaming.rs and config.rs on other branches. Keep your streaming.rs edit to the span lines, so the merge stays trivial.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --no-fail-fast -E "test(profil) or test(streaming) or test(span)", python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings
- Set BUILD_GATE_OWNER=p2. Before each command, run `df -h /c`; stop if C: has less than 15 GB free.
- `cargo fmt --all -- --check` is clean.
- Tests:
  - with profiling on, a crossing-shaped plan (a space change that unloads N cells) records `streaming/space_change_unload` once, with the root count N;
  - with profiling off, nothing is recorded;
  - the PreUpdate markers run in the intended order around bevy_asset's systems. Check this from the schedule graph, or with a test that records the order the markers run in.

# Report
- Each span's name, what it brackets, and what it leaves out.
- The commands you ran and their Summary lines.
- What could not be measured from our side, and why.
Foundations check: measurement only, nothing hidden, no behaviour change; it works for any worldspace and content.
