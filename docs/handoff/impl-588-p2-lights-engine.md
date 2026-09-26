# impl-588-p2-lights-engine - point lights in the engine

**Track:** Phase 2 (upstream lights PR, engine half). **Status:** complete, tests green.

## What this does

A reference whose base record has a row in the converter's `lights` table spawns a Bevy `PointLight`
as a child of the reference while the engine runs with `--lights` (default off). One system keeps the
64 nearest lights enabled and hides the rest. `docs/specs/engine/point-lights.md` is the spec.

## Files

| File | Change |
| :--- | :--- |
| `crates/engine/src/lights.rs` | new: `point_light`, `radius_of`, `intensity_for_radius`, the flag bits, the constants, `SkyrimLight`, `LightsPlugin`, `budget_lights`, and 11 tests |
| `crates/engine/src/lib.rs` | `pub mod lights;` |
| `crates/engine/src/world/database.rs` | `LightRow`; `ReferenceRow::light` and `light_radius_override`; `REFERENCE_COLUMNS`/`REFERENCE_JOIN`/`LIGHT_COLUMNS`/`LIGHT_JOIN`/`RADIUS_OVERRIDE_COLUMN` and the two probes; the query is assembled from them; `map_reference` reads columns 18..23; 3 tests |
| `crates/engine/src/streaming.rs` | `spawn_cell` takes `lights: bool` and places the light child; 2 tests with a `spawn_cell` test app |
| `crates/engine/src/app.rs` | registers `LightsPlugin` for every run; the world path's `GlobalAmbientLight` uses `crate::lights::AMBIENT_ILLUMINANCE` |
| `crates/engine/src/config.rs` | `pub lights: bool` and `--lights`; a parse test and a default test |
| `docs/specs/engine/point-lights.md` | new spec |

## Constants

| Constant | Value |
| :--- | :--- |
| `AMBIENT_ILLUMINANCE` | `160.0` (the world path's ambient brightness; `app.rs` now uses this constant) |
| `LIGHT_EXPOSURE` | `10.0` (round default, not fitted) |
| `HALF_RADIUS_ILLUMINANCE` | `4*PI^2 * 160 * 10` = 63165.6 |
| `HALF_RADIUS_WINDOW` | `225/256` |
| `INTENSITY_REFERENCE_RADIUS` | `256.0` |
| `ENABLED_LIGHT_BUDGET` | `64` |
| `BUDGET_RECHOOSE_DISTANCE` | `256.0` |

## Tests

`lights::` - half-radius identity (and the `4*PI^2` identity), colour-independence, the
inverse-square law and the cap boundary, the cap leaving every radius up to 512 byte-identical, the
big record keeping reach without the brightness, the torch shape, the flag skips, the `XRDS`
override, the budget keeping the 64 nearest and re-choosing on camera movement, the budget
re-choosing on a same-count set change, and the budget leaving a non-`SkyrimLight` light alone.

`world::database::` - a lit reference's `LightRow` and override, an unlit reference's `None`, a
database with the table but no override column, and a database with neither.

`streaming::` - one light for a lit reference (with the `XRDS` radius, the child hierarchy and the
reference's `Visibility`), and nothing placed while the flag is off.

## Open questions for a reviewer

1. The light reads `radius`, `color_*` and `flags`; the converter's attenuation exponent and `fade`
   columns are not read, because nothing consumes them yet (both are named in the spec as left for
   later).
2. `docs/specs/README.md` does not list the new spec; that file was outside this task's scope.
3. Re-spawning a prefab or a future `PointLight` fixture has to carry `SkyrimLight` to be budgeted,
   and must not, to be left alone - which is what the marker is for.
