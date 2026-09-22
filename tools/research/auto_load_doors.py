#!/usr/bin/env python3
"""Census of load doors in the converted database, for impl-016 (auto-load doors).

Read-only (`mode=ro`). Answers: how many load doors exist, how many match each candidate
"auto-load" rule (base editor id `AutoLoadDoor*`, base model path `%AutoLoadMarker%`), whether
the two rules agree, and which base objects the unmatched doors use - so the matching rule can be
chosen from the data instead of guessed.

Usage:
    python tools/research/auto_load_doors.py
    python tools/research/auto_load_doors.py bases [limit]     # every load-door base, by count
    python tools/research/auto_load_doors.py route             # the four Blackreach demo doors
    python tools/research/auto_load_doors.py flags             # DOOR FNAM vs the editor-id rule
    python tools/research/auto_load_doors.py probe|dump [n]    # the `records.data` blob layout

What it found (2026-09-22, `C:/Modding/SkyrimConverted/skyrim_world.db`):

* 2204 placed load doors, every one with a resolved destination and a `statics` base row;
  315 of them are auto-load doors - 289 `AutoLoadDoor01`, 21 `AutoLoadDoorMinUse01` and
  5 `AutoLoadDoorHiddenMinUse01`, all with the `AutoLoadMarker01.nif` model.
* The editor-id rule (`AutoLoadDoor%`) and the model rule (`%AutoLoadMarker%`, case-insensitive)
  each match those same 315: nothing matched one rule without the other.
* Skyrim's own "automatic door" flag is the `FNAM` byte of the `DOOR` record, bit `0x02`:
  `AutoLoadDoor01` is `0x02`, `AutoLoadDoorMinUse01` `0x0A`, `AutoLoadDoorHiddenMinUse01` `0x0E`
  (`0x04` hidden, `0x08` minimal use, per their editor ids), and every other `DOOR` record in the
  database has bit `0x02` clear - including the ordinary doors that carry `0x08`/`0x0C` for their
  own `MinUse`/`Hidden` ids (`RiftenRWDoorLoad01hiddenMinUse`, `TrapdoorLadder01HiddenMinUse`...).
  So the rule the engine uses selects exactly the flagged records.
* That flag is *not* in the database the engine reads: the converter exports only
  `statics(id, editor_id, model_path, flags, bounds...)` and `flags` is the record header, not
  `FNAM` (`AutoLoadDoor01` has `statics.flags` = 0). `FNAM` survives only inside the rkyv blob in
  `records.data`, which nothing reads at runtime. Preferring the flag would mean a converter change
  (export it into `statics`), so the engine uses the editor-id/model pair, which the data shows is
  equivalent.
"""

import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"

# The demo route's doors, exactly as the tour crosses them (`ALFTAND_ROUTE` in
# `crates/engine/src/demo_tour.rs`): the Alftand entrance and the three doors after it.
ROUTE = [0x0001_5D48, 0x0009_2809, 0x0009_256A, 0x0006_998D]

