# Goal
Measure how much CPU time loading one converted texture costs today (Basis Universal UASTC transcoded to BC7 at load) against loading a texture already stored as BC7 (only Zstandard decompression, or a plain copy), so the team can judge a converter change. Track: Phase 2 (evidence for upstream issue #117, engine loading speed). Measurement only; nothing in the project changes.

# Context
- Converted textures: `C:/Modding/Mudcrab/SkyrimConverted-p2night/textures/**.ktx2` (read only; 45,771 files). Every one is KTX2 with vkFormat 0 (Basis Universal) and supercompression scheme 0, 1024x1024-ish with full mip chains, about 500-850 KiB.
- The engine (Bevy 0.19.1, `bevy_image` KTX2 loader with the `basis-universal` feature) transcodes UASTC to BC7 on its asset IO threads when the GPU supports BC (it does on this PC). The loader source: `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_image-0.19.1/src/ktx2.rs` (read it to call the transcoder the same way: same target format, same per-level loop).
- Crates already in the local cargo registry (use these exact versions, offline): `basis-universal = "0.3.1"`, `ktx2 = "0.5.0"`, `ruzstd = "0.8.3"`, `zstd = "0.13"` (check `C:/Users/Brannock/.cargo/registry/src/*/` for the zstd version present).
- Work folder: `D:/Mudcrab/scratch/transcode-bench` (empty; create a small Cargo binary crate there, `cargo run --release --offline`). Use `CARGO_TARGET_DIR=D:/Mudcrab/scratch/transcode-bench/target`.

# What to measure (single thread, release)
Pick 60 textures with a fixed seed (mix of normal maps `_n` and colour maps, from landscape, clutter, architecture). For each:
1. Read the file (time it separately, warm cache: read twice, time the second).
2. Today's path: parse KTX2, transcode every mip level UASTC -> BC7 exactly as bevy_image does. Time it.
3. The alternative path: take the BC7 result of step 2 as if it were stored, compress each level with zstd level 3 (not timed), then time decompressing it (ruzstd, which is what Bevy's `zstd_rust` feature uses; also the C zstd for comparison), and the plain copy of uncompressed BC7.
Report per-file sizes (UASTC file, BC7 raw, BC7+zstd) and times.

# Done when
- The numbers come from a run you did, with the command shown, and a short table: mean / p50 / p95 per path in ms, and total MiB.

# Report
The table, the sizes, one paragraph on what it means for load time (the engine loads about 360 textures for a start or a jump), and the exact command. Under 500 words. Write the same report to `C:/Modding/Mudcrab/mudcrab/local/research/texture-load-cost.md`.
