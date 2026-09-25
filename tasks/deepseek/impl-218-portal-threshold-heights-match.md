# impl-218 - Both doorways' thresholds match: no step, no black strip at Gerdur's House

Owned files: crates/engine/src/doors.rs,crates/engine/src/portal.rs (threshold and quad code only),crates/engine/src/transition.rs (doorway centre functions only),crates/engine/src/world/database.rs (read queries only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's capture `local/captures/2026-09-25_12-31-03/01.png` ("misaligned"; pose and state in `01.json`, door `0x00013423` Riverwood Gerdur's House, open) was taken on a build with impl-199 (thresholds) and impl-206 (the quad stands on the anchored threshold).
- The teal gap is gone.
- But the interior floor now sits visibly **above** the exterior sill: a step of maybe 15-25 units.
- There is a **black strip** under the interior floor's front edge, between the floor and the sill. It is probably the door model's backing card, or the space below the destination floor.

## History to read

- impl-199's commit (`git log --grep impl-199`): the threshold anchor.
  - `DoorwayPlacement::threshold` in doors.rs is the box's lowest point, or the ground in front where the door is set into the ground, up to 64 units higher.
  - It is gated on the box height and the landing window.
- impl-206 (`git log --grep impl-206`): `anchored_threshold` and `quad_on_threshold` in portal.rs.
- research-205's diagnosis of the old gap is in the team log (2026-09-25).

## What to do

1. **Diagnose with numbers first.** For this door pair (source `0x00013423`, and its destination door from `door_links`), give:
   - each side's box bottom;
   - the floor or ground height in front of each doorway (how does the threshold rule find "the ground in front" on each side? Is it measured on the source only, with the destination taken from its box bottom?);
   - the chosen threshold heights;
   - the interior floor's real height near its door, from the destination cell's references or collision meshes.
   Explain the step and the black strip with those numbers.
2. **Fix it so the two thresholds are the same physical floor line on both sides.** Both sides must use the walkable floor at the doorway, not a model box.
   - Likely approach: take each side's floor height in front of its doorway from the same source, the floor meshes near the doorway, as the player's ground probe does (player.rs's ray cast), once the cells are resident.
   - Then re-anchor. The quad's bottom follows (impl-206).
   - Hide or clip the source door's backing card below and behind the threshold, if that is the black strip.
3. Keep Sven's House pixel-aligned (`docs/research/portal-frames.md` section 1 method), and keep Chillfurrow and Cracked Tusk anchored.

## Checks

- Unit tests with this door pair's numbers.
- `cargo build --profile quick --bin engine`. Re-render the capture's pose (the folder's `shots.json`, door open, see shots.rs for opening a door in a shot) before and after, plus Sven's House, the Trader and Chillfurrow. Show crops.
- The short tour and the full Riverwood tour pass.
- Under `local/`, read only `local/t218*` and the capture folders.

## Report

The numbers; the cause of the step and of the black strip; the fix; test names; before/after crops; tours' last lines.
