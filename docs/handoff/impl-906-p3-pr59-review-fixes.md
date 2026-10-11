# impl-906 (Phase 3) - PR #59 review fixes

Worker: Claude impl #906. Worktree `C:\Modding\wkj-phase3-rework`, branch `phase3/launcher-rework`.
Base 77b9869 (PR #59 head). The coordinator asked for the label fix to go in its own commit, so the branch is split in two (not pushed):

- **9d6b9a2**: fixes 1 and 2 (the stop race and the threaded delete). This one goes upstream now.
  fmt, clippy and 268 tests pass on this commit alone.
- **b812e30** (head): fix 3 (the Start label). It waits for its own approval. Its tree is identical
  to the first combined commit a4993d3, which passed 269 tests.

a4993d3 was local-only and is kept as the local branch `backup/launcher-rework-combined`. That
branch can be deleted.

## What changed

1. **Finished while Stopping**: `Input::Finished` is accepted in `Running | Stopping`. `Input::Failed`
   already accepted `Stopping` (keeping the failure's staging and cancelled flags), so it needed only tests.
2. **Delete staging off the main thread**:
   - `runner::spawn_delete` runs `remove_dir_all` on a `delete-staging` thread and reports
     `RunMessage::StagingDeleted { staging, result: Result<(), String> }`. NotFound counts as success.
   - There is a new state, `ConversionState::Deleting { staging, cancelled }`, with every control off
     (Stop and the path rows too). New inputs `StagingDeleted` and `DeleteFailed(String)`:
     - success goes to `Stopped { staging: None }`;
     - failure goes back to `Stopped { staging: Some }` with the error in the pane.
   - `ConversionStatus::deleting` makes the stage line read `Deleting <folder>...`.
   - Play is unchanged during a delete, because the delete never touches the output.
3. **Start label**: `Stopped { staging: None }` now reads "Start" ("Convert again" over a complete
   output). "Start over" remains only while a staging folder is waiting, or during Deleting.

`docs/specs/modding/launcher.md` changes: a new Deleting row in the table, "Stopped without one"
reads "Start", and new paragraphs cover the stop race and the threaded delete.

## Tests

- Checks: fmt clean, clippy `--workspace --all-targets -D warnings` clean, `cargo nextest run -p launcher -p converter` 269 passed.
- New tests:
  - state table: finish or fail while stopping, the delete flow, a failed delete, no-ops while
    deleting, stray delete results, the Start label after a delete;
  - runner: a real delete, an already-gone folder, a failure (a plain file in place of the folder fails on Windows);
  - mod: the real threaded delete end to end, and a scripted failed delete with all controls off;
  - panel: no buttons drawn during Deleting, and the label table.

No worker is running.
