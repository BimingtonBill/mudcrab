# Goal
Track: Phase 2 (input for an upstream port). Inventory the portal branch's door-destination
pre-streaming ("loading side"), so the reusable part can be ported to upstream main for issue #112:
as the player nears a load door, load the destination cell in the background with its rendering
held, then cross with a fade. Portal rendering (seeing through the door) is NOT ported.

# Context
Portal checkout: `C:/Modding/wkj-portal-merge` (branch portal-merge-main, already merged with main).
Read only. Main checkout for comparison: `C:/Modding/wkj-door-preload` (origin/main).
Likely files: `crates/engine/src/streaming.rs` (`PrestreamCells`, `wanted_cells`,
`cell_within_unload_radius`, `request_cell`, `pinned`, `ActiveCell`, `CellDetail`), `portal.rs`
(`isolate_cells`, `CellRole`, which cells are drawn), `doors.rs` (`LoadDoor`, `DoorState`,
`DoorDestination`), `transition.rs` (crossing, arrival, `ActiveCell` switch), `player.rs`
(door activation, walk-through), `door_animation.rs`, the database reader for `door_links`.

# Questions (file:line for each)
1. The trigger: what decides that a door's destination gets pre-streamed (distance, facing, open
   state, nearest-N)? Constants and their values.
2. The load: how the destination cell is requested, kept resident, and unloaded when the player
   walks away (pins, prestream set, unload radius).
3. Held rendering: how a pre-streamed destination is kept invisible (RenderLayers / CellRole /
   Visibility) and colliders disabled; which parts are portal-specific (portal camera, quad).
4. The crossing: what switches the active space (exterior to interior and back), places the player
   at the arrival point, and the render-origin handling. Is there any fade?
5. For each piece in 1-4: does it depend on portal-only code (portal camera, door layer, tiers
   `CellDetail::Terrain`, `--walk` player), and what main already has instead. Mark each piece
   PORTABLE / NEEDS ADAPTING / PORTAL-ONLY, with a rough line count.

# Scope
Read only; change nothing. Do not read `local/`.

# Report
Numbered answers, a table of pieces (name, file:line, verdict, lines, depends on), under 150 lines.
