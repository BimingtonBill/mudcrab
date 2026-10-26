# #2151.4 incremental readiness aggregation

Status: implementation and all frozen checks pass, including both releases and vendor checks. The single authorized campaign was refused by the unchanged guard before engine launch. Native performance remains unverified; both binaries remain frozen for a later quiet campaign.

Base: `5719b67f3`, branch `phase2/cluster-traversal-r4`, worktree `D:/Mudcrab/worktrees/cluster-traversal`. No push.

The preceding accepted R3 evidence distinguishes two costs: 915 fly readiness-vector misses freshly evaluated 12,279,300 batches; the loading bridge performed 6,794,855 consumer/batch visits over 199 aggregate non-replays. R4 first addresses the latter. The R3 crossing aggregate occupied 2.6946–4.3754 ms on fly and 2.7651–4.7804 ms on fast fly, when invoked. Those are historical scoped CPU observations on the earlier R3 source, not a paired comparison of the new integration.

## Contract and ownership

The bridge retains contributions per consumer, batch and domain. Exact source template/fallback tuples, actual resolved GPU material identities, prepared-material shadow policy and consumer readiness bit transitions supply dirty batches. Source/material epochs remain observation metadata; they do not authorize aggregate population walks. Inventory validation, certificate/layout coherence, admission generation, frame acceptance and publication recovery remain separate prerequisites.

Object invalidity remains OR over surviving object instances within each placed world/domain group. Runtime changes that preserve that OR do not dirty the aggregate group. Refused last contributors remove old rows. Missing consumers remain blocking on empty catalogs; missing materials conservatively require shadow consumers. Current pipeline, allocator, slot, queued-carrier, prepass, shader and material-address proof remains authoritative.

Prepared material changes use the native Bevy `Messages` buffer and independent reader cursors. A monotonic collection lifetime ID detects replacement even when numeric revisions match. New/replaced/expired readers return `None` and require a complete projection baseline. Empty reads visit no messages or assets. One rotation system per erased collection runs before preparation; preinitialized resources use an installation marker, so two source material types cannot rotate their shared collection twice. This shape expresses replacement correctly; Arc lifetime tokens and weak reader queues are unnecessary. The shadow cache's destructive `take_changed_assets` API is unchanged.

A material-collection replacement also revokes queue plans and full selection readiness. Existing prepared-material address checks are preserved. An unrelated HashMap insertion can relocate used prepared materials, so per-asset mutation IDs alone cannot safely narrow full selection readiness evaluation.

## Source footprint

- `crates/engine/src/cluster/readiness_bridge.rs`: optional-resource withdrawal, catalog semantic deltas, scope/admission publication boundaries and incremental consumer integration.
- New `readiness_aggregate.rs`: retained contributions and full-rebuild oracle regressions.
- New `readiness_inputs.rs`: exact loading tuple/policy projection and actual-binding inverse map.
- `handoff.rs`: template-row publication, readiness-bit transition receipts and prepared-material lifetime in existing full-readiness key. No source change-query/admission hunk.
- `render.rs`: actual binding mutation notifications, readiness receipts and material-collection replacement invalidation. No GPU reuse buffer-key, origin or previous-frame history hunk.
- New `readiness_source_tests.rs` and `readiness_delta_tests.rs`: producer and production bridge contracts.
- Upstream-file list: `vendor/bevy_render-0.19.0/src/erased_render_asset.rs` and `erased_render_asset_tests.rs`. This extends Mudcrab's existing vendor patch. The API documentation explicitly identifies the new methods as Mudcrab vendor extensions.

## RED evidence and review

- `round4-aggregate-red.log`: compilation succeeded, exit 100; all three tests fail as expected. One fallback-only change visits 100 rows instead of one; one bit change revisits both views; admission republication invokes an unchanged callback.
- `round4-material-red.log`: compilation succeeded, exit 100; nine of eleven new tests fail against a population-scan adapter. Two existing-behavior preservation tests pass. Failed cases include removals, deduplication, independent windows and restarted collection revisions.
- `round4-input-red.log`: compilation succeeded, exit 100; all four source/projection tests fail as expected. The tuple notifier loses fallback-only changes; shared-material changes revisit the population and unrelated changes invoke forbidden lookups.
- Independent Sol native-reader review: no concrete findings. Added a shared-ERA/preinitialized Render schedule regression to address the stated verification limit.
- Independent Sol full readiness review: no actionable findings; scope boundaries, OR semantics, cursor continuity and current publication checks are preserved. Static review alone establishes no runtime or performance acceptance.

