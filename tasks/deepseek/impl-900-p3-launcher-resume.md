# impl-900-p3-launcher-resume (track: Phase 3, upstream PR "L1")

**Worker:** Claude subagent ("Claude impl #900"). **Worktree:** `C:\Modding\wkj-phase3-resume`,
branch `phase3/launcher-resume` (cut from `main` 6f583cd). **Target dir:**
`CARGO_TARGET_DIR=D:/Mudcrab/phase3-target-l1` for every cargo command.

## The problem

A community tester reported that upstream's launcher (`crates/launcher`) restarts a Skyrim
conversion from scratch when the previous run didn't finish (window closed, crash, power loss).
The run is disk-bound and takes 20+ minutes, so losing it hurts. The maintainer wants resume.

Why it happens (read `crates/converter/src/pipeline.rs` on this branch, `AssetPipeline::run_async`
around lines 60-125 and `staging_path` ~1063): every run stages into a fresh folder beside the
output, `<output>.staging-<pid>-<nanos>` (e.g. `modern_assets.staging-1234-1727...`). A run that
fails cleanly deletes its staging; a run whose process dies leaves it behind. `PipelineConfig`
already has `resume_staging: Option<PathBuf>`: given a leftover staging folder, the pipeline reuses
it (and its `.ingestion-cache`, and the vfs extraction). The converter CLI exposes this as
`--resume-staging`. The launcher never uses it (`crates/launcher/src/handlers.rs`,
`detect_skyrim_and_start_conversion` / `run_background_converter`).

## The change (launcher-only; do NOT edit `crates/converter/**` or any other crate)

1. A pure, unit-tested function in the launcher, e.g. `find_resumable_staging(output_dir: &Path)
   -> Option<PathBuf>`: look in the output's parent folder for directories named
   `<output file name>.staging-*`, keep those that look like a real staging (contain a `vfs`
   folder), and return the most recently modified. Never delete anything. If there are several,
   return the newest and make the others visible to the caller (e.g. return a small struct with
   the chosen one and a count of ignored ones) so the status line can mention them.
   Match the prefix exactly: `modern_assets.staging-1-2` belongs to `modern_assets`,
   `modern_assets2.staging-1-2` does not.
2. In the start path: when the conversion is not complete and a resumable staging exists, set
   `PipelineConfig.resume_staging` to it and say so in the status text, e.g.
   "Skyrim found at <dir>. Resuming the unfinished conversion in <staging name>...". Otherwise
   behave exactly as today.
3. If the resumed run fails, the error message must say the staging folder is kept (the pipeline
   keeps a resumed staging on error) and where it is, so the user can delete it.
4. No Stop button in this PR: `main`'s converter has no cancel handle. Closing the window still
   leaves the staging, which the next start resumes; that is the whole fix.

Keep the existing code style (read `handlers.rs`, `main.rs` first). Doc comments in the file's
current density. No new dependencies.

## Tests

- Unit tests for the finder with a temp folder: none; one; several (newest wins, count of others);
  a non-staging sibling; a prefix-lookalike (`modern_assets2.staging-*`); a staging without `vfs`.
  Use `std::env::temp_dir()` plus a unique name, as `game_detection.rs`'s tests do; clean up.
- If the start path can be tested without a real conversion (e.g. a function that builds the
  `PipelineConfig` from the paths), test that `resume_staging` is set/unset.

## Checks (what CI runs), all with the D: target dir

```
cargo fmt --all -- --check
cargo clippy -p launcher --all-targets -- -D warnings
cargo nextest run -p launcher
```

**Build gate:** a disk-heavy conversion is running on this machine. Do not run any cargo command
until the lead tells you "builds free". Make and self-review all edits first; if you have not been
told, end your turn with a short "edits done, not built" report and the lead will resume you.

## Commit

Upstream style, no internal references, task numbers or machine paths, e.g.
`feat(launcher): resume an unfinished conversion instead of starting over`. Commit only files under
`crates/launcher/`. Then write `docs/handoff/impl-900-p3-launcher-resume.md` in
`C:\Modding\Mudcrab\mudcrab` (NOT committed there; the lead handles it): what changed, the test
list, check results, anything you were unsure of.
