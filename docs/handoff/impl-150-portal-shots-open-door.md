# impl-150 - a reference shot that looks through an open doorway

Date: 2026-09-23. Worker: impl-150-portal-shots-open-door (DeepSeek impl). Base commit `01f1681`.

## What was built

`open_door` on a shot, in the shots file's own notation: a load door **reference FormID as a hex
string** (`"0x0001CBB0"`), optional, absent from every existing file and `None` when absent.

A shot that names a door runs: place the camera -> **open that door with the engine's own
machinery** -> wait for the doorway to be one a player would see -> settle -> capture -> **close the
door again**. A shot that names no door runs exactly as before (no door is ever asked for, so none
is open).

- `crates/engine/src/shots.rs` - the field, the sequence, the phase machine and the tests.
- `docs/design/reference-shots.md` - the field in the contract (table row, example, engine steps,
  a section on the sequence).
- `tools/reference/doorway_shots.json` - the worked examples: two doorways, square and oblique.
- Renders: `local/doorway/` (delivered run), `local/doorway-1/`, `local/doorway-baseline/`
  (no `open_door`: the doors closed) and `local/doorway-candidates/` (pose fitting).

## How a door is opened, and what "open" means here

**Opening is the message a player's `E` writes and nothing else**: `OpenDoor { door }`, which
`door_animation::activate_doors` answers by running the door's own `Open` clip - or, for a door with
no clip, by promoting it to `Open { animated: false }` in the same frame. Nothing crosses: no
`CrossDoor`, no `ActivateDoor`.

The door is asked **once**, and only when it can be asked two things at once:

1. it is streamed in **and carries its `DoorState`** (a message for a reference without one is read
   by nobody), and
