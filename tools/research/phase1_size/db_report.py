"""SQLite world-database size report (read-only).

Usage: python tools/research/phase1_size/db_report.py
"""
import os
import sqlite3

DB = "C:/Modding/SkyrimConverted-upstream/skyrim_world.db"


def main():
    size = os.path.getsize(DB)
    con = sqlite3.connect("file:%s?mode=ro" % DB, uri=True)
    cur = con.cursor()
    for pragma in ("page_size", "page_count", "freelist_count", "auto_vacuum", "encoding"):
        print("%-16s %s" % (pragma, cur.execute("PRAGMA %s" % pragma).fetchone()[0]))
    page_size = cur.execute("PRAGMA page_size").fetchone()[0]
    page_count = cur.execute("PRAGMA page_count").fetchone()[0]
    free = cur.execute("PRAGMA freelist_count").fetchone()[0]
    print("file bytes      %d  (%.2f GB)" % (size, size / 1e9))
    print("pages*size      %d  (%.2f GB)" % (page_count * page_size, page_count * page_size / 1e9))
    print("freelist bytes  %d  (%.2f GB) -> VACUUM could return these"
          % (free * page_size, free * page_size / 1e9))
    print("free%%           %.1f%%" % (100.0 * free / page_count))

    print()
    print("== objects ==")
    objs = cur.execute(
        "SELECT type,name,rootpage FROM sqlite_master WHERE type IN ('table','index') "
        "ORDER BY rootpage"
    ).fetchall()
    print("tables+indexes:", len(objs))
    # Approximate size: the span from this object's root page to the next
    # allocated root page. Exact for a freshly built database (no free pages).
    for i, (otype, name, root) in enumerate(objs):
        nxt = objs[i + 1][2] if i + 1 < len(objs) else page_count + 1
        span = max(0, nxt - root) * page_size
        print("%-8s %-34s rootpage=%-8d approx=%12d (%6.1f MB) %5.2f%%"
              % (otype, name, root, span, span / 1e6, 100.0 * span / (page_count * page_size)))

    print()
    print("== row counts ==")
    counts = []
    for otype, name, _root in objs:
        if otype != "table" or name.startswith("sqlite_"):
            continue
        try:
            n = cur.execute("SELECT COUNT(*) FROM \"%s\"" % name).fetchone()[0]
        except sqlite3.OperationalError:
            continue
        counts.append((name, n))
    for name, n in sorted(counts, key=lambda kv: -kv[1]):
        print("%-32s %10d" % (name, n))
    con.close()


if __name__ == "__main__":
    main()
