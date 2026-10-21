# impl-547 — the commit-budget metric with and without distant LOD

Branch `phase2/lod-engine-terrain`, base `7e23fa4`.

## What the metric means, before and after

Standing on `main` (`git show main:crates/engine/src/streaming.rs`), the frame that
`StreamingMetrics::commit_frames`, `max_frame_commit_micros`, `commit_budget_micros` and
`commit_budget_violations` describe is one system's span:

| | `main` | branch as found | after this change |
|---|---|---|---|
| LOD **off**, window starts | `collect_cells` first statement, `frame_commit_started = Instant::now()` (`main:269`) | `begin_commit_budget`, first system of `StreamingSet`, i.e. **before** `plan_cells` (`streaming.rs:99` as found) | `collect_cells` first statement (`streaming.rs:383`) — same as `main` |
| LOD **off**, window ends | after the commit loop, `frame_commit_started.elapsed()` (`main:376`) | `lod::finish_commit_budget` (`lod.rs:805` as found), scheduled after the whole `StreamingSet` | after the commit loop in `collect_cells` (`streaming.rs:496`) — same as `main` |
| LOD **off**, work inside | dequeue + stale checks + terrain validation + `spawn_cell` + per-commit accounting | cell plan, cell commits, asset/surface readiness, render-origin rebasing, lifecycle validation, LOD plan, LOD commits | `main`'s set, unchanged |
| LOD **off**, who books the frame | `collect_cells` | `LodPlugin::finish_commit_budget` (installed even with the flag off, `app.rs:154` as found) | `collect_cells` via `record_frame_commit` |
| LOD **off**, budget systems in the schedule | none (the resource does not exist) | `begin_commit_budget` + `finish_commit_budget` | none — both are registered by `LodPlugin`, which is not installed |
| LOD **on**, window starts | n/a (no LOD) | `begin_commit_budget` before the cell plan | unchanged |
| LOD **on**, window ends | n/a | `lod::finish_commit_budget` after the LOD commit loop | unchanged |

With LOD on nothing about the window changed; what changed is that it is now documented as a new
baseline (doc comment on `CommitBudget`, `streaming.rs:47-69`, and one sentence in
`docs/roadmap/02-profiling.md`). With LOD off the window is `main`'s again, because the tier that
would have widened it is not installed.

## The audit claims

1. *"The stopwatch now starts before cell planning (around `streaming.rs` 84-100)."* **Confirmed.**
   `begin_commit_budget` (`streaming.rs:99-102` as found) was the first system of the chained
   `StreamingSet` and was followed by `plan_cells`, so the window opened before the cell plan.
2. *"It now ends after readiness, render-origin rebasing, lifecycle validation and LOD planning
   (around `lod.rs` 805)."* **Confirmed.** `finish_commit_budget` sat third in `LodPlugin`'s chain
   (`lod.rs` 156-179 as found), and that whole chain is `.after(StreamingSet)`, whose members were
   `begin_commit_budget, plan_cells, collect_cells, track_asset_readiness,
   track_surface_readiness, update_render_origin, validate_streaming_lifecycle`, so it ran after
   `plan_lod_blocks` and `collect_lod_blocks`.
3. *"`LodPlugin` is added even when LOD is disabled (`crates/engine/src/lib.rs` or `app.rs`)."*
   **Confirmed for `app.rs`, with one correction:** `crates/engine/src/lib.rs` registers no plugins
   at all — it is eleven lines of `pub mod` and `pub use app::run` — so the only install site was
   `app.rs:154`, an unconditional `.add_plugins(LodPlugin)`.
4. *"So the commit-budget numbers changed meaning against runs on `main`."* **Confirmed**, and
   worse than stated: because `LodPlugin` was installed unconditionally, an LOD-**off** run got the
   wide window too, so the default configuration's numbers were already incomparable with `main`.

## What changed

- `streaming.rs`
  - `CommitBudget`'s doc comment now defines the window it carries: opened by
    `begin_commit_budget` before the cell plan, closed by `lod::finish_commit_budget` after the LOD
    commit loop, so the span covers cell planning, cell commits, asset and surface readiness,
    render-origin rebasing, lifecycle validation, LOD planning and LOD commits — and it is
    LOD-only, absent with the flag off.
  - `StreamingPlugin`'s chain is `main`'s again, without `begin_commit_budget` at its head: the
    budget systems now belong to the tier that shares the budget.
  - `collect_cells` again owns `main`'s window when LOD is off: `frame_commit_started` on the first
    line, the commit loop, then the frame booked after it. With LOD on it increments the shared
    `budget.commits` instead and leaves the booking to `finish_commit_budget`.
  - New `record_frame_commit` holds the one definition of the booked frame
    (`commit_frames`, `total_/max_frame_commit_micros`, `commit_budget_micros`, the 1 ms tolerance
    check, the `streaming/frame_commit` span and the `streaming/commits_this_frame` gauge). Both
    owners call it, so the two windows cannot drift apart in what they report.
