# Goal
Review the #184 doors-scene commits on local branch `phase2/perf-doors` (Mudcrab, Rust/Bevy 0.19): route door steps that cross load doors through PR #128's crossing code, and per-crossing frame-time measurement, for the 160 fps goal (6.25 ms frames, 99.9% within, none over 12.5 ms).

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-doors. Review `git diff fe6268ee..HEAD` minus the cherry-picked world-ready fix (4b3a4dbc, dadc0ad0, already reviewed). fe6268ee merged #128 (386267dc) into phase2/perf-routes 9fdecea7.
- Key files: crates/engine/src/route.rs (door steps, RoutePlan::resolve_doors, lookup_door, runner), door_crossing.rs (DoorActivationRequest handled in drive_door_crossing's idle branch like the E key), new door_measure.rs (per-crossing windows: crossing frames, settle window after arrival, request-to-ready time), metrics.rs, config.rs (--door-settle-seconds), app.rs, scripts/phase2-baseline.ps1 (doors / check-doors scenarios), scripts/routes/doors.json and check-doors.json, docs/roadmap/02-profiling.md.
- Rules the code must follow: no shortcuts (the crossing must be the player's own path: no teleport around it, no hidden waits, no slowing; nothing hidden from the measurement); the engine holds no route-specific knowledge (routes are data); failures must be loud (timeouts, refused data), never silent; owners/numbers never guessed.

# Scope
Read only. Change nothing.

# Done when
You have traced: how a door step becomes a crossing (file:line), that it cannot bypass any part of #128's crossing (fade, switch, landing, view-blocked test); how each crossing window and the settle window are bounded and attributed (both directions separately); how request-to-ready time is measured (and that it uses the world-ready predicate); and the script's per-crossing table. You have checked the tests for real assertions.

# Foundations check (answer explicitly)
What shortcut does this take, and what would break for a mod (different door data, a mod moving a door) or another worldspace (a door into another worldspace, an interior with no ground)? Anything hard-coded that should be data?

# Report
Verdict (accept / fix needed), findings as path:line - issue - fix, what you verified, what you could not verify.
