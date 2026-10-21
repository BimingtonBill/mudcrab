# VG7 fast startup: full certification implementation

Worktree: `D:/Mudcrab/worktrees/vg7-fast-startup`, branch
`exp/fast-cluster-startup`, starting at `44f5f0c1`. No push or integration
checkout edit. This supersedes none of the round-1 cache trust boundary:
recorded digests are still expectations, and every source byte used by full
certification is freshly read. No persistent certification cache was added.

## Implementation slices

1. Full certification: parallel full-byte MCI verification and authoritative
   reference/instance checks, deterministic ordered result consumption, one-pass
   mapping/material reads, reused coverage groups, borrowed preloaded snapshots,
   cached per-model primitive expectations, and fewer canonical-table allocations.
   File hashing uses larger buffers; the mapped runtime index reads the map table
   without a per-instance coverage join and builds reference facts on Bevy's
   compute pool. Existing phase logs are retained; `mapped_rows_read` and
   `mapped_reference_facts` distinguish SQL reading from reference construction.
   File-backed source-row preloading uses separate read-only connections and the
   exact existing decoder on disjoint reference partitions, with the original
   source path and an explicit diagnostic for unsupported connection state.
2. Speculative batch source-scene decoding starts under the retained source lock
   after envelope/published-path validation. Initial requests are issued directly
   from the certification worker through a cloned `AssetServer`, so a delayed
   first `Update` cannot serialize the work behind certification. At most the existing `MAX_LOADING`
   requests are in flight; lazy launches bound total speculative models to that
   same limit, while explicit all-batch preload may continue its queue. Strong
   handles remain private until certification succeeds, then the normal lazy
   loader reuses the exact handles. Refusal drops pending queues/handles and
   preserves the existing nonzero exit. Outstanding speculative source requests
   retain their capacity reservations through handoff; ordinary batches reusing
   them do not count the source twice. Blocked wanted demand remains pending on
   unchanged cached frames. Speculation alone is never view readiness demand.
   Prefetch creates no scene entities and cannot satisfy world-ready while
   certification is pending. Success polling drains the worker channel after
   task completion, covering a send between the frame's receiver pass and poll.
3. Ownership projection indexes object instances by batch on certificate/scene
   changes and revisits only batches whose template/material/tangent inputs
   changed. Ownership decisions and source demand update by delta. Reference
   counts preserve unions across batches; unresolved tangents still withdraw
   demand, while unrelated terminal ownership remains publishable. The original
   full projection is retained as a fixture oracle.

## Verification and measurement status

Gated `cargo fmt --all -- --check` passed. Gated
`cargo clippy -p engine -p shared --all-targets --all-features -- -D warnings`
passed initially (exit 0, 1m 24s), after handoff capacity/readiness corrections
(exit 0, 25.79s), and after the worker-dispatch/channel-race corrections
(exit 0, 17.57s). Gated formatting passed again on that final source tree.
Cargo reported the existing dependency future-incompatibility notice for
`proc-macro-error2 2.0.1`; there were no denied code warnings.
The final engine/shared all-target nextest check passed **2,220 tests with eight
skips** (37.852s, exit 0), including worker dispatch before the first Update,
late-channel-send handoff, refusal cleanup, parallel/sequential certification and
full/incremental ownership equivalence. Final logs in the owned target are
`fmt-worker-prefetch.log`, `clippy-worker-prefetch.log` and
`nextest-worker-prefetch.log`. Initial compile diagnostics (bulk-reader type inference and
loader tuple type complexity) were fixed before this passing clippy check.
The checks respected the parent's timing hold and the shared-PC memory gate.
No comparative timing benchmark was run. The optional single TEST launch is
recorded below; it does not establish the under-five-second target.

## Optional single launch (inference only)

Ran once through gate `test --owner vg7-fast-startup --label startup-single`,
using the nextest-built `debug/engine.exe` (test profile, opt-level 1, debug 0),
the supplied small-primitives package for both `--assets` and `--cluster-package`,
`--allow-incomplete-assets --terrain-radius 2 --benchmark-duration 30`.
This was the first launch after a build, on the shared PC, without a timing hold;
it is not an A/B result or renderer/performance acceptance. Logs and output:
`D:/Mudcrab/builds/vg7-fast-startup-target/startup-single.log` and
`startup-single.json`.

| Certification phase | Wall ms |
| --- | ---: |
| envelope | 8452.0925 |
| clusters tables_and_indexes (inside envelope) | 4431.2472 |
| clusters owned_files (inside envelope) | 3677.8898 |
| source_authority_open | 8521.4002 |
| source_reference_preload (inside authority open) | 1241.1470 |
| validate_mapping | 8190.1842 |
| authority_snapshot_preload (inside validation) | 3770.8980 |
| mapped_reference_index | 651.8730 |
| mapped_rows_read (inside index) | 411.8734 |
| mapped_reference_facts (inside index) | 239.9398 |
| examined_reference_index | 1039.5623 |
| coverage_and_cell_index | 299.6715 |
| total | 27154.9222 |

