# impl-265 - Audit leftovers: one heading helper, one doorway geometry owner, no production panic

Owned files: crates/engine/src/transition.rs,crates/engine/src/player.rs,crates/engine/src/config.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user asked for more parallel work on 2026-09-28; these are open audit findings (search each id in `local/agents/memory/findings.jsonl` for the evidence). Line numbers are approximate; find the code by name.

## Goal
1. **CC-20260926-01 (transition.rs):** `doorway_frames` (~885-892) and `source_doorway_frame` (~933) turn a Creation heading with hand-rolled `Quat::from_rotation_y(-heading)`, while the same file declares `heading_to_rotation` (~675) the one entry point for that turn. Route them through `heading_to_rotation` and extend the guard test `heading_to_rotation_agrees_with_every_former_call_shape` so it covers these call shapes too. No behaviour change: the existing transition tests must still pass unchanged.
2. **DCP-20260926-01 (player.rs):** the walk-through plane (~1457-1470) re-derives the doorway frame and centre with `source_doorway_frame(...)` / `source_doorway_centre(...)` instead of taking them from the same doorway map the portal camera and quad use (`door_map` in transition.rs, whose source side is built from those helpers). Make the walk-through plane read its frame and centre from the `door_map` result (call `door_map`, or the smallest shared function it and `door_map` both use, so there is one owner). Add a test that fails if the walk plane and `door_map`'s source side ever disagree, for example by comparing both for an anchored door and for an unanchored one.
3. **config.rs ~150:** `EngineConfig::from_args` ends with `panic!("{error}")` on a bad flag; only tests call it, and the real entry `from_env` exits 2 with a message. Make `from_args` test-only (`#[cfg(test)]`) or remove it and have the tests call `try_from_args(...).unwrap()`, so no production code path in the module can panic on user input. Keep the tests' meaning.

Not in scope: streaming.rs, app.rs, world/cache.rs (another merge lands there on Monday), portal.rs.

## Checks
- `cargo test -p engine --lib` in your worktree; check that your new tests appear in the run (`cargo nextest list -p engine --lib` or the test output) - a shared target can serve a stale binary.
- The short walked tour passes: `<quick exe> --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour <dir under your worktree> --tour-doors 2`. Report its last line.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target, `CARGO_BUILD_JOBS=6`; check `quick/engine.d` names your worktree.
- No timing runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Handoff note in `docs/handoff/impl-265.md`.

## Report
For each item: what changed (file:line), test names, and the tour's last line.
