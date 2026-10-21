# impl-975-p1-pr114-review-nits — final review fixes on #114 before pushing

Track: Phase 1 (upstream PR #114, branch phase1/native-rgba8). Base: e717514f (after impl-969/972). Reviews: local/agents/runs/review-974-pr114-round3.1/report.md (DeepSeek) and local/agents/runs/review-974.1-pr114-round3-mimo.1/report.md (MiMo); read both.

## Do (one commit each, upstream commit style)
1. **Honour a padded header pitch on 32-bit layouts (MiMo finding 1).** In `plan_packed_chain` (crates/converter/src/texture.rs), when mip 0's tight and aligned pitch are equal (all 32-bit layouts; 24-bit widths that are multiples of 4), the header pitch is currently ignored. If the header pitch is larger than the row bytes, use it for mip 0's rows (as main's X8 reader did: it honoured mip 0's pitch), and keep tight rows for lower mips unless the payload length shows otherwise; if the header pitch is smaller than the row bytes, fall back (error → UASTC path) as before. Add a test: X8R8G8B8 16x16 with header pitch 68 and a 68*16 mip-0 payload (plus lower mips) decodes to the right pixels, and A8R8G8B8 with a bad smaller pitch falls back.
2. **Name the file in the skip log (both reviews).** The "packed native BC path skipped" line must include the texture's path. Log where the input path is known (`convert_dds_to_ktx2_with_options` or the pipeline call site), keep it one line per texture.
3. **Restore the release-mode size check (DeepSeek finding 1).** Before passing a decoded buffer to `intel_tex_2` in `compress_packed_levels`, check `rgba.len() == width*height*4` in release builds too (return an error or `assert!`, matching the function's signature after impl-972 made it infallible: prefer making it return `Result` again only if simple; otherwise `assert_eq!`).
4. **Narrow the portable cache key (both reviews, minor).** In crates/converter/src/pipeline.rs (~1124), append `:portable-textures` only for textures the flag actually changes (packed 8-bit RGB(A) layouts on the CPU path), e.g. by reading the DDS header the way `texture_gpu::takes` narrows the GPU label. If that is not simple, leave it and say so.
5. **Tests (DeepSeek finding 4).** In `packed_decoding_ignores_trailing_bytes_*`, use a genuinely odd pitch where the test means "odd pitch"; assert BC7 opaque alpha decodes to exactly 255 where tests now accept `>= 253`, if the BC7 opaque mode guarantees it (check by running).

## Owned files
crates/converter/src/texture.rs, crates/converter/src/texture_gpu/mod.rs, crates/converter/src/pipeline.rs, docs/specs/converters/dds-to-ktx2.md

## Rules
No git stash, rebase, reset or amend. Match surrounding style.

## Accept
cargo fmt --all -- --check; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo nextest run -p converter --all-targets

## Report
Per item: done/skipped with commit hash and one line; check results.
