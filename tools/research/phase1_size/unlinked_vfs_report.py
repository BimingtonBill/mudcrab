"""Why are some vfs/ files separate copies of their cache blob, not hard links?

Usage: python tools/research/phase1_size/unlinked_vfs_report.py

Reads the conversion manifest and the published tree at ROOT, finds every vfs/
file with link count 1 whose bytes also exist as a `.ingestion-cache` blob, and
classifies it: which archive(s) list its path, whether several paths share the
blob, and whether the file's bytes are the ones the manifest recorded. Read-only.

Writes local/research/phase1_size/unlinked_vfs.tsv (the file list) and prints
the summary.
"""
import hashlib
import json
import os
import time
from collections import Counter, defaultdict

ROOT = r"D:/Modding-overflow/p1-timing/SkyrimConverted"
VFS = os.path.join(ROOT, "vfs")
CACHE = os.path.join(ROOT, ".ingestion-cache")
OUT = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/unlinked_vfs.tsv"

DATA = r"C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data"


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        while True:
            chunk = fh.read(1 << 20)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def loose_paths():
    """Lower-cased relative paths of every file in the game Data folder."""
    found = set()
    for d, _dirs, files in os.walk(DATA):
        for name in files:
            full = os.path.join(d, name)
            found.add(os.path.relpath(full, DATA).replace("\\", "/").lower())
    return found


def main():
    t0 = time.time()
    with open(os.path.join(ROOT, "conversion-manifest.json"), encoding="utf-8") as fh:
        manifest = json.load(fh)
    print("manifest: schema=%s complete=%s failures=%d archives=%d entries=%d"
          % (manifest.get("schema_version"), manifest.get("complete"),
             len(manifest.get("failures", {})), len(manifest["archives"]),
             len(manifest.get("entries", {}))))
    if manifest.get("failures"):
        for key in list(manifest["failures"])[:5]:
            print("   failure: %s" % key)

    # path -> [(archive_key, hash, size)] in manifest order (archive load order)
    listed = defaultdict(list)
    for archive_key, entry in manifest["archives"].items():
        for file in entry["files"]:
            listed[file["path"]].append((archive_key, file["hash"], file["size"]))
    print("manifest lists %d distinct vfs paths, %d path entries"
          % (len(listed), sum(len(v) for v in listed.values())))

    # blobs present, and how many vfs paths claim each hash
    paths_per_hash = Counter()
    for path, entries in listed.items():
        for _archive, digest, _size in entries:
            paths_per_hash[digest] += 1

    unlinked, linked, other_nlink = [], 0, Counter()
    for d, _dirs, files in os.walk(VFS):
        for name in files:
            full = os.path.join(d, name)
            st = os.stat(full)
            if st.st_nlink != 1:
                linked += 1
                other_nlink[st.st_nlink] += 1
                continue
            rel = os.path.relpath(full, VFS).replace("\\", "/")
            unlinked.append((rel, st.st_size, full))
    print("vfs: %d files with nlink 1, %d with nlink > 1" % (len(unlinked), linked))
    print("nlink histogram (linked files): %s" % sorted(other_nlink.items())[:8])

    loose = loose_paths()
    print("Data folder holds %d loose files" % len(loose))

    rows = []
    stats = Counter()
    archives_of_unlinked = Counter()
    shared_with_blob_count = Counter()
    dirs = Counter()
    examples_by_class = defaultdict(list)
    t_hash = time.time()
    for rel, size, full in unlinked:
        digest = sha256_file(full)
        blob = os.path.join(CACHE, "sha256", digest[:2], digest)
        blob_exists = os.path.isfile(blob)
        blob_nlink = os.stat(blob).st_nlink if blob_exists else 0
        entries = listed.get(rel, [])
        archives = [a for a, _h, _s in entries]
        hashes = [h for _a, h, _s in entries]
        if not entries:
            klass = "not-in-manifest"
        elif digest not in hashes:
            klass = "bytes-differ-from-manifest"
        elif len(entries) > 1 and len(set(hashes)) > 1:
            klass = "overridden-with-different-bytes"
        elif len(entries) > 1:
            klass = "listed-by-several-archives"
        else:
            klass = "listed-by-one-archive"
        if not blob_exists:
            klass += "+no-blob"
        else:
            klass += "+blob"
        stats[klass] += 1
        stats["loose-in-data" if rel.lower() in loose else "not-loose"] += 1
        for a in archives:
            archives_of_unlinked[a] += 1
        shared_with_blob_count[paths_per_hash.get(digest, 0)] += 1
        dirs["/".join(rel.split("/")[:2])] += 1
        if len(examples_by_class[klass]) < 2:
            examples_by_class[klass].append(rel)
        rows.append("%s\t%d\t%s\t%s\t%s\t%d\t%d" % (
            rel, size, klass, ",".join(archives[:3]), digest, blob_exists, blob_nlink))

    print("hashing took %.0fs" % (time.time() - t_hash))
    print("\nclassification:")
    for k, v in stats.most_common():
        print("  %-45s %d" % (k, v))
    print("\narchives listing the unlinked paths (path entries, top 15):")
    for k, v in archives_of_unlinked.most_common(15):
        print("  %-45s %d" % (k, v))
    print("\nvfs paths sharing the unlinked file's content hash (from manifest):")
    for k in sorted(shared_with_blob_count)[:8]:
        print("  %d path(s): %d files" % (k, shared_with_blob_count[k]))
    print("\ntop vfs subfolders: %s" % dirs.most_common(10))
    print("\nexamples:")
    for k, v in examples_by_class.items():
        print("  %s: %s" % (k, v))

    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("path\tsize\tclass\tarchives\tsha256\tblob_exists\tblob_nlink\n")
        fh.write("\n".join(rows) + "\n")
    print("\nwrote %s (%.0fs total)" % (OUT, time.time() - t0))


if __name__ == "__main__":
    main()
