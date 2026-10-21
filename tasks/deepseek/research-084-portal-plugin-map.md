# research-084-portal-plugin-map - Where the portal track's changes live, and where they should

## Why

The work is now split into two tracks (`docs/pr/two-tracks.md`, and the "Two tracks" section of
`AGENTS.md` - read both first). This checkout is the `portal` branch. Upstream is `main`
(`2105bfd`). Changes flow `main` -> `portal`, never back.

The user has approved a refactor: **every portal-only system registers through one
`PortalPlugin`**, so that `portal` differs from `main` in the files everyone else also edits by as
little as possible. The acceptance the other track set is:

- `git diff main portal -- crates/engine/src/main.rs` shows only one `add_plugins` call;
- `git diff main portal -- crates/engine/src/config.rs` shows only one grouped block of CLI flags;
- the Riverwood tour still passes 8/8 and the Alftand tour 4/4.

**This is a read-only research task. Change no code.** It produces the map the refactor will be
built from. Getting the map wrong is expensive: the refactor touches the files every other task
touches.

## What is known

Measured against `main` today:

| file | vs `main` |
|---|---|
| `crates/engine/src/main.rs` | **identical** |
| `crates/engine/src/config.rs` | +275 lines |
| `crates/engine/src/app.rs` | +1,601 / -22 lines |
| `crates/engine/src/lib.rs` | +10 lines (module declarations) |

and these files exist only on `portal`: `demo_tour.rs`, `door_animation.rs`, `doors.rs`,
`lights.rs`, `model_animation.rs`, `player.rs`, `portal.rs`, `shots.rs`, `snow.rs`,
`transition.rs`, `world/lighting.rs`, `shaders/portal.wgsl`, `shaders/snow.wgsl`.

`main.rs` already matching upstream means the "one `add_plugins` call" has to come from somewhere:
work out where the engine's plugins are actually added today (almost certainly `app.rs`) and
whether the call belongs in `main.rs` or in `app.rs`. Say which, and why, given the goal.

## What to produce

Run `git diff main portal -- crates/engine/src/app.rs crates/engine/src/config.rs crates/engine/src/lib.rs`
and classify **every hunk** into exactly one of:

1. **Portal-only** - exists only to serve doorways, crossings, door and model animation, the
   player controller, the demos, the demo tour or the shots camera. These move into `PortalPlugin`.
2. **Phase 2 in nature** - a fix or feature the upstream engine needs too, whatever track it was
   written on: materials, shadows, lighting and atmosphere, streaming robustness, terrain. These
   do **not** move into `PortalPlugin`; they are candidates the other track will redo against
   upstream. `docs/pr/two-tracks.md` has that track's candidate list - cross-check it and say
   where you disagree.
3. **Glue** - wiring that exists because of (1) but lives in shared code: a `run_if`, a system
   ordering constraint, a resource insert. For each, say whether it can move into the plugin or
   must stay, and why.
4. **Upstream drift** - a hunk that is only whitespace, reordering or a stale copy of upstream
   code. Say what it should become.

For each hunk, give the `app.rs`/`config.rs` line range on the `portal` side and one line saying
what it does. A table per file.

Then answer:

- **The plugin boundary.** What `PortalPlugin` contains, in order, and what it must **not** contain.
- **The CLI flags.** Which of `config.rs`'s additions are portal-only (`--walk`, `--demo`,
  `--demo-tour`, `--shots`, `--start-position` ... check each), and whether they can be one
  grouped block without changing their parsing behaviour. If any flag is shared with Phase 2
  (`--terrain-radius`?), say so.
- **The hard cases.** Anything that cannot cleanly move - a system that reads both portal and
  upstream resources, an ordering that crosses the boundary, a `run_if` on a portal resource inside
  an upstream system. Name each, and propose how to handle it.
- **The order of work.** How to cut this into steps that each leave both tours passing, so a
  failure is caught at the step that caused it.

## Constraints

- Two coding tasks are running right now and will land before the refactor: one in
  `crates/converter/`, one in `portal.rs` and `door_animation.rs`. A lighting change to `lights.rs`
  and `app.rs` is queued after them. Your map is of the tree as it stands; say which parts the
  queued lighting change will move.
- Do not run the engine or any build.

## Deliverable

Your report is the deliverable. Lead with the proposed `PortalPlugin` in five lines, then the
tables, then the hard cases, then the order of work.
