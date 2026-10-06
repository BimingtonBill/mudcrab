# Goal
For each visual gap between Mudcrab's renders and real Skyrim screenshots, work out exactly
what data drives the effect in Skyrim, what our converter and engine do with that data today, and
the smallest correct way to implement it. The result is an implementation spec the lead can turn
into coding tasks.

# Context
- Mudcrab is a Rust/Bevy 0.19 reimplementation of the Skyrim SE renderer and world streaming.
  - The workspace is at `C:/Modding/MudcrabProject`, with the crates `converter`, `engine`
    and `shared`.
  - The converter turns Skyrim.esm and the BSA archives into glTF, KTX2 and a SQLite world
    database: `C:/Modding/SkyrimConverted/skyrim_world.db`, read-only. Open it with the URI
    `file:...?mode=ro`.
  - The game install is read-only at `C:/Games/Steam/steamapps/common/Skyrim Special Edition`.
  - `tools/research/` has Python helpers for the database (`world_db.py`, `uesp_probe.py`,
    `space_lights.py`, `glb_emissive.py`, `exposure_stats.py`).
  - Reference screenshots of the real game are in `local/reference/uesp/`. The engine's renders
    of the same poses are in `local/poses2/final-4x3/`, and the side-by-side pairs are in
    `local/poses2/cmp-final/`. Workers can see images (`Read` a `.jpg`/`.png`).
  - The gap list comes from `docs/research/reference-shot-poses.md`.
- The gaps are independent of each other:
  1. **Glow / emissive.** Blackreach's mushrooms, light columns, lit doorways and the golden
     "sun" orb render dark instead of bright cyan. Look at NIF `BSLightingShaderProperty`:
     - its emissive colour and multiple;
     - the glow-map shader flag (`SLSF2_Glow_Map`) and the `_g.dds` glow textures;
     - `BSEffectShaderProperty`.
     Check what `crates/converter/src/material.rs` exports today into glTF (emissive factor,
     emissive texture, `KHR_materials_emissive_strength`) and whether the engine's materials use
     it. impl-013 added some glow handling; find out what it covers and what it misses.
  2. **Per-cell lighting and fog.**
     - Interiors: `CELL` `XCLL` (ambient, directional, fog near/far/colour, directional ambient
       colours) and `LTMP` lighting templates (`LGTM` records), including the inherit flags.
     - Exterior and underground worldspaces: sky colour, fog and ambient come from weather
       (`WTHR`) through climate (`CLMT`) and region weather lists.
     - Blackreach (WRLD 0x1EE62) and the Alftand cavern (WRLD 0x69857) have their own weather or
       lighting.
     - The engine today uses one hand-tuned `GlobalAmbientLight` per kind of space
       (`crates/engine/src/app.rs`, `update_atmosphere`) and no interior fog. Find which fields
       matter most for the Alftand and Blackreach references, and whether the converter already
       reads them.
  3. **Directional snow and ice.**
     - Roofs render as bare bronze where the game shows snow caps, for example
       `DweFacadeTowerRoof01SnowHeavy`. In Skyrim this is the `STAT` `DNAM` directional material:
       a material object (`MATO`) and a max angle, projected onto up-facing surfaces.
     - Ice and glacier meshes read as dark rock. Find out why, from their shader flags, env maps,
       specular and textures.
  4. **Broken or odd objects.**
     - Black voids where the game has walls, probably the `BlackPlane01` references (111 in
       Alftand01, 74 in Alftand02): what are they, and are they meant to be drawn?
     - Two Blackreach models fail validation: `meshes/Furniture/CreatureExit/WispAmbush.glb`
       (REFR 000F9907) and `FrostSpiderAmbush01.glb` (REFR 0007C068), with the error "converted
       model has no validated aggregate bounds".
     - One `FalmerCityLight02NS` with a radius of 4334 floods Blackreach's ceiling orange. What
       does Skyrim do with such a light (its fade, falloff exponent, or a portal-strict or other
       flag)?
- Every claim needs evidence: a record or field dump from the data, a converter or engine
  `file:line`, or the Bevy 0.19 source (read-only in
  `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`). UESP's file-format
  pages (`https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format`) describe the record layouts.

# How to split it
- Four research workers, one per gap above. Add a fifth only if one gap turns out to be two.
- Crosstalk: off. The parts do not touch each other.
- Each worker's brief must contain the shared context above, its own gap, and the deliverable
  below.

# Scope
- The lead and its workers are read-only. Nobody edits files. Scripts a worker needs go through
  `python tools/research/<existing script>` only, or no scripts.

# Done when
- Every gap has a worker report, and you have checked the key claims (at least one data dump and
  one `file:line` per gap).

# Report
One merged answer, per gap:
- **What Skyrim does:** the records and fields, the NIF blocks and flags, and the numbers for the
  Alftand and Blackreach cases.
- **What we do today:** converter and engine `file:line`.
- **Implementation spec:**
  - converter changes (the files, and the new database columns or tables with their types);
  - engine changes (the files, and how the effect is rendered in Bevy 0.19);
  - whether a database schema bump and a reconversion are needed;
  - the tests;
  - how to verify it against the reference screenshots.
- **Effort and risk.**

Then: a recommended order, which of the four specs share files (so the lead can sequence the
coding tasks), what is still unknown, and per worker the run id, a one-line result and what you
checked.
