# Goal
Review a coder's fix that makes the converter's KTX2 writers store mip level data smallest-first (KTX 2.0 section 3.7), with the level index unchanged. Decide accept or fix first. Phase 1 track.

# Context
- The change: the diff of `crates/converter/src/texture.rs` between main f40ca1df and the coder's commit (both given under "This run"). Read the whole diff and the functions it touches.
- The brief it answered: `tasks/deepseek/impl-1900-p1-ktx2-mip-order.md`. The note: `local/research/converter/ktx2-mip-order-2026-10-07.md`.
- Spec: https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html, sections 3.7 (levelCount, data order), 3.9.7 (level index), 3.10 (level alignment: lcm(texel block size, 4) unless supercompressed, where it is 1). Mip padding sits between levels.

# Check
1. Every KTX2 writer in texture.rs now lays data out smallest-first: native 2D, supercompressed (zstd), combined mips, volume, cubemap (faces inside a level stay face order 0-5, layers inside a level stay layer order). The Basis-encoded path: is its output in spec order, and is the coder's claim about it right?
2. Index entries: byte_offset and byte_length right for each level; uncompressed_byte_length right for supercompressed levels; offsets aligned per 3.10; no overlap; data fits the file. The DFD, KVD and SGD offsets in the header are unchanged and still correct.
3. What used to work, now fails: any reader in the converter or engine that assumed level 0 comes first in the file (search crates/ for code reading KTX2 bytes by position rather than through the index, e.g. `combine_ktx2_*`, `inspect_ktx2`, `validate_ktx2`, cache verification, `texture_gpu.rs`, the engine's texture loading). Name each with file:line and whether it still works.
4. Tests: do the new tests actually fail on the old layout (would they catch a regression)? Is any existing test weakened or removed?
5. Foundations check: what shortcut does this take, and what would break for a mod or another worldspace (for example mod textures with odd sizes, non-power-of-two, a single level, arrays, cubemaps with mips)?

You may run, foreground only: `git diff`, `git log`, `git show`, and the gated tests `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p converter --all-targets texture` in the coder's worktree.

# Report (under 400 words)
Verdict (accept / fix first), then findings as `file:line - problem - fix`, most severe first, each tagged must-fix or nice-to-have, then answers to checks 1-5 in one line each.
