# impl-199 - The doorway view goes through the door the player looks at, and anchors non-farmhouse doorways

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Starts after impl-188/189 merge (same file). Read research-193's report first: `local/research/portal-capture-faults-2026-09-25.md` (faults 1 and 2, with numbers). User captures: `local/captures/2026-09-25_03-57-58/` 01, 06, 14.

1. **Fault 1 (Riverwood Trader, capture 01):** `select_portal_door` takes the nearest eligible open door, so with the Trader's upper door `00070E69` (179.8 u) nearer than the front door `0001341F` (220.5 u) the portal draws through the wrong one; the front doorway gets no quad and shows the door's own plug. Pick the doorway the player sees: among eligible doors, the one whose doorway is on screen and best aimed at (reuse impl-188's `doorway_screen_rect`; the largest visible rect, or the aim rule `player::target_door` uses since impl-182), nearest only as the tie-break. Keep the far-door hiding and the mirror following the same pick.
2. **Fault 2 (Chillfurrow 06, Cracked Tusk 14):** the doorway anchor is refused at Chillfurrow (arrival-height term -44.5 u over `ANCHOR_HEIGHT_CAP` 40) and falls back to the XTEL map, whose clip plane stands 99 u inside the room; at Cracked Tusk the doorway-centre height is wrong (12.6 / 158.3 u where Sven's FarmhouseLDoor01 gives 88). Derive each door model's doorway centre height from the model's own bounds (or the leaf's) instead of the farmhouse-shaped assumption, and revisit the cap with the corrected centre. Check Sven's House, the Trader, Chillfurrow, Cracked Tusk and one Bleak Falls door keep or gain an anchor.

## Checks
- Unit tests with the report's numbers for both faults.
- `cargo build --profile quick --bin engine`; short tour passes; the full Riverwood tour (`--demo-tour local/t199-full`, 8 doors) passes; log the picked door and anchor tier at the Trader and show the doorway image (tour door shots).
- Read only `local/t199*`, the report and the capture folder under `local/`.

## Report
Changes, test names, anchor tiers per door before/after, tours' last lines.