- `lod.rs` — `LodPlugin` now registers both ends of the window: `begin_commit_budget.before
  (StreamingSet)` and `finish_commit_budget` in its chain after it, so the budget systems exist
  only in an LOD-on schedule. `finish_commit_budget` calls `record_frame_commit` and documents its
  half of the window; the plugin and module docs say the window belongs to this tier.
- `app.rs` — the install site is now `add_streaming_tiers(&mut app, lod_enabled)`, which adds
  `StreamingPlugin` always and `LodPlugin` only when the flag asks. One call site in `run`, one in
  the tests, so the decision is testable rather than buried in `run`'s body. The LOD fixture test
  app no longer registers `begin_commit_budget` by hand — `LodPlugin` brings it.
- `docs/roadmap/02-profiling.md` — one sentence in the streaming bullet (line 24) says what the
  frame covers with LOD off (unchanged) and with `--lod` (new baseline). The brief named
  `02-core-engine.md` or `02-integration-and-acceptance.md`; neither mentions the commit budget —
  `02-profiling.md` is the roadmap doc that describes it, so the sentence went there.
- No metric field, gauge or event label changed, so `profiling.rs` is untouched and
  `scripts/phase2-acceptance.ps1`'s `commit_budget_violations` read keeps working.

## Trade-off considered

Two alternatives were rejected. Keeping `LodPlugin` installed with every system early-returning on
`!config.lod_enabled` leaves `finish_commit_budget`, `LodWorld` and six systems' parameter fetches
in every default run, and the LOD-off schedule still would not be `main`'s — the reason this task
exists. Keeping `begin_commit_budget` in `StreamingPlugin`'s chain with an early return (the
obvious first cut) leaves a budget system in the LOD-off schedule that exists only to do nothing,
and a reviewer asking "which system times an LOD-off frame?" would find it. Moving both ends to
`LodPlugin` makes the answer "none, and the cell tier measures itself", which is `main`'s answer.

## Test

`app::tests::lod_off_keeps_upstreams_commit_window` (in `crates/engine/src/app.rs`) builds the
streaming fixture (no LOD rows) through the same `add_streaming_tiers` call `run` makes, then:

- asserts `LodPlugin` is not added and no `LodWorld` resource exists — no LOD system can run;
- runs frames until a cell commits and asserts the frame *is* booked
  (`commit_frames > 0`, `max_frame_commit_micros > 0`, `commit_budget_micros == 16_670`,
  `commit_budget_violations == 0`) while `CommitBudget::commits == 0` — the numbers can only have
  come from `collect_cells` timing itself, which is `main`'s definition.

This is the LOD-off half of the `--streaming-fixture` contract too (`validate_streaming_fixture`
requires `commit_frames > 0`, `app.rs:616`): had `LodPlugin` merely been dropped without restoring
the cell path's accounting, that run's `passed` verdict would have gone false.

The other half of the move — that the LOD tier registering `begin_commit_budget` did not stop the
shared window from booking — is one added line in
`lod_fixture_streams_blocks_across_band_boundaries`: `commit_frames > 0` once the tier has streamed
blocks. Nothing else asserted that.

## Checks run

- `cargo fmt --all -- --check` — clean.
- `cargo clippy -p engine --all-targets -- -D warnings` — `Finished dev profile ... in 2.39s`, no
  diagnostics.
- `cargo test -p engine` — `74 passed; 0 failed; 0 ignored` (lib), plus 3 in `world-inspect` and 0
  doc-tests. That includes `lod_off_keeps_upstreams_commit_window` and the four LOD fixture tests,
  which are the ones that exercise a plugin-registered `begin_commit_budget`.

Not run here: the `--streaming-fixture` acceptance scenario itself (`scripts/phase2-acceptance.ps1`
line 272 runs it with `--max-commit-ms 16.67`), because it needs a release build and a GPU run.
Its metric contract is what the new test asserts; the cheapest run that would falsify it is
`.\scripts\phase2-acceptance.ps1 -Quick` on a release build.