Owned-file validation read all 9,017 cluster files, 3,069,899,039 bytes, with
`recorded=0`. The runtime index contained 448,539 records / 210,140 references.
The single-run total is far above five seconds. Remaining large phases are
envelope SQL canonicalization/file reads, authority opening and mapping
validation; no same-profile isolated baseline was run to assign causality.

The run also exposed that the initial main-Update prefetch implementation logged
its first requests *after* certification completed. The retained worker-dispatch
correction described above was made afterward and is covered by fixtures; it
has no second live timing measurement. No timing was silently attributed to the
corrected implementation.

The launch completed naturally with exit 1 and benchmark `passed=false`. It
reported shader resolution failure for `bevy_pbr::mesh_view_bindings::fog` and
Vulkan VUIDs `VkPresentInfoKHR-pImageIndices-01430` and
`vkAcquireNextImageKHR-semaphore-01286`. Certification itself passed. These
renderer failures were outside this task and were not changed; no GPU or FPS
acceptance claim is made.

D: preflight passed at 67.6 GB free for a 25 GB request. Cargo uses
`CARGO_BUILD_JOBS=2`, the owned target
`D:/Mudcrab/builds/vg7-fast-startup-target`, and the machine build gate.
This installed gate's `run` subcommand has no `--owner`; the equivalent owner is
set with `BUILD_GATE_OWNER=vg7-fast-startup`, retaining `--label` per check.
The initial unsupported `--owner` invocation exited before starting Cargo.
The two Bevy license copies were restored to their exact committed LF bytes
(after verifying equality to the pinned dependency) to avoid the known
`core.autocrlf` checkout artifact; no license text or assertion was changed.

## Acceptance limits

No intended refusal-verdict change. Refusal tests and parallel/sequential
fixtures are part of the required engine/shared check. No settings, quality, readiness proof,
refusal test, or world-ready deadline is relaxed. Runtime startup/phase timing,
portal-heavy GPU acceptance and the performance target remain with the parent.
No owned expired Class A folders were removed; target/check evidence remains
owned for the parent review.


## Round 3 implementation plan (2026-10-19)

The user supplied the execution plan and requested implementation in this existing isolated
worktree. Keep refusal verdicts, source-lock lifetime and no cross-launch trust unchanged.

1. Replace the all-world `InstanceMapRow` map with the original indexed query, streaming rows
   within each chunk. Check shared validation/equivalence tests and commit separately.
2. Restrict speculative source decoding to explicit `ClusterBatchPreload`; reserve two logical
   CPUs in certification worker counts (minimum one). Exercise lazy/preload policy and existing
   prefetch handoff/refusal tests, then commit separately.
3. Remove failed speculative scene handles while pending, at handoff and before lazy reuse.
   Add a real asset-load failure/retry regression and commit separately.
4. Add timings for source connection/scope, manifest read/parse, read_sources, examined-ID
   discovery, catalog construction, model resolution, snapshot construction, mapping structure,
   reference/material checks and instance checks. Build quick, discard warm-up, inspect measured run.
5. Optimize only measured costs; candidate is batching repeated whole-reference-table scans.
   Preserve the original query as error replay and compare exact ordering/decoding in fixtures.
   Do not retain gigabytes of bytes or persist trust. Measure before considering digest reuse.
6. Preserve a quick base executable from 44f5f0c1, then run warmed base/current back to back with
   the exact requested command. Label single-run conclusions inference. Run engine/shared fmt,
   all-feature/all-target clippy and all-target nextest before final evidence/handoff commit.


### Round 3 review corrections and interim verification

- `41fb3597`: stream the original indexed instance-map query one row at a time per chunk.
  The world-sized `HashMap<ChunkKey, Vec<InstanceMapRow>>` is gone; no substitute world-sized
  row buffer is introduced. Column decode results retain their original validation positions.
- `4982d19d`: ordinary lazy launches issue no speculative requests. Only explicit all-batch
  preload may prefetch, still behind certification. Mapping, source and envelope workers use
  `max(available_parallelism - 2, 1)` rather than all reported logical CPUs.
- Failed speculative scenes are now removed while pending, at a late certification handoff,
  and immediately before a lazy candidate's reuse. Independent review caught and removed an
  initial whole-scene-map runtime scan; cleanup is candidate-scoped. A real missing-file test
  restores a self-contained GLB and verifies fresh-load recovery. The late-send handoff test
  also includes a genuinely failed load and proves it is omitted.
