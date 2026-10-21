"""Full hash pass over vfs/: how much of it is duplicated in .ingestion-cache?

Usage: python tools/research/phase1_size/vfs_vs_cache.py

Reads the cached listing (local/research/phase1_size/vfs.tsv) and hashes each
vfs file, then checks whether a blob for that hash exists in the content-addressed
store. Reports duplicated bytes (what hard-linking the two would save) and
content duplication inside vfs itself. Read-only.
"""
import hashlib
import os
import time

ROOT = r"C:/Modding/SkyrimConverted-upstream"
VFS = os.path.join(ROOT, "vfs")
CACHE = os.path.join(ROOT, ".ingestion-cache")
LIST = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/vfs.tsv"
OUT = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/vfs_hashes.tsv"


def main():
    with_blob = 0
    without_blob = 0
    missing_bytes = 0
    seen = {}
    dup_bytes = 0
    t0 = time.time()
    n = 0
    no_blob_dirs = {}
    with open(LIST, encoding="utf-8") as fh, open(OUT, "w", encoding="utf-8", newline="\n") as out:
        for line in fh:
            rel, size, _mtime = line.rstrip("\n").split("\t")
            size = int(size)
            path = os.path.join(VFS, rel.replace("/", os.sep))
            h = hashlib.sha256()
            with open(path, "rb") as src:
                while True:
                    chunk = src.read(1 << 20)
                    if not chunk:
                        break
                    h.update(chunk)
            digest = h.hexdigest()
            out.write("%s\t%s\t%d\n" % (rel, digest, size))
            blob = os.path.join(CACHE, "sha256", digest[:2], digest)
            exists = os.path.isfile(blob)
            if exists:
                with_blob += 1
            else:
                without_blob += 1
                missing_bytes += size
                top = rel.split("/")[0]
                no_blob_dirs[top] = no_blob_dirs.get(top, 0) + 1
            if digest in seen:
                dup_bytes += size
            else:
                seen[digest] = size
            n += 1
            if n % 20000 == 0:
                print("  %d files, %.0fs" % (n, time.time() - t0), flush=True)
    total = with_blob_bytes = 0
    with open(LIST, encoding="utf-8") as fh:
        for line in fh:
            total += int(line.split("\t")[1])
    with open(OUT, encoding="utf-8") as fh:
        for line in fh:
            rel, digest, size = line.rstrip("\n").split("\t")
            size = int(size)
            if os.path.isfile(os.path.join(CACHE, "sha256", digest[:2], digest)):
                with_blob_bytes += size
    print("files=%d  with_blob=%d  without_blob=%d" % (n, with_blob, without_blob))
    print("vfs total bytes      = %d (%.2f GB)" % (total, total / 1e9))
    print("bytes also in cache  = %d (%.2f GB)   <- option A saving" % (with_blob_bytes, with_blob_bytes / 1e9))
    print("bytes only in vfs    = %d (%.2f GB)" % (total - with_blob_bytes, (total - with_blob_bytes) / 1e9))
    print("duplicate content inside vfs = %d (%.2f GB); distinct contents=%d"
          % (dup_bytes, dup_bytes / 1e9, len(seen)))
    print("no-blob vfs dirs (file counts):", sorted(no_blob_dirs.items(), key=lambda kv: -kv[1])[:12])
    print("elapsed %.1fs" % (time.time() - t0))


if __name__ == "__main__":
    main()
