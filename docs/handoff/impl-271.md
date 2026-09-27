# impl-271 - The last grid-offset copies go through streaming's helpers (portal)

Follow-up of impl-268 (523683d), which made `crates/engine/src/streaming.rs` own the grid-to-render
offset (`grid_translation`, its inverse `absolute_render_position`, and `camera_grid`). This routes
the copies left in the files impl-268 did not own through those helpers. No behaviour change: each
replacement computes the same floating-point values as the code it replaces (the old
`-z + origin.y * CELL_SIZE` and the helper's `-(z + -origin.y * CELL_SIZE)` round identically, and
adding `0.0` for the zero origin changes no floor).

## Call sites changed

- `portal.rs`, `ActiveSpace`: the private `ActiveSpace::camera_grid` copy is gone; `ActiveSpace::of`
  takes its centre from `streaming::camera_grid`. `CELL_SIZE` is no longer imported.
- `portal_bench.rs`, `camera_cell`: `grid_of` on a hand-offset position replaced by
  `streaming::camera_grid(position, origin.0)`. `config::grid_of` and `CELL_SIZE` are no longer
  imported.
- `transition.rs`, `creation_from_render`: the exterior arm is `absolute_render_position`.
- `transition.rs`, two further copies of the grid read in the same owned file, both against the
  absolute Bevy frame (the zero origin): `destination_grid`'s `XTEL` fallback and `switch_space`'s
  new render origin now call `camera_grid(.., IVec2::ZERO)`. `CELL_SIZE` moved into the test
  module's imports, where the tests still use it.

## Tests added

- `portal::tests::the_active_space_is_centred_on_the_camera_cell_away_from_the_zero_origin`
- `portal_bench::tests::the_camera_cell_is_read_against_the_render_origin`

Both use hand-worked grids at a non-zero origin on both sides of a cell corner. `creation_from_render`
was already pinned at a non-zero origin by `transition::tests::the_crossing_maps_the_pose_the_design_specifies`
(exterior destination, origin (19, 18)).

## Copies left elsewhere (not owned, unchanged)

- `streaming.rs` ~3147-3151 (`out_of_range_roots` in the streaming invariant check): a verbatim
  copy of `camera_grid(camera.translation, origin.0)`. The obvious next one to route.
- `streaming.rs` ~3076-3077 (`update_render_origin`): the rebase shift uses `trunc` of the camera's
  render position, deliberately different (a whole-cell move, not a grid read), so not a copy.
- `world/components.rs` 23-40 (`ExteriorCellGrid`): Creation-axis helpers, excluded by the brief.
- `world/database.rs` ~758 (`(position / 4096.0).floor()`) and `config.rs` `grid_of`: Creation-space
  grid reads with no origin, not grid-to-render conversions.