- Fresh all-target/all-feature engine/shared clippy passed (15.85 s). Full engine/shared
  all-target nextest passed 2,221 tests with eight existing skips (43.624 s), including the new
  real retry test and late-failure handoff test. These precede moving the runtime cleanup to
  its candidate; focused revalidation is queued. Logs: `round3-clippy-review.log` and
  `round3-nextest-review.log` in the owned target. The policy regression first failed with
  actual 16 requests versus expected zero; `round3-red` compilation encountered a mixed-source
  API build and did not establish a red retry regression. No claim otherwise.
- Independent read-only reviews found no changed refusal or source-ownership authority. A
  removed test-only import was identified and corrected before the passing clippy/full tests.

### Round 3 SQL and duplicate-read investigation (before quick measurement)

Read-only Python SQLite 3.51.0 plans are retained as `round3-query-plans-python.json`. The engine
now logs its bundled SQLite version and actual examined-ID query plan too; that diagnostic does
not execute or replace authoritative source reads. Python plan findings are not engine timings.

- Instance rows use `SEARCH cluster_instance_map USING PRIMARY KEY
  (worldspace_id=? AND tier=? AND anchor_x=? AND anchor_y=?)`.
- Examined-ID discovery uses `SCAN r` plus a records primary-key lookup, separately for each of
  twelve source worldspaces. A proposed all-world IN/json_each query still scans references,
  but only once. A read-only investigator compared exact ordered results (323,499 tuples);
  its ungated timings are inference only and are not the requested quick A/B.
- Coverage runtime indexing aggregates primitive rows once and uses an indexed join. No
  runtime index is added to the read-only converted database or its certified schema.
- Source metadata discovery reads only 49 worldspace rows; ModelCatalog::new builds manifest
  maps and performs no file reads. The added timings separate these from manifest I/O/parsing,
  examined-ID discovery, model byte verification and per-reference snapshot/digest creation.
- The duplicate-read premise needs qualification: cluster_chunks comprises 85 MCI files,
  44,676,352 bytes. cluster_meshes comprises 8,531 payload files, 2,860,029,440 bytes. Mapping
  rereads the MCI files, not the mesh payloads. Later read-only inventory further separates the
  mesh files: 8,271 mapped files total 185,594,624 bytes, while 260 unmapped but still owned
  files total 2,674,434,816 bytes. Runtime's 230,270,976-byte package read counter equals the
  mapped mesh bytes plus MCI bytes; it does not reread all 3 GB. Every owned file still requires
  envelope verification. Retaining 3 GB of buffers is neither needed nor implemented. No
  cross-launch or manifest-digest trust has been introduced.

The lead's fixed-length Tracy process was left alone. It exited naturally; subsequent focused
checks queued behind the lead-owned vg7-timing hold. No other thread's process or hold was stopped.


### Recursive failure review follow-up

The pinned Bevy 0.19 AssetServer has a second failure case: Request mode retries root Failed,
not a Loaded root whose recursive dependency failed. A one-shot cached-source restart therefore
records the old template World's runtime identity and holds the existing loading slot until a
new template World publishes or the root itself fails. An in-place annotation cannot acknowledge
that restart. Same-source batches share the pending attempt; failed paths retain only a one-shot
retry marker. Ordinary newly produced failures retain the existing terminal policy.

A missing old Assets publication invalidates ScanCache while deferring, so a stationary lazy view
cannot preserve that blocked candidate forever. The new regression covers this publication gap,
restored external image bytes, and a permanent image failure with no automatic reload loop.
The headless GLB loader fixture has no PBR handler: its first probe therefore did not expose the
scene-to-image dependency and timed out before exercising the retry. The fixture now declares
that actual external Image dependency through glTF's real LoadContext and retains its real handle
in the template World; it does not fabricate asset states. The corrected package suite passed all 25 tests (1.390 s), including both dependency outcomes.

Clippy after the one-shot production change passed (24.67 s). The initial package run had 24
passes and the fixture-setup timeout above; it is not a passing regression result. Earlier 24
package tests passed (0.894 s) before this additional dependency case. All raw logs are retained.
The user reserved a 60-minute build/measurement window after the lead's remaining Tracy job;
no active engine or another owner's hold is to be stopped, and this task keeps the gate queue.
Corrected-fixture all-target/all-feature engine/shared clippy passed (1m 08s). Raw logs:
`round3-clippy-retry-fixture.log` and `round3-nextest-retry-fixture.log` in the owned target.


### Round 3 first correctly profiled quick launches (instrumented c3ef757a)

