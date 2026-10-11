# Publish robustness (Phase 2, upstream)

Not committed: this note is worker bookkeeping, and this checkout is cut from `main`, where
`docs/handoff/` does not exist. The two commits leave it out.

## What changed

`crates/converter/src/pipeline.rs`: `staging_path` and `publish_directory` (now split into
`publish_directory`, `publish_directory_with`, `ensure_no_stale_backup`, `stale_backup`,
`discard_backup`, `sibling_with_suffix`) and the call in `AssetPipeline::run_async`.
`crates/converter/src/config.rs`: `parent_or_cwd` is `pub(crate)` so `stale_backup` reuses it
instead of a second copy.

| Finding | Change | Test |
| --- | --- | --- |
| 1 failed restore discarded | both errors and the backup path are reported | `a_failed_publish_keeps_the_publish_error_and_the_failed_restore` |
| 2 cleanup failure failed a published run | `discard_backup` warns and returns; the caller pushes the warning and exits 0 | `a_failed_cleanup_warns_instead_of_failing_the_publish` |
| 3 pid-scoped stale check | `stale_backup` matches any `<output name>.backup-*` and refuses, before the conversion and before the rename; it never deletes one it did not create | `publishing_refuses_a_backup_left_by_another_run`, `refuses_to_publish_over_a_crashed_runs_backup` |
| 4 dotted output name | `sibling_with_suffix` appends to the whole file name | `a_dotted_output_name_is_still_its_own_resume_directory` |
| 5 no absolute-path guard | fixture test scans the database text values and both JSON artifacts | `generated_interior_plugin_converts_end_to_end`, `absolute_path_signature_sees_the_paths_a_publish_must_not_record` |

## Decisions a reader may question

- **Refusal, not repair.** A stranded `<output name>.backup-*` makes the run fail with the path in
  the message. A run only deletes the backup it created, so nothing removes a folder a crashed run
  left behind; the user does. Combined with finding 2, a failed cleanup therefore blocks the next
  run until the leftover is deleted — deliberate: the leftover holds the previous output.
- **The refusal is checked twice.** Once at the top of `run_async` (a stranded backup should not
  cost a five-hour conversion) and once inside `publish_directory_with`, so the rename is guarded
  whatever calls it. Both go through `ensure_no_stale_backup`.
- **Injection for the failure paths.** `publish_directory_with` takes `rename` and `remove`. Closing
  a handle inside a directory is not a reliable way to make `remove_dir_all` fail on Windows, and a
  publish rename cannot be made to fail halfway from outside; the parameters let both paths run
  against a real tree on every platform.
- **Pre-fix leftovers of a dotted output.** A backup stranded by the *old* code for a dotted output
  is named `<stem>.backup-<pid>` (from `with_extension`) and does not match the new
  `<output name>.backup-` prefix, so it is not detected. Only reachable by upgrading mid-crash with
  an output folder whose name contains a dot.
- **`report.complete` stays true when only the cleanup warned.** The manifest in the published tree
  is written before the publish and already says `complete: true`; the warning is in
  `report.warnings` and on stderr. `converter` bails only when `report.complete` is false, so the
  exit code is 0 — a correctly published tree.

## Verification

`cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets -- -D warnings`,
`cargo nextest run -p converter` (136 passed) — all in this checkout, `--target-dir
C:/Modding/shared-check-phase2` for the latter two as briefed, no schema-looking failure.
The pre-fix runs restored the old bodies (`with_extension` names, pid-scoped check, discarded
restore, propagated `remove_dir_all`) into the same signatures; 4 of the 5 new unit tests failed
there, the happy-path one passed.
