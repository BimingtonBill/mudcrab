# Goal
Track: Phase 2 (for upstream). The converter's publish step (staging folder renamed over the output
folder) is all-or-nothing and recoverable in every failure the project has seen: no stranded
backup, no false failure after a successful publish, and `--resume-staging` works for any output
folder name.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`fix(converter): ...`), no
  `[deepseek]` prefix, no project-internal references in code, comments or docs.
- The code: `crates/converter/src/pipeline.rs` `staging_path` and `publish_directory` (about
  :1063-1090 on main; line numbers drift, find them), their callers in the same file, and the resume
  check in `crates/converter/src/config.rs` (`--resume-staging` validation, which requires the
  folder name to start with the output's full file name plus `.staging-`).
- Findings from the standing audit (2026-09-26), each confirmed by reading:
  1. A failed restore is discarded (`let _ = fs::rename(&backup, output)`) and only the original
     error returned, so after a staging-rename failure the previous output can stay named
     `*.backup-<pid>` while the message says only "failed to publish converted assets". Report both
     errors and the backup's path.
  2. `fs::remove_dir_all(backup)?` runs after the new tree is in place. A handle held inside the old
     output (Windows "Access is denied": an Explorer window, an indexer, antivirus, a running
     engine) then fails the whole run after a successful publish: a `complete:false` failure report
     and a non-zero exit over a correctly published tree, with the staging folder already gone.
     Removing the old backup must not fail the publish: warn with the backup's path and succeed.
  3. The stale-backup check and the cleanup are pid-scoped, so a backup that a crashed publish left
     under another pid is neither refused nor removed. Detect any `<output name>.backup-*` sibling
     before publishing; refuse with a clear message naming it (never delete a folder the current run
     did not create).
  4. `staging_path` (and the backup name) use `output.with_extension(...)`, which replaces a dotted
     final component (`SkyrimConverted.v2` becomes `SkyrimConverted.staging-...`), while the resume
     validation requires `<full name>.staging-`. So for any output folder whose name contains a dot,
     the converter's own staging folder is rejected by `--resume-staging`. Build the sibling name by
     appending to the full file name.
  5. Unguarded: no test publishes and then checks that `conversion-manifest.json`,
     `integration-report.json` and the database contain no absolute drive/colon paths. Add one if
     it's cheap with the dummy-content fixture; otherwise say why not.
- No test calls `publish_directory` directly today; add direct tests (tempdir; a sibling backup;
  a dotted output name; simulate a failing remove by a read-only or held file only if it is
  reliable on Windows, otherwise test the "warn, don't fail" path through a small injectable
  function).

# Scope
Owned files: `crates/converter/src/pipeline.rs`, `crates/converter/src/config.rs`, converter tests
under `crates/converter/tests/` if needed, `docs/specs/converters/` (only where publishing or
resume is described), and `docs/handoff/impl-607-p2-publish-robustness.md`. If you need another
file, say so in the report instead of editing it.

# Done when
- Each of the 5 points has a test that fails on main and passes after the change (say which for each).
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo nextest run -p converter --target-dir C:/Modding/shared-check-phase2`
- One or two commits in upstream style.

# Shared build folder caveat
`C:/Modding/shared-check-phase2` is shared by several Phase 2 branches, and cargo can reuse another
branch's build of the `shared` crate there (different schema constants). If a test fails naming a
schema or version number, re-run that test without `--target-dir` (the checkout's own `target/`)
before believing it, and report both results.

# Report
Per point: the change, the test name, and whether the test failed before the fix. Anything you
chose not to do and why.
