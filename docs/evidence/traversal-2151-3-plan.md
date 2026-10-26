# I22 traversal steady cost and frame band (#2151.3)

Start from integration `94d8722a0`, fast-forwarded into the isolated traversal
worktree. Owner `sol-2151`; target `D:/Mudcrab/builds/cluster-traversal-target`.
No push. The user supplied profiler-free I22 standing and route measurements;
these replace old-round aggregate timings as the performance baseline.

The independent saved-log audit retains all 36 input hashes under
`D:/Mudcrab/builds/cluster-traversal-target/round3-i22-audit`. Conditional mean
frame time at or below 33 ms is 8.1508/8.2732 ms standing, 9.2004/9.1984 ms
fly, and 10.2285 ms fast. Fly spikes increase the full mean by 0.1132 ms;
steady moving work therefore needs separate attribution. These are workload
comparisons, not isolated causal effects. The latest acceptance target is the
user's 120 FPS target below; the engine's historical 160 FPS threshold is not
this round's target.

The baseline capture helper uses a monotonic 25-minute campaign deadline,
bounded owned-process cleanup, unique hold identity, successful nonempty Tracy
recording, explicit route completion, and matching I22 content comparison.
Trace exports run only after the hold clears. A separate bounded watcher waits
for the frozen baseline and free gate slots before invoking that campaign once.

At 10:27 Adelaide the lead explicitly overrode the baseline scheduling rule:
arm immediately, allow a 30-minute drain, and reserve about 45 minutes total to
stop newly queued jobs from slipping in. The free-slot watcher was stopped
before starting the sole baseline campaign. Its hold begins at 10:28:04 and
expires at 11:13; the monotonic cleanup deadline agrees with that time. This
exception applies only to baseline. The final campaign remains at 25 minutes.

## Acceptance and invariants

- Fly: at least 120 FPS, p99 approximately 12 ms or lower, no frame above 33 ms,
  and no one-second p99 window above twice the route median.
- Fast: no frame above 50 ms. Both routes retain complete content, no owned
  stand-ins, no lowered settings, no hidden loading or frozen views.
- Distinguish steady moving-frame cost from the contribution of rare crossings.
  Attribute merged-head spans before changing behavior. Instrument the branch
  that separates competing explanations before choosing a fix.
- Preserve certificate, layout, generation, frame, material/view/domain and
  source semantic validation. Retain raw in-place ECS mutation support and
  exact origin-rebase/missed-rebase/full-oracle tests.
- Keep source-query and GPU-reuse changes coordinated with #2145.4/#2153.1;
  list any shared hunk explicitly in the report.

## Execution

1. Audit supplied I22 launches, clocks, guards, frame distributions and available
   CPU/GPU telemetry. Compare equivalent complete-content standing/flying runs.
2. Build a counter-only Tracy baseline. Separate selection readiness/material/
   base/mask phases, readiness aggregation visits, and flat producer admission,
   member visits, pose computations and actual writes. No behavior fix yet.
3. Run one discarded warm-up, standing, fast and fly baseline captures under
   one drained owner hold. The lead explicitly approved separate baseline and
   final campaigns; each hold must be at most 25 minutes and finally-cleared.
4. Join actual cycle clocks and spans for steady ordinary route frames and all
   requested threshold spikes. Form one causal hypothesis per retained fix.
5. Observe meaningful red regressions, implement changes at the responsible
   producer/consumer, then verify green. Preparation may move off-thread with
   coherent generation-matched publication; no content throttles or caps.
6. Run fmt, strict engine/workspace all-target/all-feature Clippy, engine and
   workspace nextest, and workspace doctests through the gate. Freeze both
   Tracy and non-Tracy release executables with source hashes.
7. In the build gap #2153.1 may capture. Its window completed before final
   verification. The lead subsequently gave #2145.4 the next window because
   both peer binaries froze first. The bounded final watcher must inspect fresh
   gate status and wait for every foreign timing hold, including `timing-2145-*`,
   before acquiring one own hold for all final captures. Never bypass holds.
8. Discard the first run of each binary. Retain guarded TEST standing/fly/fast
   comparisons for Tracy attribution and profiler-free-equivalent non-Tracy
   evidence. Finally-clear immediately, then export/parse traces outside holds.
9. Report targets honestly; every remaining above-threshold frame needs exact
   attribution and a concrete root-layer plan. Record clean visual status,
   update N30/handoff, and commit with the required co-author trailer.

Two bounded Codex read-only audits run alongside parent integration and builds:
I22 baseline audit and origin/flat producer invariants. No Claude children.

## Round outcome

Baseline attribution preceded retained fixes. Observed REDs, all required
engine/workspace gates, both frozen release builds and the accepted final
Tracy/plain campaign are complete. Two disturbed discarded warm-ups were
rejected; the accepted campaign finally-cleared before exports and released
the window with `FINAL COMPLETE`.

The performance target remains open: profiler-free fly105.31FPS/p99 21.4631ms,
41 frames above33ms; fast92.34FPS/p99 45.1165ms,21 frames above50ms. Every one of
84 remaining Tracy hard frames has an exact join and root plan. All79 plain
hard frames have validated native-budget joins;68 retain coarse Unknown
ownership and an explicit next instrumentation plan. No inner cause is
invented for them. See `traversal-2151-3.md` and retained per-frame ledgers.

N30 remains queued for the source/flat, batch proof, scene/asset and per-view
render work. The round commits the bounded correct work reductions and honest
evidence, not a claim that the 120FPS/no-stall bar has been achieved.
