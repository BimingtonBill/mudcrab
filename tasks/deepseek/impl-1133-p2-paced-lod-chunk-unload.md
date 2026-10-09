# Goal
Take the door-crossing despawn spike off the main thread's single frame. Today `lod::plan_lod_chunks` queues `try_despawn()` on every LOD chunk root that leaves range, and on an exterior-to-interior crossing that is one command flush despawning ~158,000 entities (~150-190 ms on one thread), followed by a second ~165 ms frame of asset handle drops and render-world removal. Make LOD-chunk unloads go through the same paced, hidden retire that cells already use, and measure the alternative of keeping them resident but hidden across an interior visit. Track: Phase 2 (queue A4; Team Lead's hand-out). Base: portal.

# Context
- Evidence: local/research/engine-foundations/analysis-1130-a14-crossing-trace-2026-10-09.md (frame 808: `system_commands: engine::streaming::lod::plan_lod_chunks` 97% of the frame; 158,292 despawns x 4 observers; frame 809: `Assets<T>::track_assets` ‖ `prepare_erased_assets`). Earlier: analysis-1074-despawn-cost.md, analysis-1103-window-read-2026-10-09.md (crossing = c+1 despawn + c+2 handle drops).
- The despawn sites: crates/engine/src/streaming/lod.rs around :292, :327, :386, :905 (`commands.entity(root).try_despawn()`).
- The cell pacer to reuse, not duplicate: crates/engine/src/streaming.rs `CellStatus::Retiring` (~:997), `hide_retiring_root` and `RetireHidden` (the 1117 rule: hide the root the frame it retires, restore on revive), `despawn_cells` paced by `--max-unload-ms-per-frame` (default 3.0 ms) or `--max-unloads-per-frame`. A retiring cell that comes back in range is revived, not respawned.
- Probe to reuse: `--probe-retire-visibility <dir>` (streaming.rs) counts retiring roots that still show; extend it to retiring LOD chunks if cheap.
- Bevy 0.19.0 (stay). Say whether Bevy 0.20 changes command flush or observer cost if you rely on it ("unchecked" is fine).

# What to build
1. Option A, paced despawn (the default): a LOD chunk leaving range is hidden that frame (Visibility::Hidden on its root, recorded so a revive restores it), keeps its slot as "retiring", and is despawned under the existing unload budget, sharing the cells' budget (one budget, not two). A retiring chunk that comes back in range is revived. Entities per frame and ms per frame are both bounded by the existing knobs.
2. Option B, keep hidden (behind a flag, e.g. `--lod-keep-hidden-in-interiors`, off by default): on a move into an interior space, exterior LOD chunks stay resident and hidden instead of retiring; on return they are shown at once. Optionally a timeout after which they retire under A; say what you chose.
3. Portal Dev (owner of portal.rs) agreed to one change there: `release_layer_source` (portal.rs ~:3545) is a global `On<Remove, RenderLayers>` observer, so every RenderLayers removal in the world (158k LOD entities on a crossing) pays its dispatch (~0.4 us each, ~60 ms traced), although its first line already returns for non-sources. Make it a per-entity observer instead: attach it (`entity.observe(release_layer_source)`) where a node becomes a layer source (find where `PortalLayerSource` / the sources query's component is inserted), and remove the global registration. Same timing, so behaviour is unchanged. Keep the portal.rs change to that observer and its registration only, with a test that a source node still gets adopted on removal and a non-source triggers nothing. Nothing else in portal.rs.

# Measure (no timing window needed for these; the frame-cost read is a later window)
Before every engine run, `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`; if any hold is listed, wait. Release build via the gate (`... run -- cargo build --release --bin engine`), then portal-door-tour (scripts/phase2-profile.ps1 -Scenario portal-door-tour -Repetitions 1 -SkipBuild, pwsh; CARGO_TARGET_DIR set inside pwsh if you use your own target) for: today (base build or a flag that restores the old one-flush path if you keep one for A/B), A, and B. Report per variant:
- the crossing frames' real_ms (frame-series.csv, the worst 5 frames and the frames after each crossing), only as an indication (not a timing window);
- for A: how many frames the despawn tail lasts after an exterior-to-interior crossing at the default 3 ms budget, and whether it ends before the tour's return crossing (~10 s later); what happens on a quick return (revive, not respawn);
- for B: entities held and process memory held while inside (peak_process_memory_gib from acceptance.json, entity_count, plus the LOD chunk count), and the cost on return;
- 0 invariant failures and world_ready reached in each.

# Foundations check (answer each)
No shortcuts: the chunks are out of view and hidden, so pacing the teardown hides nothing the user would see; nothing is skipped. Optimised: one budget for cells and chunks; cost per chunk bounded. Flexible/moddable: no worldspace or data knowledge; flag-gated B is off by default. Aligned: the 160 fps door-crossing goal.

# Scope
Owned files: crates/engine/src/streaming/lod.rs, crates/engine/src/streaming.rs, crates/engine/src/config.rs, crates/engine/src/portal.rs (the observer change only). Tests for: hide on retire, paced despawn under the budget, revive on return, B's keep-and-show. Checks through the gate: `cargo fmt --all -- --check`, `... run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`, `... run -- cargo nextest run -p engine --all-targets`. Check `df -h /d` before each build; below 15 GB, stop and report. Commit in upstream style (`perf(engine): ...`).

# Report
Status / Summary / Left / Next; the per-variant table; tail length in frames and seconds for A; memory and entity count held for B; the foundations answers; the commit.
