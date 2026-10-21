# FL1: incremental terrain identity and terrain LOD visibility

Implementation branch: `opt/fl1-incremental-streaming`, exact base `3c5ddc464854d1430871a4703739a3b6b8b5b7fa`. Worktree: `D:/Mudcrab/worktrees/fl1-incremental`; Cargo target: `D:/Mudcrab/builds/fl1-target`. Source brief and retained raw evidence: `D:/Mudcrab/review/drafts/FL1`. Nothing is pushed or published. The integration and DS1 checkouts are untouched.

## Behavior and ownership

Ordinary streaming installs a terrain component lifecycle journal, independently of cluster enablement. Each consumer has its own message cursor. Identity invalidation maintains terrain candidates and ancestor dependencies, including refused candidates, so removing a conflict can repair an identity. It maintains reverse parent edges for safe relationships whose hooks skipped `Children`; event-local edge updates do not inspect every sibling. This adds storage proportional to current `ChildOf` edges. An ordinary update scales with changed entities and affected terrain ancestry/descendants. Unrelated object changes preserve the identity snapshot. The render handoff shares the `Arc<HashMap<MainEntity, TerrainPrimitiveIdentity>>` directly instead of converting and copying it.

LOD visibility owns a separate patch index with cached world ancestry, duplicate-safe readiness counts, tier counts and membership per grid/quadrant. Changed patches and ancestors update their affected keys. Camera/config/demand changes reselect from cached facts without walking terrain ancestry. External visibility overrides retain the previous idle semantics and are repaired on the original refresh triggers. Cluster resource presence and replacement participate in ownership invalidation. No cap, time slicing, delayed correctness, lower setting, or hardcoded world identity is introduced.

Both indices explicitly rebuild after initialization or lost invalidation guarantees. Known default query disablers are observed; late unsupported disablers retain exact query-domain recovery with a diagnostic. Identity also detects parameter-fetch gaps. Debug builds compare the maintained state to fresh full rebuilds. A standalone legacy caller without the journal preserves the old refresh behavior, including intentionally stale idle state.

## Correctness evidence

The unchanged base failed the new `unrelated_mesh_streaming_reuses_terrain_snapshot` regression: `object arrivals must not rebuild or copy the terrain snapshot` (one failed, 2,127 skipped). The initial mistaken `--bin engine` selection found no tests; the corrected negative control uses `--lib`. Logs: target `identity-red.log` and `identity-red-lib.log`.

Focused retained-source nextest passed 107 tests, 2,047 skipped, after a 3m12s compile; execution took 1.999s. The filter was `test(streaming::terrain_identity) | test(streaming::lod)`. Identity's randomized oracle drives four deterministic seeds through 512 transitions each, starting with 24 valid identities. LOD compares with the original full-pass algorithm across randomized additions/removals/reparenting/readiness/coverage/ownership, cluster lifecycle and disabler cases. Directed tests cover malformed ancestry, duplicate ready patches, skipped relationship hooks, empty-root despawn, expired messages, fetch gaps, held external visibility overrides, and a 4,096-child object parent where one visibility change inspects one edge. A real open-door fixture exercises portal activation, camera movement/inactivation, world retargeting and exterior main-view precedence without new ancestry work.

Two independent source reviewers found the prior blockers; each was fixed and source-reviewed. The final identity review found no blocker. The final LOD review's legacy idle-audit finding is fixed and the new regression passes. Reviews are source evidence, not substitutes for executed checks. Review ledger: `D:/Mudcrab/review/drafts/FL1/REVIEW.md`.

## Measurement protocol

The exact-base optimized `quick` executables were built and saved before installing production edits. The profile inherits release, without LTO. Binary and manifest SHA256 values and full routes are retained in `provenance.json`. First launch of each new binary is discarded. Frame-time runs use the supplied `fly-fast` (4,000 units/s) and `fly` (900 units/s) routes, unchanged showcase/TAA/as-played/cluster flags, Vulkan RTX3090 driver 616.92, 1600x900 and database schema 9. Only route frames are scored by `route_hitches.py`, excluding startup. Frame ownership comes from the benchmark's route-scoped `frame_budget` object. Full-run threshold failures are retained separately.

