# impl-285-portal-crossing-frame-attribution (track: Portal)

## Goal
Make the door-crossing frames' cost attributable. The 2026-10-08 baseline found the two
crossing frames (door-tour 895: 357.9 ms, 2666: 318 ms) unattributable — and the follow-up
analysis found WHY: `real_ms`/`frame_ms` are not main-thread wall time. Under pipelined
rendering Bevy feeds main-world `Time<Real>` from the render thread via `TimeReceiver`
(bevy_time 0.19.1 src/lib.rs:146-184, bevy_render 0.19.1 src/lib.rs:425,483), so a row's
`real_ms` is a render-paced, lagged reading: the 357.9/317.7 rows are catch-up readings of the
PREVIOUS frame's stall (the materialization frame, which carries the 26-43 ms propagate walk).
On the stall frame itself, tracked spans sum to ~33 ms and at least ~60-95 ms (plausibly
150-250) sits outside `app.update()` — the bevy_winit runner region between updates, where
nothing laps. Full note: `local/research/portal-crossing-frames-2026-10-08.md` (supersedes
`portal-crossing-frames-895-2666-2026-10-08.md`). Measurement only; no behaviour change.

## Context
- `crates/engine/src/render_timing.rs` owns the frame-cost measurement: spans `main_world`
  (First→Last), `wait_for_render_thread`, `extract`, `render_thread` with phases
  (`render/extract_commands_and_assets`, `render/specialize_and_views`, `render/queue`,
  `render/prepare`, `render/graph_and_present`, `render/cleanup`, `render/swapchain_acquire`),
  and the per-cycle `CycleSides` record (main schedule, extract, render schedule, swapchain
  wait per main-thread cycle, benchmark runs only, `track_cycles`). The render schedule of
  cycle n runs alongside main frame n+1 and is filed under n+1 — keep that convention.
- `crates/engine/src/profiling.rs` owns `ProfilingState` and the per-frame series
  (`frame-series.csv`, one row per frame, opened in `First`, columns listed at ~line 40,
  written by `write_bundle`). Phase 2's propagate amortisation (impl-1058) and Portal's
  prestream pre-warm are in flight against the same demo build: do not touch
  `portal/*`, `streaming/*` or prestream code; this brief adds measurement only.

## Design (decided; follow unless wrong, report deviations)
1. **Wall clock, not `Time<Real>`.** Because `Time<Real>` is render-thread-fed under pipelined
   rendering (see Goal), every new measurement here is its own `Instant` lap on the main
   thread. The wall cycle time is the lap from one `First` to the next. The CSV also carries
   the cycle's `Time<Real>`-derived delta next to the wall lap, so the next re-profile can
   quantify the clock skew directly instead of assuming it.
2. **Lap the untracked window directly.** Record an `Instant` at the end of `Last` (after
   `end_main_world`) and close it at the start of `First` on the next cycle. That raw gap
   includes the extract and the wait, so also record it minus those two when both ran in the
   window — the reported number must be "main-thread time outside main_world, extract and
   wait". Name: `frame/remainder`. Gate: benchmark runs (with `track_cycles`), zero cost
   otherwise.
3. **Per-frame render phases.** Extend the cycle record with the render phases
   (`extract_commands_and_assets`, `specialize_and_views`, `queue`, `prepare`,
   `graph_and_present`, `cleanup`, `swapchain_acquire`) — one value per cycle each, not
   distributions. Write them as a second bounded CSV beside `frame-series.csv`, e.g.
   `render-cycles.csv`, one row per cycle: `cycle`, `wall_cycle_ms`, `real_ms`,
   `main_world`, `extract`, `wait_for_render_thread`, `frame/remainder`, `render_thread`,
   then the seven phase columns. Leave `frame-series.csv` exactly as it is (other tooling
   reads it). Document the n → n+1 render offset in the module doc and the CSV's companion
   doc comment.
4. **The identity as a test.** A unit test (synthetic timings, no window) asserts
   `main_world + extract + wait + frame/remainder ≈ wall_cycle_ms` within a small epsilon —
   against the wall lap, never against `real_ms` — and that the phase columns sum to
   `render_thread` within epsilon. This is what makes the next re-profile trustworthy: the
   stall frame's true wall time must be accounted to within a few ms by its row before anyone
   attributes anything.

## Scope (owned files)
`crates/engine/src/render_timing.rs`, `crates/engine/src/profiling.rs` (only the parts the
cycle record and the new CSV need). Nothing else. Never git stash, amend, rebase or push.

## Done when
- Unit tests: the identity test above; a test that `render-cycles.csv`'s header lists every
  column in the documented order; a test that the remainder excludes extract/wait when both
  ran in the window; a test that nothing is recorded when `track_cycles` is off.
- Checks, through the build gate (`BUILD_GATE_OWNER=portal`):
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check`
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
- No timing run in this task: the verification re-profile happens in a booked window after
  this lands together with the prestream pre-warm.

## Report
Files changed with line ranges; the new CSV header; how a `render-cycles.csv` row joins a
`frame-series.csv` row (state the offset again); test names; check results (last lines);
deviations.

## Foundations check (required in the report)
(1) no shortcuts — measurement only, nothing hidden or capped, no behaviour change;
(2) foundationally optimised — zero cost when profiling is off, bounded memory;
(3) flexible — no worldspace/scene assumptions, works for any route;
(4) moddable — nothing vanilla-specific, the demo stays switchable;
(5) aligned — this is the 160 fps goal's measurement prerequisite.
Also: what shortcut does this take, and what would break for a mod or another worldspace?