Built exactly `cargo build -p engine --profile quick` through the build gate with owner
`vg7-fast-startup`, two jobs and the owned target. Cold build completed in 15m 30s.
The clean-source executable was copied with SHA-256/provenance retained in
`engine-instrumented-c3ef757a-quick.provenance.json` (owned target). All launches use the
requested TEST wrapper and exact assets/package/radius/duration arguments. There is no quiet
GUARD verdict; two-second gate observations found no competing gated job in the second launch.
These are single-run inference, not renderer acceptance or a guarded performance result.

- `round3-instrumented-warmup`: discarded first post-build launch, certification 24,773.0694 ms.
- `round3-instrumented-measured`: second launch, certification 13,559.3489 ms.

| Second quick launch phase | ms |
| --- | ---: |
| Envelope | 4,893.5195 |
| Envelope tables/indexes (inside envelope) | 2,246.8451 |
| Envelope owned files (inside envelope) | 2,308.2574 |
| Source authority open | 3,233.5711 |
| scoped_reference_ids (inside authority open) | 1,903.3897 |
| read_sources / ModelCatalog::new (inside authority open) | 0.8341 / 108.0503 |
| Source reference preload (inside authority open) | 1,040.0673 |
| Mapping validation, including snapshots | 3,538.6288 |
| Authority model resolution / snapshots (inside mapping) | 744.7100 / 277.6832 |
| Mapping structure (inside mapping) | 2,155.2102 |
| Mapped reference index | 648.6692 |
| Examined reference index | 1,004.4342 |
| Coverage/cell index | 240.4091 |

Engine SQLite is bundled 3.46.0, unlike Python's 3.51.0. The engine confirms `SCAN r` and
records INTEGER PRIMARY KEY lookup for the original examined-ID query. This identifies the
repeated scans as a real cost; source metadata discovery is not worth parallelizing.
The next iteration batches those scans, configures connection-local 64 MiB caches / up-to-2 GiB
mmap on dedicated read-only connections, reuses one bounded 1 MiB hash buffer per worker and
adds file-walk/finalization/worker and mapping-structure attribution. It retains every path,
byte, SQL and refusal check. These changes are not yet measured.

Both quick launches exit 1 after certification. The second quick log has this exact shader
failure (lines 33-40):

```text
2026-10-19T08:59:57.306804Z ERROR bevy_render::render_resource::pipeline_cache: failed to process shader error:
error: no definition in scope for identifier: `bevy_pbr::mesh_view_bindings::fog`
    ┌─ embedded://engine/shaders/water.wgsl:312:25
312 │         var miss_fill = bevy_pbr::mesh_view_bindings::fog.base_color.rgb;
```

No Vulkan VUID appears in this second quick log. The benchmark reports `passed=false`; no FPS
or visual acceptance is claimed. Raw `.log`, `.json`, `.phases.json`, `.gate-status.log`,
`.gate-observations.jsonl` and the second `.benchmark.json` are retained in the owned target.


### Round 3 retained optimization and verification before measurement

- The examined-ID reader uses one dynamically bound multi-world scan, compact ID/cell buckets
  and the original source-pair order (including duplicate source/block mappings). Any fast-path
  prepare/bind/step/decode failure replays the original implementation. Empty/single sources
  keep that original implementation. Six tests compare exact vectors and errors, including
  competing corruptions and the variable-limit/extra-column compatibility paths.
- Dedicated read-only connections use connection-local 64 MiB page caches and request up to
  2 GiB mmap (SQLite may clamp it). This is roughly 1 GiB of cache budget across 14 workers and
  two retained connections, not a content limit or persistent certification cache. Queries and
  byte verification still execute. Workers retain the original connection limits and recovery.
- Envelope table/index and owned-file checks overlap only with more than three CPUs; file
  hashing then uses available_parallelism - 3 workers, leaving two CPUs after the table worker.
  Both results are joined before selection; original table/index errors precede file errors.
  Low-CPU paths retain serial phase ordering. Spawn failure reports a diagnostic and performs
  original serial file checks; panic refuses after checking the table result. Four added tests
  compare converter digests and simultaneous-corruption error priority across serial/parallel
  policies. Spawn failure is not injected in a test. Every ancestor/link/regular-file check and
  every hash remains. File workers reuse one 1 MiB buffer each; they return digests, not files.
- Timings explicitly mark overlapping envelope subphases. owned_files includes owned_file_walk;
  worker sums overlap across workers; mapping_structure includes its new three subphases.
  Do not sum these nested/overlapping values as independent wall costs.

The first complete optimization test attempt ran 2,152/2,228 cases: 2,150 passed, two existing
parallel-preload tests failed and 76 were not run because of fail-fast. Added assertion output
identified `Execute returned results - did you mean to call query?`: mmap_size returns a row,
so the update-only pragma API incorrectly triggered original-read recovery. It now uses
pragma_update_and_check and consumes SQLite's effective value. No assertion was weakened.
The corrected shared-only run passed all 105 tests (1.086 s).