EDITOR_ID_RULE = "s.editor_id LIKE 'AutoLoadDoor%'"
MODEL_RULE = "LOWER(COALESCE(s.model_path,'')) LIKE '%autoloadmarker%'"
ANY_RULE = f"({EDITOR_ID_RULE} OR {MODEL_RULE})"


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def census(conn):
    total = conn.execute("SELECT COUNT(*) FROM door_links").fetchone()[0]
    resolved = conn.execute(
        "SELECT COUNT(*) FROM door_links WHERE destination_cell_id IS NOT NULL"
    ).fetchone()[0]
    placed = conn.execute(
        "SELECT COUNT(*) FROM door_links d JOIN \"references\" r ON r.id = d.ref_id"
    ).fetchone()[0]
    print(f"door_links rows {total}, of which placed references {placed}, resolved {resolved}")

    missing_base = conn.execute(
        "SELECT COUNT(*) FROM door_links d JOIN \"references\" r ON r.id = d.ref_id "
        "LEFT JOIN statics s ON s.id = r.base_form_id WHERE s.id IS NULL"
    ).fetchone()[0]
    print(f"load doors whose base has no statics row: {missing_base}")

    for name, rule in (
        ("editor_id LIKE 'AutoLoadDoor%'", EDITOR_ID_RULE),
        ("model_path has 'autoloadmarker' (ci)", MODEL_RULE),
        ("either rule", ANY_RULE),
    ):
        count = conn.execute(
            f"SELECT COUNT(*) FROM door_links d JOIN \"references\" r ON r.id = d.ref_id "
            f"LEFT JOIN statics s ON s.id = r.base_form_id WHERE {rule}"
        ).fetchone()[0]
        print(f"  {count:6d} load doors match {name}")

    both = conn.execute(
        f"SELECT COUNT(*) FROM door_links d JOIN \"references\" r ON r.id = d.ref_id "
        f"LEFT JOIN statics s ON s.id = r.base_form_id "
        f"WHERE {EDITOR_ID_RULE} AND {MODEL_RULE}"
    ).fetchone()[0]
    print(f"  {both:6d} load doors match both rules")

    other = conn.execute(
        "SELECT COUNT(*) FROM door_links d JOIN \"references\" r ON r.id = d.ref_id "
        "WHERE r.base_form_id IN (SELECT id FROM statics WHERE editor_id LIKE 'AutoLoadDoor%') "
        "AND r.base_form_id NOT IN "
        "(SELECT id FROM statics WHERE LOWER(COALESCE(model_path,'')) LIKE '%autoloadmarker%')"
    ).fetchone()[0]
    print(f"  {other:6d} load doors have an AutoLoadDoor* editor id but a different model")

    # The other direction: a model that says marker but a name that does not.
    reverse = conn.execute(
        "SELECT COUNT(*) FROM door_links d JOIN \"references\" r ON r.id = d.ref_id "
        "WHERE r.base_form_id IN "
        "(SELECT id FROM statics WHERE LOWER(COALESCE(model_path,'')) LIKE '%autoloadmarker%') "
        "AND r.base_form_id NOT IN (SELECT id FROM statics WHERE editor_id LIKE 'AutoLoadDoor%')"
    ).fetchone()[0]
    print(f"  {reverse:6d} load doors have an AutoLoadMarker model but another editor id")

    # Every other base a load door uses, i.e. the doors that stay on the E key. Printed grouped
    # so a pattern that also means auto-load would show up as a common editor-id stem.
    print("load-door bases that match no rule (editor id stem, model, count):")
    for row in conn.execute(
        "SELECT COUNT(*), s.editor_id, s.model_path FROM door_links d "
        "JOIN \"references\" r ON r.id = d.ref_id LEFT JOIN statics s ON s.id = r.base_form_id "
        f"WHERE NOT {ANY_RULE} GROUP BY r.base_form_id ORDER BY COUNT(*) DESC LIMIT 25"
    ):
        print(f"  {row[0]:6d}  {str(row[1])!r:34} {row[2]}")

    print("matched bases (editor_id, model_path, door count):")
    for row in conn.execute(
        "SELECT s.editor_id, s.model_path, COUNT(*) FROM door_links d "
        "JOIN \"references\" r ON r.id = d.ref_id LEFT JOIN statics s ON s.id = r.base_form_id "
        f"WHERE {ANY_RULE} GROUP BY s.id ORDER BY COUNT(*) DESC"
    ):
        print(f"  {row[2]:6d}  {row[0]!r:30} {row[1]}")


def bases(conn, limit):
    print(f"every base of a placed load door, most placed first (top {limit}):")
    for row in conn.execute(
        "SELECT s.editor_id, s.model_path, COUNT(*), s.flags FROM door_links d "
        "JOIN \"references\" r ON r.id = d.ref_id LEFT JOIN statics s ON s.id = r.base_form_id "
        "GROUP BY r.base_form_id ORDER BY COUNT(*) DESC LIMIT ?",
        (limit,),
    ):
        print(f"  {row[2]:6d}  {str(row[0])!r:34} {str(row[1])!r:60} flags {row[3]}")


