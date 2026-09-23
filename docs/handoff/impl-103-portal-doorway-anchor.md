# impl-103 — Anchor the portal on the two doorways instead of the teleport target

**Task:** `tasks/deepseek/impl-103-portal-doorway-anchor.md`. **Deliverable:** the doorway anchor,
in `crates/engine/src/{doors,transition,portal,player,streaming,world/database}.rs`.

## What changed

The door -> destination map is now built per door, from the data, by `transition::door_map`:

- **tier 1** (both doors the same base model) and **tier 2** (different models whose placements
  agree on an axis convention): the map pivots on the source **doorway's centre** and lands on the
  destination **doorway's centre**, facing onto facing.
- **tier 3**: the centres are anchored and today's facings (the door's link-derived frame, the
  `XTEL` arrival heading) are kept.
- **tier 4**: no anchor component at all, and `door_map` builds exactly the inputs the map has
  always had. `a_door_the_data_does_not_support_keeps_todays_map_exactly` asserts that bit for bit.

**The facings are the subtle half** (`doors::DoorwayFacings`). The map's *turn* needs only the
doorways' facing **difference**, which for one model on both sides is exact with no convention at
all (the conventions cancel) - the report's tier 1. But the *frame* is also what says which side a
player walks in from, which the portal's door gate, the quad's own facing and the player's
walk-through plane all read, and a model whose front axis is ~180 degrees from its reference's (the
Farmhouse doors, C = 178.7) would have had all three backwards if tier 1 had used the raw reference
yaws. So:

- both doorways' facings known (`Known`): the map is built in the doorways' own frames - everything
  exact, clip plane included;
- one model, no convention (`SameModel { turn }`): the source frame is the door's **link-derived**
  frame (the side the engine has always trusted, from the link data) and the destination's is that
  frame turned by `turn`, so the room's turn is exact and the side is right, while the clip plane
  inherits the source's own link error (median 3.0 degrees over the install, p90 84.5);
- no facing at all (`Kept`, tier 3): today's two frames.

144 of the 1,358 tier-1 directions are the middle case (`SameModel { turn }`); the other 1,214 are
`Known` on both sides.

The tier decision is `doors::doorway_anchor`, from `DoorwayPlacement`s the database layer reads:
one pass over `references` x `statics` x `door_links` at load (2,204 rows), which also fits each
model's axis convention (circular mean of link-derived facing minus reference yaw; known at >= 4
placements and >= 80% within 15 degrees). No schema change, no converter change, no reconversion.

Everything that used the map uses it: the portal camera, the **clip plane** (the destination
doorway's own plane, which is the image of the source doorway's plane under the anchor), the
doorway quad (laid out in the doorway's own frame), the doorway mirror, the player's walk-through
plane, the crossing, and the streaming paths (`destination_keys` / `destination_grid`, the
pre-stream and the residency gate now use the destination *reference's* cell under an anchor).

Two verified counts from the installed database (`skyrim_world.db`): 2,204 placements, 104 models,
1,515 placements with a convention; tiers 1,358 / 186 / 55 / 605 (the report's tool counts 1,356 /
185 / 55 / 608; the difference is boundary arithmetic in the arrival gate). 358 of the tier-4
directions have no doorway box on one side, exactly the report's number.

## The decisions that were not in the brief

- **The destination door of the pair is hidden while the portal renders through it.** With the clip
  moved to the destination doorway's plane, that door - whose closed leaf stands in exactly that
  plane - would fill the aperture and hide the room. `PortalState::destination_door` +
  `drawn_door_visibility` hide it, and only it: the door the portal renders *through* keeps its
  model, frame and swing. Today's map hid the destination leaf by accident (its clip stands tens of
  units inside the room), so this is a new rule rather than a fix to an old one.
- **`Snap` (demo_tour's `ActivateDoor`) still lands on the `XTEL` point.** It is the game's own
  landing and scripted runs/tests pin it; a scripted run never opens the door it crosses, so no
  portal is drawing an anchored view of that door for the snap to disagree with.
- **Tier 1 refuses an empty model string**: two doors with no model at all are not "the same model".

## Checks

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test -p engine` (329 unit tests, 3 integration, 4 ignored doc-tests) - all pass. New tests
are named in the result report; the two that pin the user's case are
`transition::tests::svens_houses_interior_lines_up_with_its_doorway` (real rows; the old map on the
same rows is the report's 82.8 units and 12.27 degrees) and
`transition::tests::the_destination_doorway_is_drawn_where_the_source_doorway_is_from_off_axis_too`.

## Not done

- No run against the converted assets: the acceptance is the build and the unit tests. Nothing
  rendered, so the visual check at the report's section 8 poses is still the lead's.
- `demo_tour.rs`'s standoff (`door_frame`/`distance_in_front_of_door`) is not anchor-aware; it is a
  tour heuristic and the file was out of scope.
