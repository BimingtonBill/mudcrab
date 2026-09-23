# PR 2 - fix(shared): Creation reference angles turn clockwise

Branch `phase2/refr-rotation` (worktree `wkj-phase2`), commits `b9cdff8`, `0fd260f` on `2105bfd`. Drafted by DeepSeek research-080, edited by Claude (unverified mechanism and OpenMW claims removed). Opened as realfakenerd/wah-krah-jol#25 on 2026-09-23.

---

`shared::coordinates::creation_euler_to_runtime_quaternion` converted a `REFR/DATA`
rotation by building `Rz(z) * Ry(y) * Rx(x)` and conjugating that by the
Creation-to-runtime basis. Measured against the game data, the stored angles describe the inverse of that
rotation, so the object is rotated by
`Rx(-x) * Ry(-y) * Rz(-z)`, applied in Creation space before the basis conjugation.
For a yaw-only reference the old result was `Rz(+z)` where the data means `Rz(-z)`,
so every reference with a non-zero yaw (or tilt) was turned the wrong way: a yaw
of +90° rendered as -90°.

## Evidence

Both measurements only need an unmodded install (Skyrim.esm) and the converter's
world database; nothing here depends on a mod or a fork of the data.

*Doors — the yaw sign.* A load door's `XTEL` carries the arrival point, and for a
given door model it sits at that model's usual angle. Take the door references in
Skyrim.esm and group them by base model, keeping the models with 5 or more
placements (63 models). Under the clockwise convention, 88% of those placements put
their `XTEL` arrival along the model's usual angle; under the counter-clockwise
convention used until now, 42%.

*Tilted references — the tilt signs and the Euler order.* Take the 82,731 exterior
references that are tilted more than 3° and stand on sloped ground (|`pos_z` −
terrain| ≤ 128 units, terrain more than 5° from horizontal), sample the cell's
`LAND` heightmap for the terrain normal, then rotate the model's up axis `(0,0,1)`
by each candidate convention and measure the angle to that normal. Median error is
8.18° for the convention in this PR, 19.45° for the nearest genuinely different
candidate, and 21–32° for the rest. (The up axis is invariant under the yaw, so this
audit ranks the tilt signs and the Euler order; the z sign is settled by the doors.)

## What changed

- `crates/shared/src/coordinates.rs`: the stored `Rz * Ry * Rx` product is
  conjugated before the basis conjugation, i.e. `Rx(-x) * Ry(-y) * Rz(-z)`; the doc
  comment now states the convention and how it was measured.
- Tests in the same file follow the measured convention: a 90° yaw takes Creation
  `+X` (east) to `-Y` (south, runtime `+Z`), the 90° Creation-X and Creation-Y
  rotations flip direction, `converted_rotation_matches_converted_source_vector`
  builds its reference rotation as the inverse, and a new test
  `a_yaw_turns_clockwise_like_a_heading` pins a yaw-only reference to looking along
  Creation `(sin z, cos z)` — a heading measured clockwise from north.
- One engine assertion encoded the old sign and moves with it:
  `crates/engine/src/streaming.rs` (`maps_creation_position_and_rotation_through_the_same_basis`)
  expected `creation_rotation_to_bevy([0.0, 0.0, FRAC_PI_2]) * Vec3::X` to be
  `Vec3::NEG_Z`; under the new convention it is `Vec3::Z`. The position half of that
  test (`creation_to_bevy(Vec3::Y) == Vec3::NEG_Z`, `Vec3::Z == Vec3::Y`) is
  unchanged.

## No reconversion needed

The converter stores the raw `DATA` radians (`crates/converter/src/esm/exporter.rs`
copies `rot_x/rot_y/rot_z` straight out of the subrecord) and the runtime performs the
conversion when it spawns a reference (`crates/engine/src/streaming.rs`), so existing
converted databases stay valid. `WORLD_DATABASE_SCHEMA_VERSION` and
`CELL_CACHE_VERSION` are unchanged, and the cell cache holds terrain, not references.

## Roadmap

This is item one of Marco 1 in `docs/roadmap/02-completion-plan.md`, which lists
"correção das rotações `REFR`" (line 86) among the coordinate-contract work, and
`docs/roadmap/02-milestone-1-implementation-plan.md:50` makes settling the
`REFR/DATA` Euler order a precondition for consolidating the contract. This PR
settles it against real data and pins it with tests.

## Tests

`cargo test -p shared` exercises the convention (7 tests, no game data needed);
`cargo test --workspace` covers the engine assertion above and the rest of the
workspace.
