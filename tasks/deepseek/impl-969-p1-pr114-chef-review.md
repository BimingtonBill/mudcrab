# impl-969-p1-pr114-chef-review — answer Chef's review of #114 (native BC for uncompressed DDS)

Track: Phase 1 (upstream PR #114, Mudcrab-Team/mudcrab, branch phase1/native-rgba8). Base: fddbdd27 (the PR head; up to date with main).

## Context
#114 makes the converter's CPU path block-compress uncompressed 8-bit RGB(A) DDS (24-bit, X8R8G8B8, A8R8G8B8, A8B8G8R8, DXGI BGRA/BGRX) to native BC1/BC7 with `intel_tex_2`, instead of Basis UASTC. Maintainer TaylorTurnerIT (Chef) reviewed it with 9 threads; the full text is in `local/review/pr-114-chef-threads.md` (read it first). The user decided the direction: **native BC stays the default, and a converter flag brings back portable UASTC** for these textures.

## Do
1. **Portable flag (thread ...fdfj).** Add `--portable-textures` to the converter CLI (crates/converter/src/main.rs, same style as `--texture-encoder`). With it, the packed RGB(A) textures go to UASTC exactly as main did before #114 (the existing Basis path); without it, native BC. It must reach `TextureConverter` the same way the other texture options do (follow how `texture_encoder` is plumbed through the config/pipeline). If the converter records its options in a cache label or manifest so a rerun with a different flag reconverts, include the new option there the same way. GPU mode (`--texture-encoder gpu`) already writes UASTC; leave it. Update `docs/specs/converters/dds-to-ktx2.md` and the README paragraph ("GPU texture encoding") to say: native BC by default (desktop GPUs), `--portable-textures` keeps UASTC for targets without BC (e.g. Android/Adreno). Test: the routing test (`uncompressed_dds_routing_per_texture_encoder`) or a new one covers the flag (UASTC with it, native BC without).
2. **BC7 for every native texture (thread ...fdfg).** Drop the BC1 branch: opaque colour textures also get BC7, so there is no BC1 banding question and GPU memory matches what UASTC->BC7 gave before. Keep alpha handling correct. Update docs/tests that mention BC1.
3. **Trailing bytes (threads ...fdfh, ...fdfc).** Require an exact payload length only in the ambiguous 24-bit case (tight vs DWORD-aligned rows): there, aligned rows are chosen only when the payload equals the aligned chain size and not the tight one; otherwise tight rows, and trailing bytes after the last mip are ignored, as main and `image_dds` do. For 32-bit layouts ignore trailing bytes too (still reject truncation). In `texture_gpu/mod.rs`, read a 24-bit chain in place whenever the decoder would choose tight rows (payload >= tight size, header pitch absent or tight), so a tight chain plus trailing bytes stays on the GPU path as on main. Then the separate `decode_x8r8g8b8_mips`/`decode_x8r8g8b8_level` decoder can go: use the packed decoder for X8 everywhere (CPU fallback and GPU), and make sure its existing tests (`decodes_x8r8g8b8_as_opaque_rgba`, `rejects_truncated_x8r8g8b8`) exercise the decoder that remains (thread ...fdfo). Add tests: 32-bit with trailing bytes takes the BC path; 24-bit tight + trailing bytes stays in place on the GPU path; 24-bit aligned exact still decodes aligned; truncation still fails.
4. **8K textures (thread ...fdfk).** Base the size cap on what actually matters: raise `PACKED_MAX_OUTPUT_BYTES` so an 8192x8192 RGBA chain with full mips (~341 MiB) passes, or cap by mip-0 size; keep a sane upper bound. When the packed path is skipped for size, log the reason (one line per texture, like other converter warnings), so it is never silent.
5. **Copies (thread ...fdfm).** In `compress_packed_levels`, pass decoded rows to `intel_tex_2`'s `RgbaSurface` directly with their stride; pad (copy) only when a mip's width or height isn't a multiple of 4, and then only what the trailing partial block needs if that is simple, else the whole small mip.
6. **Nit (thread ...fdfq).** Compute tight and aligned totals once inside the plan loop instead of the separate `chain_bytes` closure; drop per-mip checks in `compress_packed_levels` that `convert_packed_to_native` already did.
7. **Not this task:** thread ...fdff (ISPC output per CPU instruction set) is being researched separately; don't change the encoder choice or its build for it. If the snapshot test `conversion_manifest_matches_snapshot` output size changes because of BC7-only, update the snapshot.

## Owned files
crates/converter/src/texture.rs, crates/converter/src/texture_gpu/mod.rs, crates/converter/src/main.rs, crates/converter/src/pipeline.rs (option plumbing only), crates/converter/src/lib.rs or config module if the option lives there, crates/converter/tests/** snapshots, README.md, docs/specs/converters/dds-to-ktx2.md.

## Rules
Upstream commit style (`fix(converter): ...`, `feat(converter): ...`, `docs(...)`), one commit per numbered item where practical. No git stash, no rebase/reset/amend. Match surrounding code style and comment density.

## Accept
cargo fmt --all -- --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo nextest run -p converter --all-targets

## Report
Per thread id: what changed (one line) and a one-or-two-sentence reply text starting "Fixed in <short hash>: ...". Plus check results and anything left.