Runner copies `run-fl1.sh` and `capture-fl1.sh` use timing-recognized holds with `--drain --drain-minutes 0`: active work refuses a hold and clears only the failed own hold. They also record `build_gate.py guard --owner sol-fl1 --retries 0` verdicts. No own Cargo job overlaps a launch. Only quiet non-Tracy observations support frame-time reporting. Tracy provides inclusive system attribution; its FPS is not compared. One retained run per route/build supports inference, not a statistical performance guarantee.

Every Cargo invocation uses the build gate, `CARGO_BUILD_JOBS=2` and the owned target. Both baseline warm-ups are discarded. The supplied quick profile is used as explicitly requested by the brief; these measurements do not establish release-profile or 160 fps acceptance.

## Reproduction commands

Run from `D:/Mudcrab/worktrees/fl1-incremental` in PowerShell. Every Cargo job uses the same owned target and two jobs:

```powershell
$env:CARGO_TARGET_DIR = 'D:/Mudcrab/builds/fl1-target'
$env:CARGO_BUILD_JOBS = '2'
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy --workspace --all-targets --all-features -- -D warnings
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run --workspace --all-targets
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo test --workspace --doc
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick --bin engine
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick --features tracy --bin engine
```

The retained runner copies use Git Bash explicitly because Windows `bash.exe` selected an unusable WSL installation. Substitute the label, immutable binary and route; discard the first launch of each new binary:

```powershell
& 'C:/Program Files/Git/bin/bash.exe' D:/Mudcrab/review/drafts/FL1/run-fl1.sh <label> <binary.exe> fly-fast
& 'C:/Program Files/Git/bin/bash.exe' D:/Mudcrab/review/drafts/FL1/capture-fl1.sh <label> <tracy-binary.exe> fly-fast
python D:/Mudcrab/review/drafts/FL1/check-run.py D:/Mudcrab/perf/flight/<label>
python D:/Mudcrab/perf/flight/tools/route_hitches.py D:/Mudcrab/perf/flight/<label>
python D:/Mudcrab/review/drafts/FL1/export-attribution.py <capture-label>
```

Repeat with `fly`. `export-attribution.py` streams `tracy-csvexport -u -f "system{"` into the provided `route_names.py` with the capture's exact `frames_on_route`, then separately exports the cluster/identity/LOD subspans. It does not materialize the complete zone-event CSV. The first two capture summaries were unfiltered; later summaries filter system names while retaining all raw trace events. No runner is edited while it executes.


## Verification results

- `cargo fmt --all -- --check`: passed.
- Engine all-target/all-feature clippy passed after the fixture/query lint fixes; workspace all-target/all-feature clippy with `-D warnings` also passed in 6m07s. Both retain the dependency future-incompatibility warning for `proc-macro-error2 v2.0.1`.
- First engine all-target nextest stopped after 744 passes and one existing copied-license byte comparison failure, leaving 1,424 tests unrun. `pinned_lighting_copy_reverses_to_exact_locked_stock` compared Windows-checkout CRLF bytes with installed LF license bytes. The two local copied license files were restored byte-for-byte from their committed Git blobs; Git diff is empty and no test/source/license content was changed. MIT SHA256: `508a77d2e7b51d98adeed32648ad124b7b30241a8e70b2e72c99f92d8e5874d1`; Apache SHA256: `a6cba85bc92e0cff7a450b1d873c0eaa2e9fc96bf472df0247a26bec77bf3ff9`.
- The complete engine all-target rerun passed 2,169 tests with 13 existing skips in 116.787s. The streaming fixture passed in 116.583s. Logs: target `engine-nextest.log` and `engine-nextest-second.log`. No assertion, threshold or skip was relaxed.


