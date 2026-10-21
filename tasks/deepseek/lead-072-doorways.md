# lead-072 - Doorways: the portal, the crossing, and the races around them

You are a **lead**: you plan, write briefs, spawn coding workers with `tools/ds_impl.ps1`, review
their diffs, and report. You do not write production code yourself.

## Your territory, and nothing outside it

You own `crates/engine/src/portal.rs`, `transition.rs`, `door_animation.rs`, `doors.rs`,
`demo_tour.rs` and `crates/engine/src/shaders/portal.wgsl`.

Two other leads are running: one owns `crates/converter/**`, one owns the lighting files including
`lights.rs`, `world/lighting.rs` and the lighting blocks of `app.rs`. **`app.rs` is not yours** -
if you need a change there, state the exact lines in your report and leave them for Claude.

## Hard constraints

- **At most ONE `ds_impl.ps1` coding worker at a time** (~11 GB per checkout, ~50 GB free).
  Discard each worktree once you have read its diff.
- **Nobody runs the engine.** Claude runs it and reports back.

## Read first

`docs/design/animated-doors-and-seamless-crossing.md`, the module docs at the top of `portal.rs`,
and `docs/handoff/impl-066-portal-matches-the-world.md` - that task landed today and its handoff
names most of what is left.

## The work

**1. The door is swallowed by the portal.** The user's own note: *"the door should animate and
render over/on top of the portal"*. impl-066 established that Riverwood's doors do animate and are
drawn, and that the remaining case is geometric: the portal quad sits in the doorway plane, and a
leaf swung fully past that plane away from the viewer ends up **behind the portal image**. Its
handoff says fixing it needs the door drawn in a second pass, or as an overlay for source-space
geometry. Design that and land it. This is the highest-value item you have, because it is the one
the user can see.

**2. The sun leaks into the doorway.** impl-066 measured it: the `DirectionalLight` is global and
cannot be made per-camera, so the source space's sun lights the destination image. Riverwood makes
this obvious - a daylit street seen from inside a house.

**3. The first frame after a crossing wears the old side's atmosphere.** Measured by impl-066
across a real crossing (means 0.668 to 0.616, correlation 0.973 with the steady frames). A
system-ordering pop.

**4. Exterior/interior alignment through portals.** A note from the user that has not been pinned
down: something about how the interior lines up with the doorway it is seen through. Characterise
it precisely from the existing frames before proposing anything - `local/demo/riverwood2/walk-through/`
holds every frame either side of eight crossings and `local/demo/rwbt/` is another full tour.
**If you cannot find a defect, say so.** A clear "here is what I checked and it looks correct" is
a useful result and much better than inventing one.

**5. The tour does not catch crossing bugs.** This matters more than it looks. A crash on every
door crossing killed the user's session four times today, and `--demo-tour` crossed all eight
Riverwood doorways cleanly on the *same binary*, every time. The reason is that the tour waits six
seconds in front of each door for the destination to pre-stream, while a player walks straight up
and crosses with work still in flight. Close that gap so the tour can fail the way a player does.
Do not change what the tour photographs or its contact sheets.

**Not yours: the crash itself.** Claude is tracking it with a `bevy/debug` build that names the
panicking system, and will fix it directly. Item 5 is about the *test*, not the bug.

## How to report

A brief per task in `tasks/deepseek/`, numbered from 073 upwards. Design notes go in
`docs/design/`. Your final report says what landed, what you could not reproduce, and which frames
Claude should render to check each change.

## What "done" means for each worker you spawn

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

Read the whole diff of every patch before you accept it.
