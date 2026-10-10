# Goal
Run the Khronos `ktx validate` tool (KTX-Software 4.4.2, `C:/Tools/KTX-Software/bin/ktx.exe`) on KTX2 files written by the converter's writers in two trees: OLD level order (main c39449b5, worktree `D:/Mudcrab/worktrees/p1-ktx-old`, detached) and NEW level order (branch `phase1/ktx2-mip-order` 649fc45e, worktree `D:/Mudcrab/worktrees/p1-ktx2`). Phase 1 track; evidence for an upstream PR. Nothing is committed in either tree.

# What to produce
For each tree, files from each writer path, all with several mip levels:
1. native BC (the writer `write_native_ktx2` in `crates/converter/src/texture.rs`), uncompressed;
2. native BC after Zstandard supercompression (`supercompress_ktx2_levels`);
3. UASTC combined mips (`combine_ktx2_mip_levels`);
4. GPU UASTC writer `crates/converter/src/texture_gpu/ktx2.rs` `write_uastc`: raw (no Zstandard) and after `supercompress_ktx2_levels`;
5. if cheap, a cubemap (`combine_ktx2_cubemap_faces`) and a volume (`combine_ktx2_volume`).
Name them `<old|new>-<path>.ktx2` in `D:/Mudcrab/scratch/p1-ktx-validate/`.

# How
- In EACH worktree, add a temporary `#[test] #[ignore]` function at the end of the existing `mod tests` of `crates/converter/src/texture.rs` (and reuse helpers the existing tests use to build inputs, e.g. the ones the KTX2 tests in that file call) that writes the files above to `D:/Mudcrab/scratch/p1-ktx-validate/`. Use the same input data in both trees (same sizes, same bytes), so only the writer differs. Keep the test code identical in both trees except the `old`/`new` prefix.
- Run it with the gate, each tree with its OWN target inside its worktree (never share a target between the two):
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo test --manifest-path D:/Mudcrab/worktrees/p1-ktx-old/Cargo.toml -p converter --lib ktx_validate_dump -- --ignored --nocapture`
  and the same with `p1-ktx2`. Check `df -h /d` first; below 15 GB stop and report.
- Then run `C:/Tools/KTX-Software/bin/ktx.exe validate <file>` on every file (also try `--warnings-as-errors` once per file) and save each output to `<file>.validate.txt`.
- Finally revert the temporary test in both worktrees (`git -C <worktree> checkout -- crates/converter/src/texture.rs`) and confirm `git status --untracked-files=no` is clean in both.

# Done when
`D:/Mudcrab/scratch/p1-ktx-validate/RESULTS.md`: a table of file, writer path, old/new, ktx validate exit code, and the first error or warning line, plus the exact commands. Say plainly whether every new-order file passes and which old-order files fail, and with which message (quote it). If a new-order file fails, quote the message and say which spec rule it names.

# Rules
Do not read or scan any `SkyrimConverted*` folder. No commits, no pushes. Foreground commands only. No env-var prefixes on commands.

# Report (under 250 words)
The table in short, the verdict, the paths.