After integrating envelope overlap, fresh fmt and diff checks passed; engine/shared all-target,
all-feature clippy with -D warnings passed (18.58 s); full engine/shared all-target nextest with
--no-fail-fast passed 2,232 cases with eight existing skips (42.762 s). Logs:
`round3-clippy-overlap.log`, `round3-nextest-overlap.log`, and diagnostic/fix logs named
`round3-nextest-cache-diagnostic.log` / `round3-nextest-shared-cache-fix.log` in the owned target.
Independent review found no remaining refusal/trust blocker. Optimized quick build is next;
no sub-eight-second result is claimed from this verification.

### Round 3 warmed quick A/B at 403e09db

After the lead cleared its own hold, its owner reserved the machine until this task reports.
No other owner's engine or hold was stopped. Required workspace checks completed before timing:
all-target/all-feature workspace clippy with -D warnings passed (7m 26s); workspace all-target
nextest passed 3,920 tests / 50 skips (225.933 s); workspace doctests passed with four existing
ignored engine examples. Logs: `round3-clippy-workspace.log`, `round3-nextest-workspace.log`,
`round3-doctest-workspace.log`. These checks cover 403e09db, before the follow-up below.

Both binaries were built with `cargo build -p engine --profile quick`, through owner
`vg7-fast-startup`, CARGO_BUILD_JOBS=2 and CARGO_TARGET_DIR pointing to the owned target.
The base build used detached 44f5f0c1, then restored exp/fast-cluster-startup in a finally block.
The current quick build was restored after the workspace checks (3m 03s). Clean source status,
build commands, timestamps, sizes and SHA-256 are in the copied executable provenance files:

- `engine-base44f5f0c1-quick.provenance.json`: base44f5f0c1,
  `e2103bd8622443e9d675651846f5ec323678e97d705343a8cb38f6f1a65bb58f`.
- `engine-restored-403e09db-quick.provenance.json`: current403e09db,
  `b50a4ec3e13c217564b7ebcb43e72cfb7899acce06f29298fe13be8dec250247`.

First post-build launches were discarded. The base's earlier warm-up overlapped a lead leg and
is not comparative evidence. The restored current warm-up (18,646.0997 ms) was discarded too.
The measured base and current launches then ran back to back, with no intervening build:
`round3-ab-base44` and `round3-ab-optimized`. Each is the second launch of its respective built
binary. Their two-second gate observations contain zero other-owner entries; TEST supplies no
quiet GUARD verdict, so these are **single-run inference**, not guarded performance acceptance.
Both use exactly:

```text
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py test --label <label> -- <quick.exe> --assets D:/Mudcrab/SkyrimConverted-vg7-showcase-20261018-small-primitives --allow-incomplete-assets --cluster-package D:/Mudcrab/SkyrimConverted-vg7-showcase-20261018-small-primitives --terrain-radius 2 --benchmark-duration 30
```

| Phase (seconds) | Base 44f5f0c1 | Current 403e09db |
| --- | ---: | ---: |
| Envelope | 6.3308 | 3.2626 |
| Tables/indexes, within envelope | 2.6286 | 2.9325 |
| Owned files, within envelope | 3.2773 | 2.7874 |
| Source authority open | 4.0271 | 0.9422 |
| Source reference preload, within authority open | 1.9153 | 0.3583 |
| Mapping, including snapshots | 9.6600 | 4.4409 |
| Snapshot preload, within mapping | 4.5259 | 1.5069 |
| Mapped reference index | 1.7737 | 0.5717 |
| Examined reference index | 1.0149 | 1.3171 |
| Coverage/cell index | 0.2383 | 0.2238 |
| **Certification total** | **23.0449** | **10.7584** |

Current envelope child phases overlap; do not add them. Both verify 9,017 cluster-owned files,
3,069,899,039 bytes, and index 448,539 mapped instances / 210,140 mapped / 269,761 examined
references. The 8 s target is not met by 403e09db. The measured current source open attributes
295.4727 ms to scoped IDs, 125.4881 ms to catalog construction and 358.3007 ms to source preload.
read_sources is only 0.1702 ms. Mapping structure costs 2,611.1792 ms, including 1,443.0680 ms of
chunk/SQL rows, 781.5207 ms of coverage rows and 320.1408 ms of comparison. Snapshots add
1,506.9168 ms before structure, including 1,156.4269 ms of model resolution. Runtime indexing
then rereads already checked mapping and coverage rows. This identifies the next retained work.

