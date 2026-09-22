# impl-023 - Door front from the link data, not the door model's axis

Owned files: crates/engine/src/doors.rs,crates/engine/src/world/database.rs,crates/engine/src/streaming.rs,crates/engine/src/portal.rs,crates/engine/src/player.rs,crates/engine/src/transition.rs,crates/engine/src/demo_tour.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/research/object-rotation-convention.md`;
- `portal.rs`, especially `door_to_arrival_rotation`, `portal_pose`, the front-side test and
  `measured_portal_extents`.

## Goal

The portal decides which side of a load door is its front, and how the view maps through the door,
from the **door model's local axis** (`door_to_arrival_rotation = arrival * Y180 * door⁻¹`). Door
models do not agree on which local axis is their front. On Skyrim.esm's load doors, measured
against where the game puts the arriving player:
- 43% of placements are 180 degrees from the local `+Y` heading;
- 22% are at 270 degrees;
- 13% are at 0 degrees;
- the rest vary.

(See the per-model analysis in commit `933b95f`'s message.) So for many doors, the portal treats
the back as the front. For example, the Alftand Ruined Tower door `0005BDF1` faces **east**: its
return link puts the arriving player 32 units east, facing heading 92. But the portal only opens it
from the west (impl-022's finding).

Use the data instead. The link that leads **to** a door (a `door_links` row whose
`destination_ref_id` is that door) holds the `XTEL` arrival point in front of it, and a heading
facing away from it. That gives each door's outward direction in world space, independent of its
model.

## What to build

1. **Contract** (`doors.rs`): `LoadDoor` gains `pub outward: Option<[f32; 3]>`. This is the
   door's outward horizontal unit direction in Creation space, taken from the return link: from
   the door's position toward the return link's arrival point, or from that link's arrival heading
   `(sin z, cos z, 0)` when the point is within about 16 units of the door. It is `None` when no
   link leads back. Document it. Update every construction site.
2. **Database** (`world/database.rs`): a `LEFT JOIN door_links back ON back.destination_ref_id =
   r.id` (probe for the table as the existing joins do) that provides the return arrival point and
   heading. Compute `outward` there or in `streaming.rs`, whichever keeps the row type simple. Test
   it with a fixture shaped like the tower door.
3. **Portal** (`portal.rs`): build the source door's frame from `outward` when it is known, and
   keep the model axis only as the fallback.
   - The front-side test becomes "the camera is on the `outward` side of the door".
   - The mapping carries "looking into the door", which is `-outward`, onto the arrival heading at
     the destination.
   - Write it so it is testable as pure functions, and keep `portal_pose`'s property that a camera
     at the door's position lands on the arrival point.
4. **Auto-door trigger** (`player.rs`): "moving toward the door" uses `-outward` when it is known.
   Keep its tests passing and add one for a door whose model axis points the wrong way.

## Tests (write them to fail before your change)

- `outward` for a fixture door with a return link, both from the point and from the heading, and
  `None` without a link.
- The portal's front-side test and mapping, for a door whose model `+Y` points west while
  `outward` points east: the camera east of the door sees the portal, and looking west (into the
  door) maps onto the arrival heading.

## Check it (you can build and run the engine)

Run `cargo build --release -p engine`, then:
- the demo tour, `target/release/engine.exe --assets C:/Modding/SkyrimConverted --demo alftand --demo-tour local/tour`,
  which must pass 4/4, with `NN-door.png` and `NN-portal-view.png` showing each destination;
- a shots file (`docs/design/reference-shots.md`) with the tower door seen from the east at
  about 300 units: Tamriel (73850, 78431, -5500), yaw 270, pitch 0. The portal must now show
  AlftandZCell through the open door.

You can see images: look at them.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template.
