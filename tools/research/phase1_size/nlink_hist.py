"""Histogram of hard-link counts under a folder: python nlink_hist.py <dir>"""
import os, sys
from collections import Counter
count, size = Counter(), Counter()
examples = {}
for d, _, fs in os.walk(sys.argv[1]):
    for n in fs:
        p = os.path.join(d, n); st = os.stat(p)
        count[st.st_nlink] += 1; size[st.st_nlink] += st.st_size
        examples.setdefault(st.st_nlink, p)
for k in sorted(count):
    print(f"nlink {k}: {count[k]} files, {size[k]/1e9:.2f} GB, e.g. {examples[k]}")