2. its own animation has resolved - its scene carries the `AnimationGraphHandle` that
   `door_animation` attaches - **or the view has had nothing left to load for
   `DOOR_QUIET_FRAMES` (120 frames, deliberately `door_animation`'s own `SCENE_WAIT_FRAMES`)**, by
   which time the engine has either given the door its clips or decided it has none.

Asking earlier is what a first cut did, and it is a real trap: a door asked while its clips are in
flight opens as a *static leaf* - a hole with no leaf and no way back, because `activate_doors` has
no path out of `Open { animated: false }`. Two earlier revisions of this task hit it (a 2.5 s timer,
then 5 quiet frames) - both are in the run logs under `local/doorway-candidates/`.

An **already open** door is never asked: `OpenDoor` is a toggle, so asking an open door would close
it. A door another shot left open is recorded as "asked" and simply waited for.

## What the shot waits for before it photographs the doorway

`DoorFacts` (all of it in `shots.log` when a shot fails) and `door_step`:

| fact | where it comes from |
|---|---|
| `door_spawned` | a reference with that FormID in a resident cell, carrying `DoorState` |
| `activation_ready` | the graph, or the quiet-view patience above |
| `open` | `doors::DoorState::is_open` - the state the portal and the crossing read |
| `destination_resident` | `transition::destination_is_resident` |
| `portal_drawn` | the doorway quad is drawn and stands within 512 units of this door, and the camera whose `RenderTarget` is `PortalTexture` is active (`PortalState::open_door` is private to `portal.rs`; this is the same fact read off the geometry it writes) |
| `swing_finished` | the door's `AnimationPlayer::all_finished` - the clip has run to its last key |
| `camera_in_front` | `distance_in_front_of_door >= portal::MIN_PORTAL_DOOR_DISTANCE`, the portal's own test |

Only then does the shot settle (the existing rule, with the quiet window restarted) and capture.

**Failure.** A door that does not get there within `DOOR_TIMEOUT_SECONDS` (30 s) fails **that shot**:
`shots.log` gets a `FAILED:` line naming the door and printing every fact plus the view's counts,
and the run exits non-zero - but the shots after it are still rendered. One such failure was
produced on purpose and is in `local/doorway-candidates/defect1/shots.log`: a shot asking the
*interior* door from behind it (`camera_in_front=false`, `portal_drawn=false`, and the door opened
statically because its clips had not arrived) - which is the diagnostic working.

**Closing.** After a door shot's image is on disk the engine asks the door to close again (the same
message: `E` on an open door closes it) and waits for `DoorState::Closed`, so a shot's frame does
not depend on the shots before it. A door with no clip of its own **cannot** be closed - there is no
swing to play back, which is the same reason a player's `E` cannot close one - and `shots.log` says
so and leaves it open. A close that does not finish in `DOOR_CLOSE_TIMEOUT_SECONDS` (10 s) is a
warning line, not a failure. Both happened in the delivered run (the Sven's House oblique's close
timed out; the Trader's doors have no clip in that run).

## Tests

`cargo test -p engine shots::` - 26 tests, including: the field is optional and absent from the
design example (field-for-field against a struct literal, so a shot without it is the shot it always
was); a malformed `open_door` is rejected at load with the hex notation in the message; `door_step`
ready/waiting/timed-out with every fact load-bearing on its own; the quad counts only for the door
it stands in; a shot with no door never enters the sequence (`phase_after_move`), and a timed-out
door fails that shot and not the rest of the file.

## Renders, and what they show

`tools/reference/doorway_shots.json` -> `local/doorway/` (1400x1050, hfov 75):

| shot | door | what it shows |
|---|---|---|
| `sven-door-square` | 0x0001CBB0 | the leaf swung open and the room beyond it: the hearth wall and its fire, through the doorway, from 350 units out on the door's own axis |
| `sven-door-oblique` | 0x0001CBB0 | the same doorway 25 degrees off its axis; in this run the swung leaf is drawn across the opening |
| `trader-door-square` | 0x0001341F | the shop's counter, horn and hanging herbs through the doorway, from 200 units under the walkway |
| `trader-door-oblique` | 0x0001341F | the doorway 25 degrees off its axis, seen along the walkway, the shop visible through the opening |

Also rendered and usable for comparison: the same four poses with the doors **closed**
(`local/doorway-baseline/`), a 100-unit threshold view of Sven's doorway
(`local/doorway-candidates/mapped1/sven-door-close-100.png`), the Sven's House room from the door's
own `XTEL` arrival (`local/doorway-candidates/defect1/sven-inside-arrival.png`), and the pose the
portal's camera stands at for a 350-unit, 25-degree shot
(`local/doorway-candidates/mapped1/sven-mapped-west-25.png`).

### Is the user's Sven's House defect reproduced? **No - not with this instrument, and I say so plainly**

The user's defect is the interior seen through an open doorway "at the wrong angle"
(`local/demo/user-portal-angle.png`). The instrument now renders that class of view - square, 25
degrees off-axis, at 350, 200 and 100 units, and from the threshold - and **in none of them does the
room look geometrically wrong**: the floor, walls and ceiling converge consistently, the doorway's
edges line up with the portal image, the visible part of the room moves with the camera in a
parallax-consistent way (the closer the camera, the more of the room through the opening), and the
room seen through the doorway at 100 units matches the room seen from the door's own `XTEL` arrival
(`sven-door-close-100.png` against `sven-inside-arrival.png`: the hearth wall centred, the table and
the hanging fish to the left, in both).

Two things this does *not* prove, and the lead should weigh them: the user's screenshot is from the
*threshold*, where the doorway covers most of the frame, and a wrong mapping is a **rotation about
the doorway** - which a comparison of "the room through the opening" against "the room from the
arrival" would show as a rotation of the room around the doorway's centre. I compared the two by
eye, not by overlay; and the demo's own view may have been rendered before the seamless-crossing
work (`49f2df5`, `b535a30`) landed. A lead-side check with the overlay tooling
(`tools/research/uesp_pose_check.py` style) or a fresh look at the user's own repro would settle it.

### Findings the lead may want (engine-side, not this task's files)

1. **A Dwemer large load door draws closed-looking geometry over its doorway while the engine's own
   state says open and the portal is rendering through it.** The AlftandWorld -> Blackreach door
   `0x0006998D` (`local/doorway-1/alftand-blackreach-door-square.png`, log line "open, portal
   rendering through the doorway") is pixel-for-pixel the closed door
   (`local/doorway-baseline/alftand-blackreach-door-square.png`); the Blackreach-side door
   `0x0004E504` does the same. `local/doorway-candidates/render2/alftand-40-at-250.png` and
   `render2/blackreach-door-oblique.png` are the same. The model's own `Plane02` plug - the panel
   `portal.rs` calls the game's black plug - is the suspect, and the doorway quad should be covering
   it. Both doors are useless as instrument sites until that is understood.
2. **What is drawn in the opening at Sven's oblique pose varies between runs** with the same state
   and the same log lines: the swung leaf across the opening (delivered run) against the room and no
   leaf (`local/doorway-candidates/oblique3/oblique-a.png`, and `render1/sven-oblique-west-25.png`).
   The instrument's lines are identical; only the picture differs, so it is portal/door rendering
   (`portal.rs`'s doorway mirror, or the leaf's swing resolution), not the sequence.
3. **The door's swing resolution varies between runs too**: the Riverwood Trader's door
   (`0x0001341F`, the same `FarmhouseLDoor01` model as Sven's) opened *with* its own clip in
   `local/doorway-candidates/trader1/` and opened as a **static leaf** in the delivered run, with no
   `door_animation` warning and no "swing scaled up" line either way. The instrument reports which
   of the two happened, in `shots.log`, and both are one run's answer.

## Could not do / did not check

- **The AlftandWorld -> Blackreach doorway has no usable oblique pose**: the door stands in a
  corridor, and every candidate (20/25/30/40 degrees at 150-500 units) puts the eye inside a wall
  or beside the opening - the five attempts are in
  `local/doorway-candidates/render2/`. The second doorway in the delivered file is therefore the
  Riverwood Trader's, a second *site* rather than a second door model.
- **I could not check whether a conversion is running** (`tasklist` and any process listing are
  refused in this session), so the engine rule "no reader in a folder a conversion is publishing
  into" was verified only by the runs succeeding; nothing was written to
  `C:/Modding/SkyrimConverted`.
- Timing measurements were not taken (not this task's to take).

## Files

| path | what |
|---|---|
| `crates/engine/src/shots.rs` | the field, `DoorWorld`, `DoorFacts`, `DoorStep`, the `Door` and `CloseDoor` phases, the tests |
| `docs/design/reference-shots.md` | the contract: the field, the steps, the section on the sequence |
| `tools/reference/doorway_shots.json` | four shots: Sven's House and the Riverwood Trader, square and oblique |
| `docs/handoff/impl-150-portal-shots-open-door.md` | this note |
| `local/doorway/`, `local/doorway-candidates/`, `local/doorway-baseline/`, `local/doorway-1/` | renders and logs (git-ignored) |
| `tools/_dev_150_*.py` | throwaway database queries (git-ignored by `.gitignore`'s `tools/_dev_*.py`) |
