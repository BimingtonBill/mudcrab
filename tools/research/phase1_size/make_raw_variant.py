"""Build a timing twin of a converted tree whose textures are the raw (not supercompressed) originals.

python make_raw_variant.py <zstd_tree> <raw_textures_tree> <out_tree>
Everything the engine reads except textures/ is hard-linked from <zstd_tree>; textures/ is copied
from <raw_textures_tree>/textures. vfs/ and the ingestion cache are left out (the engine reads neither).
"""
import os, shutil, sys
src, raw, out = sys.argv[1:4]
os.makedirs(out, exist_ok=False)
for name in os.listdir(src):
    if name in ("vfs", ".ingestion-cache", "textures"):
        continue
    s, d = os.path.join(src, name), os.path.join(out, name)
    if os.path.isdir(s):
        for root, _, files in os.walk(s):
            target = os.path.join(d, os.path.relpath(root, s))
            os.makedirs(target, exist_ok=True)
            for f in files:
                os.link(os.path.join(root, f), os.path.join(target, f))
    else:
        os.link(s, d)
shutil.copytree(os.path.join(raw, "textures"), os.path.join(out, "textures"))
print("done", out)
