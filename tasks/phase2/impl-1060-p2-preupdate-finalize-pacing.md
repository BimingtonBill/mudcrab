# impl-1060 — Pace the PreUpdate asset-finalize flush (crossing-owner fix 2)

Track: **phase 2** (engine runtime, streaming/assets). Branch: `phase2/preupdate-finalize-pacing`,
cut from portal tip. One worktree, one brief, independent review before any merge (hard rule 1).
Task no.: 1060. Owner: Phase 2 Dev.

## Problem (measured, 2026-10-08 re-profile)

`main_schedule/PreUpdate` worst samples: **297.5 / 173.6 / 152.7 ms** (r1/r2/r3, p99 5.7-6.5 ms —
single-frame outliers), and `render_world/main_world` worst = PreUpdate worst + only 6.7-14.4 ms:
the worst main-world cycle of every run is PreUpdate-dominated. The catch-up rows the player
feels are 546.5 / 338.1 / 308.2 ms (`local/research/portal-crossing-owner-reprofile-2026-10-08.md`
§1-2).

## Root cause (mechanism verified against vendored bevy 0.19.1)

bevy_asset drains its internal asset-event queue in PreUpdate
(`handle_internal_asset_events`, unbounded `try_iter`; bevy_asset `lib.rs:419-429`,
`server/mod.rs:2061`). One frame's cost is exactly the set of load tasks that finished since the
last PreUpdate. At the crossing, the interior's last chunk finalizes
(`asset_ready` timeline events at f1254 and f1269-1270), so the whole remaining batch of the
prestreamed interior drains in **one** PreUpdate. Nothing in the engine bounds that batch today:
streaming issues cell loads as fast as it plans them (`request_cell`, streaming.rs:472; scene
load at streaming.rs:2634, terrain quadrant loads at 2480-2485), so parallel completions
coalesce on the same frame.

## Design — throttle load starts, not the drain

Chunking inside bevy_asset means carrying an upstream patch (rejected for now: upstream PR
burden, changes in-flight asset semantics for every user). The engine-owned lever is the rate
at which streaming lets loads be **in flight**:

1. New `EngineConfig` knob `max_cell_loads_in_flight` (CLI `--max-cell-loads-in-flight`,
   default picked from the pricing window's A/B; candidate 8-16; `0` = uncapped, today's
   behaviour). `request_cell` (streaming.rs:472) refuses to issue a new cell's loads while the
   cap is reached; the refused cell stays in its current state and is retried on a later frame —
   same pattern as the existing commit budget (`finish_streaming_commit_budget`).
2. Count in-flight = cells whose root/terrain loads are issued and not yet
   `is_loaded_with_dependencies` (the readiness probe at streaming.rs:3037-3044 already exists —
   reuse its predicate; keep one counter, no polling fan-out).
3. **Readiness gate unchanged.** `track_asset_readiness` and the prestream wait
   (`PortalPrewarm::settled`) still hold the crossing until the interior is fully loaded; the cap
   stretches the wait, it must never let a crossing proceed on a half-loaded interior. Add a
   test: with the cap, a crossing held on an unsettled interior waits longer, never shorter.
4. The cap applies to streaming-issued loads only (cells, terrain quadrants, prestream). Model/
   glTF spawner loads already have their own pacing (impl-626/impl-662) — do not couple them.

Expected effect: per-frame finalize batch ≤ in-flight completions per frame (a few ms of
PreUpdate), removing the 153-298 ms worst; total interior load time grows by at most the drain
spread (~a few hundred ms), paid inside the already-existing prestream wait, not as frame
freezes.

## Tests (nextest, build-gate form)

- Cap respected: with `max_cell_loads_in_flight = N`, at most N cells hold issued-but-unsettled
  loads at any frame boundary; refused requests retry and eventually complete.
- `0` = uncapped preserves current behaviour (all existing streaming tests pass unmodified).
- Readiness gate: an interior under the cap is still `settled`-gated — no crossing, prestream
  wait holds until `is_loaded_with_dependencies` for every prestreamed cell.
- No starvation: with the cap active, every requested cell eventually loads (tour-length run
  acceptance, no cell stuck refused).

Gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features
-- -D warnings`; `cargo nextest run -p engine --all-targets` — every heavy command through
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- <cmd>` with
`BUILD_GATE_OWNER=p2`.

## Acceptance / pricing

The impl-287 pricing window (per-cycle `render-cycles.csv` from d2be20ee+) prices before/after
on portal-door-tour: `main_schedule/PreUpdate` worst falls from 297.5/173.6/152.7 ms; the
resolves the report §8 UNVERIFIED items first (which crossing carries the worst sample,
second-biggest values). Speed claim only after that A/B on a quiet PC. impl-285's `frame/remainder`
column prices the run-3 residual (~36-58 ms) — do not attribute it to this fix.

## Foundations check

- **No shortcuts:** the flush is not disabled, deferred past the gate, or hidden; loads still
  complete and the readiness gate still blocks the crossing. The wait a player spends at the
  door replaces a frame freeze of the same wall time. TEMPORARY: none.
- **Foundationally optimised:** right layer — the consumer side of the asset pipeline, where the
  unbounded coalescing happens; cost scales with concurrent loads, priced before/after.
- **Flexible:** knob is config/CLI, no content-specific numbers; works for any plugin set,
  load order, worldspace; the in-flight predicate is the existing readiness probe, not a
  special case.
- **Moddable:** a mod that loads many cells still converges (no starvation test); optional
  (`0` = off); per-cell load events and the residency log keep their meaning.
- **Aligned:** serves the 160-fps portal demo (removes the worst main-world frames); MP-safe —
  per-cell load state stays deterministic, no cross-frame coupling a second client could diverge
  on.

## Risks / coordination

- **impl-286 Stage B overlap:** Upstream's impl-286 brief stages "asset finalize + upload burst
  move from the crossing frame into the prestream wait" as Stage B, not yet implemented
  (team log 03:4x). This brief's start-throttle and Stage B's move compose (Stage B shifts when
  the batch runs, this bounds each frame's share), but check Stage B's landing status before
  implementing; if it lands first, re-price and re-scope so the two do not duplicate a knob.
- Dependency chains (material → textures → meshes) complete inside bevy_asset independent of
  the root cap; the cap bounds *root* starts, and per-frame finalize is bounded only as tightly
  as chains spread. If the window still shows big PreUpdate drains with the cap on, the follow-up
  is the upstream-patched chunked drain (rejected for now) — record the numbers, do not patch
  silently.
- PreUpdate also runs non-asset systems; the A/B must attribute via `render-cycles.csv`, not the
  catch-up row alone.
