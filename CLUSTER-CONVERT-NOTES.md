# Cluster conversion selection (#2144)

## Contract and scope

The launcher offers **Clustered geometry** before conversion, defaulting to off. The choice is saved with the Data/output paths and source selection in the per-user `mudcrab-launcher.json` file. Start and Resume apply the choice to the converter configuration. Play follows the existing output's manifest, independently of the checkbox's current value.

Forced `--lod-mode cluster` builds objects, trees and terrain, omitting baked LOD products. The conversion manifest records `geometry_mode: "cluster"` and an exact `cluster_package: "derived/clusters-<settings-id>"` before the base fingerprint is finalized. The package selector derives from the same settings object as the actual overlay. The manifest is not rewritten after derivation. A failed derive therefore leaves a base that clearly requires its missing package, rather than authorizing baked runtime fallback.

Legacy manifests default to baked mode and omit the new fields when serialized. `--lod-mode baked` keeps its existing producers. `auto` retains its historical baked base plus optional overlay behavior. Any engine run selecting cluster geometry requires a tree-inclusive terrain package and a prepass, including explicit selection of a legacy overlay; incomplete old overlays must be derived again with those inputs.

The engine selects the manifest's exact package before opening runtime sources. It validates the ordinary database schema without native LOD admission, never opens native object/tree inventory or billboard hashes, and never submits baked terrain queries or requests baked terrain GLBs. Cluster terrain supplies distant terrain availability, fog reach and camera distance. Canonical source heights and near terrain remain shared inputs. Cluster ownership/refused-stock behavior from #2143 remains in force.

## Products omitted in cluster conversions

The following are output patterns, parameterized by the actual source/worldspace; they are not game-specific paths baked into the implementation.

- Converted native object LOD meshes: outputs of `.bto` inputs, normally `meshes/terrain/<worldspace>/objects/<worldspace>.<level>.<x>.<y>.glb`.
- Converted native terrain meshes: outputs of `.btr` inputs, normally `meshes/terrain/<worldspace>/<worldspace>.<level>.<x>.<y>.glb`.
- Generated tree billboards: `meshes/terrain/<worldspace>/trees/<worldspace>.tree.<index>.glb`.
- Baked-only DDS conversions under `textures/terrain/` and `textures/lod/`: their `.ktx2` products and generated `.opensky-srgb.ktx2` aliases. A texture named by a surviving model, record, LAND or configured near-terrain default is a shared source and remains available. The filter is not permission to break a mod that reuses a LOD texture on a near model.
- Compiled baked terrain: `lod/<worldspace-id-as-eight-hex-digits>/<tier-side-cells>/cell_<anchor-x>_<anchor-y>.glb`. Its mipmapped KTX2 atlas is embedded in that GLB, so there is no separate baked chunk atlas file.
- Native inventory population, tree instance/type population and baked terrain chunk/spatial rows are skipped. The base schema can still contain empty compatibility tables.

`lod-manifest.json` and the `lod_build` identity remain as a source recipe/core identity for deriving and binding the cluster terrain package; their retained presence is not a baked mesh product. The engine's cluster runtime does not read that baked terrain manifest. Cluster payloads, terrain atlases/heights, ordinary meshes/materials, near-terrain inputs, world records and cell caches remain required.

Metadata-only rebuilds classify retained shared textures after rebuilding the current record graph. Reader migrations can correct a texture path, so the previous database cannot decide whether the current output still needs verified texture bytes or their aliases. Retained source GLBs and configured near-terrain defaults continue to contribute their own dependencies.

## Size and time inference

A read-only inspection of the existing `C:/Modding/Mudcrab/SkyrimConverted-main28` used immutable SQLite and existing file sizes. It did not convert, derive, launch or modify that dataset. The target-local receipt is `2144-baked-products.json`.

| Existing product class | Files | Bytes |
| --- | ---: | ---: |
| Native object LOD GLBs | 1,078 | 270,825,344 |
| Native terrain GLBs | 9,584 | 202,868,632 |
| Tree billboard GLBs | 120 | 195,080 |
| Compiled baked terrain GLBs, including embedded atlases | 4,639 | 8,897,206,652 |
| Total geometry products omitted | **15,421** | **9,371,095,708** |

For equivalent content, omitting those producers is inferred to avoid approximately **9.37 GB (8.73 GiB)** of baked geometry output. This is a gross omission estimate, not the net size of a newly converted cluster package: new tree clusters and cluster terrain add their own data. The old package also has 18,858 candidate texture files totaling 252,852,939 bytes under the two excluded texture namespaces; shared textures prevent counting that entire set as additional savings. Database row/page savings are not quantified.

The code removes 10,662 native mesh conversions, 120 billboard generations and 4,639 baked terrain mesh/atlas compilations for the inspected content, plus their baked-only texture encodes, validation, publication and writes. It also skips native inventory parsing. The terrain source recipe and cluster derivation still run. **No defensible number of seconds follows from source code alone:** reuse state, encoder, storage throughput and the added cluster work determine net elapsed time. No conversion timing or runtime performance result is claimed.

## Verification and acceptance limits

All required checks passed through the build gate with owner `sol-2144`: formatting, touched-crate and workspace all-target/all-feature Clippy with warnings denied, touched-crate all-target nextest, workspace nextest and workspace doctests. Touched nextest passed **5,316 tests, 58 skipped**; workspace nextest passed **5,445 tests, 63 skipped**; doctests passed **18, 15 ignored**. The touched suite includes one additional converter benchmark target because it requests all targets. The existing `proc-macro-error2 v2.0.1` future-incompatibility notice remains.

The 34-file source/manifests snapshot stayed `3f186074766729a97099ed167cd2cdca3827fbf0899478149a2246a5f6ed6ff8` throughout final verification. Target-local `2144-verification-summary.json` records all seven exit codes, log hashes, per-crate counts and the file map. The coordinator independently checked its receipts and the current source before committing.

Fixtures cover manifest compatibility and selectors, converter omission/reuse/shared-source preservation, metadata reader migration, runtime admission and cluster readers without baked files/tables, missing cluster inputs, launcher persistence and runtime flags, keyboard focus and edit locking. The real terrain-source loader fixture uses the ordinary DDS-to-KTX2 producer; the loader's strict 2D atlas requirement is unchanged.

No full conversion, standalone engine/GPU run, push or PR is authorized in this task. CPU fixtures cannot establish visual terrain coverage, real renderer admission or performance. The parent must perform its permitted renderer acceptance before integration.
