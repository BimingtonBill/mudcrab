# impl-562: WATR colours from DNAM, not NAM0/NAM1/NAM2

Branch base: `phase2/water-look` (`main` plus the water-shader fix). Track: Phase 2 (for upstream).

## Problem

`crates/converter/src/esm/exporter.rs` filled `waters.shallow_color/deep_color/reflection_color`
from `NAM0`/`NAM1`/`NAM2`. In SSE those are Linear Velocity, Angular Velocity (three `f32`s each)
and the layer-1 noise texture path — not colours. Every water in the game rendered with whatever
those bytes happened to decode to, and nothing downstream read the columns anyway. The real
colours, plus Fresnel Amount and Reflectivity Amount, live in `DNAM` (offsets confirmed against
`Skyrim.esm`/`Update.esm` in `docs/research/water.md` sections 0-1).

## Approach

1. **Converter** (`crates/converter/src/esm/exporter.rs`):
   - `waters` gains two additive, nullable columns: `fresnel REAL, reflectivity REAL`.
   - New `parse_water_dnam` decodes Reflectivity Amount (`DNAM+20`), Fresnel Amount (`DNAM+24`),
     Shallow/Deep/Reflection Color (`DNAM+40/44/48`, `u8` RGB + pad packed the same way the old
     `packed_color` did) via two small helpers, `dnam_color`/`dnam_f32`, that index with `.get()`
     so a short/absent `DNAM` yields `None` instead of panicking. The old
     `packed_color(Option<&[u8]>)` helper is gone; nothing else used it.
   - The WATR arm now reads all five fields from one `parse_water_dnam(view.find(b"DNAM"))` call
     and no longer touches `NAM0`/`NAM1`/`NAM2` at all.
   - **Overrides already work.** `EsmParser::merge_plugins` (`crates/converter/src/esm/mod.rs`,
     out of scope for this task) merges plugins into one `HashMap<u32, RawRecord>` keyed by
     FormID, later plugins overwriting earlier ones before `export_to_db` ever runs. Update.esm's
     WATR records (overrides for all 34 of `Skyrim.esm`'s, plus its own 4 new ones) already win
     over `Skyrim.esm`'s with no WATR-specific code needed — same as every other record type.

2. **Engine** (`crates/engine/src/world/database.rs`, `streaming.rs`, `render.rs`, `water.wgsl`):
   - `AssetCatalog` gains a `water_colors: HashMap<u32, WaterColors>` map and a
     `water_colors(form_id) -> Option<WaterColors>` accessor (`WaterColors { shallow, deep,
     reflection: [u8; 3], fresnel, reflectivity: f32 }`, decoded with a new `unpack_color(u32)`).
   - The query that builds it uses `connection.prepare(...).ok()` rather than `?`: if `fresnel`/
     `reflectivity`/the colour columns don't exist at all (a database converted before this
     change), `prepare` fails with a "no such column" error, which is swallowed into an empty map
     rather than failing `AssetCatalog::open`. Every other water-agnostic query in `open` is
     unchanged and still uses `?` (those columns have existed since the table was created).
   - `streaming.rs`'s `spawn_cell` looks up `catalog.water_colors(water_type_form_id)`. Present:
     the water mesh's `base_color` becomes the water's own deep colour (RGB from the catalog,
     alpha 153 — the existing 60% cover is unchanged), and fresnel/reflectivity are threaded into
     the new `WaterExtension::with_reflection_and_factors(reflection, flow_normal, fresnel,
     reflectivity)`. Absent: falls back to the previous hard-coded deep colour `(5, 14, 18)` and to
     `render::DEFAULT_WATER_FRESNEL`/`DEFAULT_WATER_REFLECTIVITY` (0.10/0.8, DefaultWater's own
     values) — bit-identical to today's behaviour.
   - `render.rs`: `WaterExtension::with_reflection` (2-arg) is kept as a thin wrapper around the
     new `with_reflection_and_factors`, defaulting to the same two constants, so
     `crates/engine/src/app.rs`'s terrain/water fixture (not in this task's scope, and not
     touched) keeps compiling and behaving exactly as before. `WaterSettings` gained a third
     `Vec4` field, `fresnel_reflectivity` (x = fresnel, y = reflectivity; z/w unused).
   - `water.wgsl`: `WaterSettings` mirrors the new field; the `WATER_FRESNEL_F0`/
     `WATER_REFLECTIVITY` constants are gone, replaced by reads of
     `water.fresnel_reflectivity.x`/`.y`. The lighting math (Schlick fresnel, post-lighting
     reflection blend — last night's white-sheet fix) is untouched.

## Schema bump: none

`shared::WORLD_DATABASE_SCHEMA_VERSION` stays at 3. `fresnel`/`reflectivity` (and the pre-existing
but previously-unpopulated colour columns) are purely additive and nullable, and the engine change
above exists specifically so a database that lacks them still opens and renders — just with
today's one constant colour instead of per-water colours — rather than being refused outright the
way a `schema_info` version mismatch would refuse it. Bumping the version would force every
existing `skyrim_world.db` (including `SkyrimConverted-upstream15`, `SkyrimConverted`) to be
reconverted before the engine would open them at all, which this change does not need and which
the disk cannot currently afford. `docs/specs/converters/db-schema.md` documents the new columns
and this reasoning in a new "12. Water Definitions (`waters`)" section; `crates/shared/src/lib.rs`
is untouched.

If the lead disagrees and wants a reconversion-forcing bump instead, the one thing to change is
`shared::WORLD_DATABASE_SCHEMA_VERSION` plus the hard-coded literal `3` in `exporter.rs`'s
`create_tables` (`INSERT INTO schema_info(version) SELECT 3 ...` — not sourced from the constant,
has to be edited by hand) and the two hard-coded `3`s in `database.rs`'s own test fixtures; the
tolerant `AssetCatalog::water_colors` code would still be correct either way (it degrades
gracefully for *any* database missing the columns, version bump or not).

## Tests

New, all fail on the pre-change code:
- `converter::esm::exporter::tests::parses_water_dnam_colors_and_factors_at_both_dnam_lengths` —
  a synthetic 228-byte and a 232-byte `DNAM` (DefaultWater's final, Update.esm-applied values)
  both decode to shallow (38,39,24), deep (5,14,18), reflection (119,140,157), fresnel 0.10,
  reflectivity 0.8. Fails on old code because `parse_water_dnam` doesn't exist yet.
- `converter::esm::exporter::tests::water_export_reads_colors_from_dnam_not_nam0_nam1` — a full
  `export_to_db` run on one synthetic WATR record whose `NAM0`/`NAM1` are deliberately not
  DefaultWater's colours (three `9.0f32`s each) and whose `DNAM` is DefaultWater's real bytes; the
  exported `waters` row's colour/fresnel/reflectivity columns match `DNAM`, not `NAM0`/`NAM1`.
  Fails on old code (it would read `NAM0`/`NAM1`'s bytes as the colours instead).
- `engine::world::database::tests::catalog_returns_water_colors_and_factors` — a `waters` row with
  the new columns populated (DefaultWater's values again) decodes through
  `AssetCatalog::water_colors` to the expected `WaterColors`. Fails on old code because the method
  doesn't exist yet.
- `engine::world::database::tests::catalog_rewrites_landscape_texture_paths` (extended) — its
  existing fixture's `waters` table has no colour/fresnel/reflectivity columns at all (predates
  this change); `catalog.water_colors(9)` is `None` rather than an error. Fails on old code (the
  method doesn't exist).

## Gates

All four pass on the final tree:
- `cargo fmt --all -- --check` — clean, no output.
- `cargo clippy -p converter -p engine --all-targets -- -D warnings` — clean (only the
  pre-existing, unrelated `proc-macro-error2` future-incompat note).
- `cargo test -p converter` — lib: 103 passed, 0 failed, 7 ignored; every other test binary in the
  crate (texture-sync, fixture_round_trip, doctests) also 0 failed.
- `cargo test -p engine` — lib: 43 passed, 0 failed, 0 ignored; every other test binary in the
  crate also 0 failed.

## Not done here

- Did not confirm the DNAM offsets against `SkyrimConverted-upstream15/skyrim_world.db`'s raw
  `waters.data` blobs with a scratch script (the brief made this optional and
  `docs/research/water.md` already tags its offsets "measured" against `Skyrim.esm`/`Update.esm`
  directly).
- Did not run the engine or the converter (out of scope; GPU belongs to another session).
- Shallow/reflection colours are decoded and exposed by `AssetCatalog::water_colors` but not yet
  consumed by `streaming.rs` beyond the deep colour for `base_color`, matching the brief; a
  follow-up could use shallow colour for a depth-based colour gradient once the shader has depth
  fog (see `docs/research/water.md`'s "Recommendation for a faithful first step").
