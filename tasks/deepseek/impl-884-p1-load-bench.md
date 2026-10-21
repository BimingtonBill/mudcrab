# Goal
Build a small standalone benchmark that confirms two cell-load costs on the portal demo's real converted assets:
(a) collider build time at a model's first placement (JSON parse of the GLB collision extras, then the collider construction), per model;
(b) per-texture KTX2 zstd decode time with ruzstd (what Bevy 0.19 uses) versus C zstd.
Phase 1 track, for the 160 fps goal. Build it and do a tiny smoke run only; the real run happens later, in a booked quiet window.

# Where
- New folder: `D:/Mudcrab/scratch/p1-load-bench`. Put an `OWNER.txt` in it at once:
  `owner: Phase 1 Dev` / `purpose: cell-load benchmark (collider build, KTX2 zstd decode)` / `class: A` / `keep-until: date:2026-10-14` / `links: no` / `created: 2026-10-07`.
- Its own target: `CARGO_TARGET_DIR=D:/Mudcrab/scratch/p1-load-bench/target`. Check `df -h /d` before building; below 15 GB, stop and report.
- Not a git repo. Never touch any repo checkout.

# Match the engine exactly
- Read (do not edit) `C:/Modding/Mudcrab/mudcrab/crates/engine/src/streaming.rs`:
  - `authored_collision_from_hierarchy` (about line 4033): JSON `from_str` to a `Value`, then `from_value` into `CollisionAsset`;
  - `collider_parts_from_authored` (about line 4066): `Collider::trimesh` / `capsule` / `cuboid` / `convex_hull`.
- Depend on the same crates and versions: `bevy_rapier3d = "0.36.0"` with `default-features = false, features = ["dim3"]`, and `shared = { path = "C:/Modding/Mudcrab/mudcrab/crates/shared" }` for `CollisionAsset`/`CollisionShape`. Copy the two functions' logic, adapted to take the extras JSON string directly. Do not depend on the engine crate.
- Read the GLB JSON chunk yourself (12-byte header, then the JSON chunk) to get `scenes[0].extras`; collision sits under `mudcrabCollision` or `openSkyrimCollision`.
- KTX2: copy the approach from `D:/Mudcrab/scratch/transcode-bench` (crates `ktx2`, `ruzstd 0.8.3`, `zstd 0.13.3`; read its `src/` and `texture-load-cost.md`). Decode every level of each file both ways; check that both decoders give identical bytes.

# Inputs (read only)
- The asset root is a command-line argument; it will be `C:/Modding/Mudcrab/SkyrimConverted-portal-lod`. Never write into it. Never run `du`/`dust` on it.
- Model list: a command-line text file of relative `.glb` paths. Default sample: walk `meshes/` under the root, take every Nth file with a fixed seed to get about 300 GLBs that carry collision extras, and save the list next to the results so the run can be repeated.
- Texture list: the same idea, about 200 `.ktx2` files under `textures/`, fixed seed, saved.
- For colliders, also accept an optional `--cell-models <file>` list (the models placed in one cell; Portal Dev's door tour will provide one later). Report that subset's sum separately.

# Output
A text report, plus CSV per item. Per model: path, shape counts by kind, total vertices/triangles/hull points, JSON parse ms (both steps), build ms per kind, total ms. Then: median, p95, max, sum, and the 20 slowest models.
Per texture: path, vkFormat, compressed and decoded bytes, ruzstd ms, C zstd ms. Then median, p95, sum, and ms per MiB.
Single-threaded, release build, warm file cache (read each file once before timing), 3 repeats, report the median of the repeats.
The console window title and the report header say `Phase 1 load bench (collider + KTX2 decode)`.

# Extra check (Phase 2, 2026-10-07)
A collision proposal would put collision arrays in GLB accessors that no mesh primitive references. Check by reading (not running) `bevy_gltf-0.19.1` in `C:/Users/Brannock/.cargo/registry/src/*/bevy_gltf-0.19.1/src/loader/`: does the loader decode or convert accessors that no primitive references, or does it only load the buffer bytes? Give file:line in your report.

# Rules
- Foreground commands only; no background sleep/wait loops; no git.
- Build in release. Smoke run only: at most 10 models and 10 textures, to prove it works. Do NOT run the full sample; the lead runs that in a booked window.
- Write a `run-full.cmd` in the folder with the exact full-run command, which writes `D:/Mudcrab/scratch/p1-load-bench/results/<date>-full.txt`.

# Report (under 300 words)
Files made, the dependency versions used, the smoke-run output (10 + 10), any mismatch between ruzstd and C zstd output, and anything in the engine functions you could not reproduce exactly.
