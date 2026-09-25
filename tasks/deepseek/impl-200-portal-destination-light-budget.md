# impl-200 - The room seen through a doorway gets its own lights

Owned files: crates/engine/src/lights.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. research-193 fault 3 (`local/research/portal-capture-faults-2026-09-25.md`): Bleak Falls Barrow 02 seen through the doorway from 01 (user captures `local/captures/2026-09-25_03-57-58/10.png`, `11.png`) is lit differently from the same room once inside, because `budget_lights` ranks every point light in one shared 64-light budget by the **main** camera's distance, so the destination's lights (render layer 2) lose to the active space's. Ambient, fog, backdrop and sun already follow the destination.

## What to build
Split the budget: the active space's lights ranked by the main camera, the destination's lights (layer 2, only while a portal is showing) ranked by the portal camera's position, each with its own share (e.g. 48 + 16, constants with a comment why; the destination only needs the lights near its doorway). Re-choose on the same distance rule the budget uses now. No change when no portal is showing.

## Checks
- Unit tests: split, ranking per camera, no-portal case unchanged.
- `cargo build --profile quick --bin engine`; short tour passes; render a before/after pair of the Bleak Falls doorway view (a `--demo-tour` isn't on that route; use the capture poses `--shots local/captures/2026-09-25_03-57-58/shots.json` for 10 and 11 if a door is open in that run, otherwise add the smallest hidden flag you need, and say what you did).
- Read only `local/t200*`, the report and the capture folder under `local/`.

## Report
Changes, test names, the before/after images, the tour's last line.
