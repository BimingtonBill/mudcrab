# Goal
Apply confirmed pre-open review fixes to open PR #97 "perf(converter): validate generated artifacts in parallel" (Phase 1 track), as uncommitted edits in the worktree.

# Context
- Worktree `D:/Mudcrab/worktrees/p1-pr97`, branch phase1/parallel-validation, head 64ad53af. Only file: `crates/converter/src/pipeline.rs`. The PR's change: `git diff a896b903 HEAD -- crates/converter/src/pipeline.rs`.
- Fixes to make:
  1. Thread count parity: the validation pool uses `.num_threads(jobs.max(1))` (~line 1489) while the conversion stage uses `.num_threads(cpu_jobs)` (~line 909), where 0 means rayon's default (all cores). Make validation build its pool exactly the way the stage does (0 = rayon default), so `cpu_jobs = 0` does not silently drop validation to one thread.
  2. Deterministic error: with several bad files, the error reported now depends on which thread finishes first. Make it deterministic: report the failure of the lowest artifact index (artifact order as main iterated them), e.g. collect `(index, error)` failures with rayon and pick the minimum index. A run with one bad file must report exactly what main reported. Add a test with two bad files at known positions asserting the first-listed one is reported, run with several threads.
  3. The two GLB test fixtures (~lines 3543 and 3566) contain raw control bytes, including NUL, inside byte-string literals, which makes the source file binary to text tools. Rewrite them with escapes (`b"glTF\x02\x00..."`) producing identical bytes.
  4. In the ignored timing test, `index * total / limit` (~line 3684) can overflow `usize` on 32-bit targets; compute in u64.
- Do not change anything else. No new dependencies.

# Scope
Owned files: crates/converter/src/pipeline.rs
- No git commands except `git diff`, `git status`, `git log`.
- Always pass `--target-dir D:/Mudcrab/builds/p1-pr97` to cargo and use toolchain `+1.98.1`.

# Done when
Acceptance: cargo +1.98.1 fmt --all -- --check, cargo +1.98.1 clippy -p converter --all-targets --target-dir D:/Mudcrab/builds/p1-pr97 -- -D warnings, cargo +1.98.1 nextest run -p converter --target-dir D:/Mudcrab/builds/p1-pr97
- All pass. `git diff` of the file shows no binary hunks (no raw NUL in the source).

# Report
- What changed, per fix; commands run and results.
