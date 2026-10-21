# VG7 fast startup: recorded-input and cache preflight

Inspected at `ce7978ef`, in the isolated `exp/fast-cluster-startup` worktree.
No runtime behavior was changed. This records the user's task-2 stop condition,
not a completed startup optimization or a timing result.

## 1. Recorded cluster payload digests

Read-only inspection of
`D:/Mudcrab/SkyrimConverted-vg7-showcase-20261018-small-primitives` found:

- `cluster-manifest.json` contains summary/version/builder fields, with no
  per-file digest list.
- `conversion-manifest.json` has 80,670 conversion entries; zero entries name
  `clusters/` files or `cluster-manifest.json`.
- The `mudcrab.clusters` extension owns `cluster-manifest.json` and `clusters/`,
  recording only an aggregate extension hash, ownership and statistics.
- The database does record payload digests: `cluster_meshes.content_hash` and
  `cluster_chunks.content_hash`. These are not a persisted manifest inventory
  of **every** file covered by the extension's folder ownership.

`crates/converter/src/cluster/publish.rs` already constructs a sorted
`BaseFinal.files: Vec<OwnedFile>` containing path, bytes and SHA-256 for every
owned file. Its manifest serialization writes summary fields, and the extension
serialization writes the aggregate hash; neither persists that list.

Proposed converter change (not implemented): persist the finalized payload
inventory in the package manifest, excluding the manifest's own digest to avoid
self-reference. Keep hashing the manifest itself when recomputing the envelope.
Inventory generation must include extra owned files, not just mesh/chunk rows.
The runtime can then use `recompute_recorded` for listed payloads and hash
unrecorded files, retaining path/ownership checks and verifying payload bytes
against their expected digests before use, as the native LOD path does. Prove
missing, additional, malformed, conflicting and changed-file cases in fixtures.
This proposal alone does not solve the persistent-cache blocker below.

Existing `runs/bench-own.log` evidence, inspected rather than rerun:

- clusters `tables_and_indexes`: 4,151.8743 ms;
- clusters `owned_files`: 4,191.3106 ms, 9,017 files, recorded=0,
  read=9,017, bytes=3,069,899,039;
- envelope: 11,501.347 ms;
- source_authority_open: 4,723.6878 ms;
- validate_mapping: 9,876.6884 ms;
- mapped_reference_index: 8,614.6537 ms, 448,539 records;
- examined_reference_index: 1,252.1538 ms.

These are supplied baseline observations, not before/after evidence from this
worktree. No speedup is claimed.

## 2. Persistent certification: stop condition reached

The requested key (validator build, manifest digest, database path/size/mtime/
recorded identity, input recorded digests) cannot distinguish the following:

1. Certify a valid package and store its derived indexes.
2. Change one byte in a source GLB or texture without modifying either
   manifest or the database.
3. Open the package again: every requested key component is unchanged,
   but full certification must reject the input.

This is an existing supported refusal, not a hypothetical additional threat.
`crates/engine/src/cluster/reader.rs` already has
`authoritative_reader_refuses_changed_core_or_input_bytes_with_unchanged_envelope`.
Its model/texture cases flip the last byte while leaving the envelope untouched;
the envelope opens, then authoritative certification refuses the changed input.
`crates/shared/src/cluster_recipe/resolve.rs` implements current-file hashing in
`ModelCatalog`, independently of the conversion manifest's recorded digests.
The existing test was inspected, not executed in this investigation.

The measured database's `lod_build` has only `id` and `build_identity`; its
recorded identity is
`e81cef8960eed6c02c80540e804e7ac35ae54ca17c0e704998586b163340dc6d`.
That value does not track edits to separate GLB/texture files. SQLite sidecars
would also need an explicit policy before relying on main-file metadata.

### Trust model in plain words

Recorded hashes say what the producer published, not what the files contain
now. A cache that trusts them across launches assumes nobody edits the files
without updating those records. The shared asset lock coordinates cooperating
readers and writers during an open; it does not prove that assumption between
launches. Trusting it would weaken the existing unchanged-manifest corruption
check. File size and mtime also do not establish content equality.

Preserving the check requires rereading/hashing source bytes or introducing a
trusted immutable generation/change-tracking mechanism covering every source
and payload, with invalidation and recovery proof. No such mechanism is provided
by the current manifest/asset-lock contract. Rehashing multi-GB inputs on a hit
is explicitly disallowed by the task. Per the instruction, stop here: no cache,
cache trust-model Rust doc comment, or new acceptance of recorded bytes was
introduced.

## Remaining tasks and verification

Tasks 3 (scene prefetch overlap) and 4 (incremental ownership projection) were
not started, respecting the task order and explicit task-2 stop instruction.
They are not claimed to depend technically on solving task 2.

Only this evidence note and the handoff log are changed. The requested worktree
`OWNER.txt` exists with the exact supplied text and is ignored by repository
policy. No target folder was created, no engine timing bench was run, and no
Rust implementation or tests were changed. Cargo fmt/clippy/nextest were not
run: the work stopped at preflight, and the project exempts documentation-only
commits from Cargo checks. There is no unit or phase-log evidence of a fix.

D: disk preflight passed at 67.7 GB free for a 25 GB request. All inspection of
the measured dataset was read-only. No push, integration-checkout edit, or
cleanup of another owner's folders was performed.