def route(conn):
    print("the demo route's doors:")
    for ref_id in ROUTE:
        row = conn.execute(
            "SELECT r.id, r.cell_id, r.base_form_id, s.editor_id, s.model_path, "
            "COALESCE(s.bounds_valid,0), r.pos_x, r.pos_y, r.pos_z "
            "FROM \"references\" r LEFT JOIN statics s ON s.id = r.base_form_id "
            "WHERE r.id = ?",
            (ref_id,),
        ).fetchone()
        door = conn.execute(
            "SELECT destination_cell_id, destination_worldspace_id FROM door_links WHERE ref_id = ?",
            (ref_id,),
        ).fetchone()
        if row is None:
            print(f"  {ref_id:08X}: not in the database")
            continue
        print(
            f"  {ref_id:08X} cell {row[1]:08X} base {row[2]:08X} {row[3]!r} {row[4]} "
            f"bounds_valid {row[5]} at ({row[6]:.0f}, {row[7]:.0f}, {row[8]:.0f}) "
            f"-> cell {door[0] if door else None} world {door[1] if door else None}"
        )


# ---------------------------------------------------------------------------------------------
# The DOOR records' own subrecords, straight out of the `records` blob.
#
# `records.data` is the rkyv-serialized `ArchivedRecordData` from
# `crates/converter/src/esm/types.rs`: a `Vec<ArchivedSubRecord>`, each `{ tag: [u8; 4],
# data: Vec<u8> }`. rkyv puts the root at the end of the buffer, and an `ArchivedVec` is a
# relative pointer (i32, relative to the pointer itself) plus a `usize` length - so the layout
# below is sizeof(ArchivedSubRecord) = 24 with the pointer at +8 and the length at +16. The
# decoder checks itself against EDID/MODL, which have to read as text, before trusting FNAM.
# ---------------------------------------------------------------------------------------------

SUB_RECORD_SIZE = 12


def subrecords(blob):
    """The subrecords of one `records.data` blob, or `None` when it is not the shape assumed here
    (which is checked, because a wrong length would otherwise read the whole address space).

    Read off the bytes (`dump`): the root `ArchivedRecordData` is the last 8 bytes, a relative
    pointer and a length, and each `ArchivedSubRecord` is 12 bytes - tag, relative pointer (from
    its own address), length. The pointers are 32 bits, so the workspace builds rkyv with
    `pointer_width_32`; `crates/converter/src/esm/types.rs` is the source of the shape.
    """
    if len(blob) < SUB_RECORD_SIZE + 8:
        return None
    root = len(blob) - 8
    offset = int.from_bytes(blob[root : root + 4], "little", signed=True)
    count = int.from_bytes(blob[root + 4 : root + 8], "little")
    array = root + offset
    if not 0 <= count <= 4096 or not 0 <= array <= len(blob) - count * SUB_RECORD_SIZE:
        return None
    records = []
    for index in range(count):
        base = array + index * SUB_RECORD_SIZE
        tag = blob[base : base + 4]
        if not all(32 <= byte < 127 for byte in tag):
            return None
        relative = int.from_bytes(blob[base + 4 : base + 8], "little", signed=True)
        length = int.from_bytes(blob[base + 8 : base + 12], "little")
        at = base + 4 + relative
        if not 0 <= at or at + length > len(blob):
            return None
        records.append((tag, blob[at : at + length]))
    return records


def probe(conn):
    """Which DOOR blobs the decoder above can and cannot read, with the shape of the failures."""
    good = 0
    bad = []
    for form_id, blob in conn.execute(
        "SELECT r.form_id, r.data FROM records r JOIN formid_map m ON m.form_id = r.form_id "
        "WHERE m.record_type = 'DOOR' LIMIT 400"
    ):
        if subrecords(blob) is None:
            root = len(blob) - 16
            bad.append((form_id, len(blob), blob[root : root + 16].hex()))
        else:
            good += 1
    print(f"decoded {good} DOOR blobs, {len(bad)} did not match the assumed layout")
    for form_id, size, root in bad[:10]:
        print(f"  {form_id:08X} size {size} root bytes {root}")


def record_subrecords(conn, form_id):
    row = conn.execute("SELECT data FROM records WHERE form_id = ?", (form_id,)).fetchone()
    return None if row is None else subrecords(row[0])