The passing regressions establish bounded work on controlled fixtures, separately from native frame timing:

| Mutation | Fixture result after R4 |
|---|---|
| One fallback-only transition among 100 needed batches | One contribution visited; recovery visits one more |
| One readiness bit in one of two consumers | One contribution visited; other batches and consumer retained |
| Admission-generation republication with unchanged dependencies | Zero batch visits; fact callback must not run |
| Settled replay, unrelated material, or validated metadata-only notification | Zero batch visits/projection callbacks in the corresponding fixtures |
| Shared-material shadow-policy mutation | Only its two inverse-mapped fixture batches projected |

Repeated deltas are checked against a fresh world/domain aggregation oracle before any oracle repair. Production-path regressions also cover fallback recovery, an invalid object survivor within the same batch/world, inventory allocation replacement, missing required resources and withdrawal followed by same-input recovery. These tests establish behavior and work bounds in fixtures; they do not supply route counts or crossing milliseconds.

## Frozen verification and measurement boundary

The explicit #2145.4 WINDOW COMPLETE relay opened the gate. Frozen checks use 780 Rust/Cargo file hashes. Prefix `round4-final-v1` passed fmt, strict engine/workspace all-target/all-feature Clippy, focused 70/70 and engine 3,423/3,423 (15 skipped). Its workspace build then exhausted system commit while compiling the engine: `memory allocation of 1572864 bytes failed`, compiler exit `0xc0000409`. This is retained as a failed gate, not a test result. No source changed. Fresh prefix `round4-final-v3` uses one Cargo job and the identical 780-file source receipt; the intervening v2 invocation refused its preexisting redirected console filename before running any gate.

The capture controller uses an atomic invocation lock and a private, never-reused UUID timing hold. Every output prefix is preflighted; check receipts, frozen binaries and campaign summaries are exclusive creations. Its monotonic deadline includes drain, startup, guards and scoring, with reserved bounded cleanup. Source, binaries and helper hashes are pinned. Git provenance runs through the same deadline-bound subprocess helper. Six pure controller mocks cover duplicate invocation, existing evidence, foreign hold, failed drain, capture failure and failed clear. Review findings and corrections remain in the T3 task receipts.

The same-source v3 retry passed strict engine/workspace Clippy, focused 70/70, engine 3,423/3,423 (15 skipped), workspace 5,678/5,678 (65 skipped, six slow; 566.032 seconds), and workspace doctests. The check helper's separate pure mocks verify dotted log names, no completion marker after failure, and preservation/refusal of an existing prefix or stale completion receipt. Its controller serializes invocations; every rerun needs a fresh prefix.

Native exit 1 is retained only as a diagnostic when the sole false boolean threshold is `commit_budget_respected`; the receipt explicitly records `diagnostic_only=true` and `native_passed=false`. This classification does not establish startup-only causality or native acceptance. Other nonzero exits refuse the capture. The unchanged official content scorer must pass, followed by stricter required input/settings/geometry/qualification equality. Resource entity counts are displayed separately because Bevy resources are entities. Center and reach are compared explicitly. Historical R3/I22 standing, fly and fast pairs pass this stricter check; changed center/reach mocks are detected. This is declared-input and content-count proof, not historical dataset byte provenance or full-route pixel parity.

All eleven `round4-final-v3` jobs completed with exit zero. Results: fmt; strict engine and workspace all-target/all-feature Clippy; focused nextest 70 passed; engine all-target nextest 3,423 passed/15 skipped; workspace nextest 5,678 passed/65 skipped; workspace doctests 18 passed/15 ignored; Tracy and plain releases; strict vendor `bevy_render` all-target/all-feature Clippy; vendor all-target nextest 41 passed. The completion marker is `round4-final-v3-checks.exit`. All 780 source/manifest hashes and both binary hashes were independently rechecked after completion. Historical failed attempts remain retained.

Frozen release SHA-256 values:

- Tracy: `21583db4209400d7cf2f804ea60576a93661be285dc74d2f8de534e673c2746f`.
- Plain: `bafce9f2c4e28c0d88d8bf78b0aa48c97cccea82521446dc3ff07000847dce7d`.

