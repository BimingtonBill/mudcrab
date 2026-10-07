# impl-188 - The doorway view costs nothing when it cannot be seen, and its sun has cheaper shadows

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user (2026-09-25 demo notes): "performance isn't great: portals should only render at the size of the portal and only render what the player sees through them." This is the first half (the second, rendering at the doorway's size, is impl-189). Read `local/agents/research-175.out` section 3 first: it maps today's costs to lines of `portal.rs` and the Bevy 0.19 APIs.

## What to build (portal.rs only)

1. **Skip the portal render when the doorway is off screen** (research-175 item 2). In `update_portal`, project the doorway quad's four corners with the main camera (its `Transform` and `Projection` are already there). If every corner is behind the eye, or the corners' screen rectangle does not overlap the window, take the existing "no portal" path (camera inactive, quad hidden) for that frame. Put the test in a pure function (`doorway_screen_rect(corners, view, projection, window size) -> Option<Rect>`) so it can be unit-tested; impl-189 will reuse the rectangle to size the render.
2. **Give the portal sun its own, cheaper shadow cascades** instead of copying the engine sun's four 2048² cascades over 17,378 units (`place_destination_sun`; research-175 item 1, second half). Keep the doorway's sun shadows (impl-177 made them work, and the user wants them): 2 cascades, `maximum_distance` about 3000 units (the far side seen through a doorway is mostly close), `first_cascade_far_bound` about 600. Put the numbers in named constants with a comment saying why.
3. Keep everything else `select_portal_door` checks, and the impl-181 rule that the doorway shows while the door swings.

## Checks

- Unit tests for `doorway_screen_rect`: in front and on screen, behind the eye, off to the side, partly on screen (clamped), the camera inside the doorway plane.
- `cargo build --profile quick --bin engine`; the short tour `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour local/t188 --tour-doors 2` ends `tour PASSED after 2 crossings (short tour)`.
- A rough cost check (not a timing run; say so): the average frame time the tour logs, or a `--shots` pose looking away from an open door, before and after. Read only `local/t188*` and `local/agents/research-175.out` under `local/`.

## Report

What changed and why; test names; the tour's last line; the before/after frame times with how they were measured.