def self_check(conn):
    """Prove the decoder before using it: an EDID must read as printable text."""
    checked = 0
    bad = 0
    for (blob,) in conn.execute("SELECT data FROM records WHERE record_type = 'DOOR' LIMIT 200"):
        for tag, data in subrecords(blob):
            text = data.split(b"\0")[0]
            if tag in (b"EDID", b"MODL"):
                checked += 1
                if not text or any(byte < 32 or byte > 126 for byte in text):
                    bad += 1
    print(f"decoder self-check: {checked} EDID/MODL subrecords, {bad} not printable text")
    return bad == 0


def fnam(conn):
    if not self_check(conn):
        print("decoder layout is wrong; not trusting the flags below")
        return
    # What every DOOR record's flags say, cross-tabulated with the editor-id rule.
    census = {}
    for form_id, blob in conn.execute("SELECT form_id, data FROM records WHERE record_type = 'DOOR'"):
        editor_id = None
        flags = None
        for tag, data in subrecords(blob):
            if tag == b"EDID":
                editor_id = data.split(b"\0")[0].decode("latin-1")
            elif tag == b"FNAM" and data:
                flags = data[0]
        key = (flags, None if editor_id is None else editor_id.startswith("AutoLoadDoor"))
        census.setdefault(key, []).append((form_id, editor_id))
    print("DOOR FNAM byte -> count, for records whose EDID starts with AutoLoadDoor or not:")
    for (flags, is_auto), rows in sorted(census.items(), key=lambda item: (item[0][0] or 0, item[0][1])):
        stem_counts = {}
        for _, editor_id in rows:
            stem_counts[editor_id] = stem_counts.get(editor_id, 0) + 1
        top = sorted(stem_counts.items(), key=lambda item: -item[1])[:6]
        print(
            f"  FNAM {flags if flags is None else hex(flags):>6}  AutoLoadDoor*={is_auto}  "
            f"{len(rows):5d} records  e.g. {top}"
        )
    print("named bases of interest:")
    for editor_id in (
        "AutoLoadDoor01",
        "AutoLoadDoorMinUse01",
        "AutoLoadDoorHiddenMinUse01",
        "DweDoorLarge01Load",
        "NorDoorSmLoad01",
        "FarmhouseLDoor01",
    ):
        row = conn.execute(
            "SELECT form_id, data FROM records WHERE record_type = 'DOOR' AND "
            "data IS NOT NULL AND form_id IN (SELECT id FROM statics WHERE editor_id = ?)",
            (editor_id,),
        ).fetchone()
        if row is None:
            print(f"  {editor_id}: no DOOR record with that EDID")
            continue
        pairs = [(tag.decode("latin-1"), len(data)) for tag, data in subrecords(row[1])]
        flags = [data for tag, data in subrecords(row[1]) if tag == b"FNAM"]
        value = flags[0][0] if flags and flags[0] else None
        print(
            f"  {editor_id} ({row[0]:08X}) FNAM "
            f"{value if value is None else hex(value)} subrecords {pairs}"
        )