The R4 campaign uses one announced finally-cleared hold, at most 25 minutes. Captures must prove cover release before the measured window and route, quiet guard, matching content/reference and completed routes. Remaining full selection evaluation is reported separately and narrowed only if crossing evidence and exact producer reuse make that safe. The R4 exporter retains the entire readiness-observer system below the ordinary 30-microsecond system floor, while preserving the no-floor catalog/input/aggregate zones and separate nonadditive measurements. No fresh R4 baseline exists: historical R3 predates the integration's #2145.4 step 1, so an overall FPS comparison would not isolate R4 causality.

## Single campaign refusal

After the exact prior gated jobs finished, announced and attempted `round4-after` once with the frozen `round4-final-v3` releases. Gate status showed all build slots free and no foreign holds. The private hold was `timing-2151-r4-32ba7faeb2b14675b79cd5d0058a6b62`; drain completed and disks became quiet. The campaign took 316.792 seconds and finally-cleared at 2026-10-26 21:09:22 Adelaide, well before its 21:29 deadline. Its controller lock is absent. `FINAL COMPLETE` was relayed to the lead immediately after verifying clearance.

The first discarded Tracy warm-up never launched: the unchanged guard could not find five consecutive quiet seconds within its five-minute admission limit. It made one attempt, with zero retries. Verdict `not_quiet_before_start`, guard exit 6; the final sample reported GPU 17%, other CPU 1.5%, no intruders. GPU admission requires less than 15%. The 17% value is the final sample, not a five-minute mean. Guard limits were not changed and no user applications were closed.

`round4-after-tracy-warmup-fly-fast/exit.json` records `native_passed=false`, `diagnostic_only=false`, `timings_admitted=false`. No engine benchmark, route frames or Tracy trace exists. Consequently there is no R4 route/window admission gap, content comparison, native performance result or crossing attribution to report. The complete guard and cleanup receipts are retained under `D:/Mudcrab/builds/cluster-traversal-target`, together with the frozen releases, source manifest and helper hashes. No export or frame audit was run against missing capture data. No retry followed the refusal, as instructed.

## Before/after measurement ledger

R3 columns below are historical context, not a fresh paired R4 baseline. R4 entries are unavailable, never zero.

| Metric | Historical R3 fly | Historical R3 fast | R4 after |
|---|---:|---:|---|
| Bridge aggregate non-replays | 199 | 198 | Not measured: guard refused |
| Bridge consumer/batch visits | 6,794,855 | 6,760,710 | Not measured |
| Bridge batch fact preparations | 1,358,971 | 1,352,142 | Not measured |
| Selection readiness misses | 915 | 905 | Not measured |
| Selection readiness batch evaluations | 12,279,300 | 12,145,100 | Not measured; full evaluation retained |
| Invoked crossing bridge aggregate, ms | 2.6946–4.3754 | 2.7651–4.7804 | Not measured |
| Profiler-free FPS | 105.31 | 92.34 | Not measured |
| Profiler-free p99, ms | 21.4631 | 45.1165 | Not measured |
| Profiler-free hard frames | 41 above 33 ms | 21 above 50 ms | Not measured |

Historical profiler-free standing was 118.47 FPS. The fly target of at least 120 FPS, roughly 12 ms p99, zero frames above 33 ms, fast zero above 50 ms, and no one-second window above twice its median remain unproved. The bridge's fixture work reduction is established; its native crossing wall-time benefit and settled route counters are not.

## Remaining proof and next root plan

1. Obtain a later quiet window and reuse these exact frozen binaries/source with a fresh campaign prefix. Keep the same guard, discarded warm-ups, content comparisons, cover/route admission checks and finally-clear deadline.
2. After clearance, join complete Tracy crossings and report observer, catalog, input projection and aggregate durations independently. Report aggregate visits/fact preparations alongside selection misses/evaluations; do not combine overlapping spans or assign a Tracy cycle to plain frames.
3. If selection evaluation still contributes crossing wall time, establish exact producer authority for prepared-material relocation, allocator/slab, pipeline, queued-carrier and view/role changes before narrowing it. Per-asset mutation IDs prove shadow-policy projection, not prepared-material address stability. Preserve current fresh selection proof until that authority exists.
4. Attribute every remaining admitted hard frame and retain its root plan. No R4 native frame was admitted, so this campaign supplies no new frame ledger. Portal-heavy visual/GPU and full-route pop-in acceptance also remain open.

No source change-query/admission, origin, temporal-history, quality setting, content cap, throttle, delayed loading or frozen view was introduced. No push or PR. R3 and R4 evidence remain retained; no owned evidence folder expired or was deleted.
