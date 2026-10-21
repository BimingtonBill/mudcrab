# B3 view policy (#187): handoff to Upstream Dev, 2026-10-11

Owner: p2-viewpolicy (B3 handoff thread). Nothing published: no push, no PR, no comment, no GitHub write.

## Status

- Landable branch `phase2/view-policy`, head `c7513b1a`, 16 commits on main `c39449b5`. Rebased, one conflict in `crates/engine/src/lib.rs` (kept `mod renderer_init;` and `pub mod shadow_route;`).
- Checks on `c7513b1a` (own build folder `C:/Modding/Mudcrab/builds/p2-viewpolicy-target`, gated):
  - `cargo fmt --all -- --check`: pass.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass (log has `Finished`, no error lines).
  - `cargo nextest run --workspace --all-targets`: 1156 passed, 19 skipped.
  - `cargo test --workspace --doc`: exit 0, no doctests in the workspace.
  - Summary: `D:/Mudcrab/evidence/p2-viewpolicy/SUMMARY.txt`. The first nextest run hit the background limit at 1155 of 1156 (log kept as `nextest-first-run-killed-at-1155of1156.log`); the rerun passed.
- Portal GPU check: done on a trial tree, not on the branch. The branch is on main, which has no portal code (no `world_ready`), so the check needs the Plus line.

## Why the GPU check ran on a trial

- Branch `plus` (tip `409cf2d7`) already carries B3: commit `c4eb07b6` merged the earlier B3 head. The 16 rebased commits match that earlier head (`git range-diff`), except one context line in `lib.rs`.
- Trial `p2/b3-plus-trial` = `plus` + `phase2/view-policy` merge (commit `bf7fa289`, throwaway, local only, not pushed). What the merge adds on top of plus is main's delta `f40ca1df..c39449b5`: `--io-threads` (default 32), model spawns per frame 4 → 32, the DX12 indirect-count workaround, CODEOWNERS. The converter keeps plus's side (main commit `39fea880` is not in the trial).
- Nine conflicting files, resolved to keep plus's portal code; notes in `D:/Mudcrab/evidence/p2-b3-plus-trial/merge-notes.md`.
- Shared git rerere: the nine resolutions are recorded in the shared rerere cache. A later real merge of main into `plus` that hits `crates/converter/src/pipeline.rs` could pick "keep plus" silently and drop `39fea880`. Run `git rerere forget crates/converter/src/pipeline.rs` while that conflict is open. Records not deleted (no-delete rule).

## Portal GPU check (trial `bf7fa289`, engine `C:/Modding/Mudcrab/builds/p2-b3-plus-trial/release/engine.exe`)

Timing hold `timing-p2-b3-gpu` (owner p2-viewpolicy), cleared after the runs. Runs via `scripts/phase2-profile.ps1 -Scenario portal-heavy -Repetitions 1 -DurationSeconds 45 -SkipBuild`, script in `D:/Mudcrab/evidence/p2-b3-gpu/run-b3-gpu.sh`.

Acceptance (portal-heavy, default settings):
- `world_ready_reached: true`, `time_to_world_ready_ms` 8365 (first run; later runs 4700–5150 ms).
- `streaming_invariant_failures: 0`. Passed.
- Median fps 126.17, p95 9.95 ms on the first run. Later runs are 109–112 fps (see below). Below the 160 fps (6.25 ms) bar.

Water-preset comparison, portal-heavy, three interleaved pairs (`--water-reflection-preset baseline` vs `water`), same trial build:

| Run | Preset | Median fps | Frame p95 ms |
|---|---|---:|---:|
| 1 | baseline | 109.37 | 10.66 |
| 1 | water | 111.09 | 9.90 |
| 2 | baseline | 111.47 | 9.57 |
| 2 | water | 112.13 | 9.37 |
| 3 | baseline | 110.37 | 9.46 |
| 3 | water | 111.54 | 9.98 |