- Retained Rust source is committed locally as `2d13ded0b5488b9893aa842429972b6360903189`. No source has changed since its passing engine suite and independent reviews.
- Workspace all-target nextest passed 3,974 tests across 126 binaries, with 55 existing skips, in 442.580s after a 16m07s build. The streaming fixture passed in 269.355s; the converter metadata fixture also completed after slow notices. No test, timeout, skip or threshold was changed. Log: `D:/Mudcrab/builds/fl1-target/workspace-nextest.log`.
- Generated debug incremental data brought shared D: space to 15.57 GiB. The lead explicitly authorized the single folder `D:/Mudcrab/builds/fl1-target/debug/incremental` to be removed after Cargo exited. Both verified-variable and literal-path PowerShell removals were rejected by automatic policy (`blocked by policy`); this agent deleted nothing. The lead then performed the removal after checking no Cargo/rustc used the target and recorded 14,571,048,118 bytes in `D:/Mudcrab/builds/DELETIONS.log` at 2026-10-20T21:02:23+10:30. The folder's absence and preserved executable/dependency/quick-cache paths were verified. D: returned to 28.6 GB and `disk_guard.py --need 15` passed. This exception does not authorize any other removal.
- Workspace doc tests passed after DS1's `conversion-ds1-r2-20924` hold cleared (Cargo finished in 1.58s; four existing engine examples ignored; no failures). No own engine or Cargo process was launched through that hold. All required source checks passed for source commit `2d13ded0`.


## Retained binary provenance

Both optimized `quick` builds passed from `2d13ded0` with two Cargo jobs, the same features as their exact-base counterparts, and the owned target. No source changes followed verification. The package's three recorded manifest hashes are unchanged.

| Binary in `D:/Mudcrab/builds/fl1-target` | Build | SHA256 |
|---|---|---|
| `fl1-after-no-tracy-2d13ded0.exe` | Passed, 2m46s | `ccdf593c2bc27702ef7d79c43690440df9e2e48f0a69cf1ab4eb15b755c48a01` |
| `fl1-after-tracy-2d13ded0.exe` | Passed, 3m13s | `0b203fba24651742ace5f6898b7a6c75ac86a7d1727d4e2ddac033a2242fa901` |

The first attempted after warm-up hold was refused while the lead's integration-checkout quick engine/converter build was active. The failed own hold was cleared and no engine launched. The lead subsequently stopped its check chain and granted a quiet window for all six after launches. Those launches use the old full-copy dataset layout, as confirmed by the lead: the later overlay-layout integration commit `75544fd1` is absent from FL1's base and binaries.

## Baseline route observations

All labels below are directories in `D:/Mudcrab/perf/flight`. The table uses the exact route-frame selection and percentile rule from the supplied `route_hitches.py`; FPS is shown to two decimals. All routes completed without missing-ground frames. The baseline source is the exact base commit, not the lead's earlier measurement.

| Non-Tracy label | GUARD; mean other CPU | Route FPS | p50 / p95 / p99 (ms) | Frames >=25 / >=50 / >=100 ms |
|---|---|---:|---|---|
| `fl1-before-fast` | Quiet; 5.5% | 21.41 | 17.96 / 140.47 / 152.32 | 345 / 258 / 88 |
| `fl1-before-fly` | Disturbed; 6.7% | 95.29 | 7.89 / 12.34 / 97.77 | 401 / 295 / 130 |

There were no build intruders in either run. The quiet fast run scored 695 route frames in approximately 32.5 seconds. It recorded one 16,670-microsecond streaming commit-budget violation. The slow run recorded no streaming failures. Both benchmark verdicts are `passed=false`, retaining existing full-run performance/memory thresholds. The guard verdict is separate from the benchmark verdict. The slow retry `fl1-before-fly-quiet-r2` also completed but remained disturbed (6.3%); it is preserved and does not replace the first retained row. No slow-route speedup claim can use these disturbed baselines.

`fl1-before-warm-fast` and `fl1-before-tracy-warm-fast` are discarded warm-ups. The Tracy warm-up was quiet at 5.0% other CPU. Retained baseline profiler captures were disturbed at 6.8% (`fl1-before-tracy-fast`, 475 route frames) and 7.0% (`fl1-before-tracy-fly`, 12,103 route frames), without build intruders. Their attribution is diagnostic only and cannot support a paired speedup claim. The raw traces contain 64,804,368 and 244,759,138 bytes respectively.

An earlier edit while Bash still read the slow capture runner caused its completed tail to emit `e-zones.txt: command not found`. The trace, messages and route summary were complete; later attribution reads the preserved trace. The corrected runner passed `bash -n` and is frozen during launches. The planned fast profiler retry was refused while DS1 clippy was active; the failed own hold was cleared and no engine launched. The directory `fl1-before-tracy-fast-quiet-r2` contains only its OWNER record.

## After route observations

