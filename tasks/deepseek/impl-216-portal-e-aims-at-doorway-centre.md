# impl-216 - E measures a door by the middle of its doorway, not the foot of it

Owned files: crates/engine/src/player.rs (target_door and its callers' door positions only),crates/engine/src/demo_tour.rs (only if the tour's standoff check must follow)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Backlog item (docs/roadmap/backlog-engine-and-portal.md, "E measures a door's aim at its placement"): `player::target_door` (impl-182) measures each door's aim at its placement, the foot of the doorway 120 units below the eye, so two stacked load doors (the Riverwood Trader's `0001341F` and upper `00070E69`) are only ~3 degrees apart and a player aiming slightly high takes the wrong one. Measure the aim at the doorway's centre instead: the placement raised to the doorway's middle (use the door's `DoorAnchor` threshold/box data if present, else a constant half doorway height, e.g. 110). Keep the range test on the placement as now.

## Checks
- Unit tests: the Trader case from impl-182's test, now with a much wider margin (report the new angle difference), plus a test that aiming 3 degrees above the Trader's leaf still picks the front door.
- `cargo build --profile quick --bin engine`; the short tour and the full Riverwood tour (`--demo-tour local/t216-full`, 8 doors) pass.
- Read only `local/t216*` under `local/`.

## Report
Changes, the new margins, test names, tours' last lines.