Both quick A/B logs reproduce the exact water shader failure quoted above at
`embedded://engine/shaders/water.wgsl:312:25`, undefined
`bevy_pbr::mesh_view_bindings::fog`. Neither contains a Vulkan VUID. Both exit 1 and report
`passed=false`; no FPS or renderer acceptance is claimed. Raw logs, metadata, phases, gate
status/observations and benchmark JSON are retained in the owned target. Inventory evidence is
`round3-payload-inventory.json` / `round3-payload-use.json`; the second was obtained with two
single scans, not a repeated correlated query. No dataset/schema/index/file was changed.

### Follow-up: launch-local mapping facts and joined snapshot preparation

The prepared mapping owns only existing checked instance/coverage data and borrows its exact
SQLite connection. It cannot be cloned, serialized or used as a certificate. After joined source
preparation, consuming validation runs the original provenance/scope/reference/material/instance
checks in the same order. Only then may runtime indexing consume the original record ordinals
and checked rows. The 448k-key unmatched set is moved instead of cloned.

On more than three CPUs, serial structure checking overlaps model/snapshot preparation. That
worker reserves one additional busy CPU, so snapshot workers plus structure total at most
available_parallelism - 2. Both results join under the envelope's retained AssetLock, and source
snapshot errors retain their original earlier priority. Smaller CPU policies retain serial work.

Coverage projection requires an ordinary main table without COLLATE or temporary shadow, one
reference state, canonical scopes/statuses and u32 primitive indices. Otherwise both original
runtime SQL consumers remain, including their original errors. An info field records whether
coverage was reused. The mapped-record projection retains key ordering via the original BTreeMap.
No buffers or truth survive into another launch, and every original hash/source/refusal check runs.

Independent read-only review found no blocker or refusal/trust regression. Seven shared tests
cover real MCI ordinals, every authority phase, competing first refusals, SQL state/rejected-set
equivalence, duplicate reference states, collations, accepted wrapped indices and noncanonical
scopes. Two engine tests cover joined competing errors and serial refusal ordering. The source
preload equivalence matrix also exercises the extra CPU reservation. Explicit view/TEMP shadow,
outer snapshot-thread spawn failure/panic and small-CPU policy selection remain test gaps.
Fresh fmt/diff checks and engine/shared all-target/all-feature clippy -D warnings passed (38.41s).
Engine/shared all-target nextest with --no-fail-fast passed 2,241 tests / eight existing skips
(37.942 s), including all new regressions. Logs: `round3-clippy-reuse.log` and
`round3-nextest-reuse.log`. Whole-workspace revalidation and a new quick measurement follow;
no further speed result is claimed yet.

### Follow-up measurement at de013f2e (target still missed)

Whole-workspace clippy (`--all-targets --all-features -- -D warnings`) passed in 1m05s.
Whole-workspace all-target nextest passed 3,929 tests / 50 existing skips in 301.396s
(compile 6m54s); workspace doctests passed with four existing ignored engine examples.
No test timeout, refusal or settings were relaxed. Logs are `round3-final-clippy-workspace.log`,
`round3-final-nextest-workspace.log` and `round3-final-doctest-workspace.log` in the owned target.

The clean de013f2e quick executable was built with jobs=2 and copied to
`engine-final-de013f2e-quick.exe`, SHA-256
`71318e90e9b1a1b9c09345d867a9d44d820a1ce2783a40bcda6f9574d9b624f5`.
Its first launch, `round3-final-current-warmup`, was discarded (15.4253s certification).
The existing clean base44 quick binary was warmed again immediately before the measured pair.
The back-to-back measured labels are `round3-final-ab-base44` and `round3-final-ab-current`:

| Phase (seconds) | Base 44f5f0c1 | Current de013f2e |
| --- | ---: | ---: |
| Envelope | 6.1007 | 3.7600 |
| Tables/indexes, within envelope | 2.5365 | 3.4197 |
| Owned files, within envelope | 3.1486 | 2.2961 |
| Source authority open | 8.7082 | 3.2783 |
| Source reference preload, within authority open | 1.8818 | 0.3495 |
| Mapping, including snapshots | 9.5252 | 3.9544 |
| Snapshot preload, within mapping | 4.4881 | 3.6471 |
| Mapped reference index | 1.6033 | 0.2782 |
| Examined reference index | 1.0075 | 0.3332 |
| Coverage/cell index | 0.2294 | 0.1110 |
| **Certification total** | **27.1744** | **11.7153** |

These are **single-run inference**, using the exact TEST wrapper above, without a quiet GUARD
verdict. Two-second gate observations recorded zero other-owner entries. The base binary was
reused from its earlier quick build, with a fresh discarded warm-up; current is its second
post-build launch. Both reproduce the same water shader error and exit 1 / benchmark passed=false;
neither contains a Vulkan VUID. Counts and package identity remain unchanged.

