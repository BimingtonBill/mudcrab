# Goal
Track: Phase 1 (for upstream). In `--texture-mode native`, uncompressed X8R8G8B8 / A8R8G8B8 /
R8G8B8A8 DDS textures are stored as uncompressed RGBA8 KTX2 (with the same Zstandard
supercompression) instead of falling back to a slow UASTC encode. On a real Skyrim SE native run,
10,237 of 32,945 textures fell back to UASTC, 9,326 of them X8R8G8B8; those encodes now dominate the
native run's texture time.

# Context
- Your checkout is cut from `phase1/fast-textures` (the native mode). Upstream commit style
  (`perf(converter): ...`), no `[deepseek]` prefix, no project-internal references.
- `crates/converter/src/texture.rs`: `NativeFormat` (BC1-BC7, `block_bytes`, `ktx2_format`),
  `encode_native_container` (uses `image_dds::Surface::from_dds`, which rejects X8R8G8B8, hence
  `UnsupportedDdsFormat ... X8R8G8B8 is not BC blocks`), `combine_native_levels` (writes the KTX2,
  4x4-block arithmetic), `native_image_bytes`, `container_native_format`,
  `container_matches_texture_mode`; the UASTC path already decodes X8R8G8B8 mips in
  `decode_x8r8g8b8_mips` (pitch handling, BGRX -> RGBA): reuse it rather than writing a second
  decoder.
- Engine: Bevy 0.19 loads RGBA8 KTX2 (`R8G8B8A8_UNORM` / `_SRGB`); the colour space comes from the
  loader setting per glTF slot, as for BC formats.

# Scope
1. A native RGBA8 variant: for X8R8G8B8 (alpha forced to 255), A8R8G8B8 and R8G8B8A8 DDS, write
   `R8G8B8A8_SRGB` or `_UNORM` by the texture's encoding (and the colour-slot rule already used for
   BC), every mip level, then Zstandard supercompression. Cube maps and volumes if the UASTC path
   supports them for these formats; otherwise keep falling back for those shapes and say so.
2. `combine_native_levels`, `native_image_bytes`, `container_native_format` and
   `container_matches_texture_mode` handle a per-pixel format (4 bytes per texel, no block
   rounding); KTX2 alignment for RGBA8 levels is 4 bytes before supercompression.
3. The fallback report no longer lists these formats; other uncompressed formats (L8, etc.) keep
   falling back.
4. Tests: an X8R8G8B8 DDS (dummy-content generator) in native mode yields an RGBA8 KTX2 whose
   decompressed level bytes equal the RGBA the UASTC path decodes before encoding (the output of
   `decode_x8r8g8b8_mips`), alpha 255, sRGB/UNORM by encoding; A8R8G8B8 keeps its alpha; staged
   reuse matches the mode; uastc mode unchanged.
5. Re-run the existing ignored measurement test (`crates/converter/tests/texture_mode_measurement.rs`,
   with `OPENSKYRIM_TEXTURE_SAMPLE_DIR=C:/Modding/Mudcrab/SkyrimConverted/vfs/textures`, release) and report
   the table and the fallback count.

Owned files: `crates/converter/src/texture.rs`, `crates/converter/tests/` (new files only, plus the
measurement test), `docs/specs/converters/dds-to-ktx2.md`, `docs/handoff/impl-823-p1-native-rgba8.md`.
If you need another file, say so in the report instead of editing it.

# Done when
Use your own target dir: `D:/Mudcrab/shared-check-target/impl-823`.
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/shared-check-target/impl-823 -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/shared-check-target/impl-823`

# Report
Per scope point: the change and the tests. The measurement table. A suggested commit message.
