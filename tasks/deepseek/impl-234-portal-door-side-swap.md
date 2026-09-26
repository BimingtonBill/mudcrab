# impl-234 - The door leaf stays on the same side from inside and outside

Owned files: crates/engine/src/door_animation.rs (the far-door drive only: open_arrival_doors and what it calls; not twin_model_path, which impl-231 owns),crates/engine/src/portal.rs (door mirror and leaf visibility only: place_door_mirror, show_load_door_leaves, drawn_door_visibility),crates/engine/src/transition.rs (read-only unless needed)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user affirmed this queue item on 2026-09-26. In an earlier demo, the user found that the door leaf is on one side of the doorway seen from inside and on the other side seen from outside.

## The diagnosis (research-174, note 1; read `local/agents/research-174.out` lines 9-25)
- Seen from the near side through the window, the leaf in the aperture is a half-turned copy of the **near** door (the mirror root, `R(-r_far + 180)`), not the far door's own leaf.
- Seen from the far side, it is the far reference's own model, hinged by its own row.
- A half-turn about the vertical moves a hinged leaf to the other half of the opening: the swap.

## The fix research-174 proposes
- While the portal renders through a door (`PortalState::open_door`), put its **far door** in the same `Open` clip fraction. The machinery exists for the crossing: `open_arrival_doors` / `open_arrival_door` and `reached_fraction` in door_animation.rs.
- Draw the far entity's own leaf through the window instead of the mirror, so the leaf seen from each side is the same entity.
- Keep the mirror only as the fallback for a near door whose far reference is not resident (`destination_door` is set only for anchored doors).
- When the near door closes (including auto-close, impl-179), close the far door with it.

Check the fix against the depth composite, now the default (impl-227: filler on layer 31, slab measured per doorway): a far leaf standing behind the destination's doorway plane must still draw correctly.

## Checks
- Unit tests for the drive: the far door follows the near door's state and fraction, and closes with it.
- Render Sven's House and Gerdur's House with the door open, from outside through the window and from inside at the far door. The leaf must be hinged on the same jamb in world terms. Use `tools/reference/doorway_shots.json`, and add an inside pose if needed.
  - Note: a shot that opens a door as the FIRST shot in its file photographs it closed (a shots.rs bug), so put a warm-up shot at the same pose first.
- The short tour passes (`--demo riverwood --demo-tour <dir> --tour-doors 2`).

## Build and run rules
- Build: `cargo build --profile quick --bin engine` with `CARGO_TARGET_DIR=D:/Modding-overflow/portal-quick-target` and `CARGO_BUILD_JOBS=6`.
  - The target is shared with other portal coders, so cargo's lock runs one build at a time.
  - Straight after each build, copy `engine.exe` to `local/t234/engine.exe` and run the copy.
- Tests and clippy use `--target-dir D:/Modding-overflow/shared-check-target`.
- Assets: `C:\Modding\SkyrimConverted`.
- No timing or benchmark runs.

## Commits
Subjects start `[claude] impl-234:`; trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No amend, rebase or force-push.

## Report
- The change;
- test names;
- inside/outside crops for both houses;
- the tour's last lines;
- the commit hashes.
