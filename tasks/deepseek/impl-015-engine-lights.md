# impl-015 - Engine: real point lights from LIGH references

Owned files: crates/engine/src/world/database.rs,crates/engine/src/streaming.rs,crates/engine/src/lights.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md` and the design
`docs/design/lights-and-auto-doors.md` (its "Engine" section is your spec; the "Converter" section is
the table you read) first. You cannot compile shaders here; this task needs no WGSL.

## Goal

Skyrim's lights - torches, braziers, Dwemer lamps, Blackreach's glowing fungus lights - become Bevy
point lights, so the Alftand ruins and Blackreach are lit the way the game lights them instead of by
flat ambient light and a lantern on the camera.

## What to build

1. **Database** (`world/database.rs`): the reference load query LEFT JOINs the new `lights` table
   (design DDL) on the reference's base id and returns the row (radius, colour, flags, falloff, fade)
   on the reference. A database without the `lights` table must still load - detect it the way the
   `door_links` join already does in this file, and treat every reference as unlit.
2. **Spawning** (`streaming.rs`, `spawn_cell`): a reference with a light row gets a `PointLight` child
   entity, marked with a component defined in `lights.rs`. Colour from the row; `range` = radius;
   intensity chosen so the light clearly lights surfaces at half its radius at these distances
   (1 unit = 1 Creation unit, so metre-scale Bevy defaults are far too weak - derive it and write the
   formula down); `shadows_enabled: false`. Skip negative lights and lights flagged off by default
   (flag bits per the design / UESP `LIGH`).
3. **Budget** (`lights.rs`, a `LightsPlugin` system): keep only the nearest 64 spawned lights to the
   camera enabled (the rest `Visibility::Hidden`), re-chosen when the camera has moved more than about
   256 units. Register `pub struct LightsPlugin` in `lights.rs`; the lead adds `pub mod lights;` (a stub
   exists) and adds the plugin to the app.
4. Lights must obey the portal isolation (`crates/engine/src/portal.rs`): a light that belongs to a
   pre-streamed cell must not light the space the player stands in. Read how isolation hides cells
   (it re-layers mesh descendants and hides non-destination cells) and make sure a light child of such
   a cell is excluded too; if that needs a change in `portal.rs` (not yours), describe it exactly in your
   result instead of editing it.

## Tests (fail before your change)

- The database fixture with a `lights` row returns it on the reference; a fixture without the table
  still loads.
- A spawned cell with one lit reference has one `PointLight` child with the right colour and range; a
  negative or off-by-default light spawns none.
- The budget keeps exactly the nearest 64 of, say, 100 lights enabled and re-chooses after the camera
  moves.

## Crosstalk

`impl-014-converter-lights.1` runs at the same time and writes the `lights` table. Code against the
design's DDL; if it messages you a column change, adopt it and quote the message in your result.

## Done when

Acceptance commands pass. Report per the implementation template, including the intensity formula
and anything the lead must change in `portal.rs` or `app.rs`.
