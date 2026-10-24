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

The resume result, package proof, size and diagnostics will be recorded after they are
available. Main28 already has three reader warnings and an incomplete manifest; an
exit code of 1 alone cannot establish a new conversion defect.