The runtime coverage projection was reused. Mapping structure took 2.5666s and overlapped
3.6471s of snapshot preparation, including 3.2880s of model resolution. Source scoped-ID discovery
took 2.6085s, versus 0.2955s in the earlier 403e09db leg; catalog construction was 0.1405s and
read_sources 0.00025s. Runtime indexes together took 0.7224s. The 8s target remains unmet, and
the variation prevents treating one fast scoped-ID result as a reliable cost elimination.

Read-only investigation found the source database is 2,736,652,288 bytes; bundled SQLite 3.46.0
clamps mmap at 0x7fff0000 bytes, below its size. The references table has only a cell index,
and the batched query scans references then repeatedly looks up records by rowid. Separate
read-only SQL probes showed strong cache-order effects (2.381s direct query, 0.808s compact
materialization, then 0.452s direct query), with identical 323,499 ordered rows. Those probes
do not establish a speedup. A guarded distinct-base query and observational model/texture hash
counters are being measured before deciding whether to retain them. No source index or dataset
file was changed.

### Follow-up: concurrent texture verification misses

The exploratory dirty-source quick build (patch `round3-cte-metrics-source.patch`, based on
de013f2e) added counters before changing cache behavior. Its executable SHA-256 is
`8520dec48531efe00a891216ea4fa95ecc6a54d6e684557b21419a0c944418dd`.
`round3-probe-cte-warmup` was discarded (14.9681s certification); its second launch,
`round3-probe-cte-second`, certified in 8.3525s. These exploratory numbers are single-run inference,
not the final A/B. The warm-up hashed textures 6,467 times / 8,897,443,765 bytes for 2,429 unique
paths. The second hashed them 6,033 times / 8,684,652,899 bytes for the same unique paths.
Both resolved 2,948 unique models with exactly 2,948 computations and 375,889,208 hashed bytes.
Texture cache misses were computed outside the map lock, so workers repeated large full-file
hashes while another worker verified the same texture. The counters measure completed hash
bytes only; summed worker hash times overlap, and header-facts reads are separate.

The guarded distinct-base CTE was rejected before retaining code. An alternating read-only
Python SQLite diagnostic, after the second launch, returned identical 323,499 ordered rows:
direct queries took 494.55 / 476.69 / 478.90ms; CTE queries took 629.50 / 686.84 / 671.46ms.
This is a Python SQLite 3.51 diagnostic, not engine 3.46 acceptance, but it shows no reason to
retain additional schema guards/query complexity. `classify.rs` and EXPLAIN are restored to
the existing batched query. Artifact: `round3-cte-alternating.json`. No dataset/schema change.

The retained texture fix uses a per-path OnceLock inside the existing per-catalog map. The map
lock is released before initialization and hashing. Reserved aliases wait for their explicitly
published source's independent check, then check their own full bytes. Explicit published
outputs still take precedence. Source-first short-circuit and every reason string are unchanged.
Published sources terminate recursion, so alias/source waits cannot form a cycle. New catalogs
allocate fresh cells: nothing is serialized or reused across launches. The model cache is
unchanged because the new counters did not show duplicate model work on this package.

Three real-file regressions exercise concurrent same-path hashing, concurrent source plus two
aliases, and a fresh catalog refusing changed source/alias bytes while preserving the source-first
read count. Independent read-only review found no correctness/trust blocker. Full verification
and final warmed quick A/B remain required before completion.

Fresh checks for the retained texture fix: fmt and diff checks passed; workspace clippy with
all targets/features and `-D warnings` passed (1m28s); workspace all-target nextest passed
3,932 tests / 50 existing skips (302.915s execution, 7m36s compile), including the three new
concurrent/fresh-catalog regressions. The existing streaming fixture passed in 152.439s without
changing its configuration. Workspace doctests passed with the same four ignored engine
examples. These checks include engine and shared. Logs: `round3-texture-clippy-workspace.log`,
`round3-texture-nextest-workspace.log`, `round3-texture-doctest-workspace.log`.

### Final quick A/B at 42d8d393: 8s target remains unmet

Clean source commit `42d8d393` was built through the gate with
`BUILD_GATE_OWNER=vg7-fast-startup`, `CARGO_BUILD_JOBS=2`, the owned target, and
`cargo build -p engine --profile quick` (1m56s). The copied executable is
`engine-final-42d8d393-quick.exe`, SHA-256
`cb2c97ea6f13fdda687f66817d8711b54715f01e564ec705e17a2999d929fa9d`.
The base remains the clean quick build of `44f5f0c1`, SHA-256
`e2103bd8622443e9d675651846f5ec323678e97d705343a8cb38f6f1a65bb58f`.
Build/provenance JSON, exact commands, raw logs, phase JSON, gate observations and benchmark
JSON are retained in `D:/Mudcrab/builds/vg7-fast-startup-target`.

