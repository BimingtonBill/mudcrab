# Goal
Finish the #188 room-culling start-room rule (queue B5): when the camera stands in a room that has no portal of the cell, draw the whole cell. The rule is already written; five existing tests now fail because their fixtures start the camera in a portal-less room. Adapt them so each still tests what it was written for, and add tests for the new rule. Track: Phase 2.

# Context
- Worktree D:/Mudcrab/worktrees/p2-roomcull, branch `phase2/room-culling`, head 8ee1f983 plus an uncommitted change by the lead in crates/engine/src/room_culling.rs: `reached_rooms` returns `None` (draw everything) when any start room has no `PortalMarker` (search "analysis-1112"). Keep that change exactly; do not weaken it.
- Why: local/agents/runs/analysis-1112-p2-roomcull-floor-residual.1/report.md (in C:/Modding/Mudcrab/mudcrab). Also design-1100 §5 for the earlier overlap rule, which stays.
- The failing tests: `a_pair_of_rooms_that_both_have_portals_still_narrows`, `a_link_is_an_open_edge`, `a_portal_less_hall_reaches_the_backdrop_behind_its_corridor`, `overlapping_rooms_with_no_portal_are_one_space`, `the_membership_key_is_the_reference_id_the_loader_delivers`. A helper `park_portals` exists (it gives rooms a portal to a room parked behind the camera).

# What to do
1. For each failing test, decide what it is testing:
   - narrowing, a link edge, an overlap edge, or the membership key: give its start room a portal (`park_portals` or a similar fixture change), so the walk still runs and the original property is still asserted;
   - a portal-less start: change its assertion to the new rule, `None`.
   Say per test which one you chose and why. Do not delete an assertion; never weaken one to pass.
2. New tests:
   (a) a camera in a portal-less room gets `None`, so the whole cell is drawn;
   (b) a camera in two boxes, one with a portal and one without, gets `None`;
   (c) a camera in a portal room still walks and culls (the old behaviour);
   (d) the BleakFallsBarrow01 floor shape from analysis-1112: a portal-less hall start whose only route to the hidden rooms goes through an off-screen portal now draws the cell.
3. Update the module doc's culling section with one paragraph on the new rule. The switch is a data fact (`PortalMarker` presence); cite analysis-1112.

# Scope
Owned files: crates/engine/src/room_culling.rs
- No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2 and CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-roomcull. Check `df -h /d`; stop if D: has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
No test weakened. The rule is data-driven, can never vanish a wall, and works for any mod's interiors.

# Report
Per test what you changed and why; the new test names; the commands and their Summary lines.
