# Cluster mode covers every supported primitive (#2144.2)

## Problem and policy

The first forced-cluster conversion selected threshold 1024. Its package had
51,277 mapped instances and 503,454 primitive coverage rejections with reason
`below_triangle_threshold`. The cluster-only base contains no baked replacement
for these primitives, so the previously reported frame rate cannot establish
rendering or performance acceptance.

Forced cluster conversion now resolves the object threshold to one, independently
of the optional auto-overlay threshold setting. The manifest selector, actual
derivation, recorded recipe, and retry command all use that resolved value.
Standalone derivation refuses a threshold other than one for a cluster-only base.
Auto and baked conversion retain their existing behavior. This changes neither
material refusal rules nor stock draw permissions.

## Refused-reference diagnostics

At first qualification for each package/worldspace, the engine reports two
separate measurements for model-bearing STAT/TREE references beyond the loaded
grid that have no planned distant stock path:

- `cluster/undrawn_static_geometry`: immutable geometry refusals, including
  unclassified and below-threshold refusals. A nonzero count is an ERROR and a
  correctness gate.
- `cluster/undrawn_runtime_refused`: runtime-state refusals such as enable-parent,
  initially-disabled, and scripted references. A nonzero count is a WARN. It is
  reported honestly as an existing coverage gap and does not grant stock
  permission or fail the immutable-geometry gate.

This split follows the lead's explicit decision after inspecting the main28
baseline. Runtime-refused STAT/TREE references exist in that baseline as well;
combining the two classes would incorrectly imply that main28 has no gap.

Follow-up: provide state-aware distant drawing for enable-parent and
initially-disabled statics. Their distant ownership and enabled state need an
explicit contract; this task does not change that policy.

## Rebuild procedure

The original overlay recipe enables terrain and trees. Its original conversion
report records 16 jobs; the `cluster_build` table itself does not store scheduling.
The recorded detached terrain-source directory had already been removed. All 66
required DDS sources were recovered as hardlinks from cluster1's ingestion cache
and independently matched to the recorded SHA-256 values (72,011,152 logical
bytes). They are retained under `D:/Mudcrab/cluster-threshold-terrain-sources`
with task ownership.

The new overlay uses a distinct label so deriving it cannot retire the old
package. Its manifest selector is resolved through the converter's own settings
API and atomically selected under the base's exclusive asset lock before the
derivation computes its base fingerprint. The original manifest bytes are backed
up in the task target. The old overlay is retained at the lead's request; the
lead will retire it after timing the replacement package.

## Evidence

Task-owned logs and receipts reside in
`C:/MudcrabBuilds/cluster-threshold-target`. The conversion-policy red test
observed a recorded threshold of 1024 where one was required. The standalone
derive red test observed successful derivation with threshold 12 where refusal
was required. The positive regression reuses the existing certified synthetic
base with an 18-triangle STAT model and asserts emitted small-mesh payloads and
mapped instances, zero size refusals, and the recorded threshold and selector.

Native runs are correctness TEST runs through the build gate. They do not
establish frame-rate acceptance. No timing hold, push, or pull request is part of
this task.

## Rebuilt package and native qualification

The selected package is
`D:/Mudcrab/SkyrimConverted-cluster1/derived/clusters-b42ebe26e64f9912`, label
`all-primitives`. Derivation exited zero after **555.356 seconds** (9 minutes
15 seconds), excluding gate admission, using the development profile. Dust
reports approximately **3.60 GiB**. These are rebuild observations, not a
performance comparison.

The instance map now contains **554,731** rows, exactly 503,454 more than the
original package's 51,277. Coverage remains 948,250 rows; every original
`below_triangle_threshold` row became expanded coverage. That rejection reason
is absent from both primitive rejects and reference coverage. All other coverage
reason counts remain unchanged. Remaining primitive rejects are `effect_shader`
13, `missing_attributes` 48, `non_rendering` 26, `not_opaque` 1,095, and
`unsupported_material_feature` 1,125.

Main28 has 509,869 mapped instances. The entire 44,862-row difference comes from
36 additional worldspaces in cluster1; every shared worldspace has exactly the
same mapped-instance count. This reflects the wider cluster-only worldspace
scope established by the prior origin fix, rather than a change to thresholds
for shared worlds. The per-world comparison is retained in
`worldspace-instance-comparison.json` in the target.

Both native TEST runs used the same rebuilt engine and Tamriel cell `(5,-12)`,
radius 2. The first-qualified structured log reports:

| Dataset | `undrawn_static_geometry` | `undrawn_runtime_refused` | `other_refused_inside` | `other_refused_beyond` |
| --- | ---: | ---: | ---: | ---: |
| Rebuilt cluster1 | 0 | 4,275 | 0 | 0 |
| Main28 | 0 | 4,275 | 0 | 0 |

The runtime gap's read-only reference projection is 3,230 enable-parent, 1,034
initially-disabled, and 11 scripted static references. Native logs confirm the
aggregate. Both runs contain the expected runtime WARN and no ERROR. They were
stopped by their exact process IDs after the requested qualification; full
render readiness, screenshots, and frame-rate acceptance are not claimed.
Counter values are read from the explicit first-qualification log. The matching
ProfilingState JSON export is covered by the engine unit tests, rather than
being inferred from an absent native export file.

Main28's manifest SHA-256 remains unchanged. The old cluster1 package is retained;
the target holds the original manifest bytes for rollback. No dataset was
deleted. The task-owned build target and source DDS hardlinks remain available
for review and repeat derivation.

## Code review and verification

Independent converter-policy and engine-diagnostic reviews found no actionable
findings. Both were static reviews; their conclusions do not substitute for the
test and native qualification evidence above. Focused red/green regressions
passed six converter tests and eleven engine tests. Strict converter/engine and
workspace all-target/all-feature Clippy passed. Converter/engine all-target
nextest passed 5,076 tests with 58 skipped; the final workspace nextest passed
5,498 tests with 63 skipped. Workspace doctests passed 18 tests with 15 ignored.
Formatting passed. Final gate receipts are retained in `check-receipts.jsonl`
under the target.

The first workspace nextest attempt stopped after 1,431 passes because
`diagnostics_are_byte_identical_in_fresh_folders` encountered Windows error 5
(`Access is denied`) creating its diagnostic journal in the system temporary
directory. It did not reach a byte-comparison failure. Neither the test nor the
journal implementation differs from the base commit. That test passed in the
converter/engine suite and again in an unchanged focused workspace rerun. The
initial failure log and focused rerun are retained; no retry, skip, or weaker
assertion was added to production or test code.
The complete workspace rerun passed without source changes or test exclusions.

Owned Cargo incremental-cache files were retained using NTFS compression:
72,256 files, 28,585,811,199 logical bytes stored in 17,710,353,543 bytes. The
compression command exited zero. No old dataset or build artifact was deleted.
