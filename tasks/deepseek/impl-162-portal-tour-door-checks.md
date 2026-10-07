# impl-162 - The tour checks that each door really swings and the far door is open

Owned files: crates/engine/src/demo_tour.rs,crates/engine/src/door_animation.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Read first: `docs/research/checks-for-user-found-defects.md` (research-158), proposals
**#3** (tour half only: assert the door's swing where the tour presses `E`) and **#4** (per-crossing
far-door assertion), with file:line sketches; the sketches predate impl-160's tour rewrite (quiet
settles, ring-buffered frames, `--tour-doors`), so find the places by name. Both defects these catch
passed every tour before: an animated load door hid instead of swinging (fixed in `d420bf9`), and a
crossing landed inside the closed far door (fixed by impl-152).

1. **Swing:** when the tour opens a door whose model has an animation, the door must pass through
   `DoorState::Opening` and reach `Open { animated: true }`; a door that goes straight to
   `Open { animated: false }` although its animation exists is a `FAIL` naming the door. A door with
   no clip at all is fine. Give `DoorAnimation` the smallest public read surface this needs (for
   example `pub fn swings(&self) -> bool`), no more.
2. **Far door:** a few frames after each walked crossing, the far door of that link (the load door
   whose `ref_id` is the link's `destination_ref_id`) must be open. Not open is a `FAIL` naming both
   doors.

Each check writes one line per door to `tour.txt` (ok or FAIL) and a failure fails the tour. Unit-test
the decision logic where it can be tested without assets. Then run both full tours and one
`--tour-doors 1` smoke run (`cargo build --release -p engine`; assets `C:\Modding\Mudcrab\SkyrimConverted`,
read-only; output under `local/`): all must still pass, with the new lines present for every door.
Show the swing check is not vacuous: temporarily disable `forget_lost_door_players`'s registration
(or `DoorModel`), confirm the tour now FAILs on the swing check, and restore it; say so in the report.
Commands on entities use the `try_` forms.
