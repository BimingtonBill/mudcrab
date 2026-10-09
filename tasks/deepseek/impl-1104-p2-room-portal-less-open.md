# Goal
Fix #188 room culling's vanishing cave hall (queue row B5) with the rule from design-1100: an implicit box-overlap edge is walked open whenever either room of the pair has no portal. Only `PortalMarker` doorways narrow. Then prove it on the real data with the image harness. Track: Phase 2, branch `phase2/room-culling`.

# Context
- Design (the contract): local/research/engine-foundations/design-1100-room-connection-rule.md. Read §0, §4 and §5 closely. §5 gives the code change (~10 lines in `RoomGraph::reached_rooms`, crates/engine/src/room_culling.rs, the overlap branch ~562-571, plus the module doc ~46-66) and the tests.
- The branch is based on main, not portal; keep it so.
- The image harness:
  - Portal Dev's `test/room-culling-image` branch, worktree D:/Mudcrab/worktrees/p2-roomcull-image. Read its README or scripts for the invocation; it is read-only for you.
  - Previous results: D:/Mudcrab/scratch/portal-roomcull-image (summary.md, poses.json, poses_floor.json).
  - Assets: C:/Modding/Mudcrab/SkyrimConverted-portal-rooms.

# What to build
1. The §5 rule in `reached_rooms`, computed locally from `doorways` as the design prefers, and the module doc updated: the switch is the presence of a `PortalMarker`, not a distance.
2. Tests from §5:
   (1) the failing-shape fixture, which must fail on today's code and pass after; plus the same graph with a portal between hall and corridor, where narrowing still culls a backdrop behind a wall;
   (2) a pair where both rooms have portals still narrows.
   Keep all existing tests.
3. **The real-data check.** Build the engine (quick profile, gated) and run the harness with culling on and off over the 25 poses plus the 3 floor poses.
   - Output to D:/Mudcrab/scratch/p2-roomcull-1104/ (create it with an OWNER.txt: owner Phase 2 Dev, class A, keep-until date:2026-10-23, links no).
   - Required: 0 differing pixels on every pose.
   - Also record rooms-not-reached or `culled_last_frame` before and after for the same poses, so the culling given back is visible.
   - **Before any engine run, check `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`. If a hold is listed (a timing window may be running), wait, and recheck every few minutes. Never run the engine during a hold.**
   - Look at the BleakFallsBarrow01-room1-c pair yourself and say what it shows.

# Scope
Owned files: crates/engine/src/room_culling.rs
- The harness scripts are read-only. If the harness needs a change to run, stop and say what.
- No converter change, no new dependencies, no git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Also `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick --bin engine` for the harness.
- Set BUILD_GATE_OWNER=p2. Run `df -h /c /d` first; stop if either has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
The rule comes from data (`PortalMarker` presence), not from a tolerance. It only adds rooms to the reached set, so it can cost culling but never vanish a wall. It works for mods and any interior.

# Report
What changed (file:line); test names; the commands and their Summary lines; the per-pose pixel table; culling before and after; what the BleakFallsBarrow pair shows.
