# Cluster certification handoff — 2026-10-18

User requested wind-down and VG7 takeover. **The few-seconds goal is not met.**
Nothing pushed; no active own build, TEST or timing hold. Base: `210ab8e5`.

## Commits in order

1. `6d7b6b48` — bulk source/coverage queries, exact retained source snapshots,
   plain-thread hashing/snapshot preparation and faster byte-identical hex encoding.
   Full-path TEST: **104.227 -> 25.234 s**, identical certified counts.
2. `827f54be` — **unverified handoff** cache, independently droppable. Completed
   cold/warm TESTs prove functional startup/counts, but warm **25.029 s** misses
   performance acceptance. Final workspace gates remain incomplete. Dropping it
   retains commit 1's full-path improvement.
3. The following docs-only commit records this handoff and LOG entry.

Only certification and its direct shared consumers changed. Small shared-file
changes outside `cluster_recipe`: hex encoders in `cluster.rs`/`extensions.rs`;
serialization derives only in `cluster_mapping.rs`. No streaming/portal edits.
TREE OR STAT `0x40`, plus optional static `0x8000`, are folded into the bulk query.
`recompute` is `pub(crate)`. The accessor was left untouched: retain VG7's incoming
`placed_cells_in_window` when rebasing after its parity commits.

## STAT bit 6 provenance and certified-scope counts (2026-10-18 follow-up)

