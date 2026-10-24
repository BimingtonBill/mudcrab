# First cluster conversion (#2144.1)

## Inputs and execution

The baseline is `C:/Modding/Mudcrab/SkyrimConverted-main28`, produced by the immutable
`23aaff42` converter documented in `D:/Mudcrab/review/demo-onmain-23aaff42`.
Its command uses `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data` and
default conversion settings. The new command adds `--lod-mode cluster
--cluster-terrain`, matching the launcher's checkbox. Converter defaults are unchanged
between the baseline source and `684341e5`, apart from comments about geometry mode.

The new destination is `D:/Mudcrab/SkyrimConverted-cluster1`; no existing converted
dataset is an output. The requested ownership record is written after publication,
because the converter correctly refuses an otherwise unknown nonempty destination.
Input hashes and baseline diagnostics are recorded in target-local
`2144-1-baseline-provenance.json`. All builds use the shared gate with owner
`sol-2144-1` and target `D:/Mudcrab/builds/no-stand-ins-target`.

The lead explicitly ruled that conversions may coexist with gated builds and cleared
the initial conversion hold. No conversion hold may be recreated. Subsequent converter
execution uses normal gate admission and memory preflight, with no timing claim.

## Attempt 1

- Source: `684341e52756fc000bd6c5e1123fb613f6483359`.
- Release build passed; Cargo reported 6 minutes 39 seconds.
- Visible titled console launched one converter. It failed before publication, exit 1,
  after 919.5643369 seconds: `cluster terrain requires a LOD origin for worldspace
  WindhelmWorld (0001691D)`.
- The retained staging folder is
  `D:/Mudcrab/SkyrimConverted-cluster1.staging-25444-1792854350683885900`.
  It belongs to this task. The published output and derived package do not yet exist.
- The baseline skips worlds without native LOD origins. Cluster mode instead needs
  their geometry; a native LOD sidecar is not a necessary cluster input. A read-only
  query found 48 worlds with cell grids but no populated origin in the interrupted
  staging database. Missing values after the first failed world also include worlds
  that the interrupted loop had not processed yet.
- The fix selects missing cluster grid origins from the component-wise minima of
  actual cell coordinates and records their source explicitly. Existing authored
  origins retain priority; baked and auto modes retain their previous rules. No zero
  origin, world identifier or game-specific path is introduced into product code.

## Origin regression proof

Two real pipeline fixtures failed before the fix: terrain in a world without native
LOD settings, and an object-only world with nonzero cell minima. After the fix, all
21 focused tests passed. Additional coverage checks baked/auto parity, invalid native
settings, native-origin priority, incomplete coordinate pairs and empty worlds. The
target-local `2144-origin-receipt.json` records the red/green logs and run identifiers.

Independent review found no actionable issues. Reviewed Git blobs were
`af9bda48537cecc953a444cd2cf796852fb8e685` for `pipeline.rs` and
`bade41feb5e563b1432801342d7af5edf633ff3c` for `pipeline/overlay_tests.rs`.

All required checks passed through the gate: formatting, converter and workspace strict
Clippy, converter nextest **1,801 passed / 45 skipped**, workspace nextest **5,450 passed /
63 skipped**, and doctests **18 passed / 15 ignored**. The two-file source snapshot stayed
`2d44b3d51d21566230cf89cf81580986b124514439d84eb12373f42ee78118fd`.
The coordinator rechecked the current source and all seven log hashes/exit receipts.
The full receipt is target-local `2144-1-verification-summary.json`.

## Published result

The fixed release converter was built from commit
`874c080b03c417953e35d617e7a2fba1bd10088a`; its SHA-256 is
`56c67200594b27e4cb19006d17d65ea2897525834124f7aa84ddf92a8aac0f18`.
One visible titled console resumed the retained staging through ordinary build-gate
admission. No conversion hold was set. The converter published the new destination
and its manifest-selected package, `derived/clusters-8d3ebd4db45517df`.

- Resume wall time: **993.1436008 seconds (16 minutes 33 seconds)**, including gate
  admission. The log shows immediate admission; converter report time is 988.943 seconds.
- Attempt 1 plus resume: **1912.7079377 seconds (31 minutes 53 seconds)** of execution,
  excluding rebuilds and the gap between attempts. This is a resumed run with reused
  assets, not a clean timing comparison against main28.
- Requested `dust -d 1 -r` result: **19Gi**, including the derived package. A separate
  file scan found **57,102 files / 21,185,882,513 logical bytes**. The derived directory
  accounts for approximately 3.3 GiB in dust output.
- The manifest records `geometry_mode: "cluster"`; its exact package settings enable
  terrain and trees at the default triangle threshold of 1024. The package has 1,547
  mesh payloads, 51,277 cluster instances (19,512 tree instances), 121 object blocks,
  301 terrain blocks, 52,181 height rows and 176 unique terrain atlas files. All 2,145
  referenced package files are present; all terrain atlas hashes match.
- No native BTO/BTR products, generated billboard GLBs or compiled baked terrain GLBs
  exist. Native inventory and chunk tables have zero rows. All 199 retained textures
  in the LOD candidate namespaces have surviving dependencies; no alias hash mismatch
  or unjustified retained texture was found. All 37 derived origins match actual cell
  minima.
- Load order, archive source hashes, plugins, localization and reader settings match
  main28. DDS and PEX configuration hashes match. The NIF hash differs because the
  current producer adds `authored_transform_animation: 1`; both configuration hashes
  were reproduced from their respective producer contracts. This predates the origin fix.

The resumed conversion exited **1** with exactly the same three reader warnings as
main28, zero skipped inputs and **zero asset failures**. Its `complete` and
`record_data_complete` flags remain false, matching that baseline limitation. It has no
new top-level warning/error log lines. The existing cluster prototype notice about
unassigned `stable_id` remains; no shipping or engine acceptance is implied.

Integration validation passes. Read-only review explains every changed count through
upstream commit `9e406da49`, which adds TACT, ADDN, APPA and ARTO to the statics
projection. Those four types add 487 rows: 258 bounded, 172 with already-empty GLBs,
11 with source models unavailable in both datasets, and 46 without a model. This
exactly explains the increases in models, bounds, unavailable sources and unbounded
models. All 28,325 existing static rows retain identical paths and bounds across their
common columns; the current schema also has an additive display-name column.

The four additional room overlap rows belong to existing references whose two TACT
bases now enter that projection; neither reference transforms nor room geometry changed.
The first-100 diagnostic samples were reconstructed exactly: the new scope inserts
21 distinct messages and displaces 21 older messages without fixing or newly breaking
those older models. All 405 available unique models behind the added rows match
baseline source hashes and actual GLB bytes (810 file hashes checked). The NIF cache
identity change explains none of these integration deltas. No additional cluster
conversion regression was found. Full proof is target-local
`2144-1-integration-diagnostic-review.json`.

Proof is retained in target-local `2144-1-resume-validation.json`,
`2144-1-resume-audit.json`, `2144-1-resume-shared-textures.json` and `2144-1-dust.txt`.
The requested console log and exit receipt are
`D:/Mudcrab/SkyrimConverted-cluster1-convert.log` and
`D:/Mudcrab/SkyrimConverted-cluster1-convert.exit`. Original attempt 1 evidence is
archived under `2144-1-attempt1-*` in the target. The coordinator verified every proof
hash, independently scanned the output for omitted products and confirmed zero native
inventory/chunk rows, then independently compared all common columns of the 28,325
existing statics. Baseline evidence hashes remain unchanged. No engine was run.
