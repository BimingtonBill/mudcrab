# research-055 - Riverwood: the data behind a second demo

Read `tasks/deepseek/context.md` first, and `docs/design/blackreach-demo.md` plus
`docs/design/reference-shots.md` to see the shape of the first demo and how it was verified.

## Goal

The user wants a Riverwood demo next, built with everything the Alftand → Blackreach demo taught
us. Work out, **from the converted data**, exactly what a Riverwood demo would contain, so the
lead can design the route and the reference shots.

The database is read-only at `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` (converter schema 17,
DB schema 5; it now also carries `space_lighting`, `matos` and door animation clips in the GLBs).
Write scripts under `tools/research/` and run them with `python tools/research/<script>.py`.

## Questions

1. **Where is Riverwood?** The Tamriel (worldspace 60) exterior cells that hold it: grid range,
   cell names/`EDID`s, and the world position of the village centre. How many references per
   cell, and what the big landmarks are (the mill, the bridge, the gate, the standing stones
   nearby).
2. **The interiors.** Every interior cell reachable from Riverwood's exterior through a load
   door: the Sleeping Giant Inn, Alvor's house, the Riverwood Trader, Gerdur's house, Faendal's
   house, the lumber mill, the Guardian Stones area if it is a cell, Bleak Falls Barrow's exterior
   route if it starts here. For each: cell id, name, the door pair (`door_links` both ways), the
   `XTEL` arrival points, and whether the doors are auto-load.
3. **A route.** Propose a walking route that shows the engine off: outdoors through the village,
   into two or three interiors and out again, ending somewhere striking. Give the start position
   and heading in Creation units (like `DemoStart` in `crates/engine/src/config.rs`), the door
   `ref_id`s in order, and the approximate walking distance.
4. **What will look wrong.** Riverwood is a daylight outdoor village with wooden buildings, water,
   trees and grass, which the Alftand route never exercised. From the data and from
   `docs/research/visual-gaps-spec.md`, predict what the engine will get wrong: trees and grass
   (are they converted? how many references, what model kinds), the river water, the mill wheel
   (animated?), NPCs (not implemented), snow on roofs, LOD. Count what you can.
5. **Numbers the lead needs:** how many references the route's cells hold in total, how many
   lights, how many door links, and whether any model in those cells fails validation today.

## Deliverable

`docs/design/riverwood-demo.md`: the answers, the proposed route with its coordinates and door
list, the predicted visual gaps with counts, and a short "what to build" list (engine flags,
`DemoStart` entry, reference shots to take). Commit it with a handoff note in `docs/handoff/`,
`[deepseek]` subjects, your own paths only.

## Report

The route, the interiors table, and the top five things that will look wrong.
