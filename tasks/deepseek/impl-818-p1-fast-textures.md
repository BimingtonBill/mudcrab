# Goal
Track: Phase 1 (for upstream). A contributor's first conversion takes ~5 hours, 4.5 of them encoding
Basis UASTC textures. Add a developer texture mode, `--texture-mode native` (the default stays
`uastc`), that keeps each DDS's own GPU blocks (BC1/BC3/BC4/BC5/BC7) in the KTX2 container instead
of re-encoding them, so the texture stage takes minutes. Desktop GPUs sample BCn directly; the mode
is for development on desktop, not a shipping format (no mobile/web).

# Context
- Your checkout is cut from `phase1/ktx2-zstd` (upstream main + hard links + Zstandard
  supercompression of KTX2 levels, converter schema 15). Upstream commit style
  (`feat(converter): ...`), no `[deepseek]` prefix, no project-internal references.
- Texture code: `crates/converter/src/texture.rs` (`TextureConverter::convert_with_options`,
  `encode_container`, `supercompress_ktx2`, `TextureEncoding` = the semantic the meshes and the
  database give a texture: colour sRGB vs linear, normal map, etc.; `inspect_ktx2`,
  `inspect_runtime_ktx2`, `validate_ktx2`), the texture batch in `crates/converter/src/pipeline.rs`
  (encoding chosen per texture from its semantics; sRGB alias files `*.opensky-srgb.ktx2`),
  `crates/converter/src/config.rs` + `main.rs` (flags), `crates/converter/src/cache.rs`
  (`configuration_hash`: a texture mode change must invalidate texture outputs, nothing else).
- Engine: Bevy 0.19 loads KTX2 with any vkFormat it supports; BC formats are supported on desktop.
  The engine has no KTX2 parsing of its own. Find how the engine picks sRGB vs linear for a texture
  (material setup in `crates/engine`), read-only for you: the native path must produce exactly the
  colour space and channel meaning the UASTC path produces after transcoding for each
  `TextureEncoding` (e.g. if the UASTC path swizzles or repacks normal maps or moves a channel, the
  native path must do the same, or fall back to UASTC for that texture).

# Scope
1. `--texture-mode native|uastc` (config field, default uastc). Native: for a DDS whose format is
   BC1/BC2/BC3/BC4/BC5/BC7 (DX10 header or FourCC), write a KTX2 with the matching vkFormat
   (sRGB variant where the texture's encoding is sRGB colour, UNORM otherwise), the DDS's own mip
   levels copied block for block, cube maps/arrays as the UASTC path handles them; then the same
   Zstandard supercompression. Anything else (uncompressed, L8, other formats, a semantic the
   native path cannot represent faithfully) falls back to the UASTC path for that texture; count
   and report the fallbacks.
2. Validation (`validate_ktx2` / `inspect_*` / the pipeline's artifact validation) accepts the BC
   vkFormats in native mode; the uastc path and its checks are unchanged.
3. The configuration hash includes the texture mode, so switching modes reconverts textures and
   reuses everything else.
4. Tests: per BC format (use `dummy-content`'s DDS generator), native output's levels equal the
   DDS's blocks byte for byte after decompression, the vkFormat and sRGB/UNORM choice are right per
   encoding, fallbacks happen where they must, a mode switch invalidates only textures, uastc
   output is byte-identical to before.
5. Measure: convert 200 real DDS files in both modes (read-only game archives are extracted in
   `C:/Modding/Mudcrab/SkyrimConverted/vfs/textures`, read-only: pick a mix of diffuse, normal, terrain)
   with a small `#[ignore]` test or example writing to `D:/Mudcrab/phase1-scratch/`
   (create; delete when done); report time and output bytes per mode.
6. Docs: `docs/specs/converters/dds-to-ktx2.md` and `--help`: what the mode is for and its limits.

Owned files: `crates/converter/src/texture.rs`, `crates/converter/src/pipeline.rs` (texture batch
only), `crates/converter/src/config.rs`, `crates/converter/src/main.rs`,
`crates/converter/src/cache.rs` (configuration hash only), `crates/converter/tests/` (new files
only), `docs/specs/converters/dds-to-ktx2.md`, `docs/handoff/impl-818-p1-fast-textures.md`. If you
need another file (for example an engine file), say so in the report instead of editing it.

# Done when
Use your own target dir for every cargo command: `D:/Mudcrab/shared-check-target/impl-818`.
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/shared-check-target/impl-818 -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/shared-check-target/impl-818`

# Report
Per scope point: the change and the tests. The measurement table. Which encodings fall back and
why. Anything the engine side must do (with file:line). A suggested commit message.