- Baseline mean 110.4 fps, p95 9.90 ms. Water mean 111.6 fps, p95 9.75 ms.
- Difference +1.2 fps (about 1%), p95 −0.15 ms. Run spread is about 2–3 fps, so the two presets are within noise on this scene.
- Campaign folders: `D:/Mudcrab/worktrees/p2-b3-trial/target/profiling/20261011-2014*` to `20261011-2019*` (one per run); the acceptance run is `20261011-201455`.

Caveat: 10-09 numbers (water 65.5 vs default 61.0 fps) were measured before the rebase on an older tree; they are not comparable with these.

## Foundations check

- No shortcuts: the default is the full view (baseline preset, full size, every frame, shadowed). The water preset is a visible, switchable option, not a hidden lower setting. The cost is measured, not hidden.
- Measured: before/after numbers above; acceptance on the trial; `local/review/portal-1121-window-2026-10-09/` for the earlier pricing.
- Data-driven, no hard-coded camera names or IDs: `crates/engine/src/view_policy.rs` has none (grep). Gap: the two preset parameter sets (size, priority, staleness, jump thresholds) are compiled constants, not a data file. Moddable through the preset switch, not through data.
- Moddable, switchable: `--water-reflection-preset <baseline|water>`, `--water-reflection <on|off>`, `--shadows <on|off>`. Unknown or missing preset values are refused (config test).
- Aligned with the 160 fps bar: portal-heavy runs 110–112 fps on the trial. B3 does not reach 160 on its own; the remaining cost is in other lanes (terrain ring, graph interior; see `local/research/portal-heavy-render-thread-2026-10-08.md`).

## The one question for the user

Which water reflection preset ships by default: `baseline` (full view, every frame, shadowed) or `water` (smaller view, shadowless, lower priority)? On portal-heavy the two cost the same within noise (110.4 vs 111.6 fps), so the choice is about how the reflection looks, not speed.

## Handoff

- Branch for Upstream Dev: `phase2/view-policy` at `c7513b1a`, 16 commits (list: `git log --oneline main..phase2/view-policy`).
- Not done here: no push, no PR. The user decides what goes upstream and which preset ships.
- Build folders: `C:/Modding/Mudcrab/builds/p2-viewpolicy-target` (own checks, OWNER.txt), `C:/Modding/Mudcrab/builds/p2-b3-plus-trial` (trial release, OWNER.txt). Deleting either needs the user's yes.
- Worktree `D:/Mudcrab/worktrees/p2-b3-trial` (trial branch, junction `target`) is kept; delete only with the user's yes.

## Update 2026-10-13 (PR job)

- Head `2f6b8f46`, 17 commits, rebased on main `511f2c5f` (after PR #122 merged; conflicts in `lib.rs` and `config.rs`, both kept both sides). Pushed to `myfork`. Nothing on origin.
- Review fixes (DeepSeek read-only review): removed the unread `ViewPolicy::priority`; four doc claims corrected.
- Checks on `2f6b8f46` (own build folder, gated): fmt pass, clippy `--all-features -D warnings` pass, nextest 1278 passed / 20 skipped, doctests pass. Evidence `D:/Mudcrab/evidence/p2-viewpolicy-pr/` (pre-rebase run kept in `pre-rebase-2282/`).
- History scan: clean (9 engine source files, no secrets, no game data, no large blobs, one author).
- GPU check still the trial merge `bf7fa289`; the removed `priority` field was unread, so behaviour is unchanged. Not rerun on the new head.
- Foundations check: shortcut: none hides cost; the water preset is a visible switch, the default stays full size, every frame. Breaks for a mod or another worldspace: nothing found (no hard-coded names or IDs, no worldspace dependence). Gap: preset parameters are compiled constants, so a mod picks a preset but cannot define one.
- PR text: `local/pr-drafts/draft-187-view-policy-body.md`. Queue gate "Seen in portal" is still `no`: the user must see it in the portal demo before the draft opens.

## Parked 2026-10-16

The user parked this work. Nothing was opened upstream. Branch phase2/view-policy (head 2f6b8f46) stays on myfork. The check build folder and the trial worktree are deleted (branch p2/b3-plus-trial kept, local only). Open choices: portal look, default preset (baseline recommended), whether to split out only the shadowless reflection. A rerun needs a cold rebuild.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
