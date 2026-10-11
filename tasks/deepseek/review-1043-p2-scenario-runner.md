# Goal
Review the scenario runner v1 on local branch `phase2/scenarios` (Mudcrab, Rust/Bevy 0.19): scripted scenario files that drive the REAL player controller (issue #193's action presets) and the #184 performance routes expressed as scenarios.

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-scenarios. Review `git diff c2f4f130..HEAD` (38d1e3f5 engine, e64ad80d scripts, 4f6d6e80 docs).
- The contract: docs/specs/engine/scenarios.md (format v1, written first; the implementation added a "Clarifications from the implementation" section; check those 13 items do not change the format or weaken it).
- Key files: crates/engine/src/scenario/{file.rs, runner.rs, report.rs, mod.rs, glue.rs}, physics.rs (`change_move_mode` shared with the V key; run condition `player_input_is_free` gating look/walk/noclip/toggle systems), door_crossing.rs (crosshair request), metrics.rs, app.rs, config.rs, main.rs (exit code), shots.rs, scripts/scenarios/*.json, scripts/phase2-baseline.ps1.
- Rules: no teleport except declared setup placement; movement only through WalkIntent/LookIntent into the real controller; waits are bounded conditions that fail; intent released on every exit path; report written on every exit; unsupported actions (record, console) refused at load before a window opens; exit codes 0/1/2/3; nothing scenario-specific in engine logic.

# Scope
Read only. Change nothing.

# Done when
You have checked, with file:line: (1) every action's semantics against the spec, including move_to stuck detection, activate (aim at the door's bounds centre or fallback box, then the same activation path as the E key, completion when the destination is ready), wait timeouts, checkpoint assertions; (2) input ownership: can keyboard/mouse input leak into a running scenario, and is intent cleared on success, failure, timeout, Escape, window close, panic/Drop; (3) the report: written on every exit, sha256 of file bytes, asset/build identity, artifacts kept on failure; (4) refusals at load (unknown fields, higher version, place in actions, record/console unsupported, bad FormIDs, interior starts); (5) the frame-time measurement uses the same ledger/owner code as the route runner and the whole_run window covers the scenario's actions only; (6) the existing --route, --shots, acceptance capture and interactive controls are unchanged in behaviour; (7) tests assert real behaviour.

# Foundations check (answer explicitly)
What shortcut does this take, and what would break for a mod (other door data, a load-order change, a modded MOVT speed) or another worldspace (interior start refused; terrain-only ground probe)? Anything hard-coded that should be data?

# Report
Verdict (accept / fix needed), findings as path:line - issue - fix ordered by severity, what you verified, what you could not verify.
