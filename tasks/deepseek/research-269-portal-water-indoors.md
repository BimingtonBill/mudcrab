# research-269 - Why the water reflection runs inside a closed house

Track: portal. Read-only; change nothing.

## Goal
Find out why the portal bench records `water_active` = 1.00 in its `closed-out` state (standing inside a Riverwood house facing its closed door, portal inactive), and say whether the water reflection camera is doing work there that nobody sees.

## Context
- Bench run: `local/bench/` is gone for that run; the numbers were, per door and state (portal_active, water_active): Sven's House 0x0001CBAF closed-out (0, 1), open-out-view (1, 1); Alvor's 0x000133FB and the Sleeping Giant 0x00013419 the same; Uthgerd's House in Whiterun 0x000918E6 closed-out (0, 0), open-out-view (1, 0). Riverwood's exterior has a river; Whiterun's city has none near that door.
- How `water_active` is measured: `crates/engine/src/portal_bench.rs` (search `water_active`).
- The water reflection camera and what switches it on: search `crates/engine/src` for `reflection`, `WaterSurface`, `water_active`, `ReflectionCamera` and the reflection plugin; the portal's own layers are in `crates/engine/src/portal.rs` (`is_in_active_space`, `DESTINATION_LAYER`).
- Pre-streamed door destinations (the exterior behind a house door is loaded while you stand inside): `crates/engine/src/transition.rs` (`plan_door_prestream`).

## Done when
You can name the exact condition (file:line) that turns the reflection on in `closed-out`, say whether its image is visible anywhere in that state, and propose the smallest correct rule (for example: only water in the active space, or in the portal's destination while the portal is drawn), with the file:line to change and what a test would check.

## Report
1. The condition that switches the reflection on, file:line, and why it is true inside a closed house.
2. Is anything drawn from it in closed-out? (yes/no with evidence)
3. The proposed rule, where it goes, the test, and any risk (for example a portal that opens and needs the reflection on its first frame).
