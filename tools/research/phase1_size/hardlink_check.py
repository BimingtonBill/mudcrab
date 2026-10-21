"""Are vfs files hard links of ingestion-cache entries?

Usage: python tools/research/phase1_size/hardlink_check.py [sample]

For a deterministic sample of vfs files: hash the vfs file, look for the same
content in the content-addressed store (sha256/<ab>/<rest>), and compare the NTFS
file id (GetFileInformationByHandle) and link count of both.

Read-only.
"""
import ctypes
import ctypes.wintypes as wt
import hashlib
import os
import random
import sys

ROOT = r"C:/Modding/SkyrimConverted-upstream"
VFS = os.path.join(ROOT, "vfs")
CACHE = os.path.join(ROOT, ".ingestion-cache")
LIST = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/vfs.tsv"

GENERIC_READ = 0x80000000
FILE_SHARE_ALL = 0x1 | 0x2 | 0x4
OPEN_EXISTING = 3
FILE_FLAG_BACKUP_SEMANTICS = 0x02000000

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)


class BY_HANDLE_FILE_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("dwFileAttributes", wt.DWORD),
        ("ftCreationTime", wt.FILETIME),
        ("ftLastAccessTime", wt.FILETIME),
        ("ftLastWriteTime", wt.FILETIME),
        ("dwVolumeSerialNumber", wt.DWORD),
        ("nFileSizeHigh", wt.DWORD),
        ("nFileSizeLow", wt.DWORD),
        ("nNumberOfLinks", wt.DWORD),
        ("nFileIndexHigh", wt.DWORD),
        ("nFileIndexLow", wt.DWORD),
    ]


def file_identity(path):
    """(volume, file_index_hex, link_count) or None."""
    handle = kernel32.CreateFileW(
        wt.LPCWSTR(path), GENERIC_READ, FILE_SHARE_ALL, None,
        OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, None)
    if handle == -1 or handle == 0xFFFFFFFF:
        return None
    try:
        info = BY_HANDLE_FILE_INFORMATION()
        ok = kernel32.GetFileInformationByHandle(handle, ctypes.byref(info))
        if not ok:
            return None
        idx = (info.nFileIndexHigh << 32) | info.nFileIndexLow
        return (info.dwVolumeSerialNumber, idx, info.nNumberOfLinks)
    finally:
        kernel32.CloseHandle(handle)


def sha256_of(path, limit=None):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        while True:
            chunk = fh.read(1 << 20)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    rels = []
    with open(LIST, encoding="utf-8") as fh:
        for line in fh:
            rel = line.split("\t")[0]
            rels.append(rel)
    random.seed(20260927)
    sample = random.sample(rels, min(n, len(rels)))

    same_index = 0
    cache_hit = 0
    missing_cache = 0
    size_mismatch = 0
    link_counts = {}
    examples = []
    for rel in sample:
        vpath = os.path.join(VFS, rel.replace("/", os.sep))
        digest = sha256_of(vpath)
        # blob_path() joins the FULL hash under the 2-char prefix dir (archive/mod.rs:221)
        cpath = os.path.join(CACHE, "sha256", digest[:2], digest)
        if not os.path.exists(cpath):
            missing_cache += 1
            continue
        cache_hit += 1
        a = file_identity(vpath)
        b = file_identity(cpath)
        if os.path.getsize(vpath) != os.path.getsize(cpath):
            size_mismatch += 1
        if a and b:
            link_counts[a[2]] = link_counts.get(a[2], 0) + 1
            if a[0] == b[0] and a[1] == b[1]:
                same_index += 1
                if len(examples) < 5:
                    examples.append((rel, a[2]))
    print("sample=%d cache_hit=%d missing_in_cache=%d size_mismatch=%d same_file_id=%d"
          % (len(sample), cache_hit, missing_cache, size_mismatch, same_index))
    print("vfs link counts in sample:", sorted(link_counts.items()))
    for rel, lc in examples:
        print("  hardlink example: %s (links=%d)" % (rel, lc))


if __name__ == "__main__":
    main()
