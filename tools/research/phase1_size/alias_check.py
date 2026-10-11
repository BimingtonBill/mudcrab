"""Are textures/*.opensky-srgb.ktx2 hard links of their base texture?

Usage: python tools/research/phase1_size/alias_check.py
Read-only.
"""
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from hardlink_check import file_identity  # noqa: E402

TEX = r"C:/Modding/SkyrimConverted-upstream/textures"
LIST = r"C:/Modding/OpenSkyrimProject/local/research/phase1_size/textures.tsv"


def main():
    rows = []
    with open(LIST, encoding="utf-8") as fh:
        for line in fh:
            p = line.rstrip("\n").split("\t")
            if len(p) >= 2:
                rows.append((p[0], int(p[1])))
    aliases = [(r, s) for r, s in rows if r.endswith(".opensky-srgb.ktx2")]
    print("aliases: %d files, %.2f GB" % (len(aliases), sum(s for _, s in aliases) / 1e9))
    random.seed(11)
    sample = random.sample(aliases, min(200, len(aliases)))
    same = 0
    counts = {}
    missing = 0
    for rel, _s in sample:
        base = rel[: -len(".opensky-srgb.ktx2")] + ".ktx2"
        a = file_identity(os.path.join(TEX, rel.replace("/", os.sep)))
        b = file_identity(os.path.join(TEX, base.replace("/", os.sep)))
        if b is None:
            missing += 1
            continue
        if a and b and a[0] == b[0] and a[1] == b[1]:
            same += 1
        if a:
            counts[a[2]] = counts.get(a[2], 0) + 1
    print("sample=%d same_file_id=%d base_missing=%d" % (len(sample), same, missing))
    print("alias link counts:", sorted(counts.items()))
    # what a real disk-usage count would say
    du = sum(s for _, s in rows) - sum(s for r, s in rows if r.endswith(".opensky-srgb.ktx2"))
    print("textures/ apparent %.2f GB, deduplicated-by-hardlink %.2f GB"
          % (sum(s for _, s in rows) / 1e9, du / 1e9))


if __name__ == "__main__":
    main()