| Non-Tracy label | GUARD; mean other CPU | Route FPS | p50 / p95 / p99 (ms) | Frames >=25 / >=50 / >=100 ms |
|---|---|---:|---|---|
| `fl1-after-fast` | Quiet; 3.6% | 86.58 | 8.12 / 31.12 / 85.18 | 208 / 74 / 3 |
| `fl1-after-fly` | Quiet; 3.4% | 118.51 | 7.72 / 10.51 / 27.78 | 212 / 75 / 1 |

These launches completed at 21:24:46 and 21:28:59 local on 2026-10-20. They scored 2,810 and 17,091 route frames over approximately 32.5 and 144.2 seconds. Both have zero missing-ground frames, streaming failures, streaming commit-budget violations and build intruders. Vulkan RTX3090/616.92, 1600x900, schema 9 and the full-copy dataset match the baseline. The non-Tracy after warm-up is discarded; it completed with a quiet 4.3% GUARD and no streaming failures or missing ground.

The quiet fast pair observes 21.41 versus 86.58 fps and 88 versus 3 frames at least 100 ms. This is a single-pair inference for these quick-profile runs, not a statistical guarantee. The slow baseline is disturbed, so its row cannot support a speedup claim. Neither after run establishes the 160 fps target.

Both benchmark verdicts remain `passed=false` / engine exit 1. These full-run thresholds include startup and are separate from the route-only table. The fast run's full-run mean 48.7465 fps, p95 58.7321 ms and memory growth 1.11712 GiB miss the unchanged 60 fps / 16.67 ms / 0.5 GiB thresholds. The slow run's mean is 88.2768 fps, p95 45.1766 ms and memory growth 1.25134 GiB; its p95 and memory also fail. No shader error or Vulkan VUID was found in the retained fast log. Its shutdown warning is `Failed to download render statistics buffer: Error occurred when trying to async map a buffer`.

The route-scoped 6.25 ms frame-budget classifier records fast over-budget ownership as 243 main-world, 2,387 render-thread, zero GPU, zero upload and 70 unknown frames. Slow records 272 / 16,070 / 4 / 0 / 82 respectively. This classifier starts on the first full route frame, so its total has one fewer frame than `route_hitches.py`. GPU ownership means swapchain-acquire wait, a lower bound; it does not prove GPU work is free. Render-thread ownership still includes submit/present waits. These remaining costs require separate work rather than an FL1 scope expansion.

The Tracy binaries emit `ERROR: SymInitialize FAILED with code 87 (0x57) | The parameter is incorrect.` at startup in both baseline captures and the after warm-up/fast capture. This is not a new FL1 diagnostic; explicit tracing zones and raw trace files remain available. It limits stack-symbol interpretation. No stack-based claim is made.

Five after launches ran sequentially in the quiet window. Before the final Tracy slow launch, `disk_guard.py --drive D: --need 15` refused: `STOP: D: 24.9 GB free; after this job about 9.9 GB (floor 10 GB). Free space first (owner's yes) or wait.` No engine launched on that attempt. The lead removed 1.5 GiB of its own regenerated `vg7-lead-target` incremental caches and logged the deletion; the guard then passed at 26.4 GB free, projecting 11.4 GB after the requested reservation. The final launch was retried under the same quiet window, unchanged executable, flags, route and GUARD. FL1's `quick/incremental` remains intact. No disk guard or gate was bypassed, and this agent deleted no cache.

## Profiler attribution

All values below are inclusive milliseconds per route frame, summed across threads using the supplied `route_names.py`. The columns are nested and must not be added. The renamed LOD implementation system is `engine::streaming::lod::visibility::update_terrain_lod_visibility`.

| Capture | GUARD; other CPU | `extract_sources` | Terrain identity wrapper | LOD visibility system |
|---|---|---:|---:|---:|
| `fl1-before-tracy-fast` | Disturbed; 6.8% | 29.45 | 17.26 | 21.82 |
| `fl1-after-tracy-fast` | Quiet; 3.8% | 3.08 | 0.28 | 0.18 |
| `fl1-before-tracy-fly` | Disturbed; 7.0% | 1.91 | 0.85 | 0.93 |
| `fl1-after-tracy-fly` | Quiet; 3.6% | 0.97 | 0.13 | 0.10 |