def flags(conn):
    """The DOOR records' own `FNAM`, reached the fast way: `formid_map` lists the record ids by
    type (small rows) and `records.id` is the rowid, so fetching a blob is one lookup. Scanning
    `records` by `record_type` instead reads every blob in a 1.3 GB database."""
    form_ids = [
        row[0] for row in conn.execute("SELECT form_id FROM formid_map WHERE record_type = 'DOOR'")
    ]
    print(f"{len(form_ids)} DOOR records listed in formid_map")
    census = {}
    for start in range(0, len(form_ids), 500):
        chunk = form_ids[start : start + 500]
        query = (
            "SELECT form_id, data FROM records WHERE form_id IN ("
            + ",".join("?" * len(chunk))
            + ")"
        )
        for form_id, blob in conn.execute(query, chunk):
            editor_id = None
            value = None
            for tag, data in subrecords(blob):
                if tag == b"EDID":
                    editor_id = data.split(b"\0")[0].decode("latin-1")
                elif tag == b"FNAM" and data:
                    value = data[0]
            key = (value, None if editor_id is None else editor_id.startswith("AutoLoadDoor"))
            census.setdefault(key, []).append((form_id, editor_id))
    print("DOOR FNAM byte -> how many records, and whether their EDID starts with AutoLoadDoor:")
    for (value, is_auto), rows in sorted(
        census.items(),
        key=lambda item: (
            item[0][0] is None,
            item[0][0] or 0,
            item[0][1] is None,
            bool(item[0][1]),
        ),
    ):
        stems = {}
        for _, editor_id in rows:
            stems[editor_id] = stems.get(editor_id, 0) + 1
        top = sorted(stems.items(), key=lambda item: -item[1])[:6]
        print(
            f"  FNAM {value if value is None else hex(value):>6}  AutoLoadDoor*={is_auto}  "
            f"{len(rows):5d} records  e.g. {top}"
        )
    print("named bases of interest:")
    for editor_id in (
        "AutoLoadDoor01",
        "AutoLoadDoorMinUse01",
        "AutoLoadDoorHiddenMinUse01",
        "DweDoorLarge01Load",
        "NorDoorSmLoad01",
        "FarmhouseLDoor01",
    ):
        row = conn.execute(
            "SELECT form_id, data FROM records WHERE form_id IN "
            "(SELECT id FROM statics WHERE editor_id = ?)",
            (editor_id,),
        ).fetchone()
        if row is None:
            print(f"  {editor_id}: no DOOR record with that EDID")
            continue
        pairs = [(tag.decode("latin-1"), len(data)) for tag, data in subrecords(row[1])]
        values = [data for tag, data in subrecords(row[1]) if tag == b"FNAM"]
        value = values[0][0] if values and values[0] else None
        print(
            f"  {editor_id} ({row[0]:08X}) FNAM "
            f"{value if value is None else hex(value)} subrecords {pairs}"
        )


def near(conn, centre, radius):
    """Every auto-load door within `radius` Creation units of a point, nearest first: what a player
    walking away from that point could walk into."""
    rows = []
    for row in conn.execute(
        "SELECT r.id, r.pos_x, r.pos_y, r.pos_z, s.editor_id, r.cell_id FROM door_links d "
        "JOIN \"references\" r ON r.id = d.ref_id LEFT JOIN statics s ON s.id = r.base_form_id "
        f"WHERE {ANY_RULE}"
    ):
        position = (row[1], row[2], row[3])
        distance = sum((a - b) ** 2 for a, b in zip(position, centre)) ** 0.5
        if distance <= radius:
            rows.append((distance, row))
    print(f"{len(rows)} auto-load doors within {radius:.0f} units of {centre}:")
    for distance, row in sorted(rows):
        print(
            f"  {distance:8.0f}  door {row[0]:08X} cell {row[5]:08X} {row[4]!r} "
            f"at ({row[1]:.0f}, {row[2]:.0f}, {row[3]:.0f})"
        )


def dump(conn, count):
    """Raw hex of the smallest DOOR blobs, to read the archived layout off the bytes."""
    rows = conn.execute(
        "SELECT r.form_id, r.data FROM records r JOIN formid_map m ON m.form_id = r.form_id "
        "WHERE m.record_type = 'DOOR' ORDER BY LENGTH(r.data) LIMIT ?",
        (count,),
    ).fetchall()
    for form_id, blob in rows:
        print(f"{form_id:08X} size {len(blob)}")
        for offset in range(0, len(blob), 16):
            chunk = blob[offset : offset + 16]
            text = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in chunk)
            print(f"  {offset:4d}  {chunk.hex(' '):<47}  {text}")


def main(argv):
    conn = connect()
    if len(argv) > 1 and argv[1] == "bases":
        bases(conn, int(argv[2]) if len(argv) > 2 else 40)
    elif len(argv) > 1 and argv[1] == "route":
        route(conn)
    elif len(argv) > 1 and argv[1] == "fnam":
        fnam(conn)
    elif len(argv) > 1 and argv[1] == "flags":
        flags(conn)
    elif len(argv) > 1 and argv[1] == "probe":
        probe(conn)
    elif len(argv) > 1 and argv[1] == "dump":
        dump(conn, int(argv[2]) if len(argv) > 2 else 1)
    elif len(argv) > 1 and argv[1] == "near":
        near(
            conn,
            (float(argv[2]), float(argv[3]), float(argv[4])),
            float(argv[5]) if len(argv) > 5 else 1000.0,
        )
    else:
        census(conn)
    return 0



if __name__ == "__main__":
    sys.exit(main(sys.argv))
