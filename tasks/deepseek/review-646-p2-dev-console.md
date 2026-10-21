# Goal
Review the developer console commit as an outside reviewer would, before it goes upstream (issue #88): find bugs, behaviour that used to work and now fails, and anything a maintainer would push back on.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-console`, branch `phase2/dev-console`, commit 93556078 on upstream main a896b903. Diff: `git -C D:/Mudcrab/worktrees/p2-console show 93556078`.
- Files: new `crates/engine/src/console.rs`; changes in `physics.rs`, `app.rs`, `lib.rs`. Bevy 0.19, bevy_rapier3d 0.36.
- Intended behaviour: backtick (KeyCode::Backquote) opens/closes the console; opening releases the cursor, pauses `Time<Virtual>` (unless already paused), clears held motion; closing restores. Escape closes too. Commands: help, clear, noclip (v), collision (f3), tankard (t), grab (e); the V/F3/T/E keys keep working through the same effect systems. Unknown command: error with a "did you mean" suggestion. Tab autofills the first token.
- The coder's own notes: Escape-close leaves the cursor Released; autofill and suggestions use canonical names only; `toggle_mode_system` and `world_tankard_input` became exclusive systems that call effect systems via `run_system_once`; a `MotionReset` SystemParam; `headless::fixture_app_with` for tests.

# Questions
1. Regressions with the console closed: do V, F3, T, E, WASD, mouse look, Escape release and click recapture behave exactly as on a896b903? Look hard at the conversion of `toggle_mode_system` and `world_tankard_input` to exclusive systems (ordering inside the existing `.chain()`, system params now resolved inside `run_system_once`, anything that ran every frame and now runs only on a key press, or the reverse).
2. Input leaks with the console open: can any gameplay input still act (the toggle key frame, held keys, mouse motion accumulated while open, F3, `fly_camera`, a click)? Does the backtick ever land in the buffer? Can text typed in the console (e.g. "v", "e", "t") also trigger the gameplay keys on the same or the next frame after close?
3. Pause: is `Time<Virtual>` paused/unpaused correctly in every path (backtick, Escape, focus loss while open, already paused before open)? Anything delta-driven that misbehaves on unpause (a huge first delta)?
4. Robustness: any `unwrap`/`expect`/indexing that can panic on user input (empty line, only spaces, unicode, very long input, Tab with an empty buffer), in partial apps without the physics plugins, or headless runs (no window, no camera)?
5. Scope and style: anything beyond the agreed slice, dead code, comments that state the obvious or are wrong, names that don't match the surrounding code; would a maintainer ask to split anything?
6. Tests: do the tests actually prove the gate and the key/command parity, or could they pass with the feature broken?

# Scope
Read only. Change nothing. Only git read commands (`git show`, `git diff`, `git log`, `git grep`). No cargo.

# Done when
Every question above has an answer citing file:line in commit 93556078.

# Report
Findings first, each: severity (blocker / should fix / nit), file:line, the failure scenario (input then wrong result), suggested fix. Then a one-line answer per question. Under 800 words.
