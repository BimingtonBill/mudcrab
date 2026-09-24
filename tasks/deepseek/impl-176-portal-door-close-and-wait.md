# impl-176 - Doors can be closed with E, and a door opens only once its far side is loaded

Owned files: crates/engine/src/player.rs,crates/engine/src/door_animation.rs,crates/engine/src/transition.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's demo notes of 2026-09-25 (`docs/roadmap/backlog-engine-and-portal.md`, Portal plugin > Correctness, notes 4 and 5): "doors are openable but not closable, should be both" and "doors should always wait for the world they transition into to be fully loaded before opening". research-174 traced both (`local/agents/research-174.out` is its report; the parts you need are copied below, and the lead checked the citations).

## 1. Closing with E

**Now:** the door state machine already closes: `activate_doors` in `door_animation.rs` (~1346-1375) turns an `OpenDoor` message for an `Open` door into `Closing`, starting the `Close` clip at the pose the `Open` clip reached, and `advance_door_states` brings it to `Closed` (~1582-1588). But `player_door` in `player.rs` (~946-957) filters its target to doors that are **not open** (`closed_target = target.filter(... !door_is_open(state))`), so `E` never sends `OpenDoor` for an open door.

**Build:**
- In `player_door`, target *settled* doors: `Closed` and `Open { .. }`, but not `Opening`/`Closing` (which `activate_doors` ignores anyway). `E` writes `OpenDoor` for either; the state machine decides open vs close.
- The prompt says `E  Close  <label>` for an open door and `E  Open  <label>` for a closed one (keep today's wording otherwise).
- Refuse the close (no prompt, no message) while the player stands inside the doorway, where the leaf turns solid again the moment the state leaves `Open` (`mesh_is_out_of_the_way`, `doors.rs` ~509-526; the walk-through trigger in `player.rs` ~1163). Use whatever doorway volume or distance test the walk-through trigger already uses; say which in the report.
- A door whose model has an `Open` clip but no `Close` clip: `activate_doors` would replay Open (~1369-1374). Offer no Close prompt for it.

## 2. Open only once the far side is loaded

**Now:** `E` -> `OpenDoor` is gated on nothing but the door's own state, and `activate_doors` starts the swing the same frame. Readiness exists but is used elsewhere: `transition::destination_is_ready` (~712-718) -> `destination_is_resident` (~685-706), used by the portal's door choice and by the crossing (which holds with `CrossingHeld`). It checks the **landing cell only**, at `CellDetail::Full` with a root (`StreamingWorld::is_resident`, `streaming.rs` ~129-143), not the other keys `destination_keys` pre-streams (`transition.rs` ~624-648). So a door opened early swings onto a doorway with nothing behind it.

**Build:**
- In `transition.rs`, a stricter `pub(crate) fn destination_is_loaded(...)` beside `destination_is_ready`: every key of `destination_keys` for that door resident at `Full`. Leave the crossing's own check as it is (the comment at ~676-684 explains why it stays landing-cell-only).
- In `door_animation.rs`, hold an `OpenDoor` for a **closed load door** (one with a `LoadDoor`/destination; a door without one opens at once as now) whose destination is not loaded yet: keep it in a small pending resource (a sibling of `ArrivalOpenings`, ~1399-1420) and start the swing the first frame `destination_is_loaded` is true. Drop the pending request if the door despawns, or if the player presses E on it again (a cancel). A close request is never held.
- The prompt shows `E  Open  <label> (loading)` while the far side is not loaded (`player.rs` needs the same predicate; read `StreamingWorld` as an `Option<Res<...>>` so apps without streaming still work).
- `streaming.rs` is not yours: use only what it already exposes (`StreamingWorld::is_resident` and friends). If you need something it doesn't have, stop and say so in the report.

## Checks

- Unit tests in the style of the existing `door_animation.rs` / `player.rs` / `transition.rs` tests (small Bevy `App`s):
  - E on an open door starts `Closing`; E on a door that is `Opening` or `Closing` does nothing; no close while standing in the doorway.
  - an `OpenDoor` for a load door whose destination is not loaded stays pending and starts the swing the frame it becomes loaded; a door without a destination opens at once; a despawned door's pending request is dropped.
  - `destination_is_loaded` is false when any pre-streamed key is missing and true when all are resident.
- `cargo build --profile quick --bin engine`, then a short door tour (the user's standing check: 1-2 doors):
  `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t176 --tour-doors 2`
  It must end `tour PASSED after 2 crossings (short tour)`. Read only `local/t176` under `local/`.

## Report

Files changed and why; how "standing in the doorway" is decided; the new predicate and the pending resource; test names; the short tour's last line.
