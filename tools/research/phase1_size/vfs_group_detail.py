"""Who made the copies? Archive breakdown, failures, times for the copy groups.

Usage: python tools/research/phase1_size/vfs_group_detail.py
"""
import json
import os
import time
from collections import defaultdict

ROOT = r"D:/Modding-overflow/p1-timing/SkyrimConverted"
VFS = os.path.join(ROOT, "vfs")
CACHE = os.path.join(ROOT, ".ingestion-cache")


def main():
    with open(os.path.join(ROOT, "conversion-manifest.json"), encoding="utf-8") as fh:
        manifest = json.load(fh)

    failures = manifest.get("failures", {})
    print("failures=%d; keys by suffix:" % len(failures))
    kinds = defaultdict(int)
    for key in failures:
        kinds[os.path.splitext(key)[1]] += 1
    print("  %s" % dict(kinds))
    for key, value in list(failures.items())[:3]:
        print("  e.g. %s -> %s" % (key, value[:160]))
    print("failures naming an archive: %s"
          % [k for k in failures if k.lower().endswith((".bsa", ".ba2"))][:10])

    hash_paths = defaultdict(set)
    path_archives = defaultdict(list)
    for key, entry in manifest["archives"].items():
        for file in entry["files"]:
            hash_paths[file["hash"]].add(file["path"])
            path_archives[file["path"]].append(key)

    def stat_names(names):
        rows = []
        for rel in names:
            full = os.path.join(VFS, rel.replace("/", os.sep))
            st = os.stat(full)
            rows.append((rel, st.st_nlink, st.st_mtime, st.st_ctime))
        return rows

    groups = [(h, sorted(p)) for h, p in hash_paths.items() if len(p) > 400]
    groups.sort(key=lambda g: -len(g[1]))
    for digest, paths in groups:
        blob = os.path.join(CACHE, "sha256", digest[:2], digest)
        blob_stat = os.stat(blob) if os.path.isfile(blob) else None
        stats = stat_names(paths[:4] + paths[-4:])
        linked = [r for r in stat_names(paths) if r[1] > 1]
        copies = [r for r in stat_names(paths) if r[1] == 1]
        print("\ngroup %s... size=%d linked=%d copies=%d" % (digest[:12], len(paths), len(linked), len(copies)))
        if blob_stat:
            print("  blob nlink=%d mtime=%s ctime=%s" % (
                blob_stat.st_nlink, time.ctime(blob_stat.st_mtime), time.ctime(blob_stat.st_ctime)))
        for kind, rows in (("copy", copies[:3]), ("linked", linked[:3])):
            for rel, nlink, mtime, ctime in rows:
                print("  %-6s nlink=%d mtime=%s ctime=%s %s (%s)" % (
                    kind, nlink, time.ctime(mtime), time.ctime(ctime), rel,
                    ",".join(path_archives[rel])))
        linked_archives = defaultdict(int)
        for rel, _n, _m, _c in linked:
            for key in path_archives[rel]:
                linked_archives[key] += 1
        copy_archives = defaultdict(int)
        for rel, _n, _m, _c in copies:
            for key in path_archives[rel]:
                copy_archives[key] += 1
        print("  archives of linked: %s" % dict(linked_archives))
        print("  archives of copies: %s" % dict(copy_archives))


if __name__ == "__main__":
    main()
