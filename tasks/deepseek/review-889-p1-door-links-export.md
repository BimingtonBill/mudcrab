# Goal
Review commit 14be1bf3 on branch `phase1/door-links-export` (worktree `D:/Mudcrab/worktrees/up-doorlinks`): the converter now exports load doors to a `door_links` table in the world database. Phase 1 track, converter half of upstream issue #112 for PR #128 (go through load doors). Find what is wrong or what used to work and now fails. Read only.

# Context
- Read the diff: `git -C D:/Mudcrab/worktrees/up-doorlinks show 14be1bf3` (exporter.rs, tests/fixture_doors.rs, tests/plugin_references.rs, docs/specs/converters/db-schema.md).
- What the engine reader of PR #128 expects (from Phase 2 Dev): detection by `sqlite_master` name='door_links'; `LEFT JOIN door_links d ON d.ref_id=r.id`; columns destination_ref_id, destination_cell_id, destination_worldspace_id, pos_x/y/z, rot_x/y/z. destination_ref_id = XTEL bytes 0..4 remapped to load-order numbering like references.id; destination_cell_id = the destination reference's cell (NULL if unresolved); destination_worldspace_id NULL for an interior; pos = XTEL arrival position (Creation units), rot = XTEL rotation (radians). One row per door REFR (ref_id unique). A door is crossable if worldspace_id is not NULL, or if it is NULL and destination_cell_id is non-zero.
- The portal branch already has the same table: `C:/Modding/Mudcrab/mudcrab/crates/converter/src/esm/exporter.rs` (search `door_links`). Main's version must keep the same shape, so a later merge into portal stays clean.
- Record reference: xEdit 4.1.5f defs, `Core/wbDefinitionsTES5.pas:12344-12350`: XTEL = Door FormID [REFR], wbPosRot (3 floats position, 3 floats rotation), Flags u32. FormID remapping of XTEL: `crates/converter/src/esm/mod.rs` (search `XTEL`) in the worktree.
- No schema version bump: the table is optional and readers probe for it.

# Questions
1. Is the row content correct for the reader above (column meaning, numbering, NULL rules, units)?
2. Subset exports (`export_to_db` with a partial master over an existing database): can rows go stale or be wrong? The post-pass UPDATE runs over all rows.
3. Any cost problem on a full Skyrim conversion (about 1M references; a DELETE by primary key per reference plus the post-pass UPDATE with two correlated subqueries over ~6,000 doors)?
4. Do the tests actually discriminate (would they fail if the code were wrong)?
5. Is "no schema bump" safe for the engine on main (a DB without the table)?

# Scope
Read only. No cargo, no builds, no engines, no git writes.

# Report
Under 400 words: verdict (accept / small fixes / fix first), then findings as `path:line - issue - suggested fix`, most severe first.
