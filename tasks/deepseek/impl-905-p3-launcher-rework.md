# impl-905-p3-launcher-rework (track: Phase 3, upstream PR "launcher rework")

**Worker:** Claude subagent ("Claude impl #905"). **Worktree:** `C:\Modding\wkj-phase3-rework`, branch
`phase3/launcher-rework` = main 6f583cd + #55 (`phase1/contributor-experience` e45183c, fast-forward).
**Target dir:** `CARGO_TARGET_DIR=D:/Mudcrab/phase3-target-rework`.

## Why

The launcher rework (converter window folded into the launcher, Stop / Resume / Delete staging / Check /
Full check, leftover staging offered for Resume) exists as `phase3/launcher-conversion` e1ffb36, but that
branch stacks on a converter bundle the project has dropped. Rebuild the same launcher on main, stacked
only on #55 (already merged here) and Phase 1's re-cut progress branch (`converter-progress-main`,
coming; the lead will give you its head).

## Steps

1. Bring the launcher over from e1ffb36 exactly: `git checkout e1ffb36 -- crates/launcher docs/specs/modding/launcher.md`.
   Do NOT bring `crates/converter-gui` (it never existed on main) or anything under `crates/converter/`.
   Compare root `Cargo.toml`/`Cargo.lock` with e1ffb36 for launcher dependency changes and apply only
   those (there may be none). Do not bring L4's `engine_process.rs` (it is upstream separately in #56).
2. Find every converter API the launcher uses (`grep converter:: crates/launcher`) and check each one
   exists at this base (e45183c). Expected: `check_output_with_cancel`, `CheckCancelled`, `CheckMode`,
   `CheckReport` exist (from #55); `Cancellation`, `run_async_with_cancel`, `PipelineFailure`,
   `ProgressEstimate`, `progress::{format_clock, format_bytes, format_elapsed}` come from Phase 1's
   progress branch. List precisely which symbols are missing, with their expected signatures from
   e1ffb36's usage.
3. Commit step 1 as-is (it will not compile yet), e.g.
   `feat(launcher): convert, stop, resume and check from the launcher window` - the lead may ask you to
   squash later. Then STOP and report the missing-symbol list: the lead merges the progress branch when
   it is ready and resumes you.
4. After the lead's go (with the progress head merged): make it compile against the real signatures,
   run the checks, fix, commit, build the exe (`cargo build --profile quick -p launcher --bin launcher`)
   and report the exe path.

Anything the launcher needs from #51 / zstd / native textures / cheap-io / cache-dir: say so rather than
merging it.

## Checks (D: target dir)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p launcher -p converter
```

## Rules

Upstream commit style, no internal references or machine paths. Only `crates/launcher/**`,
`docs/specs/**` and root `Cargo.toml`/`Cargo.lock` if needed. No push. No cargo until step 4's go.
Handoff note `C:\Modding\Mudcrab\mudcrab\docs\handoff\impl-905-p3-launcher-rework.md` (not committed).
