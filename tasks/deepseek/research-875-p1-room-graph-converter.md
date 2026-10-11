# Goal
Prepare the converter side of a per-cell interior room graph (upstream issue Mudcrab-Team/mudcrab#188): find where and how the converter should decode room and portal markers into new optional tables and assign each placed object to rooms by bounds, and whether that needs a world DB schema bump. Phase 1 track. Design study, read only.

# Context
- Read code ONLY from `D:/Mudcrab/worktrees/p1-main-ro` (a checkout of upstream main 2c11897b). The checkout at `C:/Modding/Mudcrab/mudcrab` is a different branch: never cite it.
- Prior design study (2026-09-26, older main): `C:/Modding/Mudcrab/mudcrab/local/research/interior-room-culling-design.md`. It found: markers are ordinary `references` rows (base_form_id 31 RoomMarker, 32 PortalMarker) whose `data` blob keeps raw subrecords; proposed tables `room_bounds`, `room_portals`, `room_links`; claimed "no schema bump" by the `waters` precedent (unverified against our contracts). It did NOT materialise membership; the new plan DOES: assign membership at conversion, flag objects in no room or several.
- Layouts (confirmed in that note against Skyrim.esm bytes): XPRM 32 bytes (float[3] half-extents, float[3] colour, float class, u32 primitive); XPOD two u32 RoomMarker FormIDs; XRMR u8 count, u8 flags, u16 unknown, then `count` XLRM u32 FormIDs. Membership rule measured: pivot inside the rotated box (centre plus or minus XPRM half-extents), 90.6-99.8% coverage.
- Relevant main files (verify paths yourself): `docs/specs/converters/db-schema.md` (references table near line 277, waters near 362-390 and its "no bump" wording), `crates/shared/src/lib.rs` (WORLD_DATABASE_SCHEMA_VERSION = 7, MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION = 3), `crates/converter/src/esm/` (exporter, record parsing, FormID remapping from #136), engine reader `crates/engine/src/world/database.rs`.

# Questions to answer, each with file:line evidence from p1-main-ro
1. Schema rules: what does the spec or code say decides a WORLD_DATABASE_SCHEMA_VERSION bump? Did adding `waters` (or any other table or column) bump it? Would three new optional tables plus a membership table need a bump, given the engine's version check (how does the engine react to a DB newer or older than it knows, and to a missing table)?
2. Where references are written: the function(s) that insert `references` rows, what parsed fields exist there (position, rotation, scale, base FormID, cell, the raw `data` blob and its trailer), and how FormIDs are remapped into load-order numbering (#136). XPOD and XLRM FormIDs inside markers would need the same remap: point to the remap helper.
3. Plugin overrides: if a mod overrides or deletes a marker or moves an object, which record wins when tables are built (does the exporter work from resolved, winning records)? Where would a per-cell membership pass run so it sees final positions?
4. Object bounds: is any per-object size available at conversion (OBND on base records, GLB bounds, a column)? Membership by pivot needs only position; say what extra exists if we wanted bounds overlap.
5. Proposed table shape: draft SQL for room_bounds, room_portals, room_links and a membership table (or column), with indexes, in the spec's style; estimate rows for Skyrim.esm (3,561 room markers, 2,612 portals, about 400k interior refs of which about 291k are in cells with rooms).
6. Tests: which existing converter tests build a tiny ESM in memory (dummy-content?) that a room-marker test could extend; name the helper.

# Scope
Read only. Change nothing. No builds.

# Report
Under 600 words: numbered answers 1-6 with file:line evidence, the draft SQL, and a one-line recommendation on the schema bump. Mark anything you could not confirm.
