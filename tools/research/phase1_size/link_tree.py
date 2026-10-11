"""Hard-link copy of a converted tree (no data duplicated): python link_tree.py <src> <dst>"""
import os, sys
src, dst = sys.argv[1:3]
for root, dirs, files in os.walk(src):
    target = os.path.join(dst, os.path.relpath(root, src))
    os.makedirs(target, exist_ok=True)
    for f in files:
        os.link(os.path.join(root, f), os.path.join(target, f))
print("linked", dst)
