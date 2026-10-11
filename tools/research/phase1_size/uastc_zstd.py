"""Would Zstd supercompression shrink the existing UASTC KTX2 payloads?

Usage: python tools/research/phase1_size/uastc_zstd.py [sample]

The shipped KTX2 files carry supercompressionScheme = 0 (none), although
docs/roadmap/01-asset-pipeline.md calls the output supercompressed. This samples
KTX2 files, slices out the level payload (everything from the first mip's
byteOffset to EOF) and zstd-compresses it. Read-only.
"""
import os
import random
import struct
import sys
from collections import defaultdict

TEX = r"C:/Modding/SkyrimConverted-upstream/textures"
LIST = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/textures.tsv"


def payload_bounds(data):
    level_count = struct.unpack_from("<I", data, 40)[0]
    if level_count == 0:
        level_count = 1
    # first level index starts at byte 80; LevelIndex: byteOffset(u64), byteLength(u64), uncompressedByteLength(u64)
    first = struct.unpack_from("<Q", data, 80)[0]
    return first


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 60
    import zstandard
    rows = []
    with open(LIST, encoding="utf-8") as fh:
        for line in fh:
            p = line.rstrip("\n").split("\t")
            if len(p) >= 2 and p[0].endswith(".ktx2") and not p[0].endswith(".opensky-srgb.ktx2"):
                rows.append((p[0], int(p[1])))
    random.seed(3)
    sample = random.sample(rows, min(n, len(rows)))
    c = zstandard.ZstdCompressor(level=3)
    raw = comp = whole = 0
    for rel, _size in sample:
        with open(os.path.join(TEX, rel.replace("/", os.sep)), "rb") as fh:
            data = fh.read()
        off = payload_bounds(data)
        body = data[off:]
        raw += len(body)
        comp += len(c.compress(body))
        whole += len(data)
    ratio = comp / raw if raw else 0
    total = sum(s for _, s in rows)
    print("sample=%d level payload %d bytes -> zstd %d (%.3f)" % (len(sample), raw, comp, ratio))
    print("non-payload header bytes in sample: %d of %d (%.1f%%)" % (whole - raw, whole, 100.0 * (whole - raw) / whole))
    print("primary ktx2 total %.2f GB -> zstd-payload estimate %.2f GB (saves %.2f GB)"
          % (total / 1e9, (total * ratio) / 1e9, (total * (1 - ratio)) / 1e9))


if __name__ == "__main__":
    main()
