# Goal

Explain, with file:line evidence, why four door behaviours the user reported in the portal demo happen, and propose how to fix each. Track: portal. Change no files.

# Context

Mudcrab is a Rust/Bevy 0.19 reimplementation of Skyrim's engine. On the `portal` branch, load doors are "portals": looking through an open load door shows the destination cell live (a second camera renders it), and walking through crosses with no loading screen. The destination cell is pre-streamed. Doors swing with their Open/Close animation clips from the converted model.

The user's notes from playing the Riverwood demo (2026-09-25):
1. **Door side swaps:** the door leaf is on one side of the doorway when seen from inside and on the other side when seen from outside. It should be on the same side from both.
2. **Doors cannot be closed:** a door can be opened (E) but not closed again. Both should work.
3. **Door opens before the far side is ready:** a door should not open until the world it leads to is fully loaded.
4. **No auto-close:** a door the player opened and walked away from should play its close animation by itself, and finish closing *before* the far side's cells are unloaded, so the player never sees the unload.

Code to read (all under `C:\Modding\MudcrabProject`):
- `crates/engine/src/doors.rs`: the door contract and state.
- `crates/engine/src/door_animation.rs`: the door state machine and clips.
- `crates/engine/src/portal.rs`: doorway rendering, which door is the portal, when the far door is hidden or opened (look for `select_portal_door`, `isolate_cells`, and the far door opened at the swap, impl-152).
- `crates/engine/src/transition.rs`: pre-streaming and the crossing (`DoorTransition`).
- `crates/engine/src/player.rs`: the E key.
- `crates/engine/src/streaming.rs`: large; search it for what you need rather than reading it whole (cell load/unload, `ActiveCell`, readiness).
- Design notes: `docs/design/portal-performance.md` (item 5, auto-close), `docs/design/portal-visuals.md`, `docs/research/portal-frames.md`, `docs/roadmap/backlog-engine-and-portal.md` (the Portal plugin section).

Do not read anything under `local/`, `target/` or `tasks/deepseek/` except this brief.

# Scope

Read-only. Change no files.

# Done when

Every one of the four notes has a cause traced to specific lines (or an honest "could not find") and a proposed fix that says which files and systems would change.

# Report

For each note 1-4:
- **Now:** what the code does today, with `path:line` citations.
- **Why:** the cause.
- **Fix:** the proposed change: which systems and files, and the states or events involved. For note 1, say whether the far door is a separate entity from the near one (each cell has its own door reference linked by `door_links`) and which one is drawn open or hidden from each side. For note 4, say what currently triggers unloading the far side and how a close could be ordered before it.
- **Confidence:** high, medium or low.

End with any interactions between the fixes (for example, a closable door and the portal hiding the far door), and a suggested order to implement them.
