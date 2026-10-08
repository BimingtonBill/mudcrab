# Goal
Spread the asset-finalize burst that stalls one PreUpdate frame for 153-298 ms at a door crossing. Pace how many streaming-issued asset loads are in flight at once, counted per issued load, not per cell. Track: Phase 2 (engine streaming). This re-scopes the original brief tasks/impl-1060-p2-preupdate-finalize-pacing.md after a design review (fix first, confirmed by Team Lead; the user's decision 2026-10-08).

# Context
- Mechanism (confirmed against vendored bevy 0.19.1): bevy_asset drains every finished load in one PreUpdate (`handle_internal_asset_events`, an unbounded `try_iter`). Each finished load is cheap, but a whole interior's per-reference `.glb` loads are issued in one `spawn_cell` commit (streaming.rs around 2620-2648: `asset_server.load(GltfAssetLabel::Scene(0)...)` into `PendingModel`). They finish together and finalize together in one frame.
- Why the original brief fails: it counted cells in flight, but an interior destination is ONE cell (transition.rs:301-302, `request_interior(cell_id)` :392-394). So a cap of 8-16 cells never binds on the measured exterior-to-interior worst case.
- Design review: local/agents/runs/review-1062-p2-design-1060.glm.1/report.md. Evidence: local/research/portal-crossing-owner-reprofile-2026-10-08.md §1-2, §7.
- Existing pieces to reuse:
  - `model_can_spawn` (streaming.rs ~3035) is the per-load "finished or failed" probe.
  - `arm_pending_models` (~2948) and `max_model_spawns_per_frame` pace the SPAWN of loaded models. That is a separate budget: do not merge the two.
  - The unload pacer `max_cell_unloads_per_frame` (config.rs ~218) is the naming and labelling precedent.
- Other loads issued per cell in `spawn_cell`: the water flow-normal image (~2480) and the terrain quadrant textures. Count these too when they are issued per cell. If a texture handle is shared or cached across cells (it is already loaded, or loaded once by a catalog), it does not count. Say in the report which loads you count and why.

# What to build
1. New `EngineConfig` knob `max_asset_loads_in_flight`, with the CLI flag `--max-asset-loads-in-flight`. **Default 0, meaning uncapped, which is today's exact behaviour.** The real default is chosen later from an A/B; do not pick a number. Its doc comment calls it plainly a pacing knob, like the unload pacer: "paces how many streaming-issued asset loads are in flight so their completions do not finalize in one frame; total work is unchanged".
2. `spawn_cell` no longer calls `asset_server.load` for every model at once. It records each model as queued (its path and sequence) on the reference's entity. An issuing system issues queued loads in stable order (cell request order, then `PendingModel` sequence) while in-flight is below the cap. In-flight means issued and `model_can_spawn` is false. Keep a cheap running count: no full scan per frame, if the code allows it.
3. **The readiness gate is unchanged and must count queued loads as not ready.** `track_asset_readiness`, `pending_asset_instances` and the prestream wait (`PortalPrewarm::settled`) must never treat a cell as ready while any of its loads are still queued. A crossing may only proceed on a fully loaded interior.
4. Starvation: every queued load is issued eventually. At least one load is issued per frame whenever any are queued and in-flight is below the cap. A cell that despawns while it has queued loads drops them cleanly: it never issues them and does not leak the count.
5. Metrics in `StreamingMetrics`: the queued-load count, the in-flight count, and how many frames the cap was binding.

# Scope
Owned files: crates/engine/src/streaming.rs, crates/engine/src/config.rs
- Change only these and their in-file tests. Read transition.rs, portal.rs and metrics.rs for reference only.
- impl-1059 (despawn time budget) edits the same two files on another branch. Keep your changes local to the load path and the config knob, so the merge stays small.
- impl-286 Stage B (portal prestream prewarm, already merged on portal) moves the finalize and upload burst into the prestream wait. This brief bounds each frame's share of that burst. The two compose; do not add a second knob for the same thing.
- No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --no-fail-fast -E "test(streaming) or test(portal) or test(transition)", python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings
- Set BUILD_GATE_OWNER=p2. Before each acceptance command, run `df -h /c` and `df -h /d`; stop if the build's drive has less than 15 GB free.
- `cargo fmt --all -- --check` is clean.
- Tests, written to catch the change being wrong:
  (a) ONE cell with more model references than the cap (for example 40 models, cap 8) never has more than 8 loads in flight, and all 40 finish. This is the case the old design missed.
  (b) With cap 0, behaviour is unchanged: all of a cell's loads are issued in its `spawn_cell`, and the existing streaming tests pass unmodified.
  (c) A cell with queued loads is not reported ready. `pending_asset_instances` counts them, and the prestream or portal settle does not pass while any are queued.
  (d) A cell despawned with loads still queued never issues them, and the in-flight and queued counts return to 0.
  (e) A failed load frees its in-flight slot.

# Foundations check (answer in the report)
This is a concurrency cap on load starts, so be precise. Is any work skipped, hidden, or allowed past the readiness gate? What does it cost a roaming player at a small cap (pop-in)? What would break for a mod with very model-heavy cells, or another worldspace? Do not tune any number to the test data.

# Report
- What you changed and the decisions that were not obvious: which loads count, how in-flight is tracked, the issue order.
- The commands you ran and the Summary lines they printed.
- What you assumed, and the cheapest check that would show the change is wrong.
- Leave these for the lead (do not do them):
  - the A/B that picks the default. On a quiet PC, use impl-285's render-cycles.csv; the worst PreUpdate sample must fall on the crossing run, and the roaming run must show no pop-in regression. Mind the known join offset: cycle n matches frame-series row n-1.
  - two follow-ups to record with numbers: a chunked drain in vendored bevy_asset (deferred), and converter consolidation (fewer assets per cell).
