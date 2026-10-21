**Status:** DeepSeek research-158, 2026-09-24, kept verbatim. Check 1 (the re-instanced-scene unit test) landed with the door-swing fix as `a_door_that_lost_its_animation_player_is_given_its_animation_again`; the others are proposals.

# research-158 — Checks that would have caught what only the user caught

## What I did

Read the code paths behind all five defects (no builds, no engine runs, per the brief): `door_animation.rs` (incl. the lead's uncommitted `DoorModel`/`forget_lost_door_players` fix, read as the fix), `portal.rs` (`MainCameraQuery`, `update_portal`), `demo_tour.rs`, `transition.rs` (`door_map`, `destination_grid`, `apply_door_crossings`), `world/database.rs` (`doorway_placements`), `doors.rs` (`DoorState`), `shots.rs` (`DoorFacts`), `tools/research/exposure_stats.py`, `tools/reference/riverwood_shots.json` (+ `docs/demo/README.md`, `docs/roadmap/backlog-engine-and-portal.md`). No files changed; this report is the deliverable (the brief says so, and I must not edit tracked files — the lead can drop it into `docs/research/`).

**The single biggest hole, found while reading:** the fast check the lead runs most — `--demo-tour` **without** `--walk` — never opens a door at all. `Phase::Activate` (`demo_tour.rs:627-645`) writes `ActivateDoor`, and `ActivateDoor`'s own doc (`doors.rs:528-541`) says it "is deliberately *not* what `E` writes and not what opens a door … a snap must not start an animation". So `--demo alftand --demo-tour` and `--demo riverwood --demo-tour` exercise **zero** of `door_animation.rs`: 4-5 minutes per run, and the whole door-animation subsystem is outside them. Only the `--walk` walk-through presses `E` (`demo_tour.rs:755-758`), and even there nothing asserts what the door did.

---

## Defect by defect

### 1. Load doors hid instead of swinging

- **Cause.** `activate_doors` (`door_animation.rs:1307`) → `play_clip` fails on the recorded player → `DoorState::Open { animated: false }` (`door_animation.rs:1336-1342`). The old player entity is gone because the door's scene was re-instanced after `attach_animation` recorded it (`door_animation.rs:719-737`). Fix: `forget_lost_door_players` (`door_animation.rs:1263-1293`).
- **Why every check passed.**
  - **Unit tests were structurally blind.** Every door fixture hand-builds the player and the leaf and inserts `DoorAnimation` itself (`animated_door_swinging`, `door_animation.rs:1889-1968`); nothing in the suite ever removes or replaces a player or re-instances a scene. There is **no test of `forget_lost_door_players`** (grep of the test module for `lost`/`replaced`: no hits) — the fix is currently untested.
  - **The tours were blind too.** Scripted tour: never opens a door (above). Walk-through: presses `E` but asserts nothing, and a hidden door is still walkable — `door_is_open` is true for `Open { animated: false }` (`doors.rs:469-471`), and `mesh_is_out_of_the_way` (`doors.rs:509`) makes the walk probe skip the leaf, so the tour's only verdict (crossings + walk test) is unaffected.
  - **The shots run was blind by one line.** `DoorFacts.animated` exists and is logged (`shots.rs:454`, `675-678`) but is **not** in `doorway_ready()` (`shots.rs:465-472`). Under the bug `door_player()` returns `None` (the new player has no `AnimationGraphHandle`), so `swing_finished` is `true` (`shots.rs:674`) and the shot is judged **ready** and photographed as an empty doorway — a passing shot of a hole.
- **Checks that would have caught it.**
  - **(a) A unit regression test** for the re-instancing path, in `door_animation.rs`'s test module next to `a_door_whose_scene_never_appears_is_given_up_on_as_a_static_door` (line 2767): attach the animation, despawn the recorded player and spawn a fresh `AnimationPlayer` under the same door (the loader's re-instance), run a few frames — `forget_lost_door_players` must drop `DoorAnimation`, `attach_door_animations` must re-attach to the new player — then `activate` and assert `state == Opening`. Cost: milliseconds, no assets, no GPU. It would have failed on the pre-fix tree on the first frame of the assertion (the door would be `Open { animated: false }`).
  - **(b) An assertion where `E` is pressed**: the door's model carries clips ⇒ its state must never become `Open { animated: false }`, and must reach `Opening` then `Open { animated: true }`. Cost: free in the tour's walk-through, and one field in the shots run (both sketched below).

