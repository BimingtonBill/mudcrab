# impl-902-p3-launcher-integrate (Phase 3, upstream PR "L3") - handoff

Worker: Claude impl #902. Worktree `C:\Modding\wkj-phase3`, branch `phase3/launcher-conversion`.
Target dir `D:/Modding-overflow/phase3-target`.

**Status: done.** Commits `0a9dab6` (merge eb5a0d6), `f544685` (merge b99bc4b), `e1ffb36`
`feat(launcher): resume a conversion left by an earlier run and stop a running check` (the launcher
edits and spec, one commit as the lead asked; the manifest gate and scroll clamp are listed in the
body). All green on the first run, with no fixes needed.

## Merges (committed)

- `0a9dab6 Merge commit 'eb5a0d6'` (phase3/launcher-resume): kept `staging.rs` and its tests and
  `mod staging;`; the `handlers.rs` conflict resolved to ours (the old start path, `conversion_config`,
  `start_message`, `failure_message`, is gone on this branch).
- `f544685 Merge commit 'b99bc4b'` (phase1/output-check): clean; brings
  `check_output_with_cancel` + `CheckCancelled`. No other `crates/converter/**` edits.

Between the two merges and the next commit, `staging.rs` is unused (dead-code warnings at `0a9dab6`
only).

## Edits (in e1ffb36, launcher + spec only)

1. **Leftover staging at start-up** - `state.rs`: `Input::FoundStaging { staging }`: Idle, Finished,
   Stopped{staging: None} -> `Stopped { staging: Some, cancelled: false }`; no-op elsewhere (a
   staging kept in this session wins). `mod.rs`: `refresh_output_ready` (first frame + Output change,
   not run end) calls `offer_leftover_staging` when the output is chosen and not complete; notice
   "An unfinished conversion was found in <name>: Resume continues it, Delete staging removes it."
   plus " N older unfinished conversion(s) beside it are left untouched." It replaces the "No
   complete conversion ... yet" notice. Nothing deleted. Resume is the normal in-session Resume.
2. **Stop a running check** - `Stop` in `Checking` -> stays `Checking`, `Effect::CancelCheck`;
   `Input::CheckCancelled` -> previous state; `controls(Checking).stop = true`. `runner::spawn_check`
   takes an `Arc<AtomicBool>` and uses `check_output_with_cancel`; `CheckCancelled` error ->
   `RunMessage::CheckCancelled`. `RunHandle.check_cancel` holds the flag. `status.cancel_check()`:
   clock stops, bar to 0, stage line "Quick check stopped", notice "Check stopped." (no partial result).
3. **Check needs a manifest** - new resource `OutputHasManifest`, set in `refresh_output_ready`
   (first frame, Output change, run end); `ControlButton::available(.., has_manifest)`; a press
   without one gives a notice.
4. **Mouse wheel** - `ScrollArea` already works with `DefaultPlugins`: bevy's default `ui` feature
   includes `bevy_ui_widgets` (`UiWidgetsPlugins` adds `ScrollAreaPlugin`) and `picking`/`ui_picking`
   (`UiPickingSettings::require_markers` false); `Pointer<Scroll>` auto-propagates from the hovered
   text to the pane. Found by reading `bevy_ui_widgets-0.19.0/src/scrollarea.rs` and
   `bevy_ui-0.19.0/src/layout/mod.rs`: the layout clamps only the drawn offset, not the stored
   `ScrollPosition`, so after `draw_labels` set y = 1e6 the first wheel-up step was eaten. Added
   `panel::clamp_notice_scroll` (PostUpdate, after `UiSystems::Layout`) using ScrollArea's own
   max-range formula. Not verified by a real wheel in the window (needs a human; the lead's run).
5. Spec `docs/specs/modding/launcher.md` §3 updated (leftover staging, Stop during check, manifest
   gate, runner/staging rows, scroll clamp).

## Tests added/changed

state.rs: Stop during check -> CancelCheck then CheckCancelled -> previous; CheckCancelled outside a
check; leftover found -> Resume/Delete on, Resume carries it; leftover ignored while running,
stopping, checking or with a session staging; `only_stop_is_on_while_checking`. runner.rs: pre-set
flag -> only `CheckCancelled`. mod.rs: Stop during scripted check (no partial result),
CancelCheck sets the flag, leftover at start-up enables Resume (+ Resume effect), older leftovers
counted and kept, leftover beside a complete output ignored, manifest flag follows the Output.
panel.rs: `only_stop_is_on_while_a_check_runs`, no-manifest case. main.rs: whole-launcher leftover
start-up (Resume/Delete/Start on, Check off, no run).

## Checks (CARGO_TARGET_DIR=D:/Modding-overflow/phase3-target, 2026-09-28 14:52-14:55)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (24 s).
- `cargo nextest run -p launcher -p converter`: 270 passed, 9 skipped (was 248 at a1e55cd).
- `cargo build --profile test -p launcher --bin launcher`: `D:/Modding-overflow/phase3-target/debug/launcher.exe`.
- Ran it 10 s each (killed by `timeout`, exit 124), nothing pressed:
  `D:/Modding-overflow/phase3-scratch/integrate-fresh` and `.../integrate-leftover`. The second holds
  a fake `modern_assets.staging-1-1/vfs`, so the lead can screenshot the Resume offer there. No
  panic or error (only gilrs' usual gamepad warning), nothing written, the staging folder untouched.

## Open

- The mouse wheel has not been tried by hand in the window.
- After Delete staging on a found folder, older leftovers are offered again only when the launcher
  restarts or the Output folder changes.
