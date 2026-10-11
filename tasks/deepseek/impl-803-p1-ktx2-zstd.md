# Goal
Track: Phase 1 (for upstream). The converter writes its UASTC KTX2 textures with Zstandard
supercompression (KTX2 `supercompressionScheme = 2`), as `docs/roadmap/01-asset-pipeline.md` §1.4
already promises ("supercompressed Basis Universal KTX2"). Today every one of the ~36,400 output
textures has scheme 0 (none): measured on a Skyrim SE conversion, 19.3 GB of KTX2 whose UASTC level
data zstd-compresses to ~43% (level 3, 60-file sample), so ~11 GB saved per install. The decoded
texels must be bit-identical: supercompression is lossless over the level data.

# Context
- Your checkout is cut from `phase1/vfs-hard-links` (upstream main 6f583cd + 2 converter commits).
  Upstream commit style (`perf(converter): ...`), no `[deepseek]` prefix, no project-internal
  references (task numbers, fork branch names, local paths) in code, comments, docs or messages.
- The texture code is `crates/converter/src/texture.rs` (KTX2 writing, `Ktx2Metadata`,
  `inspect_ktx2`, `inspect_runtime_ktx2`; the tests already decode Zstandard levels with `ruzstd`,
  e.g. `decode_uastc_level`). Find where the KTX2 container is written (the `basis-universal` 0.3
  crate has no supercompression option, so the converter or the `ktx2` crate path writes it) and the
  reuse path for textures in `crates/converter/src/pipeline.rs` and `cache.rs` (a cache hit copies a
  previous output). The engine reads the files with Bevy 0.19's KTX2 loader, which decodes
  Zstandard when built with its `zstd_rust` feature (pure Rust, `ruzstd`); the lead switches that
  feature on, not you.
- `ruzstd` 0.8 is already a converter dependency. Use its encoder if it gives a ratio close to real
  zstd (measure on a few real textures: any `.dds` under
  `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data` is inside archives, so use the
  existing converted outputs under `C:/Modding/Mudcrab/SkyrimConverted` (read-only) as UASTC input: read a
  KTX2, compress its levels, compare sizes). If only the C `zstd` crate gets a good ratio, do NOT add
  it: report both ratios and speeds and stop at a design; adding a dependency is the lead's call.

# Scope
1. Written KTX2: each mip level's data is Zstandard-compressed; the level index records
   `byteLength` (compressed) and `uncompressedByteLength`; the header records scheme 2; everything
   else (DFD, key/values, sRGB alias handling) unchanged. Follow the KTX2 spec for zstd (no
   supercompression global data for Zstandard). Cube maps / arrays / volumes if the writer has them.
2. `Ktx2Metadata` / `inspect_*` report the scheme and still validate the files (update any check that
   requires scheme none).
3. Reused textures: a previous output written without supercompression must not be copied as is
   forever. Design the cheapest correct path: recompress the reused file's levels (read, zstd each
   level, rewrite; minutes for the whole set) rather than re-encoding UASTC from the DDS (hours).
   Keep it in the converter; say in the report how it interacts with the manifest hashes
   (the output hash changes).
4. Tests: a written texture decodes (via `ktx2::Reader` + `ruzstd`) to exactly the level bytes the
   uncompressed path wrote; the header says Zstandard; a reused uncompressed output ends up
   supercompressed with identical decoded levels; the round-trip/never-panic tests still pass.
5. A reconversion across the schema bump must never leave a pre-zstd texture in the output (else
   old installs keep the uncompressed files and the saving silently does not arrive): the reuse
   check keys on the supercompression scheme of the previous output (recompress it, point 3) or
   excludes textures from cross-schema reuse. Add a test for exactly this.
6. Check whether anything else in the workspace reads KTX2 headers or level sizes and would reject a
   supercompressed file: the converter's own validation stage and integration report, and the
   `asset-closure` / `runtime-asset-audit` tools used by `scripts/phase2-acceptance.ps1` (read the
   script to find them). Report each with its verdict; fix only files you own, and list the rest.
7. Do NOT change the converter schema constant (it lives in `crates/shared`, the lead bumps it) and
   do not edit `Cargo.toml` files. If the reuse window in `cache.rs` needs to know about the bump,
   write it against the constant (e.g. "the previous schema") and say so.

Owned files: `crates/converter/src/texture.rs`, `crates/converter/src/pipeline.rs` (only the texture
reuse path and tests), `crates/converter/src/cache.rs` (only if the reuse rule needs it),
`crates/converter/tests/` (new files only), `docs/specs/converters/` (texture section),
`docs/handoff/impl-803-p1-ktx2-zstd.md`. If you need another file, say so in the report instead of
editing it.

# Done when
These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/shared-check-target -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/shared-check-target`
(`shared-check-target` is shared with other branches; if a test fails naming a schema or version
number, re-run it without `--target-dir` before believing it, and report both.)

# Report
Per scope point: the change, the tests, and which tests failed before the change. The measured
ratio and speed (ruzstd vs, if you measured it, C zstd) with the files used. The size of one real
converted texture before/after. A suggested upstream commit message (subject + body). Anything you
chose not to do and why.