xEdit's **STAT record-header** definition explicitly labels zero-based bit 6
`Has Tree LOD` (`1 << 6 = 0x40`):
[TES5Edit `wbDefinitionsTES5.pas`, pinned commit `9fb016884bec138ea6c7b872cec831537d464c3e`, lines 10074–10081](https://github.com/TES5Edit/TES5Edit/blob/9fb016884bec138ea6c7b872cec831537d464c3e/Core/wbDefinitionsTES5.pas#L10074-L10081).
The fetched raw file's SHA-256 is
`d79665d0812c47ea9212296ed3436c9565453f5c046d6ef98c321c426a7f683d`.
This is a header flag, not the STAT `DNAM` flags. The converter preserves the
header bits unchanged: [exporter.rs](../../crates/converter/src/esm/exporter.rs),
lines 785–807 at `283be715`, handles both STAT and TREE in the shared `statics`
table and writes `record.flags` into `statics.flags`.

Read-only SQL against
`D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified/skyrim_world.db`
on 2026-10-18 gives:

| Definition in the certified examined scope | References |
| --- | ---: |
| All examined references | 269,761 |
| `record_type = 'TREE'` | 129,949 |
| `record_type = 'STAT'` | 139,812 |
| STAT with header bit 6 set (additional references) | 94 |
| TREE OR STAT with header bit 6 set | **130,043** |

The additional 94 are all STAT bases with flags exactly `64`:
`DLC2TreePineShortHeavySnow` (`0403383D`, 52 references) and
`DLC2TreePineShortHeavySnow01` (`0403383C`, 42 references). IDs are observations
from this dataset's load order, not constants in the implementation. No TREE
reference in this scope has bit 6 set, so requiring that bit on TREE would omit
all 129,949 of them.

Vanilla dump cross-checks (`D:/Mudcrab/skyrim-records`) identify the two bases
as `03383D:Dragonborn.esm` and `03383C:Dragonborn.esm`, both STAT with
`MajorRecordFlagsRaw: 64` and `MajorFlags: HasTreeLOD`. Commands:

```powershell
python tools/skyrim_rec.py show DLC2TreePineShortHeavySnow --type Statics --plugin Dragonborn
python tools/skyrim_rec.py show DLC2TreePineShortHeavySnow01 --type Statics --plugin Dragonborn
```

For a field used to select **tree-LOD treatment**, including these flagged STATs
is supported by the header definition; TREE-only expresses literal record type.
Neither definition alone proves a usable billboard exists. These counts cover
the certified examined set, including its rejected/unknown references, not every
reference in the database. No Rust or accessor change was made in this follow-up;
VG7 owns the parity/rebase decision.

Exact count command (PowerShell; Python standard library, database opened read-only):

```powershell
@'
import sqlite3
c = sqlite3.connect('file:D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified/skyrim_world.db?mode=ro', uri=True)
base = '''WITH examined AS (
 SELECT DISTINCT reference_id FROM cluster_reference_coverage WHERE scope='reference'
), bases AS (
 SELECT r.id,r.base_form_id,b.record_type,s.editor_id,s.flags
 FROM examined e JOIN "references" r ON r.id=e.reference_id
 JOIN records b ON b.form_id=r.base_form_id LEFT JOIN statics s ON s.id=r.base_form_id
) '''
print(c.execute(base + '''SELECT count(*),
 sum(record_type='TREE'), sum(record_type='STAT'),
 sum(record_type='STAT' AND (coalesce(flags,0) & 64)!=0),
 sum(record_type='TREE' OR (record_type='STAT' AND (coalesce(flags,0) & 64)!=0)),
 sum(record_type='TREE' AND (coalesce(flags,0) & 64)!=0) FROM bases''').fetchone())
print(c.execute(base + '''SELECT printf('%08X',base_form_id),editor_id,flags,count(*)
 FROM bases WHERE record_type='STAT' AND (flags & 64)!=0
 GROUP BY base_form_id,editor_id,flags ORDER BY count(*) DESC''').fetchall())
print(c.execute("SELECT count(*),count(DISTINCT reference_id) FROM cluster_reference_coverage WHERE scope='reference'").fetchone())
'@ | python -
```

Results: `(269761, 129949, 139812, 94, 130043, 0)`;
`[('0403383D', 'DLC2TreePineShortHeavySnow', 64, 52), ('0403383C', 'DLC2TreePineShortHeavySnow01', 64, 42)]`;
coverage rows/distinct references `(269761, 269761)`, confirming no missing join
rows or multiplication. Existing research read for context:
`docs/research/skyrim-lod-formats.md` and canonical local
`research/mods/lod-worldspace-seamless.md`; the pinned xEdit definition above is
the primary bit-number evidence. Docs-only validation: SQL above, diff and
whitespace review; no new build or engine TEST.

## Completed TEST observations

Shared-PC, normal quick profile, no timing hold: these are startup observations,
not isolated benchmarks, FPS or visual/demo acceptance. Baseline is the open run
in [the original measurements](cluster-measurements-2026-10-18.md). All values
below are **seconds**; warm “reused” means checks are bound to the prior complete
certificate by matching actual input-content hashes, not freshly executed SQL.

| Sequential phase | Baseline | T1 bulk/hash | T2 full, no disk cache | T3 cold/cache population | T4 warm hit |
| --- | ---: | ---: | ---: | ---: | ---: |
| Envelope | 16.557 | 14.308 | 9.272 | 5.028 | reused |
| Source authority | 2.598 | 5.616 | 5.109 | 3.482 | reused |
| Mapping validation | 40.531 | 13.580 | 7.888 | 6.457 | reused |
| Mapped index | 3.002 | 1.525 | 1.491 | 0.995 | restored below |
| Examined index | 41.287 | 1.139 | 1.185 | 1.147 | restored below |
| Coverage/cell index | 0.251 | 0.277 | 0.290 | 0.374 | restored below |
| Cache read/integrity/engine identity | — | — | — | initial miss | 0.236 |
| Cache population, **including input hashing** | — | — | — | **19.443** | — |
| Cache input enumeration/scheduling/hashing | — | — | — | 17.889, nested above | **24.091** |
| Cache index restoration | — | — | — | included in write verification | 0.694 |
| **Total** | **104.227** | **36.446** | **25.234** | **36.926** | **25.029** |

T3 full phases before population total 17.482 s. Cache population is real startup
work, included in total. Different shared-PC/cache conditions prevent attributing
all inter-run differences to code. T2 nested source-row preload 1.965 s and exact
snapshot preload 5.194 s are included in authority/mapping respectively.

All four certify **37,866 records, 35,732 mapped references, 269,761 examined
references, 8,080 examined cells and 6,395 mapped cells**. Runtime buffer counts
also match: 1,230 meshes, 38,126 instances, 1,485 batches, 340 blocks; 260 terrain
batches become ready. Every run exits 1 naturally after the same three unchanged
30-second shot-readiness failures with far-stock backlog/pending assets. No PNG
or visual/demo acceptance. Each log has zero ERROR-level records and six WARNs;
the final nonzero exit is still a failed TEST, not a successful demo run.

Raw evidence (gitignored, retained for VG7): `local/cert-speed/<label>/` with
`identity.json`, complete `engine.log`, `certification.txt`, exact `source.patch`,
source SHA-256 and immutable executable. Labels/images:

| TEST | Label | Engine SHA-256 |
| --- | --- | --- |
| T1 | `first-bulk-hash` | `bc4ac8b302292b1f929fb89f6b715302451b98b89bf0e0a1d1153b8d6aea0d5a` |
| T2 | `second-balanced-parallel` | `8d3d822df9fab48eb9034d237fafc4084c4e1b3de1c7f6c45063c54f5590e3e4` |
| T3/T4 | `third-cache-populate` / `fourth-initial-cache-hit` | `04b3fb35bd265fd498bc92d51a8d38994c6626382c331920c61e5740fb329f1a` |

Every retained Rust file was SHA-256-compared with T3/T4's recorded source after
removing the unfinished follow-up. They all match. The proposed metadata-sort
fix and parallel DB-range fingerprint are **not retained**; no half-built helper
remains in the tree.

## Cache cost, trust and open items

Every hit hashes **7,092 files / 9,446,364,262 actual bytes**: raw DB (2,599,047,168),
conversion manifest (66,427,387), owned base+terrain files (2,939,763,914), source
models/textures and any unowned mapping MCI dependencies (3,352,139,689), and
terrain cell cache (488,986,104). Present SQLite WAL/SHM/journal sidecars are also
hashed; their absence is bound. The 195,454,855-byte certificate is integrity
checked, and the actual running engine executable, builder and format bind reuse.
No timestamps, manifest claims or previously recorded file hashes replace reads.
The source guard/path policy remains. Missing/corrupt/changed cache inputs log
full recertification; transient source-read failures prevent persistence.

**Open performance defect:** `cert_cache::content` uses `sort_by_key` with an
`fs::metadata` key, repeating filesystem calls during sorting. It needs a measured
fix; scheduling and hashing are not yet separate timers. The raw DB's flat SHA
alone took **5.791 s (T3) / 9.145 s (T4)**, and source-file outer-worker elapsed
sums were 5.605 / 65.598 s (parallel, not additive wall time). Thus an unmeasured
sort fix cannot be claimed to meet the goal. The full-content hash floor remains
seconds. Any decision to trust inputs without rehashing belongs to the user.

The cache is engine-owned under `%LOCALAPPDATA%/Mudcrab/cluster-certification-v1`,
outside the dataset. Its checksum detects corruption, not a malicious same-user
cache forgery. Source views/external virtual tables decline persistence; allowed
extension virtual/shadow tables must exactly match the trusted persisted R-tree
schema. IO/schema incompatibility can therefore cause safe full-path misses.
A new engine binary invalidates reuse even for unrelated code changes.

Independent reviews resolved query-wide first-refusal order, unowned MCI input
coverage and dynamic SQLite-view dependencies. **No known remaining source-review
blocker; acceptance remains incomplete.** Mapping records already use a prepared
composite-PK query per chunk; that validator was retained, including refusal order.
Source/coverage oracles compare old per-row reads, full snapshots/digests/poses and
exact refusal text, including malformed/missing inputs and cold model/texture/alias
cases. Nine cache tests passed at the focused checkpoint. The later stored-table
eligibility and dynamic-view regression were reviewed and included in the normal
T3/T4 image's source; that regression itself has **not been executed**.

## Exact build/check/run commands and limits

All cargo used this environment and gate (no shared target, no incremental):

```powershell
$env:CARGO_TARGET_DIR='D:/Mudcrab/builds/cert-speed-target'
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_INCREMENTAL='0'
python tools/disk_guard.py --drive D: --need 20
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p shared -p converter -p engine --profile quick --all-targets --all-features -- -D warnings
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run --cargo-profile quick -p shared --all-targets
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run --cargo-profile quick -p shared -p converter -p engine --all-targets -E 'test(cluster)'
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick -p engine
```

Checkpoint fmt/clippy passed (logs `fmt*.log`, `clippy-focused.log`,
`clippy-second.log`, `clippy-cache-first.log`). They are **not final-head workspace
validation**. Shared nextest passed 92/92 at its earlier checkpoint. Focused nextest
passed **389/389**, with 3,261 excluded by the filter, across 114 binaries
(`nextest-cluster-cache.log`). Normal final-cache build passed
(`build-cache-final.log`). Final formatting ran before that image. No compile or
test failure was waived. **Full touched-crate nextest, final workspace clippy,
workspace nextest and doc tests are outstanding**; none were running at wind-down,
so no new campaign was started. The commits are for takeover, not acceptance.

The runner invocation for each row was:

```powershell
python local/cert-speed/run_test.py first-bulk-hash
python local/cert-speed/run_test.py second-balanced-parallel
python local/cert-speed/run_test.py third-cache-populate
python local/cert-speed/run_test.py fourth-initial-cache-hit
```

It copied the normal executable and unchanged cycle34 `shots.json` into each
owned evidence folder, then executed this exact argument template (expanded paths
and labels are preserved per-run in `identity.json`):

```text
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py test --owner cert-speed --label <label> -- <worktree>/local/cert-speed/<label>/engine.exe --assets D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified --cluster-package D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified --cluster-terrain --allow-incomplete-assets --graphics showcase --load-doors seamless --shots <worktree>/local/cert-speed/<label>/shots.json --shots-out <worktree>/local/cert-speed/<label>/shots --show-window --profile-output <worktree>/local/cert-speed/<label>/profile --run-label <label> --run-owner cert-speed
```

No far-terrain-shadows argument, deadline or other graphics/loading setting changed.
The immutable shot source was
`C:/Modding/Mudcrab/builds/vg7-cycle34-fallback-runtime/shots.json`.

Cleanup: attempted removal of the verified own expired target
`D:/Mudcrab/builds/cert-speed-target`; automatic approval review rejected it before
execution with **blocked by policy**. Nothing deleted; no alternate mechanism.
The target and tiny runner bytecode cache remain under H19 custody; the canonical
local disk ledger records this. Raw proof remains owned by VG7 for takeover.
