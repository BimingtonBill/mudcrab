"""DDS input formats vs KTX2 output size.

Usage: python tools/research/phase1_size/texture_scan.py

Reads only the first 148 bytes of each .dds in vfs/textures (DDS header + DX10
extension) and the first 80 bytes of each .ktx2 in textures/ (KTX2 header).
Matches input to output through the converter's canonical path rule
(asset_path.rs canonical_asset_path: keep everything after the last "textures"
component, lowercase, extension -> ktx2, prefix "textures/").

Also samples zstd-compressibility of the BCn payload so option C's
supercompressed variant can be estimated. Read-only.
"""
import os
import struct
from collections import defaultdict

ROOT = r"C:/Modding/SkyrimConverted-upstream"
VFS = os.path.join(ROOT, "vfs")
TEX = os.path.join(ROOT, "textures")
OUT = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size"

# DDS_PIXELFORMAT fourCC -> readable name
FOURCC = {
    b"DXT1": "BC1/DXT1",
    b"DXT2": "DXT2",
    b"DXT3": "DXT3",
    b"DXT4": "DXT4",
    b"DXT5": "BC3/DXT5",
    b"ATI1": "BC4/ATI1",
    b"BC4U": "BC4U",
    b"BC4S": "BC4S",
    b"BC5U": "BC5U",
    b"ATI2": "BC5/ATI2",
    b"ATI3": "ATI3",
    b"BC5S": "BC5S",
    b"DX10": "DX10",
    b"\x00\x00\x00\x00": "uncompressed",
}
DXGI = {
    28: "R8G8B8A8_UNORM", 29: "R8G8B8A8_UNORM_SRGB", 87: "B8G8R8A8_UNORM",
    71: "BC1_UNORM", 72: "BC1_UNORM_SRGB", 74: "BC2_UNORM", 77: "BC3_UNORM",
    78: "BC3_UNORM_SRGB", 80: "BC4_UNORM", 83: "BC5_UNORM", 95: "BC6H_UF16",
    98: "BC7_UNORM", 99: "BC7_UNORM_SRGB", 61: "R8_UNORM", 49: "R8G8_UNORM",
}
KTX2_SCHEME = {0: "none", 1: "BasisLZ(ETC1S)", 2: "Zstandard", 3: "ZLIB"}


def load_tsv(name):
    rows = []
    with open(os.path.join(OUT, name), encoding="utf-8") as fh:
        for line in fh:
            p = line.rstrip("\n").split("\t")
            if len(p) >= 2:
                rows.append((p[0], int(p[1])))
    return rows


def dds_format(path):
    with open(path, "rb") as fh:
        head = fh.read(148)
    if len(head) < 128 or head[:4] != b"DDS ":
        return "not_dds"
    fourcc = head[84:88]
    if fourcc == b"DX10" and len(head) >= 148:
        fmt = struct.unpack_from("<I", head, 128)[0]
        return "DX10 " + DXGI.get(fmt, "dxgi_%d" % fmt)
    return FOURCC.get(fourcc, "fourcc:" + repr(fourcc))


def canonical(rel):
    """vfs/textures/<x>.dds -> textures/<x>.ktx2 (asset_path.rs:122)."""
    parts = rel.split("/")
    idx = max(i for i, c in enumerate(parts) if c.lower() == "textures")
    rest = parts[idx + 1:]
    rest[-1] = rest[-1].rsplit(".", 1)[0] + ".ktx2"
    return "textures/" + "/".join(rest).lower()


