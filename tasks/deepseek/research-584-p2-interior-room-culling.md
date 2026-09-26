# Goal
Track: Phase 2. Produce a design for culling interior cells by Skyrim's own rooms and portals
(room markers and portal markers), so an interior draws only the rooms visible from the camera's
room. Change no code; the output is a design the lead turns into implementation briefs.

# Context
- The engine is Bevy 0.19 (`crates/engine`); interiors are streamed from a SQLite world database
  written by `crates/converter` (`crates/converter/src/esm/exporter.rs`, `docs/specs/converters/db-schema.md`).
  Today every reference in an interior cell is drawn; there is GPU occlusion culling
  (`OcclusionCulling` on the camera) and frustum culling, nothing room-based.
- Skyrim authors interiors with room bound markers and portal markers (placed references whose base
  is a room/portal marker; REFR subrecords such as XRMR (linked rooms), XPOD (portal destinations),
  XPTL (portal data), XRGD/XRGB, and the LCTN/cell-level occlusion data). Establish exactly which
  subrecords exist, their byte layouts, and how the game uses them, from the data and public
  documentation (UESP Skyrim mod file format pages, xEdit's record definitions).
- Game data: `C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data\Skyrim.esm` (read-only);
  the converted database: `C:\Modding\SkyrimConverted\skyrim_world.db`.

# Scope
Read-only. You may write and run Python scripts in your working folder to walk Skyrim.esm records
(the header is 24 bytes: type, data size, flags, form ID, version fields; compressed records carry
flag 0x40000 and a zlib body after a u32 decompressed size).

# Done when
- The subrecords' layouts are stated with evidence (a hex dump of at least two real examples each,
  decoded, from two different interiors, e.g. a Riverwood house and a Nordic ruin).
- Counts: how many interior cells use room markers and portals; rooms per cell (median, max);
  references with XRMR membership.
- How room membership of a normal reference is known (a subrecord on the reference? the marker's
  linked refs? bounds only?).
- A design: the database tables/columns to add; the engine's per-frame algorithm (camera room →
  visible rooms through portal frusta → visible references), how references outside every room are
  handled, and what a first, smallest version would be. Estimated saving on two interiors (count of
  references drawn today vs with the design, from the data).

# Report
1. Subrecord layouts with dumps. 2. Counts. 3. Membership. 4. The design and its smallest first
step. 5. Risks and open questions. Confidence per claim.

# Web questions
- UESP "Skyrim Mod:Mod File Format/REFR": the XRMR, XPOD, XPTL, XRGD, XRGB subrecords and room/portal marker fields.
- xEdit (TES5Edit) record definitions for REFR room and portal subrecords (wbDefinitionsTES5.pas).
- How Skyrim's engine uses room markers and portals for interior occlusion (Creation Kit wiki: Room Bounds, Portals).
