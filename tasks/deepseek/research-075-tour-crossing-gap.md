# research-075 - what a player's crossing has that the tour's does not

## Goal

A crash kills the user's session on door crossings when they play the Riverwood demo themselves
(`play-riverwood-demo.cmd` runs `engine --assets C:\Modding\SkyrimConverted --demo riverwood --walk`,
no tour). The project's own scripted tour, `--demo-tour`, is meant to catch that sort of thing and
has crossed all eight Riverwood doorways in a run without failing. The claim to test is:

> the tour waits six seconds in front of each door so its destination pre-streams, while a player
> walks straight up and crosses with work still in flight.

**Test it against the code and the logs and say what is actually different.** Specifically:

1. Exactly which crossing paths exist, and which one each of these takes:
   * the user's `--demo riverwood --walk` run,
   * `--demo-tour` with `--walk` (the tour's `Phase::WalkThrough`: it presses `E` and holds `W`),
   * `--demo-tour` without a player (the tour's `Phase::Activate` -> `ActivateDoor` snap).
   Files: `crates/engine/src/player.rs` (the doorway trigger, auto-load markers, the walk probe),
   `crates/engine/src/transition.rs` (the `CrossDoor`/`ActivateDoor` messages,
   `apply_door_crossings`, `PendingCrossing`/`CrossingHeld`, `plan_door_prestream`,
   `destination_is_resident`), `crates/engine/src/demo_tour.rs` (the phases and the constants
   `SETTLE_SECONDS`, `PRESTREAM_SECONDS`, `WALK_STANDOFFS`, `WALK_THROUGH_SECONDS`, and the order in
   which the tour does its work), `crates/engine/src/streaming.rs` (when a cell spawns, what
   "resident" means, when a cell is unloaded, and what the load/unload radii are versus
   `transition::DOOR_PRESTREAM_RADIUS`).
2. **The timing**, in the two runs that exist. Both logs are plain text; search and read them:
   * `local/demo/rwbt.out` - a `--demo-tour` run of Riverwood that crossed all eight doorways. Its
     `demo_tour` lines carry a timestamp for every phase, and its walk-through lines give the frame
     each swap happened on.
   * `local/demo/rwdbg.out` - a *second* `--demo-tour` run of the same route, on a debug build, which
     **crashed on stage 0's crossing**: search it for `panicked`, `Entity despawned` and
     `isolate_cells`. Compare the two runs' timings up to their stage-0 crossing - the door found, the
     standoff reached, the fallback from 60 to 320 units, the swap - and say what differs. This is the
     evidence that matters most: if the tour can hit the same failure, say so with the timestamps.
3. Whether the tour's pre-stream wait is what decides it. Concretely: how long does a cell take to
   become *resident* after `plan_door_prestream` first requests it (what does `destination_is_resident`
   actually test - the cell root, or its meshes and assets too?), and how long until the cell's
   meshes are spawned and its assets loaded (what does the streaming code wait on, and does anything
   log it)? A player walking at `WALK_SPEED` (`crates/engine/src/player.rs`) through the pre-stream
   radius gets how many seconds; how many does the tour's `Phase::Prestream` plus the stage's
   `Phase::Settle`/`Phase::Survey` give the *next* door in the route? The tour teleports the camera,
   so treat each phase's timestamp as the moment the camera arrives.
4. Then judge this proposal, and **say if it is wrong or insufficient**: for the player-driven
   walk-through, replace the fixed `PRESTREAM_SECONDS` wait with a wait for the destination to be
   *resident* (through `StreamingWorld` and `transition::destination_is_resident`), keeping the old
   value as a cap and logging how long it took; keep the fixed wait for the scripted (`ActivateDoor`)
   path; take the door photograph exactly as it is taken today, and write the same files. Would that
   make the tour's crossing happen while work is still in flight, or would it still be as warm as it
   is now (because residency arrives much earlier, or later, than that)? What does the tour *still*
   not exercise afterwards - name the paths, for example a crossing whose destination is not resident
   at all (`CrossingHeld`), if you believe it does not.
5. Note anything else in the code that makes a *player's* crossing different from the tour's: the
   player's height and velocity state, `Shift` to run, the walk probe, the number of cells resident
   after walking around a village rather than being teleported between doors, the door being
   approached from far away rather than from 320 units, and so on. Rank by how likely each is to
   matter for a crash *on the crossing frame*.

## Context

* **Read only, and no shell.** You cannot run commands, write files, or open the game database; you
  have file reading and searching, including over the logs in `local/demo/`. Do not ask for a build.
* The crash itself is being fixed by the project lead and is **not** your subject: your subject is
  what the tour's crossing does and does not reproduce. Do not propose a fix for the panic itself.
* Useful existing documents: `docs/design/animated-doors-and-seamless-crossing.md` (section 4.4 is the
  held crossing, 4.7 is what still pops), `docs/handoff/impl-066-portal-matches-the-world.md`, and the
  module docs at the top of `crates/engine/src/portal.rs` and `crates/engine/src/transition.rs`.

## Done when

You can state, with `file:line` and log evidence:

* which crossing path each of the three runs takes, and where they diverge;
* the time from "the destination was first requested" to "the crossing happened" in each run, for the
  stage-0 crossing of `rwbt` and `rwdbg`, and what state the destination was in;
* whether the tour's six-second wait is what keeps the tour out of trouble, and if not, what does;
* your verdict on the proposal in item 4, and what remains unexercised after it - as a short list, not
  prose.

## Report

A short report: the answer to each numbered item, the evidence as `file:line` or as a log timestamp,
and a final "what the tour cannot test, even after the change" list. Numbers over adjectives; where
you are inferring rather than reading, say so.
