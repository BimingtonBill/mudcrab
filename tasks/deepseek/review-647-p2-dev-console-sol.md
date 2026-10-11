# Goal
Review the developer console commit as an outside reviewer would, before it goes upstream (issue #88): find bugs, behaviour that used to work and now fails, and anything a maintainer would push back on.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-console`, branch `phase2/dev-console`, commits 93556078..8cb9428f on upstream main a896b903 (the feature plus one round of review fixes). Diff: `git -C D:/Mudcrab/worktrees/p2-console diff a896b903..8cb9428f`.
- Files: new `crates/engine/src/console.rs`; changes in `physics.rs`, `app.rs`, `lib.rs`. Bevy 0.19, bevy_rapier3d 0.36.
- Intended behaviour: backtick (KeyCode::Backquote) opens/closes the console; opening releases the cursor, pauses `Time<Virtual>` (unless already paused), clears held motion; closing restores. Escape closes too. Commands: help, clear, noclip (v), collision (f3), tankard (t), grab (e); the V/F3/T/E keys keep working through the same effect systems. Unknown command: error with a "did you mean" suggestion. Tab autofills the first token.
- After the first review: the V/T/E key systems are normal systems that run the shared effects as Bevy one-shot systems (`register_system` + `commands.run_system`); the console commands call `world.run_system`. Key systems are gated with `run_if(console_closed)`. Escape-close leaves the cursor Released; autofill and suggestions use canonical names only.

# Questions
1. Regressions with the console closed: do V, F3, T, E, WASD, mouse look, Escape release and click recapture behave exactly as on a896b903? Look hard at the switch to one-shot effect systems: when do the deferred `commands.run_system` calls actually run relative to the rest of the `.chain()` and to `walk_camera_follow_system`/`move_held_tankard`, compared with a896b903 where the effect ran inline?
2. Input leaks with the console open: can any gameplay input still act (the toggle key frame, held keys, mouse motion accumulated while open, F3, `fly_camera`, a click)? Does the backtick ever land in the buffer? Can text typed in the console (e.g. "v", "e", "t") also trigger the gameplay keys on the same or the next frame after close?
3. Pause: is `Time<Virtual>` paused/unpaused correctly in every path (backtick, Escape, focus loss while open, already paused before open)? Anything delta-driven that misbehaves on unpause (a huge first delta)?
4. Robustness: any `unwrap`/`expect`/indexing that can panic on user input (empty line, only spaces, unicode, very long input, Tab with an empty buffer), in partial apps without the physics plugins, or headless runs (no window, no camera)?
5. Scope and style: anything beyond the agreed slice, dead code, comments that state the obvious or are wrong, names that don't match the surrounding code; would a maintainer ask to split anything?
6. Tests: do the tests actually prove the gate and the key/command parity, or could they pass with the feature broken?

# Scope
Read only. Change nothing. Only git read commands (`git show`, `git diff`, `git log`, `git grep`). No cargo.

# Done when
Every question above has an answer citing file:line at 8cb9428f.

# Report
Findings first, each: severity (blocker / should fix / nit), file:line, the failure scenario (input then wrong result), suggested fix. Then a one-line answer per question. Under 800 words.
