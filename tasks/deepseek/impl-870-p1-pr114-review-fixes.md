# Goal
Apply confirmed pre-open review fixes to draft PR #114 "perf(converter): compress uncompressed DDS textures to native BC1/BC7 instead of re-encoding them with Basis" (Phase 1 track), as uncommitted edits in the worktree.

# Context
- Worktree `D:/Mudcrab/worktrees/p1-rgba8`, branch phase1/native-rgba8, head 12a257fa, base a896b903 (upstream main). The PR's change: `git diff a896b903 HEAD`. Main's version of a file: `git show a896b903:<path>`. Ignore the untracked `crates/converter/examples/` folder (never edit or delete it).
- The PR routes uncompressed 2D DDS (24/32-bit RGB(A)) through `convert_packed_to_native` (BC1/BC7 via intel_tex_2); any failure falls back (`&& let Ok(result) = convert_packed_to_native(..)` around texture.rs:258-265) to the old generic path `image_dds::SurfaceRgba8::decode_dds` + Basis UASTC.
- Confirmed problems and the fixes to make:
  1. BLOCKER: the PR deleted main's fallback arm `Err(_) if dds.get_d3d_format() == Some(D3DFormat::X8R8G8B8) => encode_x8r8g8b8(...)` (main texture.rs ~line 230 area; main's `encode_x8r8g8b8` at line 1145 and `decode_x8r8g8b8_mips`). image_dds cannot decode X8R8G8B8, and the new packed path is stricter (exact payload length, pitch tight or 4-byte aligned), so an X8R8G8B8 file with trailing payload bytes or an odd declared pitch now FAILS where main converted it. Restore main's tolerant X8R8G8B8 behaviour as the fallback when the packed path fails: the decoder that ignores trailing bytes and uses the format's tight pitch, then Basis UASTC exactly as main did. Reuse the PR's generalised decoder only if its behaviour for this case is identical to main's; otherwise restore main's functions. Add a test: an X8R8G8B8 DDS with a full mip chain plus trailing payload bytes still converts (to UASTC), as on main.
  2. When the packed path fails AND the fallback also fails, the packed error is lost. Keep it: chain it into the final error (e.g. `wrap_err` with the packed reason) so reports show why.
  3. Alpha: the layout check takes alpha only when the ALPHA_PIXELS flag is set; a file with a non-zero alpha mask but no flag gets alpha 255 and may land in BC1. When the flag and a non-zero alpha mask disagree, return None from the layout detection (so the file takes main's path unchanged). Add a test.
  4. Memory bound: the only size limit is 16384 per side, so a small file can declare a huge texture and the decoder allocates RGBA8 buffers before failing. Before allocating, check the file actually holds the bytes the declared mips need (the exact-length check), and also reject (fall back) when the decoded RGBA8 total would exceed 256 MiB.
  5. Tests: add hand-built (byte-by-byte, not via the PR's own `packed_dds` helper) A8R8G8B8 and A8B8G8R8 fixtures that pin channel order through the conversion.
  6. Nits: the comment near texture.rs:253 says GPU memory is "a quarter of RGBA8" (BC1 is an eighth; BC7 a quarter): fix. Rename the test `uncompressed_rgba_preserves_and_legacy_packed_falls_back_to_uastc` (~line 2119) to match what it asserts now. In `docs/specs/converters/dds-to-ktx2.md` (~line 57) make the fallback sentence match the code after fix 1.
- No new dependencies.

# Scope
Owned files: crates/converter/src/texture.rs, docs/specs/converters/dds-to-ktx2.md, crates/converter/tests/snapshots/
- If a snapshot changes, say exactly why; do not update snapshots for unrelated reasons.
- No git commands except `git diff`, `git status`, `git log`, `git show`.
- Always pass `--target-dir D:/Mudcrab/builds/p1-pr114` to cargo and use toolchain `+1.98.1`.

# Done when
Acceptance: cargo +1.98.1 fmt --all -- --check, cargo +1.98.1 clippy -p converter --all-targets --target-dir D:/Mudcrab/builds/p1-pr114 -- -D warnings, cargo +1.98.1 nextest run -p converter --target-dir D:/Mudcrab/builds/p1-pr114
- All pass; each new test fails if its fix is reverted.

# Report
- What changed, per fix, with the decisions that weren't obvious; commands run and results.
