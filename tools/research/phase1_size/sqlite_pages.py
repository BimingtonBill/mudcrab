"""Per-table page/byte attribution by walking each SQLite b-tree (read-only).

Usage: python tools/research/phase1_size/sqlite_pages.py

dbstat is not compiled into Python's bundled SQLite, and the caller's tables were
created up front and grown afterwards, so root-page order says nothing about size.
This walks every b-tree from its root page and counts the pages it owns.
Overflow pages are not attributed (they are reported as the remainder).
"""
import os
import sqlite3
import struct

DB = "C:/Modding/SkyrimConverted-upstream/skyrim_world.db"


def varint(buf, off):
    value = 0
    for i in range(9):
        byte = buf[off + i]
        if i == 8:
            return (value << 8) | byte, off + 9
        value = (value << 7) | (byte & 0x7F)
        if not byte & 0x80:
            return value, off + i + 1
    return value, off + 9


def walk(data, page_size, root, page_count):
    """Return the set of page numbers reachable from root."""
    seen = set()
    stack = [root]
    while stack:
        page = stack.pop()
        if page in seen or page < 1 or page > page_count:
            continue
        seen.add(page)
        base = (page - 1) * page_size
        hdr = base + (100 if page == 1 else 0)
        ptype = data[hdr]
        ncell = struct.unpack_from(">H", data, hdr + 3)[0]
        if ptype not in (2, 5):
            continue
        cell_ptrs = hdr + 8
        for i in range(ncell):
            off = struct.unpack_from(">H", data, cell_ptrs + 2 * i)[0]
            child = struct.unpack_from(">I", data, base + off)[0]
            if child not in seen:
                stack.append(child)
    return seen


def main():
    con = sqlite3.connect("file:%s?mode=ro" % DB, uri=True)
    cur = con.cursor()
    page_size = cur.execute("PRAGMA page_size").fetchone()[0]
    page_count = cur.execute("PRAGMA page_count").fetchone()[0]
    objs = cur.execute("SELECT type,name,rootpage FROM sqlite_master").fetchall()
    con.close()
    data = open(DB, "rb").read()
    total = 0
    rows = []
    for otype, name, root in objs:
        if root == 0:
            continue
        pages = walk(data, page_size, root, page_count)
        total += len(pages)
        rows.append((otype, name, len(pages), len(pages) * page_size))
    print("%-8s %-36s %10s %14s %8s" % ("type", "object", "pages", "bytes", "share"))
    for otype, name, pages, bytes_ in sorted(rows, key=lambda r: -r[3])[:16]:
        print("%-8s %-36s %10d %14d %7.2f%%"
              % (otype, name, pages, bytes_, 100.0 * bytes_ / (page_count * page_size)))
    print("attributed pages %d of %d (%.1f%%); remainder %d pages (%.1f MB) = overflow pages + freelist"
          % (total, page_count, 100.0 * total / page_count, page_count - total,
             (page_count - total) * page_size / 1e6))


if __name__ == "__main__":
    main()
