# impl-152 - A crossing under a doorway anchor must not land the player inside the far door

Owned files: crates/engine/src/transition.rs,crates/engine/src/door_animation.rs,crates/engine/src/doors.rs,crates/engine/src/portal.rs,crates/engine/src/player.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Base: the `anchor-wip` commit, which is impl-103 (the doorway anchor) plus the
lead's review corrections. Read first: `docs/handoff/impl-103-portal-doorway-anchor.md`, the module
docs of `transition.rs`, `portal.rs` and `door_animation.rs`, and
`docs/design/animated-doors-and-seamless-crossing.md`.

## The defect (seen by the lead on the real assets)

impl-103 anchors a door's map on the two doorways: a walking (`CrossDoor`, `CrossingStyle::Mapped`)
crossing now lands the player at the **destination doorway's plane** (the depth of its model's
bounds box centre), where before it landed at the link's `XTEL` point about 70 units into the
room. At the destination doorway stands the destination door of the pair - **closed**. While the
portal rendered through the source door that door was hidden (`PortalState::destination_door`,
`drawn_door_visibility`), so the window looked right. The moment the player crosses, it is in the
active space, it is drawn again, and:

- the first frames after every crossing are the back of the far door's planks at point-blank range
  (all 8 Riverwood crossings, in both directions);
- the walk is blocked by it: the tours' final walk test moves 16 units instead of hundreds, and both
  `--demo riverwood --walk --demo-tour` and `--demo alftand --walk --demo-tour` end
  `FAIL walk test: the player did not walk on the ground` / `tour FAILED` after all their crossings
  succeed. Before impl-103 both passed.

Evidence: `local/demo/t103-crossings.jpg` (each row: a frame 8 before the crossing, one 4 after),
`local/demo/t103-riverwood.out` and `local/demo/t103-riverwood/tour.txt`. (Those runs also had a
second bug, a wrong exterior grid, which the base already fixes: the exits showed only sky.)

## What it must do (the lead's design decision)

The crossing must be continuous with what the window showed. Through the window the far door was
not there, and the source door's leaves were mirrored onto the destination doorway *open*
(`PortalDoorMirror`). So at the swap the **destination door of the pair must be open**, posed as the
window showed it, so nothing appears in front of the player and nothing blocks the walk:

1. When a `Mapped` crossing is applied through a door **with a `DoorAnchor`**, put the destination
   door (the load door whose `ref_id` is the link's `destination_ref_id`, found the way
   `update_portal` finds it) into its open state in the same frame the player arrives:
   - an animated door: its `Open` clip at the point the source door's own clip has reached (or at
     its end if that cannot be had), with `DoorState` consistent with that - it must not visibly
     swing through the player;
   - a door without an animation: `DoorState::Open { animated: false }`, which `drawn_door_visibility`
     already hides.
   If the destination door is not spawned yet in that frame, retry on the following frames until it
   is (the destination is resident before a crossing applies, so this should be rare), and give up
   silently if the door goes away.
2. Nothing else changes. The door stays open afterwards until the player closes it (auto-closing is
   a separate, later idea). The portal will then render back through it at the player's back - that
   is correct and wanted.
3. `CrossingStyle::Snap` (`ActivateDoor`, scripted runs) and every door **without** an anchor keep
   today's behaviour exactly: they land at `XTEL`, clear of the door.
4. Every command on an entity that may have been despawned uses the `try_` forms (`try_insert`,
   `try_remove`, ...). A crash from a plain `insert` on a streamed-out door was fixed today; do not
   reintroduce one.

If you find the defect is something other than the closed far door (for example, the player lands
on the wrong side of the doorway plane, or facing the wrong way), stop, say so with the numbers, and
fix that instead - the evidence is what matters, not this brief's diagnosis.

## Tests

- A unit test: an anchored `Mapped` crossing leaves the destination door open (animated and
  non-animated cases), and the player's landing is not inside the destination door's leaf.
- A unit test: an un-anchored crossing and a `Snap` crossing leave the destination door as it was.
- Then run the real thing. You may build the release engine and run it (read-only assets):
  `cargo build --release -p engine`, then
  `target/release/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t152-riverwood`
  and the same with `--demo alftand` into `local/t152-alftand`. Both must end `tour PASSED`, and the
  first frames after each crossing (`walk-through/NN/`) must show the room or street, not planks.
  Report the tour lines and the frames you looked at.

## Report

The usual result JSON: what changed, the tour results, the frames checked, and anything you had to
decide that this brief did not.
