# impl-076 - make the tour cross a doorway the way a player does

(A lead-072 brief. It is written to be run with
`pwsh -File tools/ds_impl.ps1 -Brief tasks/deepseek/impl-076-tour-crosses-cold.md -Name impl-076-tour-crosses-cold -Files crates/engine/src/demo_tour.rs -Accept "cargo test -p engine"`;
lead-072's own session cannot start PowerShell.)

## Goal

`--demo-tour --walk` walks the Riverwood route and crosses eight doorways by pressing `E` and
holding `W` - the same code path a player's crossing takes - and it does that with the destination's
cell pre-streamed for about **12.5 s or more** before the crossing, because it stands still in front
of each door for `PRESTREAM_SECONDS` (6 s) plus the failed-standoff time and the walk itself. A
player who runs into a door gives its destination **2.3 s** (`Shift`, `RUN_SPEED` 350) and one who
walks in gives **5.3 s** (800-unit pre-stream radius at `WALK_SPEED` 150). The tour is therefore 2.4
to 5.4 times warmer than the thing it is supposed to be testing, and it stands still while it warms
up, so the streamer is idle on the frame it crosses.

Change the tour so its crossing is as cold as its route allows, **without changing any photograph it
takes or any file it writes**:

* in the player-driven (`Phase::WalkThrough`) case, replace the fixed `PRESTREAM_SECONDS` standstill
  with a wait for the destination to be **resident**, capped at `PRESTREAM_SECONDS` so a missing
  destination cannot hang the tour;
* keep the fixed wait for the scripted, no-player path (`Phase::Activate` -> `ActivateDoor`), which
  is not the path under test;
* report how cold each crossing was, in the tour's own log.

## Context

Read `crates/engine/src/demo_tour.rs` first - the phase machine at `run_demo_tour`, the constants at
the top (`SETTLE_SECONDS`, `PRESTREAM_SECONDS`, `WALK_STANDOFFS`, `WALK_THROUGH_SECONDS`), and
`docs/design/animated-doors-and-seamless-crossing.md` section 4 for the crossing itself.

Facts established by an audit of the code and the two Riverwood tour logs
(`local/demo/rwbt.out`, a clean run; `local/demo/rwdbg.out`, a run that **crashed on stage 0's
crossing** with `Entity despawned` inside `engine::portal::isolate_cells` - the crash the project lead
is fixing, and **not your business**):

* the tour's walk-through takes the same crossing path as a player: it presses `E` (the door's own
  `Open` clip runs), holds `W`, and the player's `player_walks_through_doors` writes `CrossDoor`,
  which `apply_door_crossings` maps with the same rigid door -> arrival map the portal uses. Nothing
  in the crossing is tour-specific.
* `destination_is_resident` (`crates/engine/src/transition.rs:412`) tests `StreamingWorld::is_resident`
  (`crates/engine/src/streaming.rs:132-140`): the destination cell's **root entity has spawned with
  `CellDetail::Full`**. It says nothing about the cell's glTF scenes, meshes, materials or textures,
  which load afterwards (`track_asset_readiness`). Residency is reached tens of milliseconds after
  `plan_door_prestream` first requests the cell (the pre-streamed keys are at the head of the worker's
  queue, `streaming.rs:904-911`); the *assets* are what take seconds.
* so with this change the standstill at the door drops from 6 s to about a frame, the crossing happens
  roughly 2.2 s after the destination was first requested (the walk in from the standoff), and the
  destination's assets are still loading when the crossing lands - which is the condition the player
  is in.
* **What this does not close, and you should say so in your report rather than trying to close it**:
  a crossing whose destination is *not* resident at all (`CrossingHeld`, `transition.rs:574-580`),
  which the tour cannot reach because it always approaches from a standoff inside the pre-stream
  radius; a running player's 2.3 s; an auto-load marker crossing (Riverwood has none); and the
  source-side churn of a player who has been walking around a village rather than being teleported
  between doors.

## Scope

All of it in `crates/engine/src/demo_tour.rs`. Nothing else in the repository.

1. **The wait.** In `Phase::Prestream`, the player branch (the `player.is_some()` arm that enters
   `Phase::WalkThrough`) must not wait the fixed `PRESTREAM_SECONDS`. Its condition becomes: the
   destination of the door the tour is standing in front of is resident
   (`transition::destination_is_resident` with the `DoorDestination` from the door's `LoadDoor`, and
   the `StreamingWorld` resource - make it `Option<Res<StreamingWorld>>` so a tour with no streaming
   world, as in the unit tests, behaves as it does today), **or** the timer has reached
   `PRESTREAM_SECONDS`. Factor the decision into a small pure function with tests (see below).
   The door photograph (`{stage}-door.png`), the portal-target capture, every file name and the
   `Phase::WalkThrough` sequence itself are unchanged: the photograph is still taken at the standoff,
   before the walk, exactly as now.
2. **The log.** One line per stage, in the tour's established style, saying what the wait was and how
   cold the crossing is: the seconds waited for residency, whether residency had arrived or the cap
   was hit, and - at the crossing, in the existing `DoorCrossed` arm - the seconds from the start of
   the walk to the swap. Facts, not prose: the lead reads these logs instead of the frames when
   something is wrong.
3. **Do not touch anything else.** In particular: the pictures, their names, the contact-sheet
   inputs, `Phase::Survey`/`Phase::Settle`/`LookAround`/`Walk`, the objective text, the exit code
   logic, the standoff ladder, and anything outside `demo_tour.rs`. If you believe a change is needed
   in another file, say so in the report instead of making it.

Tests (in the module's existing test module):

* the pure wait decision: not resident and under the cap -> keep waiting; resident -> go; not
  resident and at the cap -> go anyway; a tour with no `StreamingWorld` -> behaves as today (the
  fixed wait only);
* if you add any state to `DemoTour` (a field for the wait, for example), a test that a fresh
  `DemoTour` starts in the state the tour expects, and that `enter(Phase::WalkThrough)` resets it.

## Done when

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

all pass, the new tests fail without the change, the diff touches `crates/engine/src/demo_tour.rs`
only, and the tour's outputs (file names, photographs, contact-sheet inputs) are provably identical
to before - state in your report how you convinced yourself of that last one.

**Acceptance:** `cargo test -p engine` (the harness runs this; run the other two yourself).

The engine is not run by anyone here: the project lead runs it on the GPU. Say in your report what a
run should show, and what you could not check.

## Report

Short: what changed and why; the tests and what they pin; the three commands with their results; how
you checked the outputs are unchanged; and the list from the third bullet of the context - what this
change still does not exercise.
