# Goal
Design the first slice of a Skyrim-style developer console for the engine (upstream issue #88), against current upstream main, precisely enough that a coder can build it as one reviewable PR (~500-800 lines, engine crate only, no new dependencies).

# Context
- Checkout: this folder (branch phase2/dev-console = upstream main). Engine code: crates/engine/src (app.rs, physics.rs, config.rs, render.rs, streaming.rs and others). Bevy 0.19 (source in the cargo registry: C:/Users/Brannock/.cargo/registry/src/*/bevy_*-0.19*).
- First slice scope (agreed with the user): backtick (`) opens and closes the console and pauses the game while it is open; a command registry with `help`, a clear error for unknown commands, and autofill (Tab completes command names, as a contributor asked); the existing debug keys reachable as commands, with the keys kept as aliases. Out of scope for this slice: click-to-select an object with its ref ID, and `bugreport` (a later PR).
- Constraints: engine crate only; no new crates (Bevy's own features are fine if already enabled; check the workspace Cargo.toml's bevy features, e.g. whether bevy_ui / bevy_text / default font are available); typing in the console must not leak into gameplay (WASD, mouse look, debug keys).

# Questions to answer, with file:line on this checkout
1. Every existing debug key and toggle: which key (the user mentions V noclip, F3 collision outlines, T/E spawning test tankards; find all of them, e.g. `rg "KeyCode::" crates/engine/src`), where its system lives, what state it flips (resource or component), and the run conditions or schedule.
2. How input is read today (ButtonInput<KeyCode>, mouse motion, cursor grab) and the cleanest way to block gameplay input while the console is open (a resource flag plus run conditions, `ButtonInput::clear`, or system ordering). List each system to gate.
3. How to pause the simulation: what "the game" is here (physics, player movement, fly camera, time, streaming?). Recommend what to pause and how (Bevy `Time<Virtual>::pause`, a state, or run conditions), noting streaming should probably keep running.
4. UI: what Bevy 0.19 offers with the enabled features for a text box and a scrollback (bevy_ui Text nodes, the default font, or keyboard text input events such as `KeyboardInput` with `Key::Character`). Recommend the smallest approach. Is there existing UI in the engine to follow (an FPS counter, objective text)?
5. Registry design: a minimal API (name, help line, a handler taking args and &mut World or Commands, returning Ok(text) or Err(text)), how commands register (a plugin builder or a resource), how autofill works (a common prefix of the matching names), the unknown-command error with a "did you mean" (an edit-distance helper exists in config.rs on an open PR; on main write a small one). It should stay open for Luau scripts to register commands later (Phase 3); just note how.
6. A file plan: new module(s), and the touch points in existing files with the expected size of each.
7. A test plan: what is testable headless (registry parse/dispatch/autofill/errors; key toggle parity via the command path; the input gate) and how.

# Scope
Read only. Run only reading and searching tools and `git log`, `git show`. Change nothing.

# Report
Markdown headed "# Developer console, first slice: design", answering 1-7 in order with file:line citations, then a "Risks" list. Be concrete; no word limit.
