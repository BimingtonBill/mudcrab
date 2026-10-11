# Goal
Pre-open review of a new feature branch for the Mudcrab project (an open Skyrim engine, Rust, Bevy 0.19, bevy_rapier3d 0.36): Skyrim's `coe` (CenterOnExterior) and `coc` (CenterOnCell) console commands, exterior cells only. It opens upstream as a DRAFT PR stacked on the dev console PR. Decide what would break, panic, mislead, or embarrass before it opens.

# Context
- Your working folder is the review folder. `delta.diff` is the whole change from the console head ee4755c0 to the new head 9c0b8420 (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- New file `crates/engine/src/cell_commands.rs`: parses the commands, looks the cell up in the world database (`cells(id, worldspace_id, grid_x, grid_y, interior_name, flags, data)`; despite its name `interior_name` holds the editor id of interiors and of named exteriors; editor ids are unique ignoring case), computes the cell centre and ground height, and sends a `TeleportPlayer` message.
- In `crates/engine/src/physics.rs`, the `TeleportPlayer` block, its registration and the `teleport_tests` module are copied byte-identically from another contributor's branch (the door crossing, #112) so the two merge cleanly. Review how this branch USES it; problems inside the copied block itself are worth one line each but cannot be changed here.
- One existing test (`riverwood_tankard_spawn_pickup_and_drop`) got a one-line filter change because registering the message shifted entity ids.
- Console rule of the project: only Skyrim's real command names and syntax.
- Known, accepted gaps for the draft (do not report as findings, unless the code contradicts them): the player lands on the cell centre, not Skyrim's COC marker (the converter does not export markers); interior cells and cells of another worldspace only print an error; no did-you-mean for unknown names.

# Scope
Read only. Change nothing.

# Done when
Every file in `after/` that the delta touches has been read against `before/`. Check especially: coordinate maths (grid to render space, sign of the y axis, render origin), the ground height source when the target cell is not loaded, walk-mode fall-through, error paths (no panic on any console input), the SQL and its read-only connection, test strength, and comments that say something the code does not do.

# Report
Status, Verdict (accept / small fixes / fix first), one-paragraph summary, then findings as `path:line - problem - suggested fix`, most severe first. End with what you could not check.
