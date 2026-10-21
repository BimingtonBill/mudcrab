"""Group the vfs paths that share one content hash and look at link counts.

Usage: python tools/research/phase1_size/vfs_group_report.py

For every content hash the manifest lists for more than one vfs path, count how
many of those paths are linked to the cache blob and how many are separate
copies, and report the blob's own link count. Read-only.
"""
import json
import os
from collections import Counter, defaultdict

ROOT = r"D:/Modding-overflow/p1-timing/SkyrimConverted"
VFS = os.path.join(ROOT, "vfs")
CACHE = os.path.join(ROOT, ".ingestion-cache")
LIST = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/unlinked_vfs.tsv"


def main():
    with open(os.path.join(ROOT, "conversion-manifest.json"), encoding="utf-8") as fh:
        manifest = json.load(fh)

    hash_paths = defaultdict(set)
    for _key, entry in manifest["archives"].items():
        for file in entry["files"]:
            hash_paths[file["hash"]].add(file["path"])

    nlink = {}
    vfs_count = 0
    for d, _dirs, files in os.walk(VFS):
        for name in files:
            full = os.path.join(d, name)
            rel = os.path.relpath(full, VFS).replace("\\", "/")
            nlink[rel] = os.stat(full).st_nlink
            vfs_count += 1

    biggest = sorted(nlink.items(), key=lambda kv: -kv[1])[:6]
    print("vfs files=%d  max nlink seen: %s" % (vfs_count, biggest))
    hist = Counter(nlink.values())
    print("nlink values >= 9 (count of vfs files): %s"
          % sorted((k, v) for k, v in hist.items() if k >= 9))

    unlinked = set()
    with open(LIST, encoding="utf-8") as fh:
        next(fh)
        for line in fh:
            unlinked.add(line.split("\t")[0])

    rows = []
    for digest, paths in hash_paths.items():
        if len(paths) < 8:
            continue
        linked = [p for p in paths if nlink.get(p, 1) > 1]
        copies = [p for p in paths if nlink.get(p, 1) == 1]
        blob = os.path.join(CACHE, "sha256", digest[:2], digest)
        blob_nlink = os.stat(blob).st_nlink if os.path.isfile(blob) else 0
        rows.append((len(paths), len(linked), len(copies), blob_nlink,
                     sorted(paths)[0], sorted(paths)[-1]))
    rows.sort(key=lambda r: (-r[0], r[4]))
    print("\ngroups (>=8 paths) of identical content: total paths, linked, copies, blob nlink")
    for total, linked, copies, blob_nlink, first, last in rows[:25]:
        print("  total=%-5d linked=%-5d copies=%-5d blob_nlink=%-5d %s"
              % (total, linked, copies, blob_nlink, first))
    print("  ... %d groups in total" % len(rows))
    print("\ncopies outside the manifest groups: %d"
          % len(unlinked - {p for paths in hash_paths.values() for p in paths}))


if __name__ == "__main__":
    main()
