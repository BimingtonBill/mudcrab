#!/usr/bin/env python3
"""Shrink reference screenshots to the size the comparisons use, in place.

Usage:
    python tools/research/compress_references.py <folder>... [--long-side 1400] [--quality 85] [--dry-run]

Every .jpg/.jpeg/.png/.webp in each folder is scaled so its long side is at most `--long-side`
(the shots files render at 1400x1050, so a larger reference only costs disk and load time), keeps
its aspect ratio, and is saved as an optimized progressive JPEG without metadata under the same
stem with a `.jpg` suffix. A file is left alone when it is already that size or smaller and
re-encoding would not make it at least 10% smaller. Prints one line per file and a total.
"""

import os
import sys

from PIL import Image, ImageFile

ImageFile.LOAD_TRUNCATED_IMAGES = False
EXTENSIONS = (".jpg", ".jpeg", ".png", ".webp")


def compress(path, long_side, quality, dry_run):
    before = os.path.getsize(path)
    with Image.open(path) as image:
        image.load()
        size = image.size
        scale = min(1.0, long_side / max(size))
        target = (round(size[0] * scale), round(size[1] * scale))
        rgb = image.convert("RGB")
        if target != size:
            rgb = rgb.resize(target, Image.LANCZOS)
    stem, extension = os.path.splitext(path)
    out = stem + ".jpg"
    temporary = out + ".tmp"
    rgb.save(temporary, "JPEG", quality=quality, optimize=True, progressive=True)
    after = os.path.getsize(temporary)
    keep_original = target == size and extension.lower() in (".jpg", ".jpeg") and after > before * 0.9
    if dry_run or keep_original:
        os.remove(temporary)
        return size, (size if keep_original else target), before, (before if keep_original else after), keep_original
    os.replace(temporary, out)
    if out != path:
        os.remove(path)
    return size, target, before, after, False


def main(argv):
    args = argv[1:]
    long_side = int(args[args.index("--long-side") + 1]) if "--long-side" in args else 1400
    quality = int(args[args.index("--quality") + 1]) if "--quality" in args else 85
    dry_run = "--dry-run" in args
    skip = {"--long-side", "--quality"}
    folders, i = [], 0
    while i < len(args):
        if args[i] in skip:
            i += 2
            continue
        if not args[i].startswith("--"):
            folders.append(args[i])
        i += 1
    if not folders:
        print(__doc__)
        return 2
    total_before = total_after = 0
    for folder in folders:
        for name in sorted(os.listdir(folder)):
            if not name.lower().endswith(EXTENSIONS) or name.endswith(".tmp"):
                continue
            size, target, before, after, kept = compress(os.path.join(folder, name), long_side, quality, dry_run)
            total_before += before
            total_after += after
            state = "kept" if kept else "shrunk"
            print(f"{state:6} {size[0]}x{size[1]} -> {target[0]}x{target[1]}  {before // 1024} KB -> {after // 1024} KB  {os.path.join(folder, name)}")
    print(f"total {total_before / 1e6:.1f} MB -> {total_after / 1e6:.1f} MB")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
