# impl-206 - No gap under the doorway: the quad reaches the threshold the camera is anchored on

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's capture `local/captures/2026-09-25_10-27-59/02.png` ("misaligned", Riverwood Gerdur's House; pose and state in `02.json`, re-render with that folder's `shots.json`) shows a flat teal band under the open doorway, between the exterior threshold board and the interior stone floor.

research-205 (lead-reviewed) found the cause: the portal camera's clear colour showing through a real gap.
- The doorway quad's vertical extent comes from the door model's raw box (`measured_doorway_box`, `doorway_in_frame`, `portal_quad_extents`, `doorway_quad_transform`, portal.rs ~1006-1135, called ~2842 with no anchor).
- Since impl-199 the camera and pivot are placed at the `DoorAnchor`'s **threshold** height (`DoorwayPlacement::threshold`, doors.rs ~263; `source_doorway_centre`, transition.rs ~571), which is raised to the floor when the floor stands above the box bottom.
- When the two disagree, the interior image sits where the quad does not reach (or the reverse).

## What to build
1. Make the quad and the camera agree on the doorway's bottom. Pass the anchor into the quad sizing and put the quad's bottom edge at the anchor's threshold height (with its current margin rule) when an anchor exists; keep today's box rule for unanchored doors.
2. Check the pixels below the destination floor can never show clear colour at the threshold. If a small overlap is needed, say how much and why.
3. Change nothing else about the quad's sides or top (research-205's other findings are separate tasks).

## Checks
- Unit tests: an anchored door whose threshold is raised above its box bottom gets a quad whose bottom sits at the threshold; an unanchored door keeps today's quad.
- `cargo build --profile quick --bin engine`; re-render the user's pose 02 (`--shots local/captures/2026-09-25_10-27-59/shots.json`, with the door open: see shots.rs for opening a door in a shot) before and after, and the Sven's House short tour (must pass). Show the before/after crops of the threshold.
- Read only `local/t206*` and that capture folder under `local/`.

## Report
Changes, test names, before/after crops, the tour's last line.
