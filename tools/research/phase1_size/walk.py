"""Walk the converted-install tree once and cache the listing as TSV.

Usage: python tools/research/phase1_size/walk.py [root]

Writes one TSV per top-level area under local/research/phase1_size/:
    relpath \t size \t mtime
Read-only: never opens anything for writing inside the asset tree.
"""
import os
import sys
import time

DEFAULT_ROOT = r"C:/Modding/SkyrimConverted-upstream"
OUT = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size"

AREAS = ["vfs", ".ingestion-cache", "textures", "meshes", "scripts"]


def walk_area(root, area):
    base = os.path.join(root, area)
    rows = []
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames.sort()
        for name in filenames:
            full = os.path.join(dirpath, name)
            try:
                st = os.stat(full)
            except OSError as exc:
                rows.append((os.path.relpath(full, base).replace("\\", "/"), -1, str(exc)))
                continue
            rel = os.path.relpath(full, base).replace("\\", "/")
            rows.append((rel, st.st_size, int(st.st_mtime)))
    return rows


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_ROOT
    os.makedirs(OUT, exist_ok=True)
    for area in AREAS:
        t0 = time.time()
        base = os.path.join(root, area)
        if not os.path.isdir(base):
            print("MISSING", area)
            continue
        rows = walk_area(root, area)
        total = sum(r[1] for r in rows if r[1] > 0)
        out = os.path.join(OUT, area.replace(".", "") + ".tsv")
        with open(out, "w", encoding="utf-8", newline="\n") as fh:
            for rel, size, mtime in rows:
                fh.write("%s\t%d\t%d\n" % (rel, size, mtime))
        print("%-18s files=%7d bytes=%14d %.2f GB  (%.1fs) -> %s"
              % (area, len(rows), total, total / 1e9, time.time() - t0, out))


if __name__ == "__main__":
    main()
