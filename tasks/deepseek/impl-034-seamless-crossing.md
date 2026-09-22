# impl-034 - Engine: seamless door crossing (map the player's pose, walk through the doorway)

Owned files: crates/engine/src/transition.rs,crates/engine/src/portal.rs,crates/engine/src/player.rs,crates/engine/src/demo_tour.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- the design `docs/design/animated-doors-and-seamless-crossing.md`: section 4 in full, section 5,
  and task 4 in section 6.

impl-023 (the door front taken from the return link's `XTEL`, `LoadDoor::outward`) has landed in
your base: use it for "front".

## Goal

Walking from one cell into the next through a door must be unnoticeable. There is no snap to the
`XTEL` arrival point: the player's current pose (position, yaw, pitch) is carried through the
portal's rigid door → arrival map, in the frame the eye crosses the doorway plane. The portal image
before the swap and the real destination after it then match pixel for pixel (design section 4.2,
"Why the swap is invisible").

## What to build

These follow the design's task 4:
1. Move `door_to_arrival_rotation`, `portal_pose` and `arrival_frame` into one shared, tested place
   that both the portal and the crossing use (your choice of module within your owned files).
2. Rewrite `apply_door_crossings` to map the pose in Creation space, as in design section 4.2:
   keep the pitch and the eye height, and let `switch_space` handle the rebase once. Keep a **snap
   path** (today's behaviour) for `--demo-tour`, which writes `ActivateDoor` and expects a
   crossing. Use a flag on the message or a separate message. `demo_tour.rs` is yours to adapt.
3. The **plane trigger**, design section 4.3. When the player's feet cross the doorway plane from
   the front (`LoadDoor::outward`) to the back, while the door is open, the crossing fires, once
   per entry.
   - "Open" comes from impl-033's `DoorState`, which may land in parallel. Until it does, treat
     every non-auto load door as open once E was pressed on it. Keep this behind one small
     function the lead can re-point at `DoorState`. Tell the lead in the report exactly where.
   - Auto-load markers keep firing on contact, but take the mapped crossing, not the snap.
   - **E no longer teleports**: it opens the door (and until impl-033, marks it open), and the
     player walks through.
4. The **residency hold**, design section 4.4. A crossing whose destination is not resident is
   held, then applied in the frame it becomes resident.
5. Design section 4.7, row 1: the leaf must not reappear when the camera gets within 1 unit of
   the plane. Fix it within `portal.rs` so that an opened door stays open at any distance.

## Tests (write them to fail before your change)

These follow the design's task 4:
- the map is rigid;
- a player standing in the doorway keeps their eye height;
- the walking direction maps onto the arrival facing;
- the plane trigger fires once per entry, only from the front, and only when the door is open;
- a request whose destination is not resident is held and applied later;
- **regression:** the four real route links (`transition.rs` tests) still round-trip;
- a door 0.5 units from the camera keeps its leaf hidden once opened.

## Check it (you can build and run the engine)

- `cargo build --release -p engine`.
- `--demo-tour` (`target/release/engine.exe --assets C:/Modding/SkyrimConverted --demo alftand --demo-tour local/tour`)
  must still pass 4/4 with 0 errors.
- Add a **walk-through** check to the tour, a new phase or a flag such as
  `--demo-tour-walkthrough`: at each route door, press E, walk forward through the doorway, and
  save a screenshot every frame from 10 frames before to 10 frames after the swap. Look at them:
  you can see images. `tools/research/contact_sheet.py` makes a sheet. There must be no
  doorway-sized jump in the view. Report the sheet paths.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template, and list
anything the lead must wire in files you do not own.
