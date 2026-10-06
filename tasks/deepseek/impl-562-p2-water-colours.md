# Goal
Track: Phase 2 (for upstream). Each water surface is drawn in its own Skyrim colours and
reflectivity (from its WATR record's DNAM) instead of one constant for every water in the game.
The converter's `waters` table currently fills its colour columns from the wrong subrecords.

# Context
- Your checkout is cut from `phase2/water-look`, which is `main` plus tonight's water-shader fix.
  Use upstream's commit style (`fix(converter): ...`, `feat(engine): ...`), with no `[deepseek]`
  prefix.
- Research, measured against Skyrim.esm and Update.esm:
  `C:/Modding/MudcrabProject/local/research/water.md`. Read its sections 0 and 1 (about lines 1-160).
  - The bug: `crates/converter/src/esm/exporter.rs` (about lines 200-202 on main) fills
    `waters.shallow_color / deep_color / reflection_color` from `NAM0`/`NAM1`/`NAM2`. In SSE those
    are linear velocity, angular velocity (three f32 each) and the layer-1 noise texture path. The
    colours live in `DNAM`: Shallow Color at byte 40, Deep Color at 44, Reflection Color at 48,
    each u8 RGB + one pad byte. Also in DNAM: Reflectivity Amount f32 at 20, Fresnel Amount f32
    at 24, Above-water fog far f32 at 36. DNAM is 228 bytes (232 in eight SSE records).
  - Update.esm overrides all 34 Skyrim.esm WATR records and adds 4. The final values must be the
    override's. Check that the exporter already resolves overrides by FormID for WATR, as it does
    for other records, and say so in the handoff.
  - Expected final values (sRGB bytes): DefaultWater shallow (38,39,24), deep (5,14,18),
    reflection (119,140,157), fresnel 0.10. RiverWaterFlow and the other waters are in the report's
    table in section 1.3.
- Engine: `crates/engine/src/streaming.rs` builds each cell's water material (search for
  `WaterMaterial {`, currently a constant `Color::srgba_u8(5, 14, 18, 153)`), and the water shader
  is `crates/engine/src/shaders/water.wgsl` (constants `WATER_FRESNEL_F0` 0.10 and
  `WATER_REFLECTIVITY` 0.8). A cell's water type FormID is already known there
  (`terrain.water_type_form_id`, used to look up the flow normal through
  `catalog.water_flow(...)` in `crates/engine/src/world/database.rs`).
- The change:
  1. Converter: decode DNAM for the three colours plus Fresnel Amount and Reflectivity Amount.
     Write the colours into the existing columns, and add `fresnel` and `reflectivity` columns if
     the table lacks them. A world-DB schema change must follow the repository's rules for
     `crates/shared` schema constants and `docs/specs/converters/db-schema.md`; say exactly what
     you bumped. Stop and report if a bump looks larger than one constant.
  2. Engine: read those columns through the asset catalog (tolerating a database that lacks them,
     as `landscape_normal` does for `normal_path`). Use the deep colour as the base colour
     (keeping today's 60% cover). Pass fresnel and reflectivity to the shader through
     `WaterSettings` in place of the two constants, falling back to today's values.

# Scope
Owned files:
- `crates/converter/src/esm/exporter.rs` and the WATR decode it needs (e.g.
  `crates/converter/src/esm/records/` if a WATR module exists or belongs there)
- `crates/engine/src/world/database.rs`, `crates/engine/src/streaming.rs` (the water material only),
  `crates/engine/src/render.rs` (`WaterSettings` / `WaterExtension` only),
  `crates/engine/src/shaders/water.wgsl`
- `crates/shared/src/lib.rs` (only for a schema constant, if needed)
- `docs/specs/converters/db-schema.md`
- tests next to the code you change
- `docs/handoff/impl-562-p2-water-colours.md`

# Done when
- Tests that fail on the old code and pass on yours:
  1. A synthetic WATR record with a 232-byte DNAM (and one with 228 bytes) decodes to the right
     shallow, deep and reflection colours, fresnel and reflectivity; NAM0/NAM1 are no longer read
     as colours.
  2. The engine's catalog returns a water's colours and factors, and returns None for a database
     without the new columns.
- `cargo fmt --all -- --check`, `cargo clippy -p converter -p engine --all-targets -- -D warnings`,
  `cargo test -p converter` and `cargo test -p engine` pass.

# Report
- Files changed and the approach in 5 lines.
- Whether a schema bump was needed, and what you bumped.
- Each test's name and what it proves.