Launch order: `round3-final42-current-warmup` (first post-build launch, discarded, 15.0493s),
`round3-final42-base-warmup` (refreshed warm-up of the previously built baseline, discarded,
24.1259s), then **back-to-back** `round3-final42-ab-base44` and `round3-final42-ab-current`.
Current is its second post-build launch; the reused baseline has earlier runs, disclosed above.
All four launches use the exact TEST wrapper and arguments already quoted, including the same
assets/package, radius 2 and benchmark duration 30. No build, SQL probe or preview ran during
either measured leg. The lead's process/hold were left untouched. The gate's own queue was used;
no timing hold was taken. All four observations recorded zero other-owner gate entries.
TEST provides no quiet GUARD verdict: every timing conclusion here is **single-run inference**.

| Phase (seconds) | Base 44f5f0c1 | Current 42d8d393 |
| --- | ---: | ---: |
| Envelope | 6.4475 | 3.4744 |
| Tables/indexes, within envelope | 2.7885 | 3.1221 |
| Owned files, within envelope | 3.2101 | 3.0402 |
| Source authority open | 9.0261 | 3.2054 |
| Source reference preload, within authority open | 1.9199 | 0.3238 |
| Mapping, including snapshots | 10.0178 | 4.0098 |
| Snapshot preload, within mapping | 4.8918 | 3.6058 |
| Mapped reference index | 1.6564 | 0.2977 |
| Examined reference index | 1.1445 | 0.3074 |
| Coverage/cell index | 0.2416 | 0.0896 |
| **Certification total** | **28.5341** | **11.3845** |

Current envelope children overlap, and source snapshots overlap mapping structure. Do not add
those nested times. Both runs still verify 9,017 cluster-owned files / 3,069,899,039 bytes and
index 448,539 instances / 210,140 mapped references / 269,761 examined references / 8,080
examined cells / 7,606 mapped cells. Runtime coverage projection reuse is true. Manifest
fingerprints and DB size/mtime match the earlier pre-run fingerprint exactly
(`round3-final-package-before.json` / `round3-final42-package-after.json`). This fingerprint is
experimental input attribution only; it never authorizes certification or bypasses a read.

Measured source-open details: connection 1.1711ms, scope 0.2827ms, manifest read 25.0909ms,
manifest parse 142.8029ms, read_sources 0.2544ms, core identity 0.1000ms, scoped IDs 2,608.1500ms,
catalog construction 101.2990ms, and source preload 323.8044ms. The previously unattributed
gap is now measured. Mapping structure took 2,430.0336ms: chunk/SQL rows 1,109.8729ms,
coverage rows 897.8443ms, comparison 386.5518ms. It overlaps source snapshot preparation
(model resolution 3,271.9980ms, reference snapshots 284.0285ms). Authority checks took
244.8021ms, instance checks 15.4577ms and runtime projection 137.4157ms.

The texture fix removed duplicate work on this package: **2,429 hashes / 2,429 unique paths /
2,976,250,481 bytes**, in both final launches. Model hashes remain 2,948 / 375,889,208 bytes.
No isolated wall-time improvement from that fix is claimed: model resolution remains 3.2720s
in this A/B, and launch/cache variation was substantial in earlier probes. The 8s target is
**not achieved**; the final gap is 3.3845s. Remaining costs are the full source-reference scan
and record lookups on the fixed read-only DB (no world/exterior index), unique source file
verification, and envelope table/file verification. All required bytes still need authority.
The rejected CTE adds cost in controlled hot reads; changing converter indexes/data layout
requires a separate regenerated input comparison. Further concurrency would need a shared
CPU budget and joined error-priority proof under the retained source lock. Neither a skipped
file/verdict nor cross-launch digest trust was introduced to reach a smaller number.

Both quick measured runs exit 1 with `passed=false`. The exact current shader error is:

```text
2026-10-19T11:21:39.050067Z ERROR bevy_render::render_resource::pipeline_cache: failed to process shader error:
error: no definition in scope for identifier: `bevy_pbr::mesh_view_bindings::fog`
    ┌─ embedded://engine/shaders/water.wgsl:312:25
312 │         var miss_fill = bevy_pbr::mesh_view_bindings::fog.base_color.rgb;
```

Neither measured log contains a Vulkan VUID. The raw benchmark means are 61.1138 FPS (base)
and 43.4846 FPS (current), both failed; startup occupies different amounts of their fixed
30-second windows. No renderer/FPS acceptance or causal FPS comparison is claimed.
The fog shader remains outside this startup change. No publish/push or integration occurred.
The owned expired `__pycache__` folder was removed after verifying its owner and exact two
files; retained build/timing evidence was kept. Gate slots and holds were clear after the runs.
