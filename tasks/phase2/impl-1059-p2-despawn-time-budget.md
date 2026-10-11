# impl-1059 — Time-budget the crossing despawn (crossing-owner fix 1)

Track: **phase 2** (engine runtime, streaming). Branch: `phase2/despawn-time-budget`, cut from
portal tip. One worktree, one brief, independent review before any merge (hard rule 1).
Task no.: 1059. Owner: Phase 2 Dev.

## Problem (measured, 2026-10-08 re-profile)

On each catastrophic door crossing, `despawn_cells` (crates/engine/src/streaming.rs:1471)
unloads **all** retiring cells in one frame. The re-profile
(`local/research/portal-crossing-owner-reprofile-2026-10-08.md` §3) measured the `Update` span
worst at **204.5 / 144.8 / 136.4 ms** (r1/r2/r3) on the materialization frame — the timeline
records 434 / 417 cells `unloaded` on that single row.

## Root cause (code-verified)

`despawn_cells` is budgeted at `EngineConfig::max_cell_unloads_per_frame` (default **2**,
config.rs:218; the portal demo passes no `--max-unloads-per-frame`, so 2 is what runs), but the
overflow override at streaming.rs:1501-1506 sets `budget = backlog` whenever
`backlog > retire_backlog_bound(unload_radius)` = `(2r+1)²` = **49** at r=3. The bound's
doc-comment assumption — "the most a single crossing or teleport can retire at once"
(streaming.rs:1441-1447) — is false for a worldspace-changing crossing: the old worldspace's
whole loaded exterior set (434 cells) retires in one `plan_cells`, so the backlog trips the
override and the pacer un-bounds itself for exactly the frame that can least afford it. The
re-profile report's "drain over ~9 frames at the normal budget" is wrong on this point: the
normal budget is 2 cells/frame, so a plain override-drop would drain 434 cells over ~217 frames
(~1.4 s) — not an option. A per-cell time cost of ~0.47 ms (204.5 ms / 434) is the number the
budget must be set from.

## Design

Replace the cell-count budget in `despawn_cells` with a **per-frame wall-time budget**:

1. New `EngineConfig` knob `max_unload_ms_per_frame` (CLI `--max-unload-ms-per-frame`,
   data-driven, moddable — cost scales with measured work, not a magic cell count). Default
   picked from the pricing window's A/B (candidate 2-5 ms); `0` keeps today's unbudgeted
   behaviour for tests and escape hatches.
2. Despawn retiring cells until the budget is spent (check the elapsed clock after each
   `despawn_subtree`; always do at least one cell so the backlog always drains). Stable
   `cell_order_key` order is kept, so a replayed crossing unloads the same cells on the same
   frames.
3. **Overflow escape, gated on sustained growth, not on a burst.** The old override's real
   intent (streaming.rs:1444-1447) is bounding a backlog that grows faster than the pacer drains
   it under sustained flight. Keep an escape with that intent, but trip it on *drain failure*,
   not on size alone: if the backlog is above `retire_backlog_bound` **and did not shrink** this
   frame vs the previous frame's backlog (state: one `usize` in `StreamingWorld`), un-bound for
   that frame. A crossing burst (backlog 434 → drains every frame → falls below the bound in
   ~2-3 s) never trips it; sustained flight (retire rate > drain rate, backlog grows) trips it
   within a frame or two, as before. Bump `retire_backlog_overflows` only on the escape, and add
   a separate counter for crossing-drain frames so the window can tell the two apart.
4. Keep `despawn_cells` exclusive and `despawn_subtree` direct (`entity.despawn()` walks the
   tree inside the span) — the budget only works if the cost it measures is the real cost.

Expected effect at a 3 ms default: the 434-cell backlog drains in ~70-100 frames instead of one;
each drain frame pays ~3 ms extra main-thread time (post-crossing frames, already over budget)
instead of one frame paying 136-205 ms. It also shrinks the despawn share of the render-side
extract-commands burst (re-profile §4), priced by the same window.

## Tests (nextest, build-gate form)

- Crossing-shaped backlog (e.g. 120 retiring exteriors, budget ~1 ms simulated by shrinking the
  clock) drains over multiple frames, never one; `retire_backlog_overflows` stays 0.
- Sustained-growth backlog (retire rate > drain rate for several frames) trips the escape within
  a couple of frames and un-bounds exactly once.
- `max_unload_ms_per_frame = 0` preserves the old unbudgeted path (existing tests at
  streaming.rs:7288, 7368 keep passing).
- Overflow accounting: `retire_backlog_overflows` counts only escape trips.

Gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features
-- -D warnings`; `cargo nextest run -p engine --all-targets` — every heavy command through
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- <cmd>` with
`BUILD_GATE_OWNER=p2`.

## Acceptance / pricing

The impl-287 pricing window (impl-285 CSV columns from d2be20ee) prices before/after on
portal-door-tour: `main_schedule/Update` worst falls from 204.5/144.8/136.4 ms; crossing
catch-up rows (546.5/338.1/308.2 ms) shrink correspondingly; no new frame joins the over-budget
set from the drain itself. Speed claim is only "measured" after that A/B on a quiet PC.

## Foundations check

- **No shortcuts:** root cause is the overflow bound's false assumption; the fix re-derives the
  budget from measured per-cell cost. No caps on draw distance or loading; nothing hidden —
  retired cells are still despawned, just paced. TEMPORARY: none.
- **Foundationally optimised:** right layer (the streaming unload pacer, where the burst lives);
  cost scales per cell and per frame, priced before/after with the project's tools.
- **Flexible:** the budget is config/CLI, works for any worldspace and unload radius; the
  escape derives from `unload_radius`, no Skyrim IDs or fixed magic numbers in logic.
- **Moddable:** knob is a documented engine flag; behaviour is deterministic and replay-stable
  (`cell_order_key` unchanged); optional feature switchable off (`0` = unbudgeted).
- **Aligned:** serves the portal demo's 160-fps goal (removes the worst crossing frames); MP-safe
  (per-cell unload events keep their order and per-cell timeline records).

## Risks / to verify while implementing

- **Visibility during the drain:** the re-profile asserts retired exteriors are RenderLayers-
  isolated from the interior the player now sees (§7.1, stamp at streaming.rs:2533). Verify with
  a screenshot/probe in a walked run that a cell mid-drain is not drawn after the crossing; if
  any mod/interior can see the old worldspace through a portal or window, the drain order must
  hide-then-despawn (differ by one frame) — raise to the reviewer, do not silently accept.
- Drain wall-time vs prestream wait: a slower drain must not extend `PortalPrewarm::settled`
  (retiring cells are not prestream cells — confirm with a test).
- Coordination: impl-286 Stage B (Upstream's brief) touches the same crossing window from the
  asset side; land independently, price in the same window.
