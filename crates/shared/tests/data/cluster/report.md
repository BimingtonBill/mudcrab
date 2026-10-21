# Cluster LOD against baked LOD: converter stage report (ADR-0013 "Prototype proof", items 1 and 2)

Branch `phase1/cluster-converter-stage`, measured on converter commit 9320286d, 2026-10-13 (the data files beside
this report are `report-data.json` for the matrix and `sweep-data.json` for the threshold sweep). Machine: AMD Ryzen 7 5800X
(8 cores, 16 threads), 32 GB, NVIDIA RTX 3090 (the probe passes: Vulkan, `INDIRECT_FIRST_INSTANCE`).
Build: `cargo build --profile quick -p converter --bin cluster-stage` (release without LTO, meshoptimizer
built with `NDEBUG`). Rerun with `python scripts/cluster_bench.py --exe <cluster-stage.exe> --converted <converted output>
--work-root <empty folder> --out <new>.md --plugin Update.esm --jobs 8 --repeats 3`, inside a build-gate timing
hold. It writes `<new>.md` and `<new>.json`; this file adds the sections below by hand.

## Answers in short

- **Conversion time of the stage, cold:** `cluster` mode 4.9 s (4.8 to 6.2), `baked` mode 4.7 s (4.3 to 4.9).
  Cluster mode does **not** save time on a cold run with this baked stand-in; it is the same to slightly slower.
  The stage cost is the shared model loading and classification (about 2.5 s) plus either side's build
  (about 1 s each way when both run, 1.2 s for clusters alone).
- **Warm** (earlier output kept): 3.4 s against 3.1 s. **After one plugin changed:** 3.8 s against 4.1 s. These
  differences are inside the spread of the repeats, so the honest reading is: no measurable time difference.
  Cluster payloads are reused by mesh content hash, so a plugin change rebuilds only the instance blocks
  (0.2 s, 80 blocks of about 30 KiB); no mesh payload is rebuilt (`meshes_reused` 935 of 935).
- **Bytes on disk of the LOD output for static objects:** `baked` stand-in 1,422 MiB; `cluster` mode 874 MiB
  (94 MiB mesh payloads, 2 MiB instance blocks, 775 MiB of baked chunks for what clusters did not take,
  2 MiB of table rows): 39 % less than the stand-in. The comparison is a full cluster hierarchy per mesh, stored
  once, against one merged far level per block (35,000 units, 1 px) of the stand-in; a baked builder that keeps
  several levels per block would be larger, and one that writes less per block would be smaller.
- **Converter peak memory (stage only):** 1,475 MiB `baked` against 914 MiB `cluster`; the cluster build alone
  peaks at 135 MiB. The baked stand-in holds the geometry and chains of every placed mesh at once, which a real
  builder need not do; read this row as the stand-in's cost, not a verdict on #106.
- **Whole-package conversion time and the rest of LOD (terrain chunk compiler): not measured here.** The stage is
  3 to 5 s; the whole conversion takes tens of minutes (the VG-L baseline of the terrain compiler alone was 26
  minutes, `local/research/fun/p1-vgl-lod-baseline-2026-10-09.md`, not rerun here), so neither mode moves the headline number. A full run of both modes on the same data needs a
  long hold and disk the machine does not have free today; the user decides whether to spend it.
- **Not part of this report (other threads):** image difference, frame time, detail pops (items 3 to 5).

## What the baked side is