def main():
    tex_sizes = dict(load_tsv("textures.tsv"))
    dds_rows = [(r, s) for r, s in load_tsv("vfs.tsv") if r.lower().endswith(".dds")]

    per_in = defaultdict(lambda: [0, 0, 0, 0])   # count, dds bytes, ktx2 bytes, matched
    mismatches = []
    total_dds = total_ktx2 = 0
    matched_paths = set()
    for rel, size in dds_rows:
        fmt = dds_format(os.path.join(VFS, rel.replace("/", os.sep)))
        # textures.tsv keys are relative to the textures/ root, canonical() is not
        key = canonical(rel)[len("textures/"):]
        out_size = tex_sizes.get(key)
        total_dds += size
        e = per_in[fmt]
        e[0] += 1
        e[1] += size
        if out_size is not None:
            e[2] += out_size
            e[3] += 1
            total_ktx2 += out_size
            matched_paths.add(key)
        else:
            mismatches.append(rel)

    print("== DDS input vs KTX2 output, by input format ==")
    print("%-18s %7s %12s %12s %10s %8s" % ("input format", "count", "dds GB", "ktx2 GB", "ktx2/dds", "matched"))
    for fmt, (n, db, kb, m) in sorted(per_in.items(), key=lambda kv: -kv[1][1]):
        ratio = (kb / db) if db else 0
        print("%-18s %7d %12.2f %12.2f %10.3f %8d" % (fmt, n, db / 1e9, kb / 1e9, ratio, m))
    print("TOTAL dds %.2f GB -> ktx2 %.2f GB (matched %d/%d inputs, unmatched %d)"
          % (total_dds / 1e9, total_ktx2 / 1e9, len(matched_paths), len(dds_rows), len(mismatches)))
    for rel in mismatches[:10]:
        print("   unmatched:", rel)

    # outputs with no dds input (srgb aliases etc.)
    unclaimed = {k: v for k, v in tex_sizes.items() if k not in matched_paths}
    print("ktx2 outputs not traced to a dds: %d files, %.2f GB"
          % (len(unclaimed), sum(unclaimed.values()) / 1e9))

    # KTX2 header split
    per_scheme = defaultdict(lambda: [0, 0])
    for rel, size in load_tsv("textures.tsv"):
        if not rel.lower().endswith(".ktx2"):
            continue
        path = os.path.join(TEX, rel.replace("/", os.sep))
        with open(path, "rb") as fh:
            h = fh.read(80)
        scheme = struct.unpack_from("<I", h, 44)[0] if len(h) >= 48 else -1
        vkfmt = struct.unpack_from("<I", h, 12)[0] if len(h) >= 16 else -1
        per_scheme[(vkfmt, scheme)][0] += 1
        per_scheme[(vkfmt, scheme)][1] += size
    print()
    print("== KTX2 outputs by vkFormat/supercompression ==")
    for (vkfmt, scheme), (n, s) in sorted(per_scheme.items(), key=lambda kv: -kv[1][1]):
        print("   vkFormat=%-4d supercompression=%-16s %7d files %8.2f GB"
              % (vkfmt, KTX2_SCHEME.get(scheme, str(scheme)), n, s / 1e9))

    # zstd sample on raw DDS payloads (the option-C supercompressed estimate)
    zstd_impl = None
    try:
        import zstandard as zstd_mod  # noqa: F401
        zstd_impl = "zstandard"
    except ImportError:
        try:
            from compression import zstd as _z  # noqa: F401
            zstd_impl = "compression.zstd"
        except ImportError:
            zstd_impl = None
    print()
    print("zstd implementation available:", zstd_impl)
    if zstd_impl:
        import random
        random.seed(7)
        by_fmt = defaultdict(list)
        for rel, size in dds_rows:
            fmt = dds_format(os.path.join(VFS, rel.replace("/", os.sep)))
            by_fmt[fmt].append((rel, size))
        if zstd_impl == "zstandard":
            import zstandard
            def zc(data):
                return len(zstandard.ZstdCompressor(level=3).compress(data))
        else:
            from compression import zstd as _z
            def zc(data):
                return len(_z.compress(data, level=3))
        print("%-18s %6s %14s %14s %8s %12s" % ("format", "files", "payload B", "zstd B", "ratio", "zstd GB(est)"))
        total_zstd = 0.0
        for fmt, rows in sorted(by_fmt.items(), key=lambda kv: -sum(s for _, s in kv[1])):
            sample = random.sample(rows, min(40, len(rows)))
            raw = comp = 0
            for rel, _size in sample:
                with open(os.path.join(VFS, rel.replace("/", os.sep)), "rb") as fh:
                    data = fh.read()
                payload = data[128:]
                raw += len(payload)
                comp += zc(payload)
            ratio = comp / raw if raw else 0
            full = sum(s for _, s in rows) * ratio
            total_zstd += full
            print("%-18s %6d %14d %14d %8.3f %12.1f"
                  % (fmt, len(rows), raw, comp, ratio, full / 1e9))
        print("=> option C with zstd supercompression: about %.2f GB of texture payload"
              % (total_zstd / 1e9))


if __name__ == "__main__":
    main()
