"""Size breakdown of the converted install from the cached listings.

Usage: python tools/research/phase1_size/breakdown.py
Read-only; reads only the TSVs in local/research/phase1_size/.
"""
import os
from collections import defaultdict

OUT = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size"
ROOT = r"C:/Modding/SkyrimConverted-upstream"
FILES = {
    "skyrim_world.db": None,
    "cell_cache.rkyv": None,
    "conversion-manifest.json": None,
    "integration-report.json": None,
}
AREAS = [("vfs", "vfs.tsv"), (".ingestion-cache", "ingestion-cache.tsv"),
         ("textures", "textures.tsv"), ("meshes", "meshes.tsv"), ("scripts", "scripts.tsv")]


def load(name):
    rows = []
    with open(os.path.join(OUT, name), encoding="utf-8") as fh:
        for line in fh:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 2:
                continue
            rows.append((parts[0], int(parts[1])))
    return rows


def ext_of(rel):
    base = rel.rsplit("/", 1)[-1]
    return base.rsplit(".", 1)[-1].lower() if "." in base else "(none)"


def main():
    grand = 0
    print("== top level ==")
    for f, _ in FILES.items():
        size = os.path.getsize(os.path.join(ROOT, f))
        grand += size
        print("%-28s %14d  %7.2f GB" % (f, size, size / 1e9))
    for area, tsv in AREAS:
        rows = load(tsv)
        total = sum(s for _, s in rows)
        grand += total
        print("%-28s %14d  %7.2f GB  (%d files)" % (area, total, total / 1e9, len(rows)))
        if area == "vfs":
            per_top = defaultdict(lambda: [0, 0])
            for rel, size in rows:
                top = rel.split("/")[0]
                per_top[top][0] += 1
                per_top[top][1] += size
            for top, (n, s) in sorted(per_top.items(), key=lambda kv: -kv[1][1]):
                print("    vfs/%-16s %14d  %7.2f GB  (%d files)" % (top, s, s / 1e9, n))
        if area in ("vfs", ".ingestion-cache"):
            per_ext = defaultdict(lambda: [0, 0])
            for rel, size in rows:
                e = ext_of(rel)
                per_ext[e][0] += 1
                per_ext[e][1] += size
            print("    -- by extension (top 15) --")
            for e, (n, s) in sorted(per_ext.items(), key=lambda kv: -kv[1][1])[:15]:
                print("    %-12s %14d  %7.2f GB  (%d files)" % (e, s, s / 1e9, n))
    print("%-28s %14d  %7.2f GB" % ("TOTAL (sum of parts)", grand, grand / 1e9))
    print()
    print("== textures/ by extension ==")
    rows = load("textures.tsv")
    per_ext = defaultdict(lambda: [0, 0])
    for rel, size in rows:
        e = ext_of(rel)
        per_ext[e][0] += 1
        per_ext[e][1] += size
    for e, (n, s) in sorted(per_ext.items(), key=lambda kv: -kv[1][1]):
        print("    %-20s %14d  %7.2f GB  (%d files)" % (e, s, s / 1e9, n))


if __name__ == "__main__":
    main()
