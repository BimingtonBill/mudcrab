# impl-235 - The doorway floor anchor is re-probed until it is stable (flake A)

Owned files: crates/engine/src/portal.rs (anchor_on_measured_floors and the DoorwayFloorsMeasured / anchor components only),crates/engine/src/doors.rs (only if the database anchor needs its own component)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user affirmed this queue item on 2026-09-26. The diagnosis is research-229, flake A: read `local/research/research-229-portal-flakes.md`; runs are in `local/t229/`.

## The bug
`anchor_on_measured_floors` (portal.rs, around lines 1324-1432) keeps the **first** frame in which both floor probes hit anything, marks the door `DoorwayFloorsMeasured` and never probes again. A probe only hits meshes whose bounds and visibility were computed the frame before. So when a floor mesh spawns late, the probe falls through to the terrain below and stays wrong.
- Honningbrew's side door `0007D7A8`, capture cap04: its platform `WRStablePlat*` belongs to another cell, 38486. The anchor reads -69.5 on the planks and -80.8 on the terrain. It went wrong in 3 of 6 cold runs.
- Sven's door: 0.0 on the porch against -22.0 on the ground, wrong in 2 of 6.

## The fix (research-229's proposal)
- Keep the database anchor in its own component, so a re-probe starts from it and not from a previous probe.
- Keep probing while the portal draws through the door. Mark it measured only once the result has held within 0.5 units for about 30 frames, and nothing near the doorway is still spawning. For example: no scene of a reference within some radius still loading, or no new mesh entity near the doorway in those frames. Pick a cheap, testable rule.
- A later mesh near the doorway (a scene respawn) clears the mark, so it re-probes.
- Keep the anchoring itself as it is. impl-227 (now merged) relies on it: the slab and the tie bias.

## Checks
- Unit tests for the stability rule: a late hit changes the anchor; a held result is marked; a respawn re-arms it.
- Reproduce cold, as research-229 did. It lists its exact commands in the report; the shots harness opens a door straight after the teleport.
  - cap04: 6 cold runs, all -69.5.
  - Sven: 6 cold runs, all 0.0.
  - Say how many runs and the values.
- The short tour passes (`--demo riverwood --demo-tour <dir> --tour-doors 2`).

## Build and run rules
- Build: `cargo build --profile quick --bin engine` with `CARGO_TARGET_DIR=D:/Modding-overflow/portal-quick-target` and `CARGO_BUILD_JOBS=6`.
  - The target is shared with other portal coders, so cargo's lock runs one build at a time.
  - Straight after each build, copy `engine.exe` to `local/t235/engine.exe` and run the copy.
- Tests and clippy use `--target-dir D:/Modding-overflow/shared-check-target`.
- Assets: `C:\Modding\SkyrimConverted`.
- No timing or benchmark runs.
- Another coder (impl-234) edits portal.rs's door mirror and leaf functions. Stay out of those.

## Commits
Subjects start `[claude] impl-235:`; trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No amend, rebase or force-push.

## Report
- The rule;
- test names;
- the cold-run table, before and after;
- the tour's last lines;
- the commit hashes.
