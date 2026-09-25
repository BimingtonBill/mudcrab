# impl-181 - The doorway keeps showing the far side while its door swings (opening and closing)

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user, in play (2026-09-25): "when closing a door, the portal stops rendering before it can fully close." Since impl-176 `E` closes a door, and since impl-179 a door closes itself when the player walks away; in both cases the leaf plays its `Close` clip (~0.6 s), but the doorway goes empty the frame the close starts.

## Why

`select_portal_door` (`crates/engine/src/portal.rs` ~872-925) skips a door when `!door_is_open(state)` (~902). `door_is_open` (`transition.rs` ~178) is `DoorState::is_open()`, which is false for `Closing` (and for `Opening`). So the portal camera and the doorway quad switch off at the start of the swing, and the swinging leaf is seen against the source side's empty doorway. The same holds while a door is `Opening`: the far side appears only once the swing has finished.

## What to build

- In `portal.rs`, let the portal render through a door whose state is `Opening`, `Open { .. }` or `Closing` (the leaf is drawn swinging in front of the doorway image, which is what Skyrim shows), and stop only once it is `Closed`. Use a small helper in `portal.rs` (e.g. `portal_shows_through(state)`) rather than changing `door_is_open`, which the crossing and the walk-through trigger rely on and which must stay "fully open" (`transition.rs` and `player.rs` are not yours).
- Everything else that `select_portal_door` checks stays: destination resident, the camera in front of the door's plane, nearest wins.
- Check what else in `portal.rs` keys off `door_is_open` or `is_open()` for the same door (the far-door hiding in `drawn_door_visibility`, the mirror placement `place_door_mirror`, `update_portal`'s `open_door`) and make it consistent, so the far door is hidden and the mirror leaf swings for the whole time the doorway is drawn. Report each place you changed or deliberately left.
- The far side is already held loaded while a door is `Closing` (impl-179, `plan_door_prestream`), so the image has something to show until the leaf shuts.

## Checks

- Unit tests next to the existing `portal.rs` tests: a door `Opening`, `Open` and `Closing` is selected; `Closed` is not; the other conditions still apply.
- `cargo build --profile quick --bin engine`, then the short tour (the user's standing check):
  `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t181 --tour-doors 2` must end `tour PASSED after 2 crossings (short tour)`. Read only `local/t181*` under `local/`.

## Report

Files changed and why; every `door_is_open` / `is_open` use in `portal.rs` and what you did with it; test names; the tour's last line.
