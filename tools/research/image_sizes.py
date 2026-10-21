#!/usr/bin/env python3
"""Print the pixel size and aspect of every image in a folder (one line each).

Usage:
    python tools/research/image_sizes.py <folder> [extension]
"""

import os
import sys

from PIL import Image


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    folder = argv[1]
    extension = (argv[2] if len(argv) > 2 else ".jpg").lower()
    for name in sorted(os.listdir(folder)):
        if not name.lower().endswith(extension):
            continue
        with Image.open(os.path.join(folder, name)) as image:
            width, height = image.size
        print(f"  {name:38} {width:5}x{height:<5} aspect {width / height:.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