### 2. The doorway image lagged the camera by one frame

- **Cause (pre-fix).** `update_portal` read the main camera's `GlobalTransform` in `Update`; propagation happens in `PostUpdate`. The fix reads `Transform` (`portal.rs:1909-1910`) and the type doc records why (`portal.rs:907-923`).
- **Why every check passed.** `portal_app`'s `spawn_camera` (`portal.rs:2152-2160`) inserts `Transform` and `GlobalTransform` **with the same value**, and no portal test ever moves the main camera between updates and then asserts the portal camera's placement. Worse: **nothing asserts the portal camera's pose at all** — the selection/placement tests write `PortalState::open_door` directly via `portal_shows` (`portal.rs:2235`), and the one test that runs the real `update_portal` (`a_portal_that_stops_placing_its_camera_closes_its_door`, `portal.rs:3031`) is a test of it *not* placing the camera. `portal_pose` is unit-tested as a pure function (`portal.rs:2258`), which cannot see a frame lag.
- **Check that would have caught it.** A unit test: spawn the main camera, then move **only** its `Transform` (leaving `GlobalTransform` stale — exactly what a mid-frame player move looks like), run one `app.update()`, and assert the portal camera's `Transform` equals `door_map(...).pose(new pose)` (`transition.rs:436`, `404-413`). On the pre-fix tree the portal camera sits at the *old* pose. Cost: one fixture problem (the portal must actually place a camera, which needs a resident destination — `update_portal` returns early without `StreamingWorld`, `portal.rs:1894-1898`), so either a `#[cfg(test)]` constructor on `StreamingWorld` (`streaming.rs:120-142`, `cells` is private) that marks one `CellKey` resident, or the existing fixture database (`transition.rs:2313 write_fixture`). Then <1 ms of run time.

### 3. The wrong part of the world streamed behind exterior doors

- **Cause (pre-fix).** The anchor's `destination_grid` came from the *cell's* grid (`cells.grid_x/grid_y`, joined in `DOOR_JOIN`, `world/database.rs:458-462`), and a worldspace's persistent cell is (0, 0) wherever its references stand. Fix: the grid is computed from the reference's own position (`world/database.rs:679-692`), and read by `destination_grid`/`destination_keys`/`destination_is_resident` (`transition.rs:643-677`).
- **Why every check passed.** No test asserted the grid at all before it; the failure mode (sky behind the door) is only visible in a picture, and no tour walks an *exterior* destination from a persistent-cell door in a way that reports it.
- **Checks.**
  - The fixture unit test now exists (`world/database.rs:1347-1356`: cell 10 says (2, −3), reference 100 stands at x≈0 ⇒ grid must be (0, 0)). Its shape is slightly artificial (a cell (2,−3) cannot hold x≈0); the Riverwood shape — **cell grid (0,0), reference far away, expect the position's grid** — is worth asserting explicitly too, since it is the real case.
  - **A data-level audit over the real converted database** (new, and cheap): for every door link with an exterior destination, `door_anchor(reference).destination_grid == Some(grid_of(reference.position))` and that grid's cell exists in that worldspace with land. Cost: one SQL pass over ~1,000 exterior links, well under a second; lives as an `#[ignore]`d test gated on `MUDCRAB_CONVERTED_DIR`, which is the established pattern (`door_animation.rs:2814`, `model_animation.rs:644`, `crates/converter/src/mesh.rs:1891`), or as a mode in `tools/research/world_db.py` (`tools/research/world_db.py:22 connect()`). No GPU, no engine run.

### 4. A crossing landed the player inside the closed far door