No baked object chunk builder exists on `main` (#106 is Chef's and has no pull request), so "baked" here is a
**stand-in**: a plain discrete LOD chain per primitive (meshoptimizer, the method of the VG5 comparison) and the
instances of each 16 x 16-cell block merged into world-space batches per material at one fixed far distance
(35,000 units, 1 px). Its cost is probably well under a real object-LOD builder's, which also resolves materials
and textures and writes atlases, so the cold-time comparison favours `baked`. `cluster` mode leaves the
eligible primitives out of the stand-in, as the ADR says the exclusion set does.

## Measurements

Input: `C:\Modding\Mudcrab\SkyrimConverted-p2main-v24-doors`; threshold 1024 triangles; 8 jobs; plugin changed in the third run of each mode: Update.esm; medians of 3 repeats (stage total shows min to max). "Cold" means no previous cluster or baked output; the operating system's file cache is warm after the first repeat, so read times here are optimistic against a first conversion on a freshly booted machine.

The baked side is a stand-in (plain LOD chains merged per 16x16-cell block); no baked object chunk builder exists on `main` (#106 is unmerged). Stage times below are the stage alone; the model pass runs load, classify, build and chain in parallel, so the split of that pass is attributed by summed CPU time (an estimate).

Rest of LOD (terrain chunk compiler): not measured here.
Whole-package conversion time: not measured here; add a full run for the headline number.

## Conversion time per stage

| mode | run | probe | classify | model load (attr.) | baked build (attr.) | cluster build (attr.) | tables | stage total (min to max) | process wall | peak memory |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| baked | cold | 0.0 s | 2.3 s | 0.2 s | 1.8 s | 0.0 s | 0.0 s | 4.7 s (4.3 to 4.9) | 4.8 s | 1475 MiB |
| baked | warm | 0.0 s | 2.2 s | 0.2 s | 0.5 s | 0.0 s | 0.0 s | 3.1 s (3.0 to 3.4) | 3.2 s | 626 MiB |
| baked | plugin | 0.0 s | 2.5 s | 0.2 s | 1.2 s | 0.0 s | 0.0 s | 4.1 s (3.6 to 4.9) | 5.1 s | 1674 MiB |
| cluster | cold | 0.3 s | 2.2 s | 0.2 s | 1.0 s | 1.0 s | 0.1 s | 4.9 s (4.8 to 6.2) | 5.0 s | 914 MiB |
| cluster | warm | 0.3 s | 2.2 s | 0.2 s | 0.4 s | 0.0 s | 0.2 s | 3.4 s (3.3 to 3.4) | 3.5 s | 450 MiB |
| cluster | plugin | 0.3 s | 2.2 s | 0.2 s | 0.8 s | 0.0 s | 0.1 s | 3.8 s (3.7 to 3.9) | 5.1 s | 1020 MiB |
| cluster-only | cold | 0.4 s | 2.4 s | 0.3 s | 0.0 s | 1.2 s | 0.1 s | 4.5 s (3.7 to 5.0) | 4.6 s | 135 MiB |
| cluster-only | warm | 0.3 s | 2.6 s | 0.0 s | 0.0 s | 0.2 s | 0.1 s | 3.3 s (2.8 to 3.4) | 3.4 s | 115 MiB |
| cluster-only | plugin | 0.4 s | 2.6 s | 0.0 s | 0.0 s | 0.2 s | 0.1 s | 3.2 s (3.0 to 3.5) | 4.2 s | 115 MiB |

## Bytes on disk and counts (cold runs)

| mode | cluster payloads | instance blocks | baked chunks | database growth | primitives clustered | clusters | baked primitives | baked output triangles |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| baked | 0 MiB | 0 MiB | 1422 MiB | 0 MiB | 0 | 0 | 9598 | 25390078 |
| cluster | 94 MiB | 2 MiB | 775 MiB | 2 MiB | 935 | 52439 | 8663 | 12004671 |
| cluster-only | 94 MiB | 2 MiB | 0 MiB | 2 MiB | 935 | 52439 | 0 | 0 |

## What the cluster path did not take (primitive rejects, cold cluster run)

| reason | primitives |
|---|---:|
| below_triangle_threshold | 6216 |
| not_opaque | 1654 |
| unsupported_material_feature | 983 |
| missing_attributes | 47 |
| effect_shader | 10 |

## Reading the numbers

- Cold: `cluster` mode took 4.9 s against 4.7 s for `baked`: slower by 0.2 s.
- warm: `cluster` 3.4 s against `baked` 3.1 s.
- plugin: `cluster` 3.8 s against `baked` 4.1 s.

## Threshold sweep (cold cluster run with the baked remainder, one run each)

The ADR proposes a default triangle threshold of 1024 and leaves the final value to the comparison against plain
LOD chains. This is the size side of that choice on this data; the time side is flat (each run is about 5 s).

| threshold | primitives clustered | source triangles clustered | clusters | cluster payloads | baked chunks (remainder) | baked output triangles |
|---:|---:|---:|---:|---:|---:|---:|
| 128 | 4391 | 3628600 | 85699 | 153 MiB | 374 MiB | 5452042 |
| 256 | 3149 | 3403680 | 80087 | 143 MiB | 463 MiB | 6751698 |
| 512 | 1888 | 2944272 | 68638 | 123 MiB | 639 MiB | 9770089 |
| 1024 | 935 | 2263028 | 52439 | 94 MiB | 775 MiB | 12004671 |
| 2048 | 369 | 1450041 | 33841 | 61 MiB | 1044 MiB | 17297369 |
| 4096 | 93 | 667298 | 15952 | 29 MiB | 1413 MiB | 25270906 |

Raising the threshold moves primitives from the small cluster payloads to the larger baked chunks, so total bytes
(payloads plus baked remainder, instance blocks left out) grow with it: 527 MiB at 128, 869 MiB at 1024, 1,442 MiB
at 4096, which is more than the 1,422 MiB of the all-baked stand-in, because the hierarchy of a few big meshes
costs more than their one far level. Primitives under about 128 triangles are one
cluster with no hierarchy, so a threshold of 128 buys no detail levels for them; where the useful floor lies is
for the image comparison (VG7) to say.

## Method and limits

- **Input:** `SkyrimConverted-p2main-v24-doors`, the full converted vanilla data (Skyrim.esm, DLCs, Update.esm).
  323,499 exterior references examined; 116,163 pass the reference rules (STAT, enabled, not parented, not
  scripted, with a model); 129,949 are trees and stay on their own path. 2,758 unique converted GLBs, 9,845
  primitives.
- **"Cold" means no earlier cluster or baked output.** The operating system's file cache is warm after the first
  repeat, so reading the 2,758 GLBs and the database is faster than on a freshly booted machine. The first run
  after the build, which read everything from disk, took 17.6 s (cluster mode with the stand-in; its report was
  not kept in the data files); the repeats took 4 to 5 s.
- **Runs are the `cluster-stage` binary**, which reads a finished output and writes only payloads and a small
  tables database, not a full conversion: the terrain compiler, textures and meshes are not rerun. Each mode runs
  cold, warm and after a simulated plugin change (every reference owned by Update.esm moved one unit), three
  repeats, medians reported with min to max for the stage total. One timing hold covered the 27 runs.
- **Stage split:** loading, classifying, cluster building and chain building run in one parallel pass, so their
  shares are attributed by summed CPU time. The attributed columns are estimates; the stage total and process
  wall are measured.
- **Eligible set:** opaque, `lighting` shader family, default lighting shader type, no emissive output, directx
  normal convention, at least 1024 triangles; rejects are counted by reason above and stored in `cluster_rejects`.
  The material rule is a deny-list of what the converter records; the supported set the cluster shader will draw
  (VG7) may be narrower, which would move primitives to the baked side.
- **Warm reuse works at the model level:** with the stand-in off, the warm and plugin runs load no GLB at all
  (all 2,758 models are known from the previous output's tables) and their 3 s are mostly the database
  classification (2.6 s of 3.3 s; the rest is the probe and linking the payloads). With the stand-in on, models must load anyway because it needs their geometry. On a plugin change the
  stand-in reuses 69 of 85 blocks in `baked` mode and 65 of 81 in `cluster` mode, and the cluster path reuses
  none of its 80 instance blocks (they rebuild; 0.15 s of CPU in total).
- **Where the build differs from the ADR as written** (for the ADR owner to settle; nothing here changes the
  format): block bounds in the `.mci` header and the database are in Creation axes (x east, y north, z up), like
  `lod_chunks`, because the ADR says "Creation units" without naming axes, while instance transforms stay in the
  runtime's Y-up axes; `<N>` in the payload path is a running index over the scene's (node, primitive) pairs,
  not the glTF primitive index (which is 0 for every single-primitive mesh), so the ADR line that defines `<N>`
  needs rewording and the engine must take `<N>` only from the mesh table, never from the glTF; the vertex subset is in source order,
  not spatially sorted (the vendored clusterlod keeps the order it is given); the committed fixture is a generated
  icosphere of 5,120 triangles with three instances in three cells, not `dummy-content` meshes in two cells;
  `monotonic_fixups` was 0 on all 935 real meshes, so clusterlod's group errors were already monotonic.
- **Open ends:** `stable_id` is zero until #185 gives references a stable identity, so a real package must not
  ship from this yet (ADR open question 2). Per-block input keys do not exist yet, so instance blocks rebuild
  every run (cheap: 0.2 s). The stand-in has no chain cache, so its warm run saves only the merge and write.
- **The simulated plugin change** moves the references Update.esm itself defines, not the ones it overrides, so
  it is lighter than a real Update.esm change; it exercises the cache path, not a worst case.
