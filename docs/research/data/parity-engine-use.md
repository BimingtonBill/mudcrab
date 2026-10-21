# Parity: which converted columns the Phase 2 engine reads

Companion to `parity-engine-use.tsv`. Read at commit `03c663cb`
(`plus`); world database schema `shared::WORLD_DATABASE_SCHEMA_VERSION` =
100_701 (`crates/shared/src/lib.rs:21`), cell cache `CELL_CACHE_VERSION` = 3
(`:47`). Code read only: no converter or engine run.

## Counts

321 rows over 44 tables and 6 rkyv structures: **consumed 177, read_unused 15,
not_read 129**, unknown 0. `read_unused` is `space_lighting` (10: template_id,
fog_power, fog_clip, the three direction fields, weather_id, fog_max and both
light-fade distances - "carried, not applied", `world/lighting.rs:166-198`),
`lights.falloff`/`fade`, `plugins.id`/`name` and `movement_types.load_order`
(the last three only label the movement debug log, `physics.rs:328`).

## Tables with the most unread columns

Twelve tables are written and never read by anything in `crates/engine` or
`crates/shared`: `lod_block`, `grass_types`, `lod_grid`, `lod_tree_type`,
`lod_tree_instance`, `movement_game_settings`, `lod`, `landscape_texture_grasses`,
`scripts`, `formid_map`, `conversion_cache`, `room_graph_stats`. Partly unread:
`space_lighting` (10), `"references"` (8: worldspace_id, is_exterior, local_x/y,
header_flags, both enable-parent fields, data), `texture_sets` (6),
`exterior_spatial` (5), `land` (4 of 5 - the terrain comes from
`cell_cache.rkyv`), `waters` (4), `records` (4), `npcs` (4), `room_bounds` (3),
`matos` (2), `cells` (2).

## Files read

`crates/engine/src`: `world/database.rs`, `world/lighting.rs`, `world/cache.rs`,
`snow.rs`, `lights.rs`, `room_culling.rs`, `portal_graph.rs`, `physics.rs`,
`streaming.rs`, `atmosphere.rs`, `render.rs`, `player.rs`, `transition.rs`,
`bin/world-inspect.rs`, plus `crates/shared/src/lib.rs` and `world_assets.rs`.
Method: enumerate every query per table (`FROM`/`JOIN`), then follow each SELECT
list to the field use; `not_read` means no SELECT list, WHERE or JOIN in the
engine names the column.

## UNVERIFIED

- `read_unused` assumes Bevy's `Reflect` consumes none of these fields; no
  struct here is a reflected component carrying one.
- `consumed` rows are traced to the engine system that uses them, not into the
  GPU: `matos` values reach `snow.wgsl` through a uniform, so their shader
  effect is read from the uniform's documentation, not measured.
- `crates/launcher/src` issues no SQL at all, so it adds no reader; checked
  with a single search, not line by line.
- Not listed: `room_portals_new`, the migration copy of `room_portals` with the
  same columns, dropped and renamed in `rooms.rs:90-107`.
