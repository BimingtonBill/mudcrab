# Goal
Find what public format documentation says about Skyrim SE's room-bound and portal subrecords on placed references (REFR), especially `XRMR` and `XLRM`, so a converter can decode them correctly. Phase 1 track, upstream issue Mudcrab-Team/mudcrab#188.

# Context
- We already measured these against Skyrim.esm bytes (our own reading, needs public confirmation):
  - `XPRM` (32 bytes): float[3] half-extents, float[3] colour, float class (0.2 room / 0.25 portal), u32 primitive type (1 box, 2 sphere, 3 portal box).
  - `XMBO` (12 bytes): float[3], byte-identical to XPRM[0..12] on room markers ("multibound extents").
  - `XPOD` (8 bytes): two u32 FormIDs of RoomMarker references (portal origin and destination).
  - `XRMR` (4 bytes) on RoomMarker references: u8 count, u8 flags (values 0x00, 0x40, 0x80, 0xC0 seen), u16 unknown (0 or 1 seen); followed by exactly `count` XLRM subrecords.
  - `XLRM` (4 bytes): u32 FormID of another RoomMarker reference in the same cell.
  - `XMBP` zero-length flag on 13 room markers; `XMBR` (4 bytes) only on exterior refs.
  - Base objects: RoomMarker 0x1F, PortalMarker 0x20, MultiBoundMarker 0x15, PlaneMarker 0x17.
- Open questions: what XLRM links mean (rooms always co-visible? shared rooms? room-to-room links without a portal marker?); what each XRMR flag bit means; what the u16 field is; what XMBP means; how the Creation Kit's room bound, portal and linked-room options map onto these fields; and how the engine assigns objects to rooms (pivot inside the box? bounds overlap?) according to the Creation Kit wiki.
- Sources to check (public docs only): UESP "Skyrim Mod:Mod File Format/REFR" and its talk page; xEdit / TES5Edit record definitions (`wbDefinitionsTES5.pas`, and `wbDefinitionsFO4.pas` for the same fields if TES5's are unnamed) on GitHub (TES5Edit/TES5Edit); Creation Kit wiki (ck.uesp.net) pages on Room Markers, Portals, Room Bounds, Occlusion, optimizing cells; modding forum or Nexus articles on room bounds.

# Clean-room rule (strict)
Public format documentation, wiki text and xEdit's record definitions only. Do NOT read, search for or cite disassembly, decompiled code, reverse-engineered engine function bodies (e.g. CommonLibSSE implementations) or leaked material. Record definitions (field names, sizes, flag names) are fine.

# Scope
Read-only. Put all findings in your final report; the lead files the note.

# Done when
- Each of XRMR, XLRM, XPRM, XMBO, XPOD, XMBP has: the public source's field layout and names, a URL (exact page, or file plus line), and whether it agrees with our byte reading above.
- XRMR's flag bits and the u16 are named if any source names them; say plainly when no source does.
- The Creation Kit's documented room-membership rule is quoted (max 15 words per quote) with URL, or marked "not documented".

# Report
Under 400 words: a table field | public layout | names | source URL | agrees with our bytes (yes/no/partly); then open questions left unresolved.
