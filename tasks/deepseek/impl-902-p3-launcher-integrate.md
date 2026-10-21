# impl-902-p3-launcher-integrate (track: Phase 3, upstream PR "L3")

**Worker:** Claude subagent ("Claude impl #902"). **Worktree:** `C:\Modding\wkj-phase3`, branch
`phase3/launcher-conversion` (head a1e55cd: converter-gui folded into `crates/launcher/src/conversion/`).
**Target dir:** `CARGO_TARGET_DIR=D:/Mudcrab/phase3-target`.

Read first: `docs/handoff/impl-901-p3-launcher-conversion.md` and `impl-900-p3-launcher-resume.md`
(both in `C:\Modding\Mudcrab\mudcrab\docs\handoff\`), and `docs/specs/modding/launcher.md` §3.

## 1. Merge `phase3/launcher-resume` (eb5a0d6) into this branch (`git merge`, never rebase)

It adds `crates/launcher/src/staging.rs` (`find_resumable_staging`: the newest leftover
`<output>.staging-*` with a `vfs` folder, plus a count of older ones) and wires it into the OLD
launcher start path, which this branch deleted. Resolve by keeping `staging.rs` and its tests, and
dropping its `handlers.rs` changes (`conversion_config`, `start_message`, `failure_message` belong to
the deleted path), then wire the finder into the conversion module instead:

- At start-up and whenever the Output folder changes, if the output is not already complete and a
  resumable staging exists, put the state machine where Resume is offered for that folder (the same
  state a Stop in this session leaves, i.e. `Stopped { staging, .. }` or whatever `state.rs` uses),
  and push a notice: "An unfinished conversion was found in <name>: Resume continues it, Delete
  staging removes it." (mention the count of older ones if any; never delete them automatically).
- Resume then runs with `resume_staging` set, exactly like an in-session Resume.
- Add a state-table test or a panel test for "leftover staging found at start-up -> Resume enabled".

## 2. Merge `phase1/output-check` b99bc4b (Full check can be stopped)

It adds `converter::check_output_with_cancel(output, mode, progress, cancel: &AtomicBool)` and
`CheckCancelled` (an error type; `error.is::<CheckCancelled>()`); `check_output` is unchanged.
Merge it (`git merge b99bc4b`; it is converter-side, owned by another session: do not edit
`crates/converter/**` beyond what the merge brings). Then in the launcher:

- the check runner owns an `Arc<AtomicBool>`; Stop during a check sets it;
- enable Stop while `Checking`; a cancelled check returns to the previous state with a notice
  "Check stopped." and never shows a partial result;
- tests: the state table for Stop during Checking, and a runner test that a pre-set flag returns
  cancelled.

## 3. Small fixes from the first real run

- Check and Full check are enabled even when the Output folder does not exist; disable them unless
  the Output folder holds a `conversion-manifest.json`.
- Mouse-wheel scrolling of the notice pane: if it needs anything beyond `ScrollArea` to work with
  the wheel in Bevy 0.19, add it (check Bevy's `ui_widgets` / scroll examples in the cargo registry
  source), with a note in the handoff on how you verified it.

## Checks (all with the D: target dir)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p launcher -p converter
cargo build --profile test -p launcher --bin launcher
```

Run the built exe once for ~10 s from `D:/Mudcrab/phase3-scratch` with a timeout; confirm
no panic. Don't press anything; the lead takes the screenshots.

## Commits

Merge commits as git makes them (fix conflicts inside the merge commit), then upstream-style commits,
e.g. `feat(launcher): offer to resume a conversion left by an earlier run`,
`feat(launcher): stop a running check`. No internal references, task numbers or machine paths.
Commit only `crates/launcher/**`, `docs/specs/**` and what the merges bring. Write
`docs/handoff/impl-902-p3-launcher-integrate.md` in `C:\Modding\Mudcrab\mudcrab` (not committed).
