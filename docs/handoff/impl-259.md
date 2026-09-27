# impl-259 - the scripted crossing lands on the doorway; the tour waits for a door that is not in the world yet; and it never asks an open door to open

Track: portal. Base commit `1347f69`. Worker impl-259-portal-tour-crossing-fixes.1 (DeepSeek,
worktree `local/impl/impl-259-portal-tour-crossing-fixes`). Not committed: the lead integrates.

Three fixes, two files (`crates/engine/src/demo_tour.rs`, `crates/engine/src/transition.rs`), the
two review-258 findings the code comments name plus the door the tour asked to open twice.

## 1. A scripted crossing through an open door lands on the doorway, not on `XTEL`

`crates/engine/src/transition.rs` (`landing_style`, [`apply_door_crossings`])

`ActivateDoor` - a scripted run, `--demo-tour` among them - asked for `CrossingStyle::Snap` and got
the `XTEL` landing **whatever the door it crossed was doing**. The portal only draws its window
through a door that is open or opening (`portal_shows_through`), so a run that opened the door
(today's tour opens it before its own scripted crossing, and photographs the portal view through
it) landed in an `XTEL` pose its own `NN-portal-view.png` never showed: the view the run had just
photographed was abandoned at the swap (review-258's finding 1).

`landing_style(requested, state)` now answers the landing from the style asked for **and the door's
state**: `Snap` while the door is shut (no `DoorState`, `Closed`, `Closing` - a door with no state
counts as closed, as everywhere else), the **mapped** doorway landing for `Snap` through an open or
opening door, and for every `Mapped` request. The state is read in the frame the crossing is applied
- not when the request arrived - because a crossing held for its destination can be let go in a
frame the door is already open in, and it is the landing at the swap that has to match the window
the portal is drawing. `apply_door_crossings` now queries `Option<&DoorState>` on the doors, and the
mapped landing's `OpenDestinationDoor` follows the landing actually taken.

A run that does not add `crate::door_animation`, and the fixtures that drive crossings without one,
keep the `XTEL` landing: a `--demo-tour` of a door the tour left shut is byte-identical to before.

## 2. A door that is not in the world yet is waited for, and named separately

`crates/engine/src/demo_tour.rs` (`PortalWait`, `portal_wait`)

`Phase::PortalOpen` matched the door query's `Option<Option<DoorState>>` with `Some(_)` for "still
waiting" and `_` for "did not open within ten seconds". The outer `None` - no door entity in the
query, so its cell has not streamed in, or it was despawned - therefore fell into the give-up arm on
the phase's **first** frame, when a tenth of a second had passed, and wrote "the door did not open
within 10 s" for a door that was simply not spawned yet: the portal view was skipped for a reason
that was not true (review-258's finding 3).

`portal_wait` now has four answers: `Open` (photograph), `Waiting`, `NeverOpened`, `Missing`. A door
that is not in the world is waited for exactly as long as a door that is opening
([`WALK_OPEN_SECONDS`]), and if it never appears it gets a line of its own - "the door was not in
the world at all in the 10 s the tour waited for it (not spawned, or unloaded)" - instead of the
untrue did-not-open line. Both give-up arms still cross, so a broken door cannot stall a run.

## 3. An already-open door is never asked to open (which would close it)

`crates/engine/src/demo_tour.rs` (`Phase::PortalOpen`'s entry, `WalkAction::Walk`)

`Phase::PortalOpen` wrote `OpenDoor` at the door it found - and `OpenDoor` is what `E` writes, and
`E` at an open door **closes** it (`crate::door_animation`). A door the run's own crossing had
already opened at the arrival (`OpenDestinationDoor`, which fix 1 now also asks for on a scripted
crossing) was therefore shut again by the phase that meant to photograph through it, and the run
then waited ten seconds on a door it had just closed. The phase now reads the door's state first
and writes `OpenDoor` only for a door that is shut, exactly as the walk leaves an open door alone
(`walk_action`).

The walk-through's own line changed with it: it used to be written only after the walk had pressed
`E` (`walk_pressed && !walk_open_noted`), so a door that was already open - an auto-load marker, or
a door the crossing opened at the arrival - walked through with **no portal-view photograph at
all**. It is now written on the first frame the door stands open, pressed or not, and says which:
"door 0001CBAF open after 0.0 s (`pressed_E=false`); walking in from 60 units".

## Tests

`cargo test -p engine --lib`: **674 passed, 0 failed, 3 ignored**. New:

- `a_door_that_is_not_in_the_world_is_waited_for_and_named_separately` - `portal_wait` over the
  outer `None`, over every non-open `DoorState`, and over both `Open` shapes.
- `a_scripted_crossing_lands_on_the_doorway_only_while_its_door_is_shut` - `landing_style`'s table:
  `None`/`Closed`/`Closing` keep the snap; `Opening`/`Open` take the mapped landing; a `Mapped`
  request is mapped whatever the door is doing.
- `a_scripted_crossing_through_an_open_door_lands_where_a_walked_one_does` - the same rule through
  the real system on the Sven's House fixture: shut, the camera is at the `XTEL` point
  (`SVENS_HOUSE_ARRIVAL` + eye height) and no far door is asked for; in `Opening` and in
  `Open { animated: true }` it is at `destination_doorway_centre` and the far door is asked to be out
  of the way, as for the walked crossing in the same fixture.

`cargo clippy --workspace --all-targets -- -D warnings`: clean (only the pre-existing
`proc-macro-error2` future-incompat note). `cargo fmt --all -- --check`: clean - one rustfmt reflow
in `demo_tour.rs` was applied **after** the runs below; it is whitespace only.

## Runs

`target/quick/engine.exe` (the worktree's own target; `target/quick/engine.d` resolves through the
worktree's symlink to `D:\Modding-overflow\wt-targets\impl-259-portal-tour-crossing-fixes\quick\`).

Both are the standing short tours, 2 doors of the Riverwood route, on
`--assets C:/Modding/SkyrimConverted`:

| run | command | verdict |
|---|---|---|
| scripted | `--demo riverwood --demo-tour local/impl-259-tour/scripted --tour-doors 2` | `tour PASSED after 2 crossings (short tour)` |
| walk | `--demo riverwood --walk --demo-tour local/impl-259-tour/walk --tour-doors 2` | `tour PASSED after 2 crossings (short tour)` |

What the two runs show for these fixes (logs `local/impl-259-tour/*/tour.txt`):

- **Scripted, stage 1** (the door back out to Tamriel, `0001CBAF`): the run photographs
  `01-portal-view.png` - the daylit yard seen through the open interior door, checked by eye - and
  crosses to `Vec3(350.35352, -2.1349335, -2450.0664)`, the destination doorway. An earlier run of
  the same command (11:46, deleted with the outputs the full disk broke) instead logged "stage 1:
  the door did not open within 10 s, so the portal view is not photographed" and crossed to the
  `XTEL` point `Vec3(164.04492, -1.8624039, -2833.586)`.
- **Walk, stage 1**: `door 0001CBAF open after 0.0 s (pressed_E=false)` - the door the stage-0
  crossing opened at the arrival, left open, photographed (`01-portal-view.png`) and walked
  through; the swing and far-door checks pass on both stages, and the frames around both swaps are
  kept (`walk-through/00`: 21 of 68 frames; `walk-through/01`: 21 of 30).

## Left undone / next

- The disk was full while an earlier pair of runs was being written; those outputs are deleted and
  every PNG in `local/impl-259-tour/` is from the runs above (sizes 0.4-3.0 MB, none truncated).
- Both tours are short tours (2 doors). The full route was not run: this changes the landing of
  every scripted crossing through an open door, so a full 8-door Riverwood tour is worth one run
  before this is called done.
- Fix 1 is a rule change, not a tuning: any scripted crossing (`ActivateDoor`) that a caller makes
  through a door it has opened now lands on the doorway anchor. Tests and the tour are the callers
  in this repo; a future caller that wants `XTEL` unconditionally has to leave the door shut.