The baseline captures are disturbed, so this table is diagnostic attribution, not a paired speedup claim. No Tracy FPS is compared. The before identity implementation itself accounts for 14.85/0.73 ms per fast/slow frame; the wrapper includes the removed conversion/copy. The after identity implementation accounts for 0.28/0.13 ms. Its incremental delta resolution accounts for 0.11/0.02 ms, with wrapper worst calls 5.1/4.8 ms. The before full identity rebuild runs 0.63/0.03 times per frame and LOD `collect_ready` runs 0.49/0.02 times per frame, costing 16.66/0.71 ms per frame. Neither after route contains an identity rebuild/recovery span or a LOD rebuild span. After LOD absorb costs 0.05/0.02 ms; applying keys costs 0.06/0.01 ms and runs 0.11/0.02 times per frame. The after LOD system's worst call is 4.2/3.5 ms.

The after fast/slow captures score 2,382/15,404 route frames. Raw capture sizes are 95,760,011/297,344,086 bytes. Both routes complete without missing ground, streaming failures, commit-budget violations or build intruders. All six after GUARD verdicts are quiet: non-Tracy warm-up 4.3%, fast 3.6%, fly 3.4%, Tracy warm-up 4.0%, Tracy fast 3.8%, Tracy fly 3.6%. The Tracy warm-up is discarded and its 92,452,680-byte trace retained. Final runner/check completion was 21:42:43 local, after the raw slow trace was saved at 21:41:06. All own holds were released; the lead was notified that heavy work could resume. Attribution exports run after engines exit and read only saved traces.

Remaining after fast costs are render schedule 9.04 ms/frame (renderer 4.12 and pending-command submission 2.57 nested within it), template refresh 1.20, terrain clip finalization 1.34 and candidate observation 0.84. Slow costs are render schedule 7.76 (renderer 4.12, submission 2.60), templates 0.33, clips 0.43 and candidates 0.13. `extract_sources` still has worst calls of 72.0/70.6 ms, template refresh 33.9/34.5 ms and mesh extraction 64.2 ms on the fast route. This does not establish a new cause for every hitch; these are owners for the lead's next investigation. They are outside FL1's two requested algorithms.

## Final provenance and integration limits

`verify-provenance.py` rehashed all four binaries, three package manifests and frozen runners, compared route contents with retained inputs, checked both renderer/schema fingerprints and unchanged numeric benchmark limits, and verified all six completed quiet runs. It updated `D:/Mudcrab/review/drafts/FL1/provenance.json`. Its first version incorrectly compared the whole `thresholds` object and stopped with `AssertionError: fl1-after-warm-fast`: the object also contains evaluated result flags. Inspection showed only `commit_budget_respected` differed (`false` before, `true` after). The corrected checker compares fixed numeric limits separately and requires all after runtime/result flags; it passed. This was an evidence-checker mistake, with no executable, setting or benchmark change.

Route summaries are in `route-summary.json` and each retained non-Tracy after directory's `route-hitches.txt`. Capture attribution files are `system-top.txt`, `cluster-names.txt`, `identity-names.txt`, `identity-delta-names.txt` and `lod-names.txt`. Full traces, logs, messages, frame CSVs, benchmark JSON, GUARD JSON and immutable binaries remain on disk with their OWNER records.

Rust source remains exactly `2d13ded0`; only this evidence and the handoff log are added afterward. No push or PR is created. The lead owns integration and the required portal-heavy GPU check before merging streaming/visibility changes into Plus. Real portal-demand unit evidence is included, but the exterior flight routes do not replace that integration gate. The global 160 fps goal, release-profile acceptance and a quiet slow-route baseline remain unproven. No additional source change, test skip, threshold change or scope expansion was made after the passing checks.

Expired owned source staging `D:/Mudcrab/review/drafts/FL1/lod-staging` (409 KiB by dust) was removed after checking its exact resolved path and OWNER instruction to delete after application/commit. The removal is logged in `D:/Mudcrab/builds/DELETIONS.log`. All targets, quick incremental data and measurement/review evidence remain. Final observed free space was C: 15.98 GiB / D: 25.71 GiB. End-of-work pace check reported no workers, Claude plan 8% used in five hours and 13% used weekly; no new worker was needed after verification.
