# Goal
VG2 cluster-builder spike for the virtualized-geometry lane: run meshoptimizer's `clusterlod.h` (MIT) on 50-100 real converted meshes, outside the converter, and measure what it costs and produces. Phase 1 track. Plan and stop rules: `C:/Modding/Mudcrab/mudcrab/local/research/fun/virtualized-geometry-task-order.md` (VG2 row). Background, read first: `C:/Modding/Mudcrab/mudcrab/local/agents/runs/websearch-1070-fun-vg-offline-clusters.1/report.md` (clusterlod.h steps, config fields, defaults; the `meshopt` Rust crate does not wrap it) and `C:/Modding/Mudcrab/mudcrab/local/research/fun/p1-vg1-mesh-census-2026-10-09.md` (VG1 census, top-30 list; raw per-mesh table `D:/Mudcrab/scratch/p1-vg1-census/meshes.tsv`).

# Scope
- Everything lives in `D:/Mudcrab/scratch/p1-vg2-cluster-spike/` (create it with an OWNER.txt: owner Phase 1 Dev, purpose VG2 cluster spike, class A, keep-until date:2026-10-30, links no). Its own Cargo project and its own target folder inside it. Nothing in the Mudcrab repo changes; no converter code.
- Source of meshoptimizer: `git clone https://github.com/zeux/meshoptimizer` into `D:/Mudcrab/scratch/p1-vg2-cluster-spike/meshoptimizer`, then check out ONE pinned commit: the newest release tag whose `demo/clusterlod.h` compiles against that same tag's `src/` (if the latest tag has no clusterlod.h or it doesn't build, use the master commit of the day and record its hash). Record the tag/hash and licence (MIT) in the report. Do not copy any of it into the Mudcrab repo.
- Harness: a Rust binary (edition 2021) that compiles `src/*.cpp` plus a tiny C++ wrapper around `clodBuild` with the `cc` crate (MSVC is installed; the converter already builds C++ this way), and reads GLB with the `gltf` crate (or a minimal reader). Pin every crate version exactly in Cargo.toml.
- Inputs: converted GLBs under `C:/Modding/Mudcrab/SkyrimConverted-portal-rooms/meshes/` (read only).
- Builds: always through the gate, with the harness's own target, e.g. `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --release --manifest-path D:/Mudcrab/scratch/p1-vg2-cluster-spike/Cargo.toml`. Check `df -h /d` first; below 15 GB, stop and report. Warm-up rule: do one untimed run before timed ones; time with `std::time::Instant`, single-threaded, release, median of 3 runs.

# Sample (per primitive, not per mesh)
VG1 found that a mesh counts as cutout if ANY primitive is alphaMode MASK, so many rocks and mountains have one opaque body plus a MASK overlay. Process each glTF primitive separately and record its alphaMode.
- All 30 meshes of VG1's top-30 list (census note, section 5).
- Plus a spread to 50-100 meshes in all from `meshes.tsv`: about 15 small (100-1,000 tris), 20 medium (1,000-10,000), 10 large (10,000-100,000), and the largest few (100k+). Include some Nordic/Dwemer interior kit pieces, rocks, cliffs, trees, plants and architecture. Fixed random seed; write the list to `sample.txt`.

# Measure per primitive
Use `clodDefaultConfigRT(128)` as the main run, and `clodDefaultConfig(128)` as a second run on the same sample. Record:
1. Input: triangles, vertices, alphaMode, mesh path.
2. Build time (ms) of `clodBuild` alone, and of the whole primitive (load + build).
3. Output: cluster count, group count, DAG depth (LOD levels), triangles at the coarsest level, how many groups hit `simplify_threshold` ("stuck", error FLT_MAX before the final group).
4. Bytes: a plain payload estimate (per cluster: local indices as u8 triples, vertex reference list as u32, bounds 5 x f32, error f32) against the source primitive's index + position bytes and the GLB file size. State the formula.
5. Failures and warnings: crashes, asserts, zero clusters, non-manifold or degenerate input (count degenerate triangles and non-manifold edges in a quick pre-pass), and cutout primitives where simplification would change the alpha silhouette (flag MASK primitives; do not judge visually).

# Answer for VG3
Does the `meshopt` crate (pin a version; add it to the harness's Cargo.toml so cargo downloads it, then read its source under `C:/Users/Brannock/.cargo/registry/src/`; no web pages in this run) expose everything `clusterlod.h` uses (`meshopt_buildMeshletsFlex`/`Spatial`, `meshopt_partitionClusters`, `meshopt_simplifyWithAttributes`/`Sloppy` with the flags used, `meshopt_computeClusterBounds`, `meshopt_generatePositionRemap`, `meshopt_optimizeMeshletLevel`, `meshopt_spatialSortRemap`)? List what is missing. Then say which of these three is cheapest: (a) Rust port of clusterlod.h on top of the `meshopt` crate, (b) C++ clusterlod.h built with `cc` as in this spike, (c) the pure-Rust ports websearch-1070 names. Facts only; the decision belongs to the user and Team Lead.

# Done when
`D:/Mudcrab/scratch/p1-vg2-cluster-spike/vg2-report-2026-10-09.md` with: the pinned versions, the sample, a per-primitive table (CSV beside it), totals (ms per million input triangles, clusters per 1k triangles, payload bytes per input triangle, depth distribution), the failure list, the VG3 answer, and run time. Say where a number is an estimate.

# Rules
Never touch the Mudcrab repo, converted data or other sessions' folders. Never git push. Foreground commands only. No env-var prefixes on commands.

# Report (under 400 words)
Headline numbers, the failure shapes, the VG3 answer, files written.
