"""Size of a tree counting hard-linked files once: python unique_size.py <dir> [<dir>...]"""
import os, sys
from collections import defaultdict

seen = set()
per_top = defaultdict(lambda: [0, 0, 0])  # apparent, unique, files
for root in sys.argv[1:]:
    for dirpath, _, files in os.walk(root):
        rel = os.path.relpath(dirpath, root).split(os.sep)[0]
        for name in files:
            st = os.stat(os.path.join(dirpath, name))
            key = (st.st_dev, st.st_ino)
            entry = per_top[rel if rel != "." else name]
            entry[0] += st.st_size
            entry[2] += 1
            if key not in seen:
                seen.add(key)
                entry[1] += st.st_size
total_a = sum(v[0] for v in per_top.values()); total_u = sum(v[1] for v in per_top.values())
for k, (a, u, n) in sorted(per_top.items(), key=lambda kv: -kv[1][0])[:12]:
    print(f"{k:40s} files {n:7d}  apparent {a/1e9:7.2f} GB  unique {u/1e9:7.2f} GB")
print(f"TOTAL apparent {total_a/1e9:.2f} GB, unique on disk {total_u/1e9:.2f} GB")