- **Cause (pre-fix).** Nothing opened the far door of a mapped anchored crossing, so the swap frame put the player against a `Closed` leaf at point-blank. Fix: `OpenDestinationDoor` + `open_arrival_doors` (`door_animation.rs:1434-1528`), written by the crossing (`transition.rs:888-893`) and run after `DoorTransition`, before `PortalFrame` (`door_animation.rs:396-398`) — i.e. the far door is open **in the swap frame**.
- **Why the tour only caught the effect.** The walk-through photographs the window but asserts nothing per crossing; the verdict came at the end (`Phase::Walk`'s "moved < 100 units horizontally", `demo_tour.rs:871-874`), which names no crossing and no door.
- **Check that would have caught it.** A **per-crossing assertion at the swap frame**: for the door the player just walked through, the link's destination reference must be `door_is_open` within a few frames of the swap (10 frames is generous; the fix does it in the same frame). Under the bug that door is `Closed` and the line names it: `FAIL stage 3: the far door 0006998D of the crossing is closed in the frame the player arrived (state Closed)`. Cost: ~0 (the tour already holds the door state, see the sketch). The tour needs the destination ref id, which it can take from the source door before the swap (`demo_tour.rs:683` reads that door every frame until the crossing) — no shared-contract change. Extending `DoorCrossed` (`doors.rs:546`) with `destination_ref_id` is the tidier alternative: it is in hand at `transition.rs:891`, and there are only three construction sites (`transition.rs:896`, `player.rs:2191`, `transition.rs:2501`) — but that is a `crates/shared`-adjacent contract change, so the lead's call.
  - The walk-out assertion is the cheap fallback if the door lookup is awkward: after the swap keep `W` held ~2 s and require the player to leave the doorway (move ≥ some tens of units away from the swap position). It needs no door lookup at all and adds ~16 s to the Riverwood tour (8 stages), ~8 s to Alftand.

### 5. Water a flat white sheet; firelit interiors too dark

- **Why every check passed.** These are the only two defects of the five that are *purely* pixel facts, and nothing in the repo fails on a pixel. `exposure_stats.py` already computes exactly the right quantities — `near_white`, `saturation`, `mean`, `mean_top10`, and the 15th-45th percentile band's colour (`tools/research/exposure_stats.py:43-79`) — but it only **prints**. `compare_shots.py` builds contact sheets (`tools/compare_shots.py`), `tests/test_reference_shots.py` checks the shots-file *contract*, and none of them look at a render.
- **Checks.**
  - **A `--check` mode over a shots file**: for each render, compare the measured statistics against a band, per shot name, and exit non-zero naming the shot and the number. Bands live next to the pose (the shots file's own doc says the engine "ignores any field it does not know", `shots.rs:134-135`), so the threshold and the `note` that says what the reference shows sit together.
  - Two rules carry the two defects, and both are **absolute**, so they need no UESP reference (and can therefore run anywhere a render exists):
    - water: `flat_white` = fraction of pixels with luminance ≥ 0.9 **and** saturation < 0.05. A flat white sheet is large in that fraction and near-zero in saturation; `RW-05-mill-wheel-across-the-river`, `RW-06-river-to-the-bridge`, `RW-08-guardian-stones` (`tools/reference/riverwood_shots.json:70,86,118`) are the poses. Rough guess at a threshold: `flat_white < 0.15`; measure the current sheets to set it (`local/reference/`), the reference's own value to sanity-check.
    - firelit interiors: `mean` and `mean_top10` of the render against the reference for `RW-09-inn-common-room`, `RW-10-trader-shop-floor`, `RW-11-alvors-house`, `RW-12-faendals-house` (`riverwood_shots.json:134,150,166,182`; the Sleeping Giant's hearth is the backlog's example, `docs/roadmap/backlog-engine-and-portal.md:22-24`), with a wide band (e.g. mean ≥ 0.6 × the reference's) because the poses differ. A gross "hearth gives nothing" failure moves it far more than the pose noise does.
  - Cost: numpy over 12-26 PNGs at 1920×1080 ≈ 3-15 s, no GPU; the expensive part (the `--shots` render) is already run for the reference sheets. A one-line addition to `statistics()` gives `flat_white`.
  - A second, engine-side check specific to the fire is possible and cheap if wanted: assert that a shot's view selects at least the `LIGH` references within range (the budget is `budget_lights`, `lights.rs:309-375`) — it catches "the hearth's light is not there at all", but the calibration defect (there, just too dim) is a pixel fact and only the metric above catches it.

---

## The five checks I recommend, ranked

Costs are *added* run time on top of what is run today.

### 1. Unit test: a door survives its scene being re-instanced — defect 1
**Cost: <1 ms** (in `cargo test -p engine --lib`, no assets, no GPU). **Where:** `crates/engine/src/door_animation.rs`, test module.
**Why first:** it is the only check that tests the fix that is currently untested, it runs in CI, and it fails on the pre-fix tree.

```rust
/// A door whose scene is instanced again after its animation was attached keeps its swing: the
/// player recorded then is gone, and a door that answered that with `Open { animated: false }`
/// hid its whole model instead of opening (the user, 2026-09-24).
#[test]
fn a_door_whose_scene_is_re_instanced_gets_its_animation_again() {
    let mut app = door_app();
    let model = door_with_model(&mut app, WIDE_SWING_DEGREES);   // the existing fixture: Gltf + scene
    step(&mut app, 2);
    let lost = /* player from DoorAnimation of model.door */;
    // The loader re-instances the scene: the recorded player goes, a fresh one arrives.
    app.world_mut().entity_mut(lost).despawn();
    let fresh = spawn_player(&mut app, model.door);              // AnimationPlayer, no graph
    step(&mut app, 2);
    assert_ne!(player_of(&app, model.door), lost);               // re-attached, not the stale one
    activate(&mut app, model.door);
    assert_eq!(state(&app, model.door), DoorState::Opening,
        "the door must swing, not vanish: a lost player is re-resolved in `forget_lost_door_players`");
}
```

### 2. Unit test: the portal camera follows the camera it was moved to *this* frame — defect 2
**Cost: <1 ms** plus a small fixture. **Where:** `crates/engine/src/portal.rs`, test module (needs a resident destination: a `#[cfg(test)]` constructor on `StreamingWorld`, `streaming.rs:120-142`, or `transition.rs:2313`'s fixture DB).

```rust
#[test]
fn the_portal_camera_is_placed_from_this_frames_camera_pose() {
    let mut app = portal_app();
    let (door, _) = spawn_door(&mut app, interior_door(INTERIOR_ALFTAND01));
    let camera = spawn_camera(&mut app, Vec3::new(0.0, 120.0, 300.0));
    app.world_mut().entity_mut(door).insert(DoorState::Open { animated: false });
    resident_destination(&mut app);                 // the fixture: destination_is_resident() == true
    add_update_portal(&mut app);
    update(&mut app, 1);

    // The player moves the camera in `Update`; `GlobalTransform` is a frame behind until PostUpdate.
    let moved = Transform::from_translation(Vec3::new(40.0, 130.0, 260.0))
        .with_rotation(Quat::from_rotation_y(0.3));
    app.world_mut().entity_mut(camera).insert(moved);   // Transform only: GlobalTransform stays stale
    update(&mut app, 1);

    let (want_position, _) = case_map_for(&app, door).pose(moved.translation, moved.rotation);
    let portal = app.world().entity(entity_with::<PortalCamera>(&mut app).unwrap())
        .get::<Transform>().unwrap().translation;
    assert!((portal - want_position).length() < 1.0e-3,
        "the doorway image is a frame behind the camera it is drawn from");
}
```

The fixture's shape (`Transform` moved, `GlobalTransform` deliberately stale) is the whole point: `spawn_camera` (`portal.rs:2152`) sets both to the same value, which is why the existing tests cannot see this.

### 3. Assert the door's own swing where it is asked to open — defect 1 in play (and a door state machine check on real data)
**Cost: ~0** in the tours, **one field** in the shots run. **Where:** two plug-in points, both needing the same tiny public surface.

Public surface (`door_animation.rs:234` and `719-737`): make the type name public with private fields and one accessor —

```rust
pub struct DoorAnimation { /* fields unchanged */ }
impl DoorAnimation { pub fn swings(&self) -> bool { self.open.is_some() } }
```
(then `forget_lost_door_players`'s removal and re-attach keep working unchanged).

*In `shots.rs`* — the door shots already wait for a swing and already log `animated`; make it an assertion and the lead's existing `--shots tools/reference/doorway_shots.json` run becomes a door-animation check with no new run at all:

```rust
// DoorFacts: + pub swings: bool        (the door's model carries an Open clip)
//            + pub fn swings_without_animating(&self) -> bool { self.swings && self.open && !self.animated }
// DoorWorld.facts(): swings: animation.is_some_and(DoorAnimation::swings)
// door_step(): TimedOut already exists; add a `DoorStep::Wrong` for the hard failure and print
//              `describe()` - it already carries animated/swing_finished.
```

*In `demo_tour.rs`* — the walk-through, which already has the state in hand (`doors`'s fourth element is `Option<&DoorOpen>` = `Option<&DoorState>`, `demo_tour.rs:473`, `transition.rs:170`):

```rust
// DemoTour's doors query: (Entity, &GlobalTransform, &LoadDoor, Option<&DoorState>, Option<&DoorAnimation>)
// In Phase::WalkThrough, next to the E press at demo_tour.rs:755:
if let Some(state) = open
    && animation.is_some_and(DoorAnimation::swings)     // this model has an Open clip
    && matches!(state, DoorState::Open { animated: false })
{
    tour.note(format!("FAIL stage {}: door {:08X} opened without swinging - its scene was \
                       re-instanced after the animation attached (state {state:?})", tour.stage, door.ref_id));
    tour.failed = true;
}
```
and the positive half, once per stage: the state must pass through `Opening` and reach `Open { animated: true }` before the walk continues (log the sequence, so a door that stays `Closed` and a door that jumps straight to `Open { animated: false }` are told apart).

**Two related one-liners I recommend in the same change, both free:**
- `Phase::Prestream` (`demo_tour.rs:579`) already stands in front of the door for 6 s: write `OpenDoor` there (the same message `E` writes), so the **scripted, no-player tour** — the fast check the lead actually runs — covers door animation at all. Today it covers none.
- `activate_doors`'s silent fallback (`door_animation.rs:1336-1342`) increments a profiler counter (`profiler.increment("doors/unanimated_fallback", 1)`, the pattern of `transition.rs:894`); the tour prints it in its verdict. Cheap telemetry that turns "the walk test failed somewhere" into "three doors hid instead of swinging".

### 4. Per-crossing assertion at the swap frame — defect 4
**Cost: ~0** (or ~8-16 s for the walk-out variant). **Where:** `crates/engine/src/demo_tour.rs`, `Phase::WalkThrough` and `Phase::WalkThroughAfter { swap }` (`demo_tour.rs:662-814`).

```rust
// DemoTour: + destination_ref_id: Option<u32>, + far_door_checked: bool
// Phase::WalkThrough, where the door is read each frame (demo_tour.rs:683):
tour.destination_ref_id = Some(door.destination.destination_ref_id);

// Phase::WalkThroughAfter:
if let Some(ref_id) = tour.destination_ref_id
    && !tour.far_door_checked
    && tour.frame > swap + FAR_DOOR_FRAMES          // e.g. 10; the fix opens it in the swap frame
{
    tour.far_door_checked = true;
    let far = doors.iter().find(|(_, _, door, _)| door.ref_id == ref_id);
    match far {
        Some((_, _, _, state)) if door_is_open(*state) => { /* the doorway the player arrived in */ }
        Some((_, _, _, state)) => {
            tour.note(format!("FAIL stage {}: the far door {ref_id:08X} of the crossing was \
                               {state:?} in the frame the player arrived in its doorway", tour.stage));
            tour.failed = true;
        }
        None => { tour.note(format!("FAIL stage {}: the far door {ref_id:08X} never spawned", tour.stage));
                  tour.failed = true; }
    }
}
```

The condition is the fix's own guarantee, read from the same state the portal and the crossing read (`transition.rs:178 door_is_open`, already imported by the tour). Under the pre-fix tree this fires on the first crossing that lands in a doorway, names the stage and the door, and needs no picture to be looked at.

### 5. Image metrics on the reference sheets (a `--check` mode) — defect 5, plus any future exposure/water regression
**Cost: 3-15 s**, after a `--shots` run that is already made for the sheets. **Where:** `tools/research/exposure_stats.py` (add one `flat_white` column in `statistics()`, lines 43-79) plus a small `--check` entry point driven by `tools/compare_shots.py`, with the bands read from the shots file (unknown fields are ignored by the engine by design, `shots.rs:134-135`) and a python test in `tests/` for the checker itself (pattern: `tests/test_reference_shots.py`, which skips when `local/reference/` is absent).

```jsonc
// tools/reference/riverwood_shots.json, one shot
{ "name": "RW-06-river-to-the-bridge", ...,
  "metrics": { "flat_white_max": 0.15, "reference_mean_min_ratio": 0.6 } }
```
```python
# exposure_stats.py: statistics() gains
"flat_white": float(((luminance >= 0.9) & (saturation < 0.05)).mean()),
# check mode: fail (non-zero exit) with
#   "RW-06-river-to-the-bridge: flat_white 0.63 > 0.15 - water is a flat white sheet"
```

Absolute thresholds where the defect is gross (a white sheet, a black hearth) and reference-relative ones only where a reference exists — a UESP JPEG at another pose cannot support a tight per-pixel band. This is the only one of the five that needs a GPU render, but it re-uses a render the reference-sheet workflow already makes.

---

## Ranking and why

| # | Check | Defect | Cost | Where |
|---|---|---|---|---|
| 1 | re-instanced-scene unit test | 1 | <1 ms, CI | `door_animation.rs` tests |
| 2 | same-frame camera pose unit test | 2 | <1 ms + fixture, CI | `portal.rs` tests |
| 3 | swing asserted where `E` is pressed (+ scripted tour opens a door) | 1 in play | ~0 (shots: none) | `shots.rs` `DoorFacts`, `demo_tour.rs` |
| 4 | per-crossing far-door assertion | 4 | ~0 | `demo_tour.rs` |
| 5 | image metrics `--check` | 5 | 3-15 s after a render | `tools/research/exposure_stats.py` |

Defect 3 is the one gap not covered by a *new* check: the fixture unit test that encodes the invariant already landed with the fix (`world/database.rs:1347-1356`). I still recommend the data-level audit (grid vs the destination reference's own position, over the real DB, `#[ignore]`d on `MUDCRAB_CONVERTED_DIR`) because it covers ~1,200 real links instead of one fixture, and because both defect 3 and defect 4 were "the data said one thing, the engine used another" bugs that a single fixture row cannot guard.

## What I could not do, and what I am unsure about

- **No builds or engine runs** (per the brief), so every cost above is an estimate from the code, not a measurement. The one I would measure first is #5's runtime and #1/#2's fixture effort.
- **#2's fixture is the only non-trivial piece of work.** `update_portal` early-returns without a `StreamingWorld` (`portal.rs:1894-1898`) and `StreamingWorld::cells` is private (`streaming.rs:120-142`), so the test needs either a `#[cfg(test)]` constructor or the fixture DB. A cheaper and blunter alternative, if the lead prefers no fixture: a debug-only assertion system in `PostUpdate` (only under `debug_assertions` or a flag) that checks `portal_camera.Transform == door_map(...).pose(main_camera.Transform)` whenever `PortalState.open_door` is `Some`. It catches any lag or ordering regression, but it will false-fire on any system that moves the main camera after `PortalFrame` in a `debug` build — I have not audited every such system (`fly_camera`, `shots`, the tour all run in `Update`; `apply_door_crossings` is before `PortalFrame`), so I would not make it the only check.
- **#3's public-surface change** (`pub struct DoorAnimation` + `swings()`) is a small API change in a module that the `PortalPlugin` refactor is actively touching (`docs/design/portal-plugin.md`), and the shots variant needs a new `DoorFacts` field and a `DoorStep` variant — I did not check whether `DoorStep`'s callers handle a new variant exhaustively (`shots.rs:512-520`; `door_step` is public, so it has callers).
- **#4's tidier variant** — `destination_ref_id` on `DoorCrossed` — is a shared-contract change (three construction sites listed above); I sketched the version that avoids it. The lead's call, since the lead keeps contracts.
- **Thresholds** in #5 are guesses in the right units. They need one measuring pass over existing renders (`local/reference/`) to set, and they must be wide enough that a pose difference does not trip them: I would set them from the *current* builds plus the known-bad ones, and treat them as gross-regression tripwires, not fidelity measures.
- Nothing was written to disk: the report is the deliverable and I must not edit tracked files. If the lead wants it in the repo, `docs/research/checks-for-user-found-defects.md` is the natural place, next to `docs/research/portal-door-alignment.md`.

