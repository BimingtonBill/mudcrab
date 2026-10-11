# Goal
Re-review only the fix delta of PR #114 "perf(converter): compress uncompressed DDS textures to native BC1/BC7 instead of re-encoding them with Basis" (Mudcrab-Team/mudcrab, Rust/Bevy engine running Skyrim assets converted ahead of time), before the fix is pushed to the open PR. Be strict, like a stranger's PR. Decide whether the delta fixes the earlier findings and breaks nothing.

# Context
- Delta: `local/review/pr-114/delta.diff` (PR head 12a257fa -> f3693f59). Files after the delta: `local/review/pr-114/head-new/`. Before the delta: `local/review/pr-114/head/`. Upstream main's version: `local/review/pr-114/base/`. Full PR diff before the fix: `local/review/pr-114/full.diff`. No git is available; read the files.
- **Read code only from the review folder.** The checked-out source under `crates/` is a DIFFERENT branch; never cite it.
- What the delta claims to fix:
  1. BLOCKER: the PR had deleted main's X8R8G8B8 fallback (image_dds can't decode X8R8G8B8), so an X8 file the strict packed path refuses (trailing payload bytes, odd pitch) failed where main converted it. Main's decoder functions are restored unchanged (compare with base/crates/converter/src/texture.rs `decode_x8r8g8b8_mips`, `decode_x8r8g8b8_level`, `encode_x8r8g8b8`) as a fallback arm.
  2. The packed failure reason was swallowed; it is now chained into a later failure's error.
  3. Alpha flag/mask disagreement: ALPHA_PIXELS with a zero mask now returns no layout (generic path). Note ddsfile 0.5.2 drops the alpha mask when no alpha flag is set, so a mask without the flag cannot be seen.
  4. Memory: the payload is checked against the declared mips, and decoded RGBA8 capped at 256 MiB, before allocating.
  5. Hand-built A8R8G8B8/A8B8G8R8 channel-order tests; X8R8G8B8 with trailing bytes test.
  6. Comment/test-name/spec nits.
- The author ran fmt, clippy -D warnings and nextest on Rust 1.98.1 (all pass).
- Proposed PR body changes: `local/review/pr-114/pr-text-fixes.md`. Check they match the code.

# Scope
Read only. Change nothing.

# Done when
Each fix is checked against the diff: correct, complete, tested (tests would fail if the fix were reverted), and nothing that used to work on main now fails.

# Report
- Verdict line: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 6, verified only, each marked blocker / should fix / nit.
- Under 350 words.
