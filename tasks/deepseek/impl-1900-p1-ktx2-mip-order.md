# Goal
Make every KTX2 file the converter writes store its mip level data smallest-first, as KTX 2.0 requires. The level index stays largest-first (index 0 = the base level), with offsets pointing at the moved data. Phase 1 track, an upstream fix (draft later, batched with the next converter schema bump).

# Context
- Spec: KTX 2.0 (https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html). Section 3.7 "levelCount": mip level data is ordered from the smallest images (level_p) to the largest (level_base). Section 3.9.7 "Level Index": the index array starts with level_base at index 0. The index order does not change; only where the bytes sit in the file changes. Each level's data offset must still meet the alignment rule of section 3.9.7 / 3.10 (lcm(texel block size, 4) for non-supercompressed data; 1 for supercompressed), with padding between levels as needed.
- Note with the details: `local/research/converter/ktx2-mip-order-2026-10-07.md` (read it first).
- The writers, all in `crates/converter/src/texture.rs` (line numbers at main f40ca1df, verify them): `write_native_ktx2` (~516, the `for level_data in levels` loop at ~557-575), `supercompress_ktx2_levels` (~604-640), `combine_ktx2_mip_levels` (~776), `combine_ktx2_volume` (~1067), `combine_ktx2_cubemap_faces` (~1170). Check each one. If a writer passes through bytes that the Basis encoder (`encode_basis_ktx2`, ~1623) produced, check whether the encoder's own output is already smallest-first and say so in the report. Any KTX2 writer outside texture.rs (search the converter crate for `ktx2::` and "KTX2") must not be changed: list it with file:line in the report.
- Readers find levels through the index (Bevy's loader, the `ktx2` crate), so correct readers keep working. Do not change any reader.

# What to build (2 items)
1. Fix the writers so the level data is laid out smallest-first, the index stays largest-first with correct offsets and lengths, and the alignment and padding rules hold. Prefer one small shared helper for "lay out these level payloads in spec order and return the index entries" over five copies of the same loop.
2. Tests in texture.rs's test module:
   - a test helper `assert_ktx2_spec_level_order(bytes)` that parses the header and level index (the `ktx2` crate's `Reader` can give the index) and asserts: the level offsets strictly decrease as the level number rises (level 0 has the largest offset), the levels don't overlap, each offset meets the alignment rule, and the last level ends inside the file;
   - use it on output from every writer path you changed (native 2D with mips, supercompressed, combined mips, volume, cubemap: reuse the existing tests' inputs where they exist);
   - a round-trip test: write a texture with at least 3 mip levels, read each level back through the index, and check the bytes equal the input level bytes (for supercompressed output, after zstd decompression);
   - the existing tests must still pass unchanged (they read through the index).

# Owned files
`crates/converter/src/texture.rs` only (code and its test module). Change nothing else. If something outside it must change, stop and report why.

# Checks (machine-wide build gate; check `df -h /c` before building, below 15 GB stop and report)
- `cargo fmt --all -- --check`
- `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p converter --all-targets --all-features -- -D warnings`
- `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets`
If the gate refuses, or a hold stays listed for more than a few minutes, finish the code and report "written, unbuilt". A known flaky test, `pipeline::tests::extraction_does_not_wait_for_a_front_end_that_stopped_reading`, may fail on a busy machine; rerun it alone once before reporting it.

# Foundations check (state it in the report)
No shortcuts (every writer path fixed or listed, no test weakened), right layer (writer only, readers untouched), data-driven (no texture-specific cases). Say what shortcut, if any, remains.

# Rules
Never git stash, rebase, amend or push. Foreground commands only. Commit only texture.rs by name, upstream style, no prefix: `fix(converter): store KTX2 mip level data smallest-first, as the spec requires`.

# Report (under 400 words)
Each writer and what changed (file:line), writers outside texture.rs (listed, not changed), the Basis encoder's order, commit hash, test names and results, the check output lines, and the foundations check.
